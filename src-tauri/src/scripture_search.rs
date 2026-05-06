//! Unified scripture retrieval service.
//!
//! Every scripture-lookup path in the app — manual UI search, on-demand
//! verse fetch, live-mic capture loop, transcript analysis enrichment — must
//! route through [`search_scripture_unified`]. This collapses what used to be
//! 3+ ad-hoc lookup paths (each with subtly different fallback behavior) into
//! one tier-ordered pipeline:
//!
//! 1. **Normalize** the query (lowercase, strip noise, expand common STT errs).
//! 2. **Explicit reference parser** — deterministic grammar parser. If the
//!    query is `"John 3:16"` and the verse exists in the DB, that wins with
//!    score 1.0.
//! 3. **Disambiguation expansion** — bare ambiguous books (Chronicles,
//!    Samuel, Kings, Corinthians, Thessalonians, Timothy, Peter) emit one
//!    candidate per valid canonical option with `needs_disambiguation = true`.
//! 4. **Local FTS exact/partial** — SQLite FTS5 phrase search. Score derived
//!    from token overlap ratio.
//! 5. **Keyword detector** — existing `ReferenceKeywordDetector` for known
//!    sermon-phrase ↔ reference mappings (e.g. "all things work together
//!    for good" → Romans 8:28).
//! 6. **Lightweight semantic** — TF-IDF cosine over the loaded translation,
//!    built lazily on first use and cached. Pure Rust, no Python, no ONNX
//!    runtime, no model download. (A heavier embedding lane can be slotted
//!    in here later without touching callers.)
//! 7. **Cloud assist** — disabled by default and blocked by Data Miser /
//!    offline mode. When an OpenAI key is saved and the local score is weak,
//!    the assistant may propose references, but all verse text is still fetched
//!    from the local SQLite Bible.
//!
//! Ranked candidates are deduplicated by (book, chapter, verse), capped at
//! `MAX_RESULTS`, and returned with provenance so the UI can display "why
//! did you pick this?" tooltips.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use aletheia_detection::books::lookup_ambiguous;
use aletheia_detection::calibration::IsotonicCalibration;
use aletheia_detection::cross_encoder::SharedCrossEncoder;
use aletheia_detection::cross_refs::{CrossRefGraph, RecentVerseHistory, apply_recent_prior};
use aletheia_detection::grammar::GrammarReferenceParser;
use aletheia_detection::normalize::TranscriptNormalizer;
use aletheia_detection::phrase_catalog::detect_catalog_matches_raw;
use aletheia_detection::{DetectionTier, RankedCandidate};
use aletheia_store::{AletheiaStore, VerseRecord};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dto::SearchResultDto;
use crate::{parse_reference, sanitize_fts_query, verse_to_search_result};

/// What surface invoked the search. Used to tune which tiers run and what
/// the result count cap is — live-mic wants the top 1, manual wants 10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchContext {
    ManualSearch,
    LiveTranscript,
    OnDemandFetch,
    BibleReader,
    DashboardOpen,
}

impl Default for SearchContext {
    fn default() -> Self {
        SearchContext::ManualSearch
    }
}

const MAX_RESULTS: usize = 10;
const MAX_ENTITY_EPISODE_DOCUMENTS: usize = 8_000;

/// Public entry point. All scripture lookups route through here.
/// Wrapped in `catch_unwind` so a tier-internal panic (e.g. malformed FTS
/// regex from an exotic translation) can never tear down the Tauri shell.
pub fn search_scripture_unified(
    query: &str,
    translation_id: &str,
    context: SearchContext,
    store: &AletheiaStore,
) -> Vec<SearchResultDto> {
    let q = query.to_string();
    let tid = translation_id.to_string();
    let scored = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_scripture_unified_inner(&q, &tid, context, store)
    })) {
        Ok(v) => v,
        Err(_) => {
            log::error!(
                "[scripture-search] PANIC swallowed for query={query:?} translation={translation_id}"
            );
            return Vec::new();
        }
    };
    scored.into_iter().map(scored_to_dto).collect()
}

/// Same pipeline as [`search_scripture_unified`] but returns the ranked
/// candidates with their tier provenance so a caller (the live capture loop)
/// can run [`aletheia_detection::ConfidencePolicy::evaluate_auto_open`] and
/// honour the operating mode. Hard-capped to `LiveTranscript` semantics
/// (top 3) — live mic should not flood the operator queue.
pub fn rank_candidates_for_live(
    query: &str,
    translation_id: &str,
    store: &AletheiaStore,
) -> Vec<RankedCandidate> {
    rank_and_resolve_for_live(query, translation_id, store)
        .map(|(ranked, _)| ranked)
        .unwrap_or_default()
}

/// One-pass live detection: runs the unified pipeline once and returns both
/// the typed `RankedCandidate` list (for `ConfidencePolicy`) and the top
/// `SearchResultDto` (with verse text already resolved). The capture loop
/// uses this to avoid walking the tier pipeline twice per segment.
///
/// Returns `None` when the pipeline produced no candidates.
pub fn rank_and_resolve_for_live(
    query: &str,
    translation_id: &str,
    store: &AletheiaStore,
) -> Option<(Vec<RankedCandidate>, SearchResultDto)> {
    let q = query.to_string();
    let tid = translation_id.to_string();
    let scored = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_scripture_unified_inner(&q, &tid, SearchContext::LiveTranscript, store)
    })) {
        Ok(v) => v,
        Err(_) => return None,
    };
    if scored.is_empty() {
        return None;
    }

    let ranked: Vec<RankedCandidate> = scored
        .iter()
        .map(|r| {
            let reference = match r.range_end {
                Some(end) => format!(
                    "{} {}:{}-{}",
                    r.record.book, r.record.chapter, r.record.verse, end
                ),
                None => format!("{} {}:{}", r.record.book, r.record.chapter, r.record.verse),
            };
            RankedCandidate {
                reference,
                score: r.score,
                tiers: vec![r.tier],
            }
        })
        .collect();

    let top = scored.into_iter().next().expect("non-empty");
    let range_end = top.range_end;
    let mut top_dto = verse_to_search_result(top.record, &top.source);
    if let Some(end) = range_end {
        top_dto.reference = format!("{}-{end}", top_dto.reference);
        top_dto.verse_id = format!("{}|{end}", top_dto.verse_id);
    }
    Some((ranked, top_dto))
}

/// Knobs that shape an extended live-detection pass. Held as a single struct
/// so callers don't have to thread half a dozen positional arguments through
/// the pipeline.
pub struct LiveRankingPriors<'a> {
    /// Translation packs to query in parallel. The pipeline runs once per pack
    /// and the union is deduped by `(book, chapter, verse)`. The first pack is
    /// considered "primary" — the resolved DTO comes from it when available.
    pub translation_packs: &'a [String],
    /// Stage B: cross-encoder that rescores top-N candidates against the live
    /// query window. `None` skips Stage B (e.g. unit tests).
    pub cross_encoder: Option<&'a SharedCrossEncoder>,
    /// Stage C: cross-reference graph (read-only).
    pub graph: &'a CrossRefGraph,
    /// Stage C: recent-verse history (already cloned by the caller).
    pub recent: &'a RecentVerseHistory,
    /// Stage C: maximum upward lift the cross-ref prior may apply.
    pub max_lift: f32,
    /// Stage D: isotonic calibration to apply post-rerank. Identity-equivalent
    /// when fewer than ~25 operator samples have been recorded.
    pub calibration: Option<&'a IsotonicCalibration>,
    /// Blend ratio for Stage B: `final = (1 - w) * retrieval + w * rerank`.
    /// Typical: 0.55 (rerank dominates but retrieval still anchors).
    pub rerank_weight: f32,
}

impl<'a> LiveRankingPriors<'a> {
    /// Sane defaults for the live capture loop.
    pub fn from_state(
        translation_packs: &'a [String],
        cross_encoder: &'a SharedCrossEncoder,
        graph: &'a CrossRefGraph,
        recent: &'a RecentVerseHistory,
        calibration: Option<&'a IsotonicCalibration>,
    ) -> Self {
        Self {
            translation_packs,
            cross_encoder: Some(cross_encoder),
            graph,
            recent,
            max_lift: 0.10,
            calibration,
            rerank_weight: 0.55,
        }
    }
}

