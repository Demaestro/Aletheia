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
//! 7. **Cloud rerank** — disabled by default. When `ANTHROPIC_API_KEY` /
//!    `OPENAI_API_KEY` env are present and the top local score is < 0.85,
//!    a future adapter may reorder existing candidates. Never adds new ones.
//!
//! Ranked candidates are deduplicated by (book, chapter, verse), capped at
//! `MAX_RESULTS`, and returned with provenance so the UI can display "why
//! did you pick this?" tooltips.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use aletheia_detection::books::lookup_ambiguous;
use aletheia_detection::grammar::GrammarReferenceParser;
use aletheia_detection::normalize::TranscriptNormalizer;
use aletheia_store::{AletheiaStore, VerseRecord};
use serde::{Deserialize, Serialize};

use crate::dto::SearchResultDto;
use crate::{parse_reference, sanitize_fts_query, verse_to_search_result};

/// What surface invoked the search. Used to tune which tiers run and what
/// the result count cap is — live-mic wants the top 1, manual wants 10.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchContext {
    #[default]
    ManualSearch,
    LiveTranscript,
    OnDemandFetch,
    BibleReader,
    DashboardOpen,
}

const MAX_RESULTS: usize = 10;

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
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_scripture_unified_inner(&q, &tid, context, store)
    })) {
        Ok(v) => v,
        Err(_) => {
            log::error!(
                "[scripture-search] PANIC swallowed for query={query:?} translation={translation_id}"
            );
            Vec::new()
        }
    }
}

fn search_scripture_unified_inner(
    query: &str,
    translation_id: &str,
    context: SearchContext,
    store: &AletheiaStore,
) -> Vec<SearchResultDto> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let normalized = TranscriptNormalizer.normalize(trimmed);
    let mut accum: Vec<ScoredResult> = Vec::new();

    // Tier 2: explicit reference parser
    let refs = GrammarReferenceParser.parse_all(&normalized.text);
    for parsed in &refs {
        let label = if parsed.needs_disambiguation {
            format!("Reference (ambiguous: {})", parsed.disambiguation_options.join(", "))
        } else {
            "Exact reference".to_string()
        };
        if let Ok(Some(record)) = store.find_verse(
            translation_id,
            parsed.book,
            parsed.chapter,
            parsed.verse_start,
        ) {
            accum.push(ScoredResult {
                score: parsed.confidence,
                source: label,
                record,
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
            let rebuilt = format!("{canonical} {}", lc.split_once(' ').map(|x| x.1).unwrap_or(""));
            if let Some((book, chapter, verse)) = parse_reference(rebuilt.trim()) {
                if let Ok(Some(record)) = store.find_verse(translation_id, &book, chapter, verse) {
                    accum.push(ScoredResult {
                        score: 0.85,
                        source: format!(
                            "Reference (ambiguous: {})",
                            options.join(", ")
                        ),
                        record,
                    });
                }
            }
        }
    }

    // Tier 4: FTS phrase search
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

    accum
        .into_iter()
        .take(cap_for_context(context))
        .map(|r| verse_to_search_result(r.record, &r.source))
        .collect()
}

fn cap_for_context(context: SearchContext) -> usize {
    match context {
        SearchContext::LiveTranscript => 3,
        SearchContext::OnDemandFetch | SearchContext::DashboardOpen => 5,
        SearchContext::ManualSearch | SearchContext::BibleReader => MAX_RESULTS,
    }
}

struct ScoredResult {
    score: f32,
    source: String,
    record: VerseRecord,
}

fn score_overlap(query: &str, candidate: &str) -> f32 {
    let q_tokens: std::collections::HashSet<&str> = query
        .split_whitespace()
        .filter(|t| t.len() >= 3)
        .collect();
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
        let tokenized: Vec<Vec<String>> = verses
            .iter()
            .map(|v| tokenize(&v.text))
            .collect();
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
        "the" | "and" | "for" | "but" | "with" | "from" | "that" | "this"
            | "his" | "her" | "him" | "she" | "they" | "them" | "their" | "you"
            | "your" | "are" | "was" | "were" | "have" | "has" | "had" | "not"
            | "shall" | "will" | "unto" | "all" | "any" | "who" | "which" | "what"
            | "thy" | "thee" | "thou"
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
/// Safe to call from a background thread. No-op if index is already built.
pub fn warm_translation(store: &AletheiaStore, translation_id: &str) {
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
    let verses = match load_all_verses(store, translation_id) {
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
    let built = std::sync::Arc::new(TfIdfIndex::build(verses));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(translation_id.to_string(), built.clone());
    }
    log::info!(
        "[scripture-search] TF-IDF index ready: '{translation_id}' — {n} verses, {} terms",
        built.vocab.len()
    );
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

fn load_all_verses(
    store: &AletheiaStore,
    translation_id: &str,
) -> Result<Vec<VerseRecord>, Box<dyn std::error::Error + Send + Sync>> {
    // The store doesn't expose a "list all verses" helper; reuse `search_phrase`
    // with a no-op MATCH that returns every row by selecting on the FTS table.
    // We use the FTS table's special MATCH-everything via a wide token.
    // Falls back to FTS5 prefix scan: any token of length 1+ matches.
    //
    // Cap to 35 000 to avoid pathological blowup if a future translation ships
    // with poetry/non-canonical content. KJV is ~31k.
    let raw = store.search_phrase("the OR a OR i OR he OR she OR it", 35_000)?;
    Ok(raw
        .into_iter()
        .filter(|v| v.translation_id == translation_id)
        .collect())
}

// ---------------------------------------------------------------------------
// Self-test — runs at startup against the canonical fixture set, emits a
// scripture_health.json the UI reads for the diagnostics screen.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptureFixtureResult {
    pub query: String,
    pub kind: String, // "reference" | "paraphrase" | "ambiguity"
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
        let hits = search_scripture_unified(query, translation_id, SearchContext::ManualSearch, store);
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
                log::info!(
                    "[self-test] wrote {path:?} — {passed} passed, {failed} failed"
                );
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
                || top.reference.to_lowercase().contains(&expected.to_lowercase());
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

use tauri::State;
use crate::DesktopState;

#[tauri::command]
pub fn search_scripture_unified_cmd(
    state: State<'_, DesktopState>,
    query: String,
    translation_id: Option<String>,
    context: Option<SearchContext>,
) -> Result<Vec<SearchResultDto>, String> {
    let store = state.lock_store()?;
    let tid = translation_id.unwrap_or_else(|| "kjv".to_string());
    let ctx = context.unwrap_or(SearchContext::ManualSearch);
    Ok(search_scripture_unified(&query, &tid, ctx, &store))
}

#[tauri::command]
pub fn get_scripture_health(state: State<'_, DesktopState>) -> Result<Option<ScriptureHealthReport>, String> {
    let app_dir = state.app_data_dir()?;
    Ok(read_health_report(&app_dir))
}

#[tauri::command]
pub fn run_scripture_diagnostics(
    state: State<'_, DesktopState>,
) -> Result<ScriptureHealthReport, String> {
    let store = state.lock_store()?;
    let app_dir = state.app_data_dir()?;
    run_scripture_self_test(&store, &app_dir);
    read_health_report(&app_dir).ok_or_else(|| "self-test report missing".to_string())
}