/// Extended live-detection entry point that applies the four-stage pipeline:
/// cross-translation union → Stage B rerank → Stage C cross-ref prior →
/// Stage D calibration. Falls back to single-pack behaviour when
/// `priors.translation_packs` is empty.
pub fn rank_and_resolve_for_live_extended(
    query: &str,
    primary_translation_id: &str,
    store: &AletheiaStore,
    priors: &LiveRankingPriors<'_>,
) -> Option<(Vec<RankedCandidate>, SearchResultDto)> {
    let q = query.to_string();
    let primary = primary_translation_id.to_string();

    // Decide which packs to query. Primary always runs first so we can keep
    // its resolved DTO as the "shown to operator" surface.
    let mut packs: Vec<String> = Vec::with_capacity(priors.translation_packs.len() + 1);
    packs.push(primary.clone());
    for pack in priors.translation_packs {
        let lc = pack.to_ascii_lowercase();
        if lc != primary && !packs.contains(&lc) {
            packs.push(lc);
        }
    }

    // Cross-translation union — run the pipeline per pack; dedupe by canonical
    // verse id. Each pack runs in panic-isolation so one corrupt pack cannot
    // poison the live loop.
    let mut union_scored: Vec<(String, ScoredResult)> = Vec::new();
    for pack in &packs {
        let pack_query = q.clone();
        let pack_id = pack.clone();
        let scored = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            search_scripture_unified_inner(
                &pack_query,
                &pack_id,
                SearchContext::LiveTranscript,
                store,
            )
        })) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for r in scored {
            union_scored.push((pack.clone(), r));
        }
    }

    if union_scored.is_empty() {
        return None;
    }

    // Dedup by (book, chapter, verse, range_end), keep the highest-score copy
    // per cell. Translation tag is preserved on the winning copy so downstream
    // consumers know which pack produced the text snippet.
    union_scored.sort_by(|a, b| b.1.score.total_cmp(&a.1.score));
    let mut seen: std::collections::HashSet<(String, u16, u16, Option<u16>)> = Default::default();
    union_scored.retain(|(_, r)| {
        let key = (
            r.record.book.clone(),
            r.record.chapter,
            r.record.verse,
            r.range_end,
        );
        seen.insert(key)
    });

    // Stage B: cross-encoder rerank on the top-N union. Blend with retrieval
    // score so a single weak signal can't dominate. We rerank only the top 12
    // — anything below that is unlikely to ever be presented.
    if let Some(ce) = priors.cross_encoder {
        let n_rerank = union_scored.len().min(12);
        let pairs: Vec<&str> = union_scored[..n_rerank]
            .iter()
            .map(|(_, r)| r.record.text.as_str())
            .collect();
        let rerank_scores = ce.score_batch(&q, &pairs);
        let w = priors.rerank_weight.clamp(0.0, 1.0);
        for (i, (_, r)) in union_scored.iter_mut().take(n_rerank).enumerate() {
            let rs = rerank_scores.get(i).copied().unwrap_or(r.score);
            r.score = ((1.0 - w) * r.score + w * rs).clamp(0.0, 1.0);
        }
    }

    // Stage C: cross-reference prior. Apply this only to the broad semantic
    // lane. Exact references, phrase quotes, learned memories, and curated
    // topic/story matches must not be pulled back toward the previous live
    // passage; that is the failure mode where "Matthew 3:4" or "Joseph was
    // sold" gets interpreted through the old on-screen context.
    for (_, r) in union_scored.iter_mut() {
        if !matches!(r.tier, DetectionTier::SermonContextRerank) {
            continue;
        }
        let reference = match r.range_end {
            Some(end) => format!(
                "{} {}:{}-{}",
                r.record.book, r.record.chapter, r.record.verse, end
            ),
            None => format!("{} {}:{}", r.record.book, r.record.chapter, r.record.verse),
        };
        r.score = apply_recent_prior(
            &reference,
            r.score,
            priors.recent,
            priors.graph,
            priors.max_lift,
        );
    }

    // Stage D: isotonic calibration. Identity passthrough when not fit.
    if let Some(cal) = priors.calibration {
        for (_, r) in union_scored.iter_mut() {
            r.score = cal.lookup(r.score);
        }
    }

    // Resort after rerank/prior/calibration; retain the live cap.
    union_scored.sort_by(|a, b| b.1.score.total_cmp(&a.1.score));
    union_scored.truncate(cap_for_context(SearchContext::LiveTranscript));

    let ranked: Vec<RankedCandidate> = union_scored
        .iter()
        .map(|(_, r)| {
            let reference = match r.range_end {
                Some(end) => format!(
                    "{} {}:{}-{}",
                    r.record.book, r.record.chapter, r.record.verse, end
                ),
                None => format!("{} {}:{}", r.record.book, r.record.chapter, r.record.verse),
            };
            RankedCandidate {
                reference,
                score: r.score,
                tiers: vec![r.tier],
            }
        })
        .collect();

    let (_, top) = union_scored.into_iter().next()?;
    let range_end = top.range_end;
    let mut top_dto = verse_to_search_result(top.record, &top.source);
    if let Some(end) = range_end {
        top_dto.reference = format!("{}-{end}", top_dto.reference);
        top_dto.verse_id = format!("{}|{end}", top_dto.verse_id);
    }
    Some((ranked, top_dto))
}

/// Internal entry that returns the deduped, capped scored results so both
/// the DTO-mapping path and the ranking path can share the same pipeline.
fn search_scripture_unified_inner(
    query: &str,
    translation_id: &str,
    context: SearchContext,
    store: &AletheiaStore,
) -> Vec<ScoredResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = TranscriptNormalizer.normalize(trimmed);
    let mut accum: Vec<ScoredResult> = Vec::new();

    // Tier 2: explicit reference parser. `verse_end` (when present) means the
    // speaker said something like "John 3:16-17" or "Matthew 25 verses 14
    // through 30" — we honor that range by fetching every verse in the span
    // and concatenating them into a single result whose label reflects the
    // full range.
    let refs = GrammarReferenceParser.parse_all(&normalized.text);
    for parsed in &refs {
        let label = if parsed.needs_disambiguation {
            format!(
                "Reference (ambiguous: {})",
                parsed.disambiguation_options.join(", ")
            )
        } else {
            "Exact reference".to_string()
        };
        let range_end = parsed.verse_end.filter(|end| *end > parsed.verse_start);
        let combined = fetch_verse_range(
            store,
            translation_id,
            parsed.book,
            parsed.chapter,
            parsed.verse_start,
            range_end,
        );
        if let Some(record) = combined {
            accum.push(ScoredResult {
                score: parsed.confidence,
                source: label,
                record,
                range_end,
                tier: DetectionTier::ExactReference,
            });
        }
    }

    // Tier 3: legacy parser fallback (handles `Psalm 23:1` style not covered
    // by the grammar parser when the query is bare with no prefix words).
    if accum.is_empty() {
        if let Some((book, chapter, verse)) = parse_reference(trimmed) {
            if let Ok(Some(record)) = store.find_verse(translation_id, &book, chapter, verse) {
                accum.push(ScoredResult {
                    score: 0.99,
                    source: "Exact reference".to_string(),
                    record,
                    range_end: None,
                    tier: DetectionTier::ExactReference,
                });
            }
        }
    }

    // Tier 3b: explicit ambiguous-book expansion when the original query was
    // a bare ambiguous token (Chronicles 7:14 → both 1Chr and 2Chr). The
    // grammar parser handles this, but if the user typed it without spaces
    // around the colon we still want to try both books.
    let lc = normalized.text.clone();
    let first_token = lc.split_whitespace().next().unwrap_or("");
    if let Some(options) = lookup_ambiguous(first_token) {
        for canonical in options {
            // Try parsing the rest after the bare token as `<book> <tail>`.
            let rebuilt = format!(
                "{canonical} {}",
                lc.split_once(' ').map(|x| x.1).unwrap_or("")
            );
            if let Some((book, chapter, verse)) = parse_reference(rebuilt.trim()) {
                if let Ok(Some(record)) = store.find_verse(translation_id, &book, chapter, verse) {
                    accum.push(ScoredResult {
                        score: 0.85,
                        source: format!("Reference (ambiguous: {})", options.join(", ")),
                        record,
                        range_end: None,
                        tier: DetectionTier::ExactReference,
                    });
                }
            }
        }
    }

    // Tier 4: curated topic / sermon-phrase catalog (Beatitudes, parable of
    // the talents, prodigal son, fruit of the Spirit, etc.). Deterministic,
    // offline, instant. Run before broad FTS so known sermon shorthand such
    // as "mighty man of valor" resolves to Gideon instead of a lexically
    // similar verse about other mighty men.
    if accum.len() < cap_for_context(context) {
        let mut catalog_hits: Vec<(String, String, f32)> =
            detect_catalog_matches_raw(&normalized.text)
                .into_iter()
                .map(|hit| (hit.reference.to_string(), hit.alias, hit.score))
                .collect();
        // Append any operator-curated entries from the editable runtime
        // catalog. These are layered on top of (not replacements for) the
        // baked-in topic map, so a deployment can add a reference like
        // "fruit of the spirit" -> "Galatians 5:22-23" without recompiling.
        for entry in runtime_catalog() {
            for alias in &entry.aliases {
                if !alias.is_empty() && normalized.text.contains(&alias.to_lowercase()) {
                    catalog_hits.push((entry.reference.clone(), alias.clone(), entry.score));
                    break;
                }
            }
        }
        if let Ok(memories) = store.list_learned_scripture_phrases(translation_id, 300) {
            for memory in memories {
                let phrase = memory.phrase.trim().to_ascii_lowercase();
                let phrase_key = phrase
                    .split_whitespace()
                    .filter(|token| token.len() >= 3)
                    .collect::<Vec<_>>();
                if phrase.is_empty() {
                    continue;
                }
                let direct_match = normalized.text.contains(&phrase);
                let token_overlap = if phrase_key.is_empty() {
                    0.0
                } else {
                    let hits = phrase_key
                        .iter()
                        .filter(|token| normalized.text.contains(**token))
                        .count();
                    hits as f32 / phrase_key.len() as f32
                };
                if direct_match || token_overlap >= 0.72 {
                    let score = (memory.confidence as f32 + (memory.use_count as f32 * 0.005))
                        .clamp(0.78, 0.97);
                    catalog_hits.push((
                        memory.reference.clone(),
                        format!("learned phrase: {}", memory.phrase),
                        score,
                    ));
                }
            }
        }
        for (reference, alias, score) in catalog_hits {
            if let Some((book, chapter, verse_start, verse_end)) =
                parse_reference_with_range(&reference)
            {
                let range_end = verse_end.filter(|end| *end > verse_start);
                if let Some(record) = fetch_verse_range(
                    store,
                    translation_id,
                    &book,
                    chapter,
                    verse_start,
                    range_end,
                ) {
                    accum.push(ScoredResult {
                        score,
                        source: format!("Topic match: {alias}"),
                        record,
                        range_end,
                        tier: DetectionTier::ThematicCoreference,
                    });
                }
            }
        }
    }

    // Tier 5: FTS phrase search
    if accum.len() < cap_for_context(context) {
        let sanitized = sanitize_fts_query(trimmed);
        if !sanitized.is_empty() {
            if let Ok(records) = store.search_phrase(&sanitized, 10) {
                for record in records {
                    let score = score_overlap(&normalized.text, &record.text);
                    accum.push(ScoredResult {
                        score: score.max(0.55),
                        source: "Offline phrase match".to_string(),
                        record,
                        range_end: None,
                        tier: DetectionTier::VerseQuotation,
                    });
                }
            }
        }
    }

    // Tier 6: lightweight semantic (TF-IDF cosine). Built lazily.
    // Skipped silently if the index for this translation is still being
    // built in the background — caller never blocks waiting for it.
    if accum.len() < cap_for_context(context) {
        if let Some(top) = vector_search(store, translation_id, &normalized.text, 12) {
            for (record, score) in top {
                accum.push(ScoredResult {
                    score: score.max(0.45),
                    source: "Semantic match".to_string(),
                    record,
                    range_end: None,
                    tier: DetectionTier::SermonContextRerank,
                });
            }
        }
    }

    // Dedup by (book, chapter, verse), keep highest score.
    accum.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut seen: std::collections::HashSet<(String, u16, u16)> = Default::default();
    accum.retain(|r| {
        let key = (r.record.book.clone(), r.record.chapter, r.record.verse);
        seen.insert(key)
    });

    accum.into_iter().take(cap_for_context(context)).collect()
}

fn cap_for_context(context: SearchContext) -> usize {
    match context {
        SearchContext::LiveTranscript => 3,
        SearchContext::OnDemandFetch | SearchContext::DashboardOpen => 5,
        SearchContext::ManualSearch | SearchContext::BibleReader => MAX_RESULTS,
    }
}

#[derive(Clone)]
struct ScoredResult {
    score: f32,
    source: String,
    record: VerseRecord,
    /// When the parsed reference was a range (e.g. Matt 25:14-30), the
    /// inclusive end-verse. `record.text` already contains every verse in
    /// the range concatenated, and `record.verse` holds the start verse.
    range_end: Option<u16>,
    /// Which signal produced this candidate; threaded through to the
    /// confidence policy so it can apply the auto-open rules.
    tier: DetectionTier,
}

fn scored_to_dto(r: ScoredResult) -> SearchResultDto {
    let range_end = r.range_end;
    let mut dto = verse_to_search_result(r.record, &r.source);
    if let Some(end) = range_end {
        dto.reference = format!("{}-{end}", dto.reference);
        dto.verse_id = format!("{}|{end}", dto.verse_id);
    }
    dto
}

fn dedupe_and_cap(mut scored: Vec<ScoredResult>, context: SearchContext) -> Vec<ScoredResult> {
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut seen: std::collections::HashSet<(String, u16, u16, Option<u16>)> = Default::default();
    scored.retain(|r| {
        let key = (
            r.record.book.clone(),
            r.record.chapter,
            r.record.verse,
            r.range_end,
        );
        seen.insert(key)
    });
    scored.truncate(cap_for_context(context));
    scored
}

fn should_try_cloud_assist(local: &[ScoredResult], context: SearchContext) -> bool {
    if matches!(
        context,
        SearchContext::OnDemandFetch | SearchContext::BibleReader
    ) {
        return false;
    }
    let Some(top) = local.first() else {
        return true;
    };
    if matches!(top.tier, DetectionTier::ExactReference) && top.score >= 0.90 {
        return false;
    }
    top.score < 0.86
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CloudReferenceEnvelope {
    references: Vec<CloudReference>,
}

#[derive(Debug, Deserialize)]
struct CloudReference {
    reference: String,
    confidence: Option<f32>,
    reason: Option<String>,
}

fn extract_json_object(content: &str) -> Option<&str> {
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    (start <= end).then_some(&content[start..=end])
}

fn cloud_scripture_assist(
    query: &str,
    translation_id: &str,
    context: SearchContext,
    store: &AletheiaStore,
    api_key: &str,
) -> Vec<ScoredResult> {
    let trimmed = query.trim();
    if trimmed.len() < 6 || api_key.trim().is_empty() {
        return Vec::new();
    }

    let model =
        std::env::var("ALETHEIA_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let timeout_ms = match context {
        SearchContext::LiveTranscript | SearchContext::DashboardOpen => 1_200,
        SearchContext::ManualSearch => 2_500,
        SearchContext::OnDemandFetch | SearchContext::BibleReader => 0,
    };
    if timeout_ms == 0 {
        return Vec::new();
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            log::warn!("[cloud-ai] client init failed: {error}");
            return Vec::new();
        }
    };

    let body = json!({
        "model": model,
        "temperature": 0,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": "You are Aletheia's scripture resolver. Identify likely Protestant Bible references from spoken sermon text, paraphrases, story summaries, partial quotes, and Nigerian church speech. Return only JSON: {\"references\":[{\"reference\":\"Book chapter:verse\" or \"Book chapter:start-end\",\"confidence\":0.0-1.0,\"reason\":\"short reason\"}]}. Return at most 5 references. If uncertain, return {\"references\":[]}. Do not invent non-Bible references."
            },
            {
                "role": "user",
                "content": format!("Spoken text: {trimmed}\nReturn references only.")
            }
        ]
    });

    let response = match client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key.trim())
        .json(&body)
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            log::warn!("[cloud-ai] request failed: {error}");
            return Vec::new();
        }
    };

    if !response.status().is_success() {
        log::warn!("[cloud-ai] OpenAI returned status {}", response.status());
        return Vec::new();
    }

    let parsed: OpenAiChatResponse = match response.json() {
        Ok(parsed) => parsed,
        Err(error) => {
            log::warn!("[cloud-ai] response parse failed: {error}");
            return Vec::new();
        }
    };

    let content = parsed
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_deref())
        .unwrap_or("");
    let Some(json_text) = extract_json_object(content) else {
        return Vec::new();
    };
    let envelope: CloudReferenceEnvelope = match serde_json::from_str(json_text) {
        Ok(envelope) => envelope,
        Err(error) => {
            log::warn!("[cloud-ai] JSON payload parse failed: {error}");
            return Vec::new();
        }
    };

    let mut scored = Vec::new();
    for item in envelope.references.into_iter().take(5) {
        let Some((book, chapter, verse_start, verse_end)) =
            parse_reference_with_range(&item.reference)
        else {
            continue;
        };
        let range_end = verse_end.filter(|end| *end > verse_start);
        let Some(record) = fetch_verse_range(
            store,
            translation_id,
            &book,
            chapter,
            verse_start,
            range_end,
        ) else {
            continue;
        };
        let confidence = item.confidence.unwrap_or(0.78).clamp(0.55, 0.92);
        let reason = item
            .reason
            .unwrap_or_else(|| "cloud context resolver".to_string());
        scored.push(ScoredResult {
            score: confidence,
            source: format!("Cloud AI context match: {reason}"),
            record,
            range_end,
            tier: DetectionTier::CloudEnhancement,
        });
    }

    scored
}

pub fn cloud_rank_and_resolve(
    query: &str,
    translation_id: &str,
    store: &AletheiaStore,
    api_key: &str,
) -> Option<(Vec<RankedCandidate>, SearchResultDto)> {
    let mut scored = cloud_scripture_assist(
        query,
        translation_id,
        SearchContext::LiveTranscript,
        store,
        api_key,
    );
    scored = dedupe_and_cap(scored, SearchContext::LiveTranscript);
    let top = scored.first()?;
    let ranked: Vec<RankedCandidate> = scored
        .iter()
        .map(|r| {
            let reference = match r.range_end {
                Some(end) => format!(
                    "{} {}:{}-{}",
                    r.record.book, r.record.chapter, r.record.verse, end
                ),
                None => format!("{} {}:{}", r.record.book, r.record.chapter, r.record.verse),
            };
            RankedCandidate {
                reference,
                score: r.score,
                tiers: vec![r.tier],
            }
        })
        .collect();
    Some((ranked, scored_to_dto(top.clone())))
}

/// Parses references that may carry an inclusive verse range
/// (`"Matthew 25:14-30"` → `(book, chapter, 14, Some(30))`). Falls back to
/// the legacy `parse_reference` for plain `"Book c:v"` strings.
///
/// The catalog stores curated references as static strings, so this is the
/// path that lets a topic-match phrase resolve to a real passage.
fn parse_reference_with_range(s: &str) -> Option<(String, u16, u16, Option<u16>)> {
    if let Some((head, tail)) = s.rsplit_once('-') {
        // Only treat as range when the tail parses cleanly as digits and the
        // head still parses as a real `Book c:v` reference.
        if let Ok(end) = tail.trim().parse::<u16>() {
            if let Some((book, chapter, start)) = parse_reference(head.trim()) {
                if end > start {
                    return Some((book, chapter, start, Some(end)));
                }
            }
        }
    }
    parse_reference(s).map(|(book, chapter, verse)| (book, chapter, verse, None))
}

/// Fetches one verse, or every verse in `[start..=end]` when `range_end` is
/// set, and returns a synthetic `VerseRecord` whose `text` is the concatenated
/// passage and `verse` is the start verse. Returns `None` if the start verse
/// is missing.
fn fetch_verse_range(
    store: &AletheiaStore,
    translation_id: &str,
    book: &str,
    chapter: u16,
    verse_start: u16,
    range_end: Option<u16>,
) -> Option<VerseRecord> {
    let head = store
        .find_verse(translation_id, book, chapter, verse_start)
        .ok()??;
    let Some(end) = range_end else {
        return Some(head);
    };
    let mut text = head.text.clone();
    let mut last_seen = verse_start;
    for v in (verse_start + 1)..=end {
        match store.find_verse(translation_id, book, chapter, v) {
            Ok(Some(record)) => {
                text.push(' ');
                text.push_str(&record.text);
                last_seen = v;
            }
            // Stop on the first missing verse — chapter boundary or sparse
            // translation. The partial range is still useful to the operator.
            _ => break,
        }
    }
    if last_seen == verse_start {
        // Range parse said `start-end` but only the start verse exists; treat
        // it as a single verse so we don't claim a range we couldn't fetch.
        return Some(head);
    }
    Some(VerseRecord {
        translation_id: head.translation_id,
        book: head.book,
        chapter: head.chapter,
        verse: head.verse,
        text,
    })
}

fn score_overlap(query: &str, candidate: &str) -> f32 {
    let q_tokens: std::collections::HashSet<&str> =
        query.split_whitespace().filter(|t| t.len() >= 3).collect();
    if q_tokens.is_empty() {
        return 0.5;
    }
    let lower = candidate.to_lowercase();
    let c_tokens: std::collections::HashSet<&str> =
        lower.split_whitespace().filter(|t| t.len() >= 3).collect();
    let inter = q_tokens.intersection(&c_tokens).count();
    let union = q_tokens.union(&c_tokens).count().max(1);
    (inter as f32 / union as f32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Lightweight TF-IDF semantic lane.
// ---------------------------------------------------------------------------
//
// Rust-native, no Python, no ONNX, no model download. We hold one TfIdfIndex
// per (translation_id) in a process-global lazy cache. The index is built on
// first query and persists for the rest of the process lifetime.
//
// This is intentionally a low-cost first cut. It handles paraphrase-by-shared-
// vocabulary well enough to make the difference between "no candidate" and
// "the right candidate ranked top-3" for sermon recall queries. It does NOT
// understand semantic similarity in the dense-embedding sense — that lane is
// reserved for a future fastembed-rs integration that can drop in without
// touching callers.

struct TfIdfIndex {
    docs: Vec<VerseRecord>,
    vectors: Vec<Vec<(u32, f32)>>, // (term_id, weight) sorted by term_id
    df: std::collections::HashMap<String, u32>,
    vocab: std::collections::HashMap<String, u32>,
    n_docs: f32,
}

impl TfIdfIndex {
    fn build(verses: Vec<VerseRecord>) -> Self {
        let mut df: std::collections::HashMap<String, u32> = Default::default();
        let mut vocab: std::collections::HashMap<String, u32> = Default::default();

        // First pass: doc frequency.
        let tokenized: Vec<Vec<String>> = verses.iter().map(|v| tokenize(&v.text)).collect();
        for tokens in &tokenized {
            let unique: std::collections::HashSet<&String> = tokens.iter().collect();
            for t in unique {
                *df.entry(t.clone()).or_insert(0) += 1;
            }
        }
        for term in df.keys() {
            let next_id = vocab.len() as u32;
            vocab.entry(term.clone()).or_insert(next_id);
        }

        let n = verses.len() as f32;

        // Second pass: tf-idf vectors.
        let mut vectors: Vec<Vec<(u32, f32)>> = Vec::with_capacity(verses.len());
        for tokens in &tokenized {
            let mut tf: std::collections::HashMap<&String, u32> = Default::default();
            for t in tokens {
                *tf.entry(t).or_insert(0) += 1;
            }
            let mut vec: Vec<(u32, f32)> = tf
                .iter()
                .map(|(term, count)| {
                    let id = vocab[*term];
                    let tf_w = 1.0 + (*count as f32).ln();
                    let df_v = df.get(*term).copied().unwrap_or(1) as f32;
                    let idf = ((n + 1.0) / (df_v + 1.0)).ln() + 1.0;
                    (id, tf_w * idf)
                })
                .collect();
            // Cosine normalization.
            let norm: f32 = vec.iter().map(|(_, w)| w * w).sum::<f32>().sqrt();
            if norm > 0.0 {
                for (_, w) in vec.iter_mut() {
                    *w /= norm;
                }
            }
            vec.sort_by_key(|(id, _)| *id);
            vectors.push(vec);
        }

        Self {
            docs: verses,
            vectors,
            df,
            vocab,
            n_docs: n,
        }
    }

    fn search(&self, query: &str, top_k: usize) -> Vec<(VerseRecord, f32)> {
        let tokens = tokenize(query);
        if tokens.is_empty() || self.vectors.is_empty() {
            return Vec::new();
        }
        // Build query vector.
        let mut tf: std::collections::HashMap<&String, u32> = Default::default();
        for t in &tokens {
            *tf.entry(t).or_insert(0) += 1;
        }
        let mut q_vec: Vec<(u32, f32)> = tf
            .iter()
            .filter_map(|(term, count)| {
                let id = *self.vocab.get(*term)?;
                let df_v = self.df.get(*term).copied().unwrap_or(1) as f32;
                let idf = ((self.n_docs + 1.0) / (df_v + 1.0)).ln() + 1.0;
                let tf_w = 1.0 + (*count as f32).ln();
                Some((id, tf_w * idf))
            })
            .collect();
        let norm: f32 = q_vec.iter().map(|(_, w)| w * w).sum::<f32>().sqrt();
        if norm > 0.0 {
            for (_, w) in q_vec.iter_mut() {
                *w /= norm;
            }
        }
        q_vec.sort_by_key(|(id, _)| *id);

        // Cosine = dot product (already normalized).
        let mut scored: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(i, dv)| (i, dot_sorted(&q_vec, dv)))
            .filter(|(_, s)| *s > 0.06)
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(top_k);
        scored
            .into_iter()
            .map(|(i, s)| (self.docs[i].clone(), s))
            .collect()
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !is_stopword(t))
        .map(|t| t.to_string())
        .collect()
}

fn is_stopword(t: &str) -> bool {
    matches!(
        t,
        "the"
            | "and"
            | "for"
            | "but"
            | "with"
            | "from"
            | "that"
            | "this"
            | "his"
            | "her"
            | "him"
            | "she"
            | "they"
            | "them"
            | "their"
            | "you"
            | "your"
            | "are"
            | "was"
            | "were"
            | "have"
            | "has"
            | "had"
            | "not"
            | "shall"
            | "will"
            | "unto"
            | "all"
            | "any"
            | "who"
            | "which"
            | "what"
            | "thy"
            | "thee"
            | "thou"
    )
}

fn dot_sorted(a: &[(u32, f32)], b: &[(u32, f32)]) -> f32 {
    let (mut i, mut j) = (0usize, 0usize);
    let mut sum = 0.0f32;
    while i < a.len() && j < b.len() {
        match a[i].0.cmp(&b[j].0) {
            std::cmp::Ordering::Equal => {
                sum += a[i].1 * b[j].1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    sum
}

// Process-global cache: one index per translation_id.
static INDEX_CACHE: OnceLock<Mutex<std::collections::HashMap<String, std::sync::Arc<TfIdfIndex>>>> =
    OnceLock::new();

fn vector_search(
    _store: &AletheiaStore,
    translation_id: &str,
    query: &str,
    top_k: usize,
) -> Option<Vec<(VerseRecord, f32)>> {
    // Non-blocking: if the index is currently being built (held by a
    // background warmer), skip this tier rather than freezing the UI.
    let cache = INDEX_CACHE.get_or_init(|| Mutex::new(Default::default()));
    let index = {
        let guard = cache.try_lock().ok()?;
        guard.get(translation_id).cloned()?
    };
    Some(index.search(query, top_k))
}

/// Builds the TF-IDF index for `translation_id` if not already cached.
/// Opens its own read-only WAL connection against `db_path` so the 35k-row
/// scan never contends with the writer-backed `Mutex<AletheiaStore>`. Safe
/// to call from a background thread; no-op if the index is already built.
pub fn warm_translation_with_path(db_path: &Path, translation_id: &str) {
    let cache = INDEX_CACHE.get_or_init(|| Mutex::new(Default::default()));
    {
        let guard = match cache.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if guard.contains_key(translation_id) {
            return;
        }
    }
    log::info!("[scripture-search] warming TF-IDF index for '{translation_id}'…");
    let read_conn = match AletheiaStore::open_read_only(db_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[scripture-search] read-only open failed: {e}");
            return;
        }
    };
    let verses = match load_all_verses_on(&read_conn, translation_id) {
        Ok(v) if !v.is_empty() => v,
        Ok(_) => {
            log::info!("[scripture-search] no verses found for '{translation_id}', skipping warm");
            return;
        }
        Err(e) => {
            log::warn!("[scripture-search] cannot warm '{translation_id}': {e}");
            return;
        }
    };
    let n = verses.len();
    // Architectural-vision rule: "the operator should be able to recall a
    // parable by paraphrase even when no individual verse contains the words
    // they used". The passage pseudo-documents bake the catalog's curated
    // titles, themes, people, and places into the TF-IDF space.
    let entity_documents = synthesize_entity_documents(&verses);
    let passages = synthesize_passage_documents_on(&read_conn, translation_id);
    let passages_added = passages.len();
    let entity_documents_added = entity_documents.len();
    let mut all = verses;
    all.extend(passages);
    all.extend(entity_documents);
    let built = std::sync::Arc::new(TfIdfIndex::build(all));
    log::info!(
        "[scripture-search] index seeded with {passages_added} passage pseudo-documents and {entity_documents_added} entity episode documents"
    );
    if let Ok(mut guard) = cache.lock() {
        guard.insert(translation_id.to_string(), built.clone());
    }
    log::info!(
        "[scripture-search] TF-IDF index ready: '{translation_id}' — {n} verses, {} terms",
        built.vocab.len()
    );
}

pub fn invalidate_translation_cache(translation_id: &str) {
    let cache = INDEX_CACHE.get_or_init(|| Mutex::new(Default::default()));
    if let Ok(mut guard) = cache.lock() {
        guard.remove(translation_id);
    }
}

/// Returns true when an index for `translation_id` is built and ready to serve
/// queries without blocking. Useful for the diagnostics/health UI.
pub fn is_translation_warm(translation_id: &str) -> bool {
    let cache = INDEX_CACHE.get_or_init(|| Mutex::new(Default::default()));
    cache
        .lock()
        .map(|g| g.contains_key(translation_id))
        .unwrap_or(false)
}

/// Builds one synthetic `VerseRecord` per catalog passage: the inclusive
/// verse range concatenated together, plus the catalog's curated aliases,
/// title, themes, people, and places appended as sidecar tokens. The result
/// joins the TF-IDF index alongside individual verses so a paraphrase like
/// "the story of the man who fell among thieves" can rank the whole Good
/// Samaritan pericope at the top, not just Luke 10:33.
///
/// The `VerseRecord` fields are populated so a hit through this lane is
/// indistinguishable from a normal verse lookup — `record.verse` is the start
/// of the range and the full text is in `record.text`. The downstream
/// dedup-by-(book,chapter,verse) keeps the best score among any duplicates.
fn synthesize_passage_documents_on(
    connection: &rusqlite::Connection,
    translation_id: &str,
) -> Vec<VerseRecord> {
    use aletheia_detection::phrase_catalog::passage_descriptors;

    let descriptors = passage_descriptors();
    let mut out: Vec<VerseRecord> = Vec::with_capacity(descriptors.len());
    for desc in descriptors {
        let Some((book, chapter, start, end)) = parse_reference_with_range(desc.reference) else {
            continue;
        };
        let range_end = end.filter(|e| *e > start);
        let head =
            match AletheiaStore::find_verse_on(connection, translation_id, &book, chapter, start) {
                Ok(Some(record)) => record,
                _ => continue,
            };
        let mut text = head.text.clone();
        if let Some(end_v) = range_end {
            for v in (start + 1)..=end_v {
                if let Ok(Some(record)) =
                    AletheiaStore::find_verse_on(connection, translation_id, &book, chapter, v)
                {
                    text.push(' ');
                    text.push_str(&record.text);
                }
            }
        }
        // Sidecar tokens: title, aliases, themes, people, places. These bias
        // the TF-IDF vector toward the lexical fingerprint operators actually
        // say (e.g. "the prodigal son", "tongues of fire", "fiery furnace").
        text.push(' ');
        text.push_str(desc.title);
        for alias in &desc.aliases {
            text.push(' ');
            text.push_str(alias);
        }
        for token in desc.themes.iter().chain(desc.people).chain(desc.places) {
            text.push(' ');
            text.push_str(token);
        }
        out.push(VerseRecord {
            translation_id: head.translation_id,
            book: head.book,
            chapter: head.chapter,
            verse: head.verse,
            text,
        });
    }
    out
}

#[derive(Default)]
struct EntityEpisodeDraft {
    translation_id: String,
    book: String,
    chapter: u16,
    first_verse: u16,
    entity: String,
    co_entities: std::collections::BTreeSet<String>,
    text: String,
}

impl EntityEpisodeDraft {
    fn ranking_score(&self) -> usize {
        let text_weight = self.text.split_whitespace().count().min(500);
        let co_entity_weight = self.co_entities.len() * 1_000;
        let multi_word_weight = usize::from(self.entity.contains(' ')) * 250;
        co_entity_weight + text_weight + multi_word_weight
    }
}

fn synthesize_entity_documents(verses: &[VerseRecord]) -> Vec<VerseRecord> {
    let mut drafts: std::collections::BTreeMap<(String, String, u16, String), EntityEpisodeDraft> =
        Default::default();

    for verse in verses {
        let entities = extract_bible_entities(&verse.text);
        if entities.is_empty() {
            continue;
        }

        for entity in &entities {
            let key = (
                verse.translation_id.clone(),
                verse.book.clone(),
                verse.chapter,
                entity.clone(),
            );
            let entry = drafts.entry(key).or_insert_with(|| EntityEpisodeDraft {
                translation_id: verse.translation_id.clone(),
                book: verse.book.clone(),
                chapter: verse.chapter,
                first_verse: verse.verse,
                entity: entity.clone(),
                co_entities: Default::default(),
                text: String::new(),
            });
            entry.first_verse = entry.first_verse.min(verse.verse);
            for other in &entities {
                if other != entity {
                    entry.co_entities.insert(other.clone());
                }
            }
            if entry.text.len() < 8_000 {
                entry.text.push(' ');
                entry.text.push_str(&verse.text);
            }
        }
    }

    let mut ranked_drafts: Vec<EntityEpisodeDraft> = drafts
        .into_values()
        .filter(|draft| draft.text.split_whitespace().count() >= 8)
        .collect();
    ranked_drafts.sort_by(|a, b| {
        b.ranking_score()
            .cmp(&a.ranking_score())
            .then_with(|| a.translation_id.cmp(&b.translation_id))
            .then_with(|| a.book.cmp(&b.book))
            .then_with(|| a.chapter.cmp(&b.chapter))
            .then_with(|| a.first_verse.cmp(&b.first_verse))
            .then_with(|| a.entity.cmp(&b.entity))
    });
    ranked_drafts.truncate(MAX_ENTITY_EPISODE_DOCUMENTS);

    ranked_drafts
        .into_iter()
        .map(|draft| {
            let mut text = String::new();
            text.push_str(&draft.entity);
            text.push_str(". story of ");
            text.push_str(&draft.entity);
            text.push_str(". what happened to ");
            text.push_str(&draft.entity);
            text.push_str(". scripture about ");
            text.push_str(&draft.entity);
            text.push_str(". bible character ");
            text.push_str(&draft.entity);
            text.push_str(". ");
            if !draft.co_entities.is_empty() {
                text.push_str("connected people ");
                for co_entity in &draft.co_entities {
                    text.push_str(co_entity);
                    text.push(' ');
                }
            }
            text.push_str("chapter context ");
            text.push_str(&draft.text);

            VerseRecord {
                translation_id: draft.translation_id,
                book: draft.book,
                chapter: draft.chapter,
                verse: draft.first_verse,
                text,
            }
        })
        .collect()
}

fn extract_bible_entities(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut phrase: Vec<String> = Vec::new();

    for raw in text.split(|ch: char| !(ch.is_alphanumeric() || ch == '\'' || ch == '-')) {
        let token = raw.trim_matches(|ch: char| ch == '\'' || ch == '-');
        if token.is_empty() {
            flush_entity_phrase(&mut phrase, &mut out);
            continue;
        }

        let is_entity_token = token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            || (token.len() > 2 && token.chars().all(|ch| ch.is_ascii_uppercase()));
        if is_entity_token && !is_entity_stopword(token) {
            phrase.push(token.to_string());
        } else {
            flush_entity_phrase(&mut phrase, &mut out);
        }
    }
    flush_entity_phrase(&mut phrase, &mut out);
    out
}

fn flush_entity_phrase(phrase: &mut Vec<String>, out: &mut Vec<String>) {
    if phrase.is_empty() {
        return;
    }
    for token in phrase.iter() {
        push_entity_unique(out, token);
    }
    if phrase.len() > 1 {
        let joined = phrase.join(" ");
        push_entity_unique(out, &joined);
    }
    phrase.clear();
}

fn push_entity_unique(out: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.len() < 3 || is_entity_stopword(trimmed) {
        return;
    }
    if !out.iter().any(|existing| existing == trimmed) {
        out.push(trimmed.to_string());
    }
}

fn is_entity_stopword(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "and"
            | "but"
            | "for"
            | "the"
            | "then"
            | "therefore"
            | "now"
            | "when"
            | "where"
            | "who"
            | "whom"
            | "whose"
            | "this"
            | "that"
            | "these"
            | "those"
            | "also"
            | "behold"
            | "lord"
            | "god"
            | "king"
            | "chapter"
            | "verse"
            | "selah"
    )
}

fn load_all_verses_on(
    connection: &rusqlite::Connection,
    translation_id: &str,
) -> Result<Vec<VerseRecord>, Box<dyn std::error::Error + Send + Sync>> {
    let mut statement = connection.prepare(
        "SELECT translation_id, book, chapter, verse, text
         FROM scripture_verses
         WHERE translation_id = ?1
         ORDER BY book, chapter, verse",
    )?;
    let rows = statement.query_map([translation_id], |row| {
        Ok(VerseRecord {
            translation_id: row.get(0)?,
            book: row.get(1)?,
            chapter: row.get(2)?,
            verse: row.get(3)?,
            text: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Self-test — runs at startup against the canonical fixture set, emits a
// scripture_health.json the UI reads for the diagnostics screen.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptureFixtureResult {
    pub query: String,
    pub kind: String, // "reference" | "paraphrase" | "summary" | "ambiguity"
    pub passed: bool,
    pub matched_reference: Option<String>,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptureHealthReport {
    pub generated_at_ms: u64,
    pub translation_id: String,
    pub passed: u32,
    pub failed: u32,
    pub fixtures: Vec<ScriptureFixtureResult>,
}

const FIXTURES: &[(&str, &str, &str)] = &[
    // (query, kind, expected reference substring or "_disambig")
    ("Genesis 1:1", "reference", "Genesis 1:1"),
    ("Leviticus 3:10", "reference", "Leviticus 3:10"),
    ("Numbers 6:24", "reference", "Numbers 6:24"),
    ("Deuteronomy 6:4", "reference", "Deuteronomy 6:4"),
    ("1 Chronicles 29:11", "reference", "1 Chronicles 29:11"),
    ("2 Chronicles 7:14", "reference", "2 Chronicles 7:14"),
    ("Nahum 1:7", "reference", "Nahum 1:7"),
    ("Judges 6:12", "reference", "Judges 6:12"),
    ("John 11:35", "reference", "John 11:35"),
    (
        "the lord is my shepherd i shall not want",
        "paraphrase",
        "Psalm 23:1",
    ),
    ("mighty man of valor", "paraphrase", "Judges 6:12"),
    (
        "Joseph was sold to slavery by his brothers",
        "summary",
        "Genesis 37:28",
    ),
    ("Chronicles 7:14", "ambiguity", "_disambig"),
];

/// Runs the fixture set and writes `scripture_health.json` to `app_dir`.
/// Best-effort — never panics.
pub fn run_scripture_self_test(store: &AletheiaStore, app_dir: &Path) {
    let translation_id = "kjv";
    let mut results = Vec::with_capacity(FIXTURES.len());
    let mut passed = 0u32;
    let mut failed = 0u32;

    for (query, kind, expected) in FIXTURES {
        let hits =
            search_scripture_unified(query, translation_id, SearchContext::ManualSearch, store);
        let (ok, matched, note) = evaluate_fixture(kind, expected, &hits);
        if ok {
            passed += 1;
            log::info!("[self-test] PASS {kind} \"{query}\" -> {matched:?}");
        } else {
            failed += 1;
            log::warn!("[self-test] FAIL {kind} \"{query}\" -> {note}");
        }
        results.push(ScriptureFixtureResult {
            query: query.to_string(),
            kind: kind.to_string(),
            passed: ok,
            matched_reference: matched,
            note,
        });
    }

    let report = ScriptureHealthReport {
        generated_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        translation_id: translation_id.to_string(),
        passed,
        failed,
        fixtures: results,
    };

    let path = app_dir.join("scripture_health.json");
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[self-test] could not write {path:?}: {e}");
            } else {
                log::info!("[self-test] wrote {path:?} — {passed} passed, {failed} failed");
            }
        }
        Err(e) => log::warn!("[self-test] could not serialize report: {e}"),
    }
}

fn evaluate_fixture(
    kind: &str,
    expected: &str,
    hits: &[SearchResultDto],
) -> (bool, Option<String>, String) {
    if hits.is_empty() {
        return (false, None, "no candidate returned".to_string());
    }
    match kind {
        "ambiguity" => {
            // Expect at least one hit whose source contains "ambiguous".
            let amb = hits
                .iter()
                .find(|h| h.source.to_lowercase().contains("ambiguous"));
            match amb {
                Some(h) => (
                    true,
                    Some(h.reference.clone()),
                    format!("ambiguous expansion present ({})", h.source),
                ),
                None => (
                    false,
                    Some(hits[0].reference.clone()),
                    "no ambiguous source label on any hit".to_string(),
                ),
            }
        }
        _ => {
            let top = &hits[0];
            // Tolerant check: substring or normalized-equality.
            let matched = top.reference.eq_ignore_ascii_case(expected)
                || top
                    .reference
                    .to_lowercase()
                    .contains(&expected.to_lowercase());
            (
                matched,
                Some(top.reference.clone()),
                if matched {
                    "ok".to_string()
                } else {
                    format!("top hit was \"{}\" not \"{expected}\"", top.reference)
                },
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Editable runtime topic catalog.
// ---------------------------------------------------------------------------
//
// The baked-in `phrase_catalog::CATALOG` covers ~95 common references but is
// frozen at compile time. The architectural-vision "editable map" requirement
// is satisfied by also reading `topic_catalog.json` from the app data dir at
// startup. The on-disk format is intentionally tiny:
//
//     [
//       {
//         "reference": "Matthew 25:14-30",
//         "score": 0.9,
//         "aliases": ["parable of the talents", "five talents two talents"]
//       },
//       ...
//     ]
//
// Aliases match by case-insensitive substring against the normalized
// transcript text. Operators can edit the file with any text editor; bad
// JSON is logged and ignored — never crashes.

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeCatalogEntry {
    pub reference: String,
    pub aliases: Vec<String>,
    #[serde(default = "default_runtime_score")]
    pub score: f32,
}

fn default_runtime_score() -> f32 {
    0.85
}

static RUNTIME_CATALOG: OnceLock<Vec<RuntimeCatalogEntry>> = OnceLock::new();

/// Initialises the editable topic catalog from `app_dir/topic_catalog.json`.
/// Idempotent — only the first call has effect. Safe to call before the file
/// exists; in that case the runtime catalog is simply empty.
pub fn init_runtime_catalog(app_dir: &Path) {
    let path = app_dir.join("topic_catalog.json");
    let entries = match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Vec<RuntimeCatalogEntry>>(&raw) {
            Ok(parsed) => {
                log::info!(
                    "[scripture-search] loaded {} editable topic entries from {path:?}",
                    parsed.len()
                );
                parsed
            }
            Err(e) => {
                log::warn!(
                    "[scripture-search] {path:?} is not valid JSON ({e}); ignoring runtime catalog"
                );
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    };
    let _ = RUNTIME_CATALOG.set(entries);
}

fn runtime_catalog() -> &'static [RuntimeCatalogEntry] {
    RUNTIME_CATALOG.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Reads the most recent `scripture_health.json` written by the self-test.
/// Returns `None` if the file is missing or corrupt.
pub fn read_health_report(app_dir: &Path) -> Option<ScriptureHealthReport> {
    let path = app_dir.join("scripture_health.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

use crate::DesktopState;
use tauri::State;

fn cloud_ai_key_if_allowed(state: &DesktopState) -> Option<String> {
    let runtime = state.lock_runtime().ok()?;
    if runtime.data_miser_enabled || runtime.offline_mode_enabled {
        return None;
    }
    drop(runtime);

    crate::vault::read_secret("openai-api-key")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .filter(|key| !key.trim().is_empty())
}

fn search_with_optional_cloud(
    query: &str,
    translation_id: &str,
    context: SearchContext,
    store: &AletheiaStore,
    cloud_key: Option<&str>,
) -> Vec<SearchResultDto> {
    let q = query.to_string();
    let tid = translation_id.to_string();
    let mut scored = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_scripture_unified_inner(&q, &tid, context, store)
    })) {
        Ok(v) => v,
        Err(_) => {
            log::error!(
                "[scripture-search] PANIC swallowed for query={query:?} translation={translation_id}"
            );
            Vec::new()
        }
    };

    if should_try_cloud_assist(&scored, context) {
        if let Some(api_key) = cloud_key {
            scored.extend(cloud_scripture_assist(
                query,
                translation_id,
                context,
                store,
                api_key,
            ));
        }
    }

    dedupe_and_cap(scored, context)
        .into_iter()
        .map(scored_to_dto)
        .collect()
}

#[tauri::command]
pub fn search_scripture_unified_cmd(
    state: State<'_, DesktopState>,
    query: String,
    translation_id: Option<String>,
    context: Option<SearchContext>,
) -> Result<Vec<SearchResultDto>, String> {
    let tid = translation_id.unwrap_or_else(|| "kjv".to_string());
    let ctx = context.unwrap_or(SearchContext::ManualSearch);
    let cloud_key = cloud_ai_key_if_allowed(&state);

    // Operator-driven search uses an isolated read-only WAL connection so it
    // never serializes with audit writes or live-capture mutations. Falls
    // back to the writer lock if the read-only open fails for any reason
    // (e.g. the file vanished mid-session) so a transient error never breaks
    // the search box.
    match AletheiaStore::open_read_only(&state.database_path) {
        Ok(connection) => {
            let read_store = AletheiaStore::from_read_only_connection(connection);
            Ok(search_with_optional_cloud(
                &query,
                &tid,
                ctx,
                &read_store,
                cloud_key.as_deref(),
            ))
        }
        Err(_) => {
            let store = state.lock_store()?;
            Ok(search_with_optional_cloud(
                &query,
                &tid,
                ctx,
                &store,
                cloud_key.as_deref(),
            ))
        }
    }
}

#[tauri::command]
pub fn get_scripture_health(
    state: State<'_, DesktopState>,
) -> Result<Option<ScriptureHealthReport>, String> {
    let app_dir = state
        .database_path
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "app data dir unavailable".to_string())?;
    Ok(read_health_report(&app_dir))
}

#[tauri::command]
pub fn run_scripture_diagnostics(
    state: State<'_, DesktopState>,
) -> Result<ScriptureHealthReport, String> {
    let store = state.lock_store()?;
    let app_dir = state
        .database_path
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "app data dir unavailable".to_string())?;
    run_scripture_self_test(&store, &app_dir);
    read_health_report(&app_dir).ok_or_else(|| "self-test report missing".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aletheia_detection::{AutoOpenDecision, ConfidencePolicy, OperatingMode};
    use aletheia_store::{TranslationRecord, VerseRecord};

    fn seed_kjv() -> AletheiaStore {
        let store = AletheiaStore::open_memory().expect("store opens");
        store
            .insert_translation(&TranslationRecord {
                id: "kjv".to_string(),
                name: "King James Version".to_string(),
                language: "English".to_string(),
                license: "public-domain".to_string(),
                offline_ready: true,
            })
            .expect("translation");
        store
            .insert_verse(&VerseRecord {
                translation_id: "kjv".to_string(),
                book: "John".to_string(),
                chapter: 3,
                verse: 16,
                text: "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.".to_string(),
            })
            .expect("verse");
        store
    }

    /// Happy-path: a transcript with a clear explicit reference flows through
    /// detection → ConfidencePolicy → OperatingMode gating → an auto-sendable
    /// decision in `Auto` mode, but is downgraded under `Assisted`. This is the
    /// canonical end-to-end pipeline the architectural vision describes.
    #[test]
    fn live_pipeline_explicit_reference_auto_opens_in_auto_mode() {
        let store = seed_kjv();

        let ranked = rank_candidates_for_live("turn to John 3:16", "kjv", &store);
        assert!(
            !ranked.is_empty(),
            "explicit reference should rank a candidate"
        );
        assert_eq!(ranked[0].reference, "John 3:16");
        assert!(
            ranked[0].score >= 0.92,
            "exact reference should be Certain bucket"
        );

        let policy = ConfidencePolicy::default();
        let safety = policy.evaluate_auto_open(&ranked);
        assert_eq!(
            safety,
            AutoOpenDecision::Open,
            "explicit reference auto-opens"
        );

        // Auto trusts the safety policy verbatim.
        assert_eq!(OperatingMode::Auto.gate(safety), AutoOpenDecision::Open);
        // Assisted downgrades Open to Prepare so the operator confirms.
        assert_eq!(
            OperatingMode::Assisted.gate(safety),
            AutoOpenDecision::Prepare
        );
        // Manual demands operator approval for everything non-Ignore.
        assert_eq!(
            OperatingMode::Manual.gate(safety),
            AutoOpenDecision::RequireApproval
        );
    }

    /// Vision rule: a quote match with no corroboration must surface for
    /// approval, never auto-open. This guards against false-positive scripture
    /// going live unsupervised.
    #[test]
    fn live_pipeline_bare_quote_requires_approval() {
        let store = seed_kjv();
        // Quote-only, no explicit reference: tier should be VerseQuotation alone.
        let ranked = rank_candidates_for_live(
            "for God so loved the world that he gave his only begotten son",
            "kjv",
            &store,
        );
        assert!(!ranked.is_empty(), "phrase search should rank a candidate");

        let policy = ConfidencePolicy::default();
        let safety = policy.evaluate_auto_open(&ranked);
        // The vision forbids auto-open on a bare quote — must require approval
        // (or at most Prepare). Never Open.
        assert_ne!(safety, AutoOpenDecision::Open);
    }

    /// Empty / noise transcripts must be ignored, never produce a candidate.
    #[test]
    fn live_pipeline_empty_query_is_ignored() {
        let store = seed_kjv();
        let ranked = rank_candidates_for_live("   ", "kjv", &store);
        assert!(ranked.is_empty());
        let safety = ConfidencePolicy::default().evaluate_auto_open(&ranked);
        assert_eq!(safety, AutoOpenDecision::Ignore);
    }

    /// Gap 11: exercises the side effect `run_live_detection` performs after
    /// the gated decision lands — `rank_and_resolve_for_live` returns the top
    /// resolved candidate; the orchestrator then writes a row into
    /// `scripture_candidates`. The pure-Rust slice of that flow is asserted
    /// here without needing a Tauri AppHandle: pipeline → policy → gate →
    /// persist → re-read.
    #[test]
    fn run_live_detection_persists_candidate_for_high_confidence_segment() {
        let store = seed_kjv();
        let segment_text = "and the pastor said turn to John 3:16";
        let translation_id = "kjv";

        let resolved = rank_and_resolve_for_live(segment_text, translation_id, &store)
            .expect("pipeline produced a candidate");
        let (ranked, top_dto) = resolved;
        assert!(!ranked.is_empty());
        assert_eq!(top_dto.reference, "John 3:16");

        let policy = ConfidencePolicy::default();
        let safety = policy.evaluate_auto_open(&ranked);
        let decision = OperatingMode::Auto.gate(safety);
        assert!(
            !matches!(decision, AutoOpenDecision::Ignore),
            "auto mode should not ignore an explicit reference"
        );

        let bucket = match aletheia_detection::ConfidenceBucket::from_score(ranked[0].score) {
            aletheia_detection::ConfidenceBucket::Certain => "certain",
            aletheia_detection::ConfidenceBucket::Strong => "strong",
            aletheia_detection::ConfidenceBucket::Likely => "likely",
            aletheia_detection::ConfidenceBucket::Unsafe => "unsafe",
        };
        let status = match decision {
            AutoOpenDecision::Open => "open",
            AutoOpenDecision::Prepare => "preview",
            AutoOpenDecision::RequireApproval => "approval",
            AutoOpenDecision::Ignore => "ignored",
        };

        let session_id = "service-test".to_string();
        // FK: scripture_candidates.session_id → service_sessions.id.
        store
            .upsert_service_session(&aletheia_store::ServiceSessionRecord {
                id: session_id.clone(),
                name: "Service — test".to_string(),
                started_at_ms: 1_700_000_000_000,
                ended_at_ms: None,
                data_miser_enabled: true,
                offline_mode_enabled: true,
            })
            .expect("session");
        let candidate_id = "cand-test-1".to_string();
        store
            .insert_scripture_candidate(&aletheia_store::ScriptureCandidateRecord {
                id: candidate_id.clone(),
                session_id: session_id.clone(),
                reference: top_dto.reference.clone(),
                translation_id: translation_id.to_string(),
                language: "en".to_string(),
                score: ranked[0].score as f64,
                bucket: bucket.to_string(),
                status: status.to_string(),
                reason: format!("{:?}", ranked[0].tiers),
                created_at_ms: 1_700_000_000_000,
            })
            .expect("insert");

        let rows = store
            .recent_scripture_candidates(&session_id, 10)
            .expect("read back");
        assert_eq!(rows.len(), 1, "exactly one candidate persisted");
        assert_eq!(rows[0].id, candidate_id);
        assert_eq!(rows[0].reference, "John 3:16");
        assert_eq!(rows[0].translation_id, "kjv");
        assert_eq!(rows[0].status, status);
        assert_eq!(rows[0].bucket, bucket);
    }
}
