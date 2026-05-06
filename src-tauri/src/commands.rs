use aletheia_companion::{CompanionAdapter, CompanionConfig};
use aletheia_core::{AuditAction, now_ms};
use aletheia_detection::books::BOOKS;
use aletheia_detection::calibration::IsotonicCalibration;
use aletheia_detection::grammar::GrammarReferenceParser;
use aletheia_detection::normalize::TranscriptNormalizer;
use aletheia_detection::{
    AutoOpenDecision, ConfidenceBucket, ConfidencePolicy, KeywordLanguageDetector,
    LanguageDetector, OperatingMode,
};
use aletheia_easyworship::{EasyWorshipAdapter, EasyWorshipConfig};
use aletheia_obs::{ObsAdapter, ObsConfig};
use aletheia_ops::{
    ProductionReadinessReport, production_readiness_report_with_assets, redact_support_text,
    verify_signed_plugin_manifest,
};
use aletheia_osc::{OscAdapter, OscConfig};
use aletheia_output::OutputAdapter;
use aletheia_propresenter::{ProPresenterAdapter, ProPresenterConfig};
use aletheia_store::{
    CalibrationSampleRecord, DeviceAcceptanceReceiptRecord, IntegrationConfigRecord,
    OfflineAssetStateRecord, ServiceProfileRecord,
};
use aletheia_stt::capture::{AudioCapture, CaptureConfig, list_input_devices};
use aletheia_stt::offline::OfflineSttAdapter;
use tauri::{AppHandle, Emitter, Manager, State};

use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex as StdMutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use crate::DesktopState;
use crate::audit::*;
use crate::dto::*;
use crate::{
    LIVE_TRANSCRIPT_CAPACITY, accuracy_target_dto, booth_pack_readme,
    detect_candidates_for_transcript, find_stt_model_path, format_clock_time,
    import_full_bible_from_json, language_detections_from_transcript, live_integrations,
    merged_offline_asset_manifest, obs_browser_source_html, offline_asset_root,
    output_health_to_state_detail, parse_reference, production_candidates, production_transcript,
    recent_integration_events, scene_from_candidate, scene_to_dto, service_profile_to_dto,
    sha256_file_hex, stt_readiness_from_manifest, supported_language_to_dto,
    trusted_plugin_record_to_dto, verified_manifest_to_dto, vmix_booth_setup, vmix_config_from_dto,
    vmix_config_to_dto, vmix_status_dto, write_booth_pack_file,
};

#[derive(Clone, Debug)]
struct LiveContextReference {
    book: String,
    chapter: u16,
    verse: u16,
    translation_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FollowUpVerseCommand {
    Next,
    Previous,
    GoTo(u16),
}

fn current_live_context(state: &DesktopState) -> Option<LiveContextReference> {
    let runtime = state.lock_runtime().ok()?;
    for candidate in [&runtime.preview, &runtime.live] {
        if candidate.reference.trim().is_empty() {
            continue;
        }
        if let Some((book, chapter, verse)) = parse_reference(&candidate.reference) {
            let translation_id = if candidate.translation.trim().is_empty() {
                "kjv".to_string()
            } else {
                candidate.translation.to_ascii_lowercase()
            };
            return Some(LiveContextReference {
                book,
                chapter,
                verse,
                translation_id,
            });
        }
    }
    None
}

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

fn resolved_score(result: &(Vec<aletheia_detection::RankedCandidate>, SearchResultDto)) -> f32 {
    result
        .0
        .first()
        .map(|candidate| candidate.score)
        .unwrap_or(0.0)
}

fn classify_follow_up_verse_command(text: &str) -> Option<FollowUpVerseCommand> {
    // Never treat a phrase containing a book reference as a continuation.
    // This prevents "Matthew chapter 3 verse 4" from being reduced to
    // "current book, current chapter, verse 4" if one parser tier is weak.
    let normalized_for_reference = TranscriptNormalizer.normalize(text);
    if !GrammarReferenceParser
        .parse_all(&normalized_for_reference.text)
        .is_empty()
    {
        return None;
    }

    let normalized = text
        .to_ascii_lowercase()
        .replace(':', " ")
        .replace('.', " ")
        .replace(',', " ");
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let has = |needle: &str| tokens.iter().any(|token| *token == needle);
    let first_number = tokens.iter().find_map(|token| token.parse::<u16>().ok());

    if has("next") || (has("keep") && has("going")) || has("continue") {
        return Some(FollowUpVerseCommand::Next);
    }

    if (has("previous") && has("verse"))
        || (has("take") && has("back"))
        || (has("go") && has("back"))
    {
        return Some(FollowUpVerseCommand::Previous);
    }

    if has("verse") || (has("go") && has("to")) {
        if let Some(target_verse) = first_number.filter(|value| *value > 0) {
            return Some(FollowUpVerseCommand::GoTo(target_verse));
        }
    }

    None
}

fn find_verse_with_fallbacks(
    store: &aletheia_store::AletheiaStore,
    book: &str,
    chapter: u16,
    verse: u16,
    translation_id: &str,
) -> Option<aletheia_store::VerseRecord> {
    for candidate_translation in verse_translation_fallbacks(translation_id) {
        for candidate_book in verse_book_lookup_candidates(book) {
            let record = store
                .find_verse(&candidate_translation, &candidate_book, chapter, verse)
                .ok()
                .flatten();
            if record.is_some() {
                return record;
            }
        }
    }
    None
}

fn chapter_records_with_fallbacks(
    store: &aletheia_store::AletheiaStore,
    book: &str,
    chapter: u16,
    translation_id: &str,
) -> Vec<aletheia_store::VerseRecord> {
    for candidate_translation in verse_translation_fallbacks(translation_id) {
        for candidate_book in verse_book_lookup_candidates(book) {
            if let Ok(records) =
                store.chapter_verses(&candidate_translation, &candidate_book, chapter)
                && !records.is_empty()
            {
                return records;
            }
        }
    }
    Vec::new()
}

fn book_index(book: &str) -> Option<usize> {
    BOOKS.iter().position(|entry| entry.canonical == book)
}

fn resolve_follow_up_verse_command(
    store: &aletheia_store::AletheiaStore,
    text: &str,
    current: &LiveContextReference,
) -> Option<(String, u16, u16, String)> {
    let command = classify_follow_up_verse_command(text)?;
    match command {
        FollowUpVerseCommand::GoTo(target_verse) => {
            find_verse_with_fallbacks(
                store,
                &current.book,
                current.chapter,
                target_verse,
                &current.translation_id,
            )?;
            Some((
                current.book.clone(),
                current.chapter,
                target_verse,
                current.translation_id.clone(),
            ))
        }
        FollowUpVerseCommand::Next => {
            let same_chapter_next = current.verse.saturating_add(1);
            if find_verse_with_fallbacks(
                store,
                &current.book,
                current.chapter,
                same_chapter_next,
                &current.translation_id,
            )
            .is_some()
            {
                return Some((
                    current.book.clone(),
                    current.chapter,
                    same_chapter_next,
                    current.translation_id.clone(),
                ));
            }

            let mut start_index = book_index(&current.book)?;
            let mut chapter = current.chapter.saturating_add(1);
            loop {
                let book = BOOKS.get(start_index)?;
                while chapter <= book.max_chapter {
                    let records = chapter_records_with_fallbacks(
                        store,
                        book.canonical,
                        chapter,
                        &current.translation_id,
                    );
                    if let Some(first) = records.first() {
                        return Some((
                            first.book.clone(),
                            first.chapter,
                            first.verse,
                            first.translation_id.clone(),
                        ));
                    }
                    chapter = chapter.saturating_add(1);
                }
                start_index = start_index.saturating_add(1);
                chapter = 1;
            }
        }
        FollowUpVerseCommand::Previous => {
            if current.verse > 1 {
                let same_chapter_previous = current.verse.saturating_sub(1);
                if find_verse_with_fallbacks(
                    store,
                    &current.book,
                    current.chapter,
                    same_chapter_previous,
                    &current.translation_id,
                )
                .is_some()
                {
                    return Some((
                        current.book.clone(),
                        current.chapter,
                        same_chapter_previous,
                        current.translation_id.clone(),
                    ));
                }
            }

            let mut current_index = book_index(&current.book)?;
            let mut chapter = current.chapter.saturating_sub(1);
            loop {
                let book = BOOKS.get(current_index)?;
                while chapter >= 1 {
                    let records = chapter_records_with_fallbacks(
                        store,
                        book.canonical,
                        chapter,
                        &current.translation_id,
                    );
                    if let Some(last) = records.last() {
                        return Some((
                            last.book.clone(),
                            last.chapter,
                            last.verse,
                            last.translation_id.clone(),
                        ));
                    }
                    chapter = chapter.saturating_sub(1);
                    if chapter == 0 {
                        break;
                    }
                }
                if current_index == 0 {
                    return None;
                }
                current_index -= 1;
                chapter = BOOKS.get(current_index)?.max_chapter;
            }
        }
    }
}

fn parse_follow_up_verse_command(
    text: &str,
    current: &LiveContextReference,
) -> Option<(String, u16, u16, String)> {
    match classify_follow_up_verse_command(text)? {
        FollowUpVerseCommand::Next => Some((
            current.book.clone(),
            current.chapter,
            current.verse.saturating_add(1),
            current.translation_id.clone(),
        )),
        FollowUpVerseCommand::Previous => Some((
            current.book.clone(),
            current.chapter,
            current.verse.saturating_sub(1).max(1),
            current.translation_id.clone(),
        )),
        FollowUpVerseCommand::GoTo(target_verse) => Some((
            current.book.clone(),
            current.chapter,
            target_verse,
            current.translation_id.clone(),
        )),
    }
}

fn parse_first_explicit_reference(text: &str) -> Option<(String, u16, u16)> {
    let normalized = TranscriptNormalizer.normalize(text);
    GrammarReferenceParser
        .parse_all(&normalized.text)
        .into_iter()
        .find(|reference| reference.explicit_verse && !reference.needs_disambiguation)
        .map(|reference| {
            (
                reference.book.to_string(),
                reference.chapter,
                reference.verse_start,
            )
        })
}

fn build_query_variants(segment_text: &str, window_text: &str) -> Vec<String> {
    let mut queries = Vec::new();
    let push_unique = |value: String, out: &mut Vec<String>| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        if !out.iter().any(|existing| existing == trimmed) {
            out.push(trimmed.to_string());
        }
    };

    push_unique(segment_text.to_string(), &mut queries);
    push_unique(window_text.to_string(), &mut queries);

    let words: Vec<&str> = segment_text.split_whitespace().collect();
    for width in [6_usize, 8, 12] {
        if words.len() > width {
            push_unique(words[words.len() - width..].join(" "), &mut queries);
            push_unique(words[..width].join(" "), &mut queries);
        }
    }

    queries
}

fn resolve_reference_direct(
    store: &aletheia_store::AletheiaStore,
    book: &str,
    chapter: u16,
    verse: u16,
    translation_id: &str,
    source: &str,
) -> Option<(Vec<aletheia_detection::RankedCandidate>, SearchResultDto)> {
    let record = find_verse_with_fallbacks(store, book, chapter, verse, translation_id)?;
    let dto = crate::verse_to_search_result(record, source);
    let ranked = vec![aletheia_detection::RankedCandidate {
        reference: dto.reference.clone(),
        score: 0.995,
        tiers: vec![aletheia_detection::DetectionTier::ExactReference],
    }];
    Some((ranked, dto))
}

// ---------------------------------------------------------------------------
// Core service state
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_service_state(state: State<'_, DesktopState>) -> Result<ServiceStateDto, String> {
    let runtime = state.lock_runtime()?.clone();
    let audit_count = state.audit_count()?;

    let live_segments = state.snapshot_live_transcript();
    let transcript = if live_segments.is_empty() {
        production_transcript()
    } else {
        live_segments
    };

    // Build a dynamic session ID based on today's date in UTC.
    let checked_at = now_ms();
    let session_date = crate::utc_date_string(checked_at);
    let session_id = format!("service-{session_date}");
    let session_name = format!("Service — {session_date}");

    let health = crate::health::live_health(&state).unwrap_or_default();

    Ok(ServiceStateDto {
        session: ServiceSessionDto {
            id: session_id,
            name: session_name,
            started_at: format!("{session_date}T00:00:00Z"),
            database_path: state.database_path.display().to_string(),
            // Must be "tauri" for the frontend to recognize Desktop/Core online mode.
            // Offline vs hybrid connectivity is tracked separately via offline_mode_enabled.
            mode: "tauri".to_string(),
            data_miser_enabled: runtime.data_miser_enabled,
            offline_mode_enabled: runtime.offline_mode_enabled,
            destinations_armed: runtime.destinations_armed,
            audit_count,
            last_event_sequence: audit_count,
            checked_at_ms: checked_at,
            operating_mode: runtime.operating_mode.clone(),
        },
        transcript,
        candidates: production_candidates(),
        integrations: live_integrations(&state),
        health,
        preview: runtime.preview,
        live: runtime.live,
    })
}

#[tauri::command]
pub fn search_scripture(
    state: State<'_, DesktopState>,
    query: String,
    translation_id: Option<String>,
) -> Result<Vec<SearchResultDto>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let tid = translation_id.unwrap_or_else(|| "kjv".to_string());
    let store = state.lock_store()?;
    Ok(crate::scripture_search::search_scripture_unified(
        trimmed,
        &tid,
        crate::scripture_search::SearchContext::ManualSearch,
        &store,
    ))
}

/// Fetches a single canonical verse (or inclusive range) in every requested
/// translation. The `verse_id` form is the canonical identity emitted by
/// scripture search: `"<book>|<chapter>|<verse>"` with an optional
/// `"|<verse_end>"` suffix for ranges.
///
/// Returns a `(translation_id_uppercased, snippet)` map; missing translations
/// are omitted rather than errored — projection should always render whatever
/// is available, never block on a missing pack.
#[tauri::command]
pub fn fetch_verse_in_translations(
    verse_id: String,
    translations: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let parts: Vec<&str> = verse_id.split('|').collect();
    if parts.len() < 3 {
        return Err(format!("malformed verse_id: {verse_id}"));
    }
    let book = parts[0].to_string();
    let chapter: u16 = parts[1]
        .parse()
        .map_err(|e| format!("bad chapter in verse_id {verse_id}: {e}"))?;
    let verse_start: u16 = parts[2]
        .parse()
        .map_err(|e| format!("bad verse in verse_id {verse_id}: {e}"))?;
    let range_end: Option<u16> = parts
        .get(3)
        .and_then(|s| s.parse().ok())
        .filter(|end: &u16| *end > verse_start);

    let store = state.lock_store()?;
    let mut out = std::collections::HashMap::new();
    for raw in translations {
        let tid = raw.trim();
        if tid.is_empty() {
            continue;
        }
        let mut head = None;
        let mut hit_translation = tid.to_lowercase();
        for candidate_translation in verse_translation_fallbacks(tid) {
            for candidate_book in verse_book_lookup_candidates(&book) {
                match store.find_verse(
                    &candidate_translation,
                    &candidate_book,
                    chapter,
                    verse_start,
                ) {
                    Ok(Some(record)) => {
                        hit_translation = candidate_translation.clone();
                        head = Some(record);
                        break;
                    }
                    _ => {}
                }
            }
            if head.is_some() {
                break;
            }
        }
        let Some(head) = head else {
            continue;
        };
        let mut text = head.text.clone();
        if let Some(end) = range_end {
            for v in (verse_start + 1)..=end {
                match store.find_verse(&hit_translation, &head.book, chapter, v) {
                    Ok(Some(record)) => {
                        text.push(' ');
                        text.push_str(&record.text);
                    }
                    // Stop on first gap; partial text is still useful.
                    _ => break,
                }
            }
        }
        out.insert(tid.to_uppercase(), text);
    }
    Ok(out)
}

#[tauri::command]
pub fn render_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<OutputSceneDto, String> {
    {
        let mut runtime = state.lock_runtime()?;
        runtime.preview = candidate.clone();
    }
    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::PreviewRendered,
        &operator,
        &format!("Preview: {}", candidate.reference),
    )?;
    let _ = state.persist_session();
    let scene = scene_from_candidate(&candidate)?;
    let dto = scene_to_dto(scene);
    let _ = app.emit("aletheia://live-updated", &candidate);
    Ok(dto)
}

#[tauri::command]
pub fn set_destinations_armed(
    armed: bool,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<(), String> {
    {
        let mut runtime = state.lock_runtime()?;
        runtime.destinations_armed = armed;
    }
    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::IntegrationConfigUpdated,
        &operator,
        &format!("Destinations {}", if armed { "armed" } else { "disarmed" }),
    )?;
    let _ = state.persist_session();
    let _ = app.emit("aletheia://armed-changed", armed);
    Ok(())
}

#[tauri::command]
pub fn set_data_miser(enabled: bool, state: State<'_, DesktopState>) -> Result<(), String> {
    {
        let mut runtime = state.lock_runtime()?;
        runtime.data_miser_enabled = enabled;
    }
    let _ = state.persist_session();
    Ok(())
}

/// Update the first-class operating mode. Accepts the string form of
/// `aletheia_detection::OperatingMode` ("manual" / "assisted" / "auto" /
/// "rehearsal" / "mock"). Unknown values are rejected so the front-end can
/// surface the error rather than silently falling back.
#[tauri::command]
pub fn set_operating_mode(mode: String, state: State<'_, DesktopState>) -> Result<(), String> {
    let parsed = aletheia_detection::OperatingMode::from_str(&mode)
        .ok_or_else(|| format!("unknown operating mode: {mode}"))?;
    {
        let mut runtime = state.lock_runtime()?;
        runtime.operating_mode = parsed.as_str().to_string();
    }
    let _ = state.persist_session();
    Ok(())
}

/// Returns the operator-configured translation packs for stream-overlay
/// fan-out. Lower-cased pack ids in display order.
#[tauri::command]
pub fn get_translation_packs(state: State<'_, DesktopState>) -> Result<Vec<String>, String> {
    Ok(state.lock_runtime()?.translation_packs.clone())
}

/// Update the translation packs the stream-overlay server fans out to.
/// Pack ids are lower-cased; duplicates are removed; an empty list disables
/// fan-out (the overlay then shows only the live translation).
#[tauri::command]
pub fn set_translation_packs(
    packs: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let mut deduped: Vec<String> = Vec::with_capacity(packs.len());
    for raw in packs {
        let id = raw.trim().to_ascii_lowercase();
        if id.is_empty() || deduped.contains(&id) {
            continue;
        }
        deduped.push(id);
    }
    {
        let mut runtime = state.lock_runtime()?;
        runtime.translation_packs = deduped;
    }
    let _ = state.persist_session();
    Ok(())
}

/// Stage D: refit the per-translation isotonic calibration from the
/// `calibration_samples` table. Uses each operator-confirmed sample as a
/// `label = 1.0` and corrected/rejected as `0.0`. Returns the number of knots
/// produced (0 = identity, ≥ 1 = calibrated). When fewer than `min_samples`
/// rows are present per translation, the calibration stays as identity — the
/// architectural-vision rule "wrong scripture is worse than no scripture"
/// favours a transparent passthrough over an under-fit curve that could
/// inflate borderline scores.
#[tauri::command]
pub fn recompute_calibration(
    translation_id: Option<String>,
    min_samples: Option<usize>,
    state: State<'_, DesktopState>,
) -> Result<usize, String> {
    let translation = translation_id
        .map(|t| t.to_ascii_lowercase())
        .unwrap_or_else(|| {
            state
                .lock_runtime()
                .ok()
                .map(|r| r.live.translation.to_ascii_lowercase())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "kjv".to_string())
        });
    let min = min_samples.unwrap_or(25).max(2);

    let samples = {
        let store = state.lock_store()?;
        store
            .list_calibration_samples()
            .map_err(|e| format!("calibration sample fetch: {e}"))?
    };

    // We don't yet store the raw detector score per sample — outcome strings
    // alone aren't enough to fit a real isotonic curve. As a deliberate
    // safety-first stub, we leave the calibration as identity until score
    // capture is wired into the recording path. Returning 0 here keeps
    // diagnostics honest: "0 knots = identity passthrough".
    let _ = (samples, min);
    let calibration = IsotonicCalibration::identity();
    let knots = calibration.knot_count();
    if let Ok(mut map) = state.calibrations.lock() {
        map.insert(translation, calibration);
    }
    Ok(knots)
}

#[tauri::command]
pub fn send_live(
    candidate: Option<ScriptureCandidateDto>,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<LiveOutputResultDto, String> {
    let runtime = state.lock_runtime()?.clone();
    if !runtime.destinations_armed {
        return Err("Live output blocked — destinations are not armed.".to_string());
    }
    send_live_candidate_now(
        &state,
        &app,
        candidate.unwrap_or_else(|| runtime.preview.clone()),
    )
}

#[tauri::command]
pub fn arm_and_send_live(
    candidate: ScriptureCandidateDto,
    operator_action_id: String,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<LiveOutputResultDto, String> {
    let action = operator_action_id.trim();
    if action.len() < 12 {
        return Err("Explicit operator action id is required for arm-and-send.".to_string());
    }
    {
        let mut runtime = state.lock_runtime()?;
        runtime.destinations_armed = true;
    }
    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::IntegrationConfigUpdated,
        &operator,
        &format!("Destinations armed by explicit send action {action}"),
    )?;
    send_live_candidate_now(&state, &app, candidate)
}

fn send_live_candidate_now(
    state: &State<'_, DesktopState>,
    app: &AppHandle,
    live_candidate: ScriptureCandidateDto,
) -> Result<LiveOutputResultDto, String> {
    let scene = scene_from_candidate(&live_candidate)?;
    {
        let mut rt = state.lock_runtime()?;
        rt.preview = live_candidate.clone();
        rt.live = live_candidate.clone();
    }
    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("Live: {}", live_candidate.reference),
    )?;
    let _ = state.persist_session();
    let dto = scene_to_dto(scene);
    let _ = app.emit("aletheia://live-updated", &live_candidate);
    Ok(LiveOutputResultDto {
        scene: dto,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_all_outputs(
    source: String,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    let mut cleared = Vec::new();
    let mut failures = Vec::new();

    {
        let mut adapter = state.lock_vmix()?;
        match adapter.clear() {
            Ok(()) => cleared.push("vMix".to_string()),
            Err(error) => failures.push(format!("vMix: {error}")),
        }
    }
    {
        let mut adapter = state.lock_obs()?;
        match adapter.clear() {
            Ok(()) => cleared.push("OBS".to_string()),
            Err(error) => failures.push(format!("OBS: {error}")),
        }
    }
    {
        let mut adapter = state.lock_propresenter()?;
        match adapter.clear() {
            Ok(()) => cleared.push("ProPresenter".to_string()),
            Err(error) => failures.push(format!("ProPresenter: {error}")),
        }
    }
    {
        let mut adapter = state.lock_companion()?;
        match adapter.clear() {
            Ok(()) => cleared.push("Companion".to_string()),
            Err(error) => failures.push(format!("Companion: {error}")),
        }
    }
    {
        let mut adapter = state.lock_osc()?;
        match adapter.clear() {
            Ok(()) => cleared.push("OSC".to_string()),
            Err(error) => failures.push(format!("OSC: {error}")),
        }
    }
    {
        let mut adapter = state.lock_easyworship()?;
        match adapter.clear() {
            Ok(()) => cleared.push("EasyWorship".to_string()),
            Err(error) => failures.push(format!("EasyWorship: {error}")),
        }
    }

    let cleared_candidate = ScriptureCandidateDto {
        id: "cleared-output".to_string(),
        reference: "Output cleared".to_string(),
        translation: String::new(),
        language: "English".to_string(),
        text: String::new(),
        confidence: 100,
        source: source.clone(),
        reason: "Operator cleared all configured outputs.".to_string(),
        status: "cleared".to_string(),
    };
    {
        let mut runtime = state.lock_runtime()?;
        runtime.live = cleared_candidate.clone();
    }

    let operator = state.operator_name();
    let detail = if failures.is_empty() {
        format!("Panic clear from {source}: cleared {}", cleared.join(", "))
    } else {
        format!(
            "Panic clear from {source}: cleared {}; failed {}",
            cleared.join(", "),
            failures.join("; ")
        )
    };
    record_integration_event_state(&state, "all-outputs", "info", "output.cleared", &detail)?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, &operator, &detail)?;
    let _ = state.persist_session();
    let _ = app.emit("aletheia://live-updated", &cleared_candidate);

    Ok(cleared)
}

#[tauri::command]
pub fn run_pre_service_check(state: State<'_, DesktopState>) -> Result<Vec<HealthItemDto>, String> {
    Ok(live_integrations(&state)
        .into_iter()
        .map(|i| HealthItemDto {
            label: i.name,
            state: i.state,
            detail: i.detail,
            action: "Check".to_string(),
        })
        .collect())
}

#[tauri::command]
pub fn analyze_transcript(state: State<'_, DesktopState>) -> Result<AiDetectionResultDto, String> {
    let live_segments = state.snapshot_live_transcript();
    let transcript = if live_segments.is_empty() {
        production_transcript()
    } else {
        live_segments
    };
    let runtime = state.lock_runtime()?.clone();
    let store = state.lock_store()?;
    let manifest = merged_offline_asset_manifest(&store)?;
    let stt_readiness = stt_readiness_from_manifest(&manifest, runtime.data_miser_enabled);
    drop(store);

    let detection_start = std::time::Instant::now();
    let mut candidates = detect_candidates_for_transcript(transcript.clone())?;
    let detection_latency_ms = detection_start.elapsed().as_millis() as u32;

    // Enrich candidates with real verse text from the local SQLite library.
    // The detector returns a generic fallback string for references not in its
    // small hardcoded table; we look up the actual KJV text here.
    {
        let store = state.lock_store()?;
        for candidate in &mut candidates {
            if candidate.text == "Detected scripture candidate requires operator review." {
                if let Some((book, chapter, verse)) = parse_reference(&candidate.reference) {
                    if let Ok(Some(record)) = store.find_verse("kjv", &book, chapter, verse) {
                        candidate.text = record.text;
                    }
                }
            }
        }
    }

    let lang_detector = KeywordLanguageDetector;
    let language_detections = language_detections_from_transcript(&lang_detector, &transcript);

    let mode = if stt_readiness.offline_models_ready {
        "offline"
    } else if stt_readiness.cloud_ready {
        "cloud"
    } else {
        "manual"
    };

    Ok(AiDetectionResultDto {
        mode: mode.to_string(),
        decision_policy: "operator-confirms-all".to_string(),
        processed_segments: transcript.len() as u16,
        candidates,
        adapters: vec![AiAdapterStatusDto {
            id: "offline-whisper-local".to_string(),
            name: "Whisper (local, offline)".to_string(),
            mode: "offline".to_string(),
            state: if stt_readiness.offline_models_ready {
                "ready"
            } else {
                "pending"
            }
            .to_string(),
            detail: if stt_readiness.offline_models_ready {
                "ggml-base model loaded".to_string()
            } else {
                "No offline model installed".to_string()
            },
            latency_ms: detection_latency_ms,
        }],
        languages: language_detections,
        supported_languages: KeywordLanguageDetector::default()
            .supported_languages()
            .into_iter()
            .map(|l| supported_language_to_dto(l))
            .collect(),
        accuracy_target: accuracy_target_dto(),
        checked_at_ms: now_ms(),
    })
}

// ---------------------------------------------------------------------------
// vMix adapter commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_vmix_status(state: State<'_, DesktopState>) -> Result<VmixStatusDto, String> {
    let adapter = state.lock_vmix()?;
    Ok(vmix_status_dto(&adapter))
}

#[tauri::command]
pub fn get_vmix_config(state: State<'_, DesktopState>) -> Result<VmixConfigDto, String> {
    let adapter = state.lock_vmix()?;
    Ok(vmix_config_to_dto(adapter.config()))
}

#[tauri::command]
pub fn update_vmix_config(
    config: VmixConfigDto,
    state: State<'_, DesktopState>,
) -> Result<VmixStatusDto, String> {
    let new_config = vmix_config_from_dto(config)?;
    let config_json =
        serde_json::to_string(&vmix_config_to_dto(&new_config)).map_err(|e| e.to_string())?;
    {
        let store = state.lock_store()?;
        store
            .upsert_integration_config(&IntegrationConfigRecord {
                id: "vmix-main".to_string(),
                kind: "vmix".to_string(),
                display_name: "vMix".to_string(),
                enabled: true,
                config_json,
                secret_ref: None,
            })
            .map_err(|e| e.to_string())?;
    }
    {
        let mut adapter = state.lock_vmix()?;
        *adapter = aletheia_vmix::VmixAdapter::new(new_config);
    }
    let adapter = state.lock_vmix()?;
    Ok(vmix_status_dto(&adapter))
}

#[tauri::command]
pub fn send_vmix_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
) -> Result<VmixDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_vmix()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "vmix-main",
        "info",
        "preview.sent",
        &format!("vMix preview: {}", candidate.reference),
    )?;
    Ok(VmixDispatchResultDto {
        state: "connected".to_string(),
        detail: format!("vMix preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_vmix_live(
    candidate: ScriptureCandidateDto,
    destinations_armed: bool,
    state: State<'_, DesktopState>,
) -> Result<VmixDispatchResultDto, String> {
    if !destinations_armed {
        return Err("vMix live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_vmix()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "vmix-main",
        "info",
        "live.sent",
        &format!("vMix live: {}", candidate.reference),
    )?;
    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("vMix live {}", candidate.reference),
    )?;
    Ok(VmixDispatchResultDto {
        state: "connected".to_string(),
        detail: format!("vMix live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_vmix_overlay(state: State<'_, DesktopState>) -> Result<VmixDispatchResultDto, String> {
    {
        let mut adapter = state.lock_vmix()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "vmix-main",
        "info",
        "output.cleared",
        "Cleared vMix overlay",
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputCleared,
        &operator,
        "Cleared vMix overlay",
    )?;
    Ok(VmixDispatchResultDto {
        state: "ready".to_string(),
        detail: "vMix overlay cleared.".to_string(),
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// OBS adapter commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_obs_status(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_obs()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn update_obs_config(
    config: ObsConfigDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let new_config = ObsConfig {
        integration_id: "obs-main".to_string(),
        host: config.host.trim().to_string(),
        port: config.port,
        password: config.password,
        scene_name: config.scene_name.trim().to_string(),
        source_name: config.source_name.trim().to_string(),
        timeout_ms: aletheia_obs::DEFAULT_TIMEOUT_MS,
        allow_private_network: config.allow_private_network,
    };
    new_config.validate().map_err(|e| e.to_string())?;
    {
        let mut adapter = state.lock_obs()?;
        *adapter = ObsAdapter::new(new_config);
    }
    let adapter = state.lock_obs()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_obs_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_obs()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "obs-main",
        "info",
        "preview.sent",
        &format!("OBS preview: {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: "connected".to_string(),
        detail: format!("OBS preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_obs_live(
    candidate: ScriptureCandidateDto,
    destinations_armed: bool,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("OBS live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_obs()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "obs-main",
        "info",
        "live.sent",
        &format!("OBS live: {}", candidate.reference),
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        "operator:local-booth",
        &format!("OBS live {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: "connected".to_string(),
        detail: format!("OBS live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_obs_output(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_obs()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "obs-main",
        "info",
        "output.cleared",
        "Cleared OBS scripture source",
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputCleared,
        "operator:local-booth",
        "Cleared OBS scripture source",
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: "connected".to_string(),
        detail: "OBS scripture source hidden.".to_string(),
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// ProPresenter adapter commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_propresenter_status(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_propresenter()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn get_propresenter_config(
    state: State<'_, DesktopState>,
) -> Result<ProPresenterConfigDto, String> {
    let adapter = state.lock_propresenter()?;
    let cfg = adapter.config().clone();
    Ok(ProPresenterConfigDto {
        host: cfg.host,
        port: cfg.port,
        message_name: cfg.message_name,
        verse_token: cfg.verse_token,
        reference_token: cfg.reference_token,
        allow_private_network: cfg.allow_private_network,
    })
}

#[tauri::command]
pub fn update_propresenter_config(
    config: ProPresenterConfigDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let new_config = ProPresenterConfig {
        integration_id: "propresenter-main".to_string(),
        host: config.host.trim().to_string(),
        port: config.port,
        message_name: config.message_name.trim().to_string(),
        verse_token: config.verse_token.trim().to_string(),
        reference_token: config.reference_token.trim().to_string(),
        timeout_ms: aletheia_propresenter::DEFAULT_TIMEOUT_MS,
        allow_private_network: config.allow_private_network,
    };
    new_config.validate().map_err(|e| e.to_string())?;
    {
        let mut adapter = state.lock_propresenter()?;
        *adapter = ProPresenterAdapter::new(new_config);
    }
    let adapter = state.lock_propresenter()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_propresenter_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_propresenter()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "propresenter-main",
        "info",
        "preview.sent",
        &format!("ProPresenter preview: {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: "connected".to_string(),
        detail: format!("ProPresenter preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_propresenter_live(
    candidate: ScriptureCandidateDto,
    destinations_armed: bool,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("ProPresenter live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_propresenter()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "propresenter-main",
        "info",
        "live.sent",
        &format!("ProPresenter live: {}", candidate.reference),
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("ProPresenter live {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: "connected".to_string(),
        detail: format!("ProPresenter live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_propresenter_output(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_propresenter()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "propresenter-main",
        "info",
        "output.cleared",
        "Cleared ProPresenter scripture message",
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputCleared,
        &operator,
        "Cleared ProPresenter scripture message",
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: "connected".to_string(),
        detail: "ProPresenter scripture message hidden.".to_string(),
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// Bitfocus Companion adapter commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_companion_status(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_companion()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn get_companion_config(state: State<'_, DesktopState>) -> Result<CompanionConfigDto, String> {
    let adapter = state.lock_companion()?;
    let cfg = adapter.config().clone();
    Ok(CompanionConfigDto {
        host: cfg.host,
        port: cfg.port,
        page: cfg.page,
        row: cfg.row,
        column: cfg.column,
        verse_variable: cfg.verse_variable,
        reference_variable: cfg.reference_variable,
        allow_private_network: cfg.allow_private_network,
    })
}

#[tauri::command]
pub fn update_companion_config(
    config: CompanionConfigDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let new_config = CompanionConfig {
        integration_id: "companion-main".to_string(),
        host: config.host.trim().to_string(),
        port: config.port,
        page: config.page,
        row: config.row,
        column: config.column,
        verse_variable: config.verse_variable.trim().to_string(),
        reference_variable: config.reference_variable.trim().to_string(),
        timeout_ms: aletheia_companion::DEFAULT_TIMEOUT_MS,
        allow_private_network: config.allow_private_network,
    };
    new_config.validate().map_err(|e| e.to_string())?;
    {
        let mut adapter = state.lock_companion()?;
        *adapter = CompanionAdapter::new(new_config);
    }
    let adapter = state.lock_companion()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_companion_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_companion()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "companion-main",
        "info",
        "preview.sent",
        &format!("Companion preview: {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: "connected".to_string(),
        detail: format!("Companion preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_companion_live(
    candidate: ScriptureCandidateDto,
    destinations_armed: bool,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("Companion live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_companion()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "companion-main",
        "info",
        "live.sent",
        &format!("Companion live: {}", candidate.reference),
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("Companion live {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: "connected".to_string(),
        detail: format!("Companion live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_companion_output(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_companion()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "companion-main",
        "info",
        "output.cleared",
        "Cleared Companion variables",
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputCleared,
        &operator,
        "Cleared Companion variables",
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: "connected".to_string(),
        detail: "Companion scripture variables cleared.".to_string(),
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// OSC adapter commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_osc_status(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_osc()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn update_osc_config(
    config: OscConfigDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let new_config = OscConfig {
        integration_id: "osc-main".to_string(),
        host: config.host.trim().to_string(),
        port: config.port,
        namespace: config.namespace.trim().to_string(),
        allow_private_network: config.allow_private_network,
    };
    new_config.validate().map_err(|e| e.to_string())?;
    {
        let mut adapter = state.lock_osc()?;
        *adapter = OscAdapter::new(new_config);
    }
    let adapter = state.lock_osc()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_osc_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_osc()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "osc-main",
        "info",
        "preview.sent",
        &format!("OSC preview: {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: format!("OSC preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_osc_live(
    candidate: ScriptureCandidateDto,
    destinations_armed: bool,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("OSC live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_osc()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "osc-main",
        "info",
        "live.sent",
        &format!("OSC live: {}", candidate.reference),
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("OSC live {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: format!("OSC live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_osc_test_ping(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    {
        let adapter = state.lock_osc()?;
        adapter.ping().map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "osc-main",
        "info",
        "ping.sent",
        "OSC /aletheia/ping 1",
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: "OSC /aletheia/ping 1 sent.".to_string(),
        reference: "/aletheia/ping".to_string(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_osc_output(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_osc()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "osc-main",
        "info",
        "output.cleared",
        "Cleared OSC output",
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputCleared,
        &operator,
        "Cleared OSC output",
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: "OSC output cleared.".to_string(),
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// EasyWorship adapter commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_easyworship_status(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_easyworship()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn update_easyworship_config(
    config: EasyWorshipConfigDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let new_config = EasyWorshipConfig {
        integration_id: "easyworship-main".to_string(),
        watch_dir: PathBuf::from(config.watch_dir.trim()),
    };
    new_config.validate().map_err(|e| e.to_string())?;
    {
        let mut adapter = state.lock_easyworship()?;
        *adapter = EasyWorshipAdapter::new(new_config);
    }
    let adapter = state.lock_easyworship()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_easyworship_preview(
    candidate: ScriptureCandidateDto,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_easyworship()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(
        &state,
        "easyworship-main",
        "info",
        "preview.sent",
        &format!("EasyWorship preview: {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: "connected".to_string(),
        detail: format!("EasyWorship preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_easyworship_live(
    candidate: ScriptureCandidateDto,
    destinations_armed: bool,
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("EasyWorship live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_easyworship()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "easyworship-main",
        "info",
        "live.sent",
        &format!("EasyWorship live: {}", candidate.reference),
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("EasyWorship live {}", candidate.reference),
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: "connected".to_string(),
        detail: format!("EasyWorship live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_easyworship_output(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_easyworship()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(
        &state,
        "easyworship-main",
        "info",
        "output.cleared",
        "Cleared EasyWorship output",
    )?;
    record_audit_state(
        &state,
        AuditAction::LiveOutputCleared,
        &operator,
        "Cleared EasyWorship output",
    )?;
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: "connected".to_string(),
        detail: "EasyWorship output cleared.".to_string(),
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// Integration events
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_recent_integration_events(
    state: State<'_, DesktopState>,
) -> Result<Vec<IntegrationEventDto>, String> {
    recent_integration_events(&state, 50)
}

// ---------------------------------------------------------------------------
// Readiness + offline assets
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_production_readiness(
    state: State<'_, DesktopState>,
) -> Result<ProductionReadinessReport, String> {
    let store = state.lock_store()?;
    let offline_assets = merged_offline_asset_manifest(&store)?;

    // Count trusted plugins for the plugin policy score.
    let trusted_plugin_count = store
        .list_trusted_plugins()
        .map(|p| p.len() as u16)
        .unwrap_or(0);

    // Secrets are stored in the OS keyring; we use a fixed count of 0 here
    // since enumerating keyring entries is not cross-platform.
    let stored_secret_count: u16 = 0;

    drop(store);
    Ok(production_readiness_report_with_assets(
        now_ms(),
        trusted_plugin_count,
        stored_secret_count,
        offline_assets,
    ))
}

#[tauri::command]
pub fn install_offline_asset(
    asset_id: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    // Compute checksum from installed file if it exists in the offline-assets directory.
    let checksum = offline_asset_root(&state)
        .ok()
        .and_then(|root| {
            // Try common extensions for this asset id.
            for ext in &["", ".bin", ".gguf", ".ggml"] {
                let candidate = root.join(format!("{asset_id}{ext}"));
                if candidate.exists() {
                    return sha256_file_hex(&candidate).ok();
                }
            }
            None
        })
        .unwrap_or_default();

    let store = state.lock_store()?;
    store
        .update_offline_asset_state(&OfflineAssetStateRecord {
            id: asset_id,
            state: "installed".to_string(),
            checksum,
            updated_at_ms: now_ms(),
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn install_offline_asset_from_path(
    file_path: String,
    asset_id: Option<String>,
    expected_checksum: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let source = std::path::Path::new(&file_path);
    if !source.exists() {
        return Err(format!("File not found: {file_path}"));
    }
    let actual_checksum = sha256_file_hex(source)?;

    // Validate checksum if the caller supplied one.
    if let Some(ref expected) = expected_checksum {
        if !expected.is_empty() && actual_checksum != *expected {
            return Err(format!(
                "Checksum mismatch: expected {expected}, got {actual_checksum}"
            ));
        }
    }

    let asset_root = offline_asset_root(&state)?;
    std::fs::create_dir_all(&asset_root).map_err(|e| e.to_string())?;
    let filename = source.file_name().unwrap_or_default().to_string_lossy();
    let target = asset_root.join(filename.as_ref());
    std::fs::copy(source, &target).map_err(|e| e.to_string())?;

    // Prefer the caller-supplied asset ID; fall back to filename without extension.
    let id = asset_id.unwrap_or_else(|| {
        filename
            .trim_end_matches(".bin")
            .trim_end_matches(".gguf")
            .trim_end_matches(".ggml")
            .to_string()
    });

    let store = state.lock_store()?;
    store
        .update_offline_asset_state(&OfflineAssetStateRecord {
            id,
            state: "installed".to_string(),
            checksum: actual_checksum,
            updated_at_ms: now_ms(),
        })
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Device acceptance
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn record_device_acceptance(
    device_id: String,
    step_label: Option<String>,
    passed: Option<bool>,
    note: Option<String>,
    evidence_path: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let store = state.lock_store()?;
    store
        .insert_device_acceptance_receipt(&DeviceAcceptanceReceiptRecord {
            device_id,
            step_label: step_label.unwrap_or_else(|| "manual-acceptance".to_string()),
            passed: passed.unwrap_or(true),
            note,
            evidence_path,
            recorded_at_ms: now_ms(),
        })
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Rehearsal
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn run_local_rehearsal(
    state: State<'_, DesktopState>,
) -> Result<LocalRehearsalReportDto, String> {
    let start = std::time::Instant::now();
    let mut steps = Vec::new();

    // Step 1: Integration connectivity probe
    let integrations = live_integrations(&state);
    for integration in &integrations {
        steps.push(LocalRehearsalStepDto {
            label: format!("{} connectivity", integration.name),
            state: integration.state.clone(),
            detail: integration.detail.clone(),
            duration_ms: start.elapsed().as_millis() as u32,
        });
    }

    // Step 2: Scripture detection accuracy suite from rehearsal module
    let accuracy_report = crate::rehearsal::evaluate_accuracy_fixtures_cmd(&state)?;
    steps.extend(accuracy_report.steps);

    // Step 3: Demo transcript end-to-end round-trip
    let candidates = detect_candidates_for_transcript(production_transcript())?;
    let found = candidates.iter().any(|c| c.reference == "Romans 8:28");
    steps.push(LocalRehearsalStepDto {
        label: "Demo transcript round-trip".to_string(),
        state: if found { "healthy" } else { "degraded" }.to_string(),
        detail: format!(
            "{} candidate(s) from demo transcript{}",
            candidates.len(),
            if found {
                " — Romans 8:28 ✓"
            } else {
                " — Romans 8:28 not found"
            }
        ),
        duration_ms: start.elapsed().as_millis() as u32,
    });

    let passed = steps
        .iter()
        .filter(|s| s.state == "connected" || s.state == "healthy" || s.state == "ready")
        .count() as u16;
    let total = steps.len() as u16;

    Ok(LocalRehearsalReportDto {
        generated_at_ms: now_ms(),
        state: if passed == total {
            "passed"
        } else {
            "degraded"
        }
        .to_string(),
        passed,
        total,
        proof_path: accuracy_report.proof_path,
        steps,
    })
}

// ---------------------------------------------------------------------------
// Export: support bundle, offline pack, booth pack
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn export_support_bundle(
    include_transcript_text: bool,
    state: State<'_, DesktopState>,
) -> Result<SupportBundleExportDto, String> {
    let app_dir = state
        .database_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let bundle_path = app_dir.join("support-bundle.json");
    let store = state.lock_store()?;
    let audit_count: i64 = store
        .connection()
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .unwrap_or(0);
    // Optionally include the live transcript segments in the bundle.
    let transcript_section = if include_transcript_text {
        let segments: Vec<serde_json::Value> = state.snapshot_live_transcript().iter().map(|s| {
            serde_json::json!({ "id": s.id, "time": s.time, "speaker": s.speaker, "text": s.text })
        }).collect();
        format!(",\"transcript\":{}", serde_json::json!(segments))
    } else {
        String::new()
    };
    let content = format!(
        r#"{{"version":"0.1.0","generated_at_ms":{},"audit_events":{},"database":"{}","includeTranscriptText":{}{}}}"#,
        now_ms(),
        audit_count,
        state.database_path.display(),
        include_transcript_text,
        transcript_section
    );
    let (redacted_text, summary) = redact_support_text(&content);
    let size = redacted_text.len() as u64;
    std::fs::write(&bundle_path, &redacted_text).map_err(|e| e.to_string())?;
    Ok(SupportBundleExportDto {
        path: bundle_path.display().to_string(),
        size_bytes: size,
        redaction_summary: summary,
        included_files: vec!["support-bundle.json".to_string()],
    })
}

#[tauri::command]
pub fn export_offline_asset_pack(
    target_dir: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<OfflinePackExportDto, String> {
    let app_dir = state
        .database_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let pack_dir = if let Some(dir) = target_dir.filter(|s| !s.is_empty()) {
        std::path::PathBuf::from(dir)
    } else {
        app_dir.join("offline-pack")
    };
    std::fs::create_dir_all(&pack_dir).map_err(|e| e.to_string())?;
    let manifest_path = pack_dir.join("manifest.json");
    let checksum_path = pack_dir.join("checksums.sha256");
    let store = state.lock_store()?;
    let manifest = merged_offline_asset_manifest(&store)?;
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, &manifest_json).map_err(|e| e.to_string())?;
    // Compute real SHA-256 checksums for every installed offline asset.
    let asset_root = app_dir.join("offline-assets");
    let mut checksum_lines = String::new();
    let mut bytes_total = manifest_json.len() as u64;

    if asset_root.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&asset_root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Ok(hex) = sha256_file_hex(&p) {
                        let filename = p.file_name().unwrap_or_default().to_string_lossy();
                        checksum_lines.push_str(&format!("{hex}  {filename}\n"));
                        bytes_total += p.metadata().map(|m| m.len()).unwrap_or(0);
                    }
                }
            }
        }
    }
    // Always include the manifest itself.
    if let Ok(hex) = sha256_file_hex(&manifest_path) {
        checksum_lines.push_str(&format!("{hex}  manifest.json\n"));
    }

    std::fs::write(&checksum_path, &checksum_lines).map_err(|e| e.to_string())?;
    Ok(OfflinePackExportDto {
        path: pack_dir.display().to_string(),
        manifest_path: manifest_path.display().to_string(),
        checksum_path: checksum_path.display().to_string(),
        asset_count: manifest.installed_count,
        bytes_written: bytes_total,
    })
}

#[tauri::command]
pub fn export_booth_pack(state: State<'_, DesktopState>) -> Result<BoothPackExportDto, String> {
    let app_dir = state
        .database_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let pack_dir = app_dir.join("booth-pack");
    std::fs::create_dir_all(&pack_dir).map_err(|e| e.to_string())?;
    let runtime = state.lock_runtime()?.clone();
    let candidate = &runtime.preview;
    let vmix_config = vmix_config_to_dto(state.lock_vmix()?.config());
    let mut files = Vec::new();
    write_booth_pack_file(
        &pack_dir,
        "obs/aletheia-browser-source.html",
        &obs_browser_source_html(candidate),
        &mut files,
    )?;
    write_booth_pack_file(
        &pack_dir,
        "easyworship/current-verse.txt",
        &format!("{}\n{}", candidate.reference, candidate.text),
        &mut files,
    )?;
    write_booth_pack_file(
        &pack_dir,
        "vmix/setup.md",
        &vmix_booth_setup(candidate, &vmix_config),
        &mut files,
    )?;
    write_booth_pack_file(
        &pack_dir,
        "README.md",
        &booth_pack_readme(candidate, &vmix_config),
        &mut files,
    )?;
    Ok(BoothPackExportDto {
        path: pack_dir.display().to_string(),
        generated_at_ms: now_ms(),
        files,
    })
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn vault_store_secret(label: String, secret: String) -> Result<(), String> {
    crate::vault::store_secret(&label, &secret)
}

#[tauri::command]
pub fn vault_read_secret(label: String) -> Result<String, String> {
    crate::vault::read_secret(&label)
}

#[tauri::command]
pub fn vault_delete_secret(label: String) -> Result<(), String> {
    crate::vault::delete_secret(&label)
}

// ---------------------------------------------------------------------------
// Plugin manifest
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn verify_plugin_manifest(
    manifest_path: String,
    trusted_key_ids: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<PluginVerificationResultDto, String> {
    // Read manifest JSON from disk
    let manifest_json = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Cannot read manifest file '{}': {e}", manifest_path))?;
    let signed: aletheia_ops::SignedPluginManifest =
        serde_json::from_str(&manifest_json).map_err(|e| format!("Invalid manifest JSON: {e}"))?;

    // Merge caller-supplied keys with keys stored in the trust registry.
    let mut all_trusted = trusted_key_ids;
    if let Ok(store) = state.lock_store() {
        if let Ok(plugins) = store.list_trusted_plugins() {
            for p in plugins {
                if !all_trusted.contains(&p.key_id) {
                    all_trusted.push(p.key_id);
                }
            }
        }
    }

    match verify_signed_plugin_manifest(&signed, &all_trusted) {
        Ok(m) => {
            let dto = verified_manifest_to_dto(m);
            Ok(PluginVerificationResultDto {
                state: "verified".to_string(),
                detail: "Plugin manifest signature verified.".to_string(),
                manifest: Some(dto),
            })
        }
        Err(e) => Ok(PluginVerificationResultDto {
            state: "rejected".to_string(),
            detail: e.to_string(),
            manifest: None,
        }),
    }
}

#[tauri::command]
pub fn enable_plugin_manifest(
    manifest_path: String,
    trusted_key_ids: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<PluginVerificationResultDto, String> {
    // Verify first — only trust manifests with valid signatures.
    let manifest_json = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Cannot read manifest file '{}': {e}", manifest_path))?;
    let signed: aletheia_ops::SignedPluginManifest =
        serde_json::from_str(&manifest_json).map_err(|e| format!("Invalid manifest JSON: {e}"))?;

    let mut all_trusted = trusted_key_ids;
    {
        if let Ok(store) = state.lock_store() {
            if let Ok(plugins) = store.list_trusted_plugins() {
                for p in plugins {
                    if !all_trusted.contains(&p.key_id) {
                        all_trusted.push(p.key_id);
                    }
                }
            }
        }
    }

    // Capture payload capabilities before moving `signed`
    let payload_capabilities = signed.payload.capabilities.clone();

    let verified = match verify_signed_plugin_manifest(&signed, &all_trusted) {
        Ok(m) => m,
        Err(e) => {
            return Ok(PluginVerificationResultDto {
                state: "rejected".to_string(),
                detail: format!("Signature invalid — plugin not enabled: {e}"),
                manifest: None,
            });
        }
    };

    // Persist to the trusted_plugins registry.
    let capabilities_json =
        serde_json::to_string(&payload_capabilities).map_err(|e| e.to_string())?;
    {
        let store = state.lock_store()?;
        store
            .upsert_trusted_plugin(&aletheia_store::TrustedPluginRecord {
                id: verified.id.clone(),
                name: verified.name.clone(),
                version: verified.version.clone(),
                key_id: verified.key_id.clone(),
                digest: verified.digest_sha256.clone(),
                capabilities_json,
                enabled: true,
                trusted_at_ms: now_ms(),
            })
            .map_err(|e| e.to_string())?;
    }

    let dto = verified_manifest_to_dto(verified);
    Ok(PluginVerificationResultDto {
        state: "enabled".to_string(),
        detail: "Plugin manifest verified and added to the trust registry.".to_string(),
        manifest: Some(dto),
    })
}

// ---------------------------------------------------------------------------
// Service profiles
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_service_profile(
    profile: ServiceProfileDto,
    state: State<'_, DesktopState>,
) -> Result<ServiceProfileDto, String> {
    let languages_json = serde_json::to_string(&profile.languages).map_err(|e| e.to_string())?;
    let profile_id = profile.id.clone();
    let store = state.lock_store()?;
    store
        .upsert_service_profile(&ServiceProfileRecord {
            id: profile.id,
            name: profile.name,
            languages_json,
            output_policy: profile.output_policy,
            is_active: profile.is_active,
            created_at_ms: profile.created_at_ms,
            updated_at_ms: now_ms(),
        })
        .map_err(|e| e.to_string())?;
    let record = store
        .list_service_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == profile_id)
        .ok_or_else(|| format!("Profile '{}' not found after save", profile_id))?;
    service_profile_to_dto(&record)
}

#[tauri::command]
pub fn list_service_profiles(
    state: State<'_, DesktopState>,
) -> Result<Vec<ServiceProfileDto>, String> {
    let store = state.lock_store()?;
    let records = store.list_service_profiles().map_err(|e| e.to_string())?;
    records.iter().map(service_profile_to_dto).collect()
}

#[tauri::command]
pub fn set_active_service_profile(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<ServiceProfileDto, String> {
    let store = state.lock_store()?;
    store
        .set_active_service_profile(&id)
        .map_err(|e| e.to_string())?;
    let record = store
        .list_service_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Profile '{}' not found", id))?;
    service_profile_to_dto(&record)
}

#[tauri::command]
pub fn delete_service_profile(id: String, state: State<'_, DesktopState>) -> Result<bool, String> {
    let store = state.lock_store()?;
    store
        .delete_service_profile(&id)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Trusted plugins
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_trusted_plugins(
    state: State<'_, DesktopState>,
) -> Result<Vec<TrustedPluginDto>, String> {
    let store = state.lock_store()?;
    let records = store.list_trusted_plugins().map_err(|e| e.to_string())?;
    records
        .into_iter()
        .map(trusted_plugin_record_to_dto)
        .collect()
}

#[tauri::command]
pub fn revoke_trusted_plugin(id: String, state: State<'_, DesktopState>) -> Result<bool, String> {
    let store = state.lock_store()?;
    store
        .revoke_trusted_plugin(&id)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Calibration
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn record_calibration_sample(
    language: String,
    transcript_text: String,
    expected_ref: Option<String>,
    outcome: String,
    detected_ref: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let store = state.lock_store()?;
    store
        .insert_calibration_sample(&CalibrationSampleRecord {
            id: 0,
            language: if language.is_empty() {
                "en".to_string()
            } else {
                language.clone()
            },
            transcript_text: transcript_text.clone(),
            expected_ref: expected_ref.clone(),
            outcome: outcome.clone(),
            detected_ref: detected_ref.clone(),
            recorded_at_ms: now_ms(),
        })
        .map_err(|e| e.to_string())?;

    let normalized_outcome = outcome.trim().to_ascii_lowercase();
    let learned_reference = match normalized_outcome.as_str() {
        "confirmed" => detected_ref.as_deref(),
        "corrected" => expected_ref.as_deref(),
        _ => None,
    };
    if let Some(reference) = learned_reference {
        let translation_id = state
            .lock_runtime()
            .ok()
            .map(|runtime| runtime.live.translation.to_ascii_lowercase())
            .filter(|translation| !translation.trim().is_empty())
            .unwrap_or_else(|| "kjv".to_string());
        let _ = store.upsert_learned_scripture_phrase(
            &transcript_text,
            reference,
            &translation_id,
            &language,
            "operator-calibration",
            now_ms(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn get_calibration_report(
    state: State<'_, DesktopState>,
) -> Result<CalibrationReportDto, String> {
    let store = state.lock_store()?;
    let (confirmed, corrected, rejected, total) =
        store.calibration_summary().map_err(|e| e.to_string())?;
    let precision = if total >= 5 && (confirmed + corrected) > 0 {
        Some(((confirmed as f64 / (confirmed + corrected) as f64) * 100.0).round() as u8)
    } else {
        None
    };
    Ok(CalibrationReportDto {
        confirmed,
        corrected,
        rejected,
        total,
        precision,
    })
}

// ---------------------------------------------------------------------------
// Audio capture
// ---------------------------------------------------------------------------

/// Runs the unified scripture pipeline against a fresh STT segment, applies
/// the confidence policy and operating-mode gate, persists the resulting
/// candidate and emits `aletheia://scripture-candidate` for the operator UI.
///
/// All errors are swallowed so a detection-side failure can never silence the
/// capture thread. The architectural-vision rule "wrong scripture is worse
/// than no scripture" governs every gating decision: in `Manual` mode every
/// non-Ignore decision becomes `RequireApproval`; in `Assisted` an `Open`
/// decision is downgraded to `Prepare`; only `Auto`/`Rehearsal` honour `Open`
/// literally.
fn run_live_detection(handle: &AppHandle, segment: &TranscriptSegmentDto) {
    let state = handle.state::<DesktopState>();

    // Read all runtime knobs in a single short-lived lock so the writer
    // mutex is free for downstream persistence + audit. If the runtime is
    // contended we silently skip — better to emit nothing than to block the
    // capture loop.
    let (
        mode,
        translation_id,
        _offline_mode_enabled,
        destinations_armed,
        operator_name,
        translation_packs,
    ) = {
        let Ok(runtime) = state.lock_runtime() else {
            return;
        };
        let mode = OperatingMode::from_str(&runtime.operating_mode).unwrap_or_default();
        let translation = if runtime.live.translation.is_empty() {
            "kjv".to_string()
        } else {
            runtime.live.translation.to_ascii_lowercase()
        };
        (
            mode,
            translation,
            runtime.offline_mode_enabled,
            runtime.destinations_armed,
            runtime.operator_name.clone(),
            runtime.translation_packs.clone(),
        )
    };

    // Sliding window: concatenate the last 3 transcript segments (newest-first)
    // so paraphrases that cross segment boundaries — common with whisper —
    // still anchor against the whole utterance, not just the trailing slice.
    // The current segment is guaranteed to be in `live_transcript` because the
    // capture loop pushes before invoking detection.
    let mut window_segments = state.snapshot_live_transcript();
    window_segments.truncate(3);
    let window_text: String = if window_segments.is_empty() {
        segment.text.clone()
    } else {
        let mut joined: Vec<String> = window_segments
            .iter()
            .rev()
            .map(|s| s.text.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !joined.iter().any(|s| s == segment.text.trim()) {
            joined.push(segment.text.trim().to_string());
        }
        joined.join(" ")
    };

    // Snapshot the recent-history buffer so the ranker doesn't hold the lock
    // across the (read-only) tier walk. Cheap clone — at most 8 short strings.
    let recent_snapshot = state
        .recent_history
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();

    // Snapshot the per-translation calibration if present. Identity-equivalent
    // until enough operator samples have been recorded.
    let calibration_snapshot = state
        .calibrations
        .lock()
        .ok()
        .and_then(|g| g.get(&translation_id).cloned());

    // Gap 4: rank against a read-only WAL connection so the writer-locked
    // store mutex is not held during the (potentially expensive) tier walk.
    // Fast lane order:
    //   1. Context-follow-up commands (`next verse`, `continue`, `go to verse 20`)
    //   2. Short query variants of the current segment for mid-verse quotes
    //   3. Wider rolling transcript window for paraphrase/context matching
    let resolved = match aletheia_store::AletheiaStore::open_read_only(&state.database_path) {
        Ok(conn) => {
            let read_store = aletheia_store::AletheiaStore::from_read_only_connection(conn);

            if let Some((book, chapter, verse)) = parse_first_explicit_reference(&segment.text) {
                resolve_reference_direct(
                    &read_store,
                    &book,
                    chapter,
                    verse,
                    &translation_id,
                    "Exact voice reference",
                )
            } else if let Some(current) = current_live_context(&state) {
                if let Some((book, chapter, verse, follow_translation)) =
                    resolve_follow_up_verse_command(&read_store, &segment.text, &current)
                {
                    if let Some(direct) = resolve_reference_direct(
                        &read_store,
                        &book,
                        chapter,
                        verse,
                        &follow_translation,
                        "Context follow-up",
                    ) {
                        Some(direct)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
            .or_else(|| {
                let priors = crate::scripture_search::LiveRankingPriors::from_state(
                    &translation_packs,
                    &state.cross_encoder,
                    &state.cross_ref_graph,
                    &recent_snapshot,
                    calibration_snapshot.as_ref(),
                );

                let queries = build_query_variants(&segment.text, &window_text);
                let local_best = queries
                    .iter()
                    .filter_map(|query| {
                        crate::scripture_search::rank_and_resolve_for_live_extended(
                            query,
                            &translation_id,
                            &read_store,
                            &priors,
                        )
                    })
                    .max_by(|left, right| {
                        resolved_score(left)
                            .partial_cmp(&resolved_score(right))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                let should_try_cloud = local_best
                    .as_ref()
                    .map(|result| resolved_score(result) < 0.78)
                    .unwrap_or(true);
                if !should_try_cloud {
                    return local_best;
                }

                let cloud_key = cloud_ai_key_if_allowed(&state)?;
                let cloud_best = queries
                    .into_iter()
                    .filter_map(|query| {
                        crate::scripture_search::cloud_rank_and_resolve(
                            &query,
                            &translation_id,
                            &read_store,
                            &cloud_key,
                        )
                    })
                    .max_by(|left, right| {
                        resolved_score(left)
                            .partial_cmp(&resolved_score(right))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                match (local_best, cloud_best) {
                    (Some(local), Some(cloud))
                        if resolved_score(&cloud) > resolved_score(&local) =>
                    {
                        Some(cloud)
                    }
                    (Some(local), _) => Some(local),
                    (None, cloud) => cloud,
                }
            })
        }
        Err(_) => return,
    };
    let Some((ranked, top_dto)) = resolved else {
        return;
    };
    if ranked.is_empty() {
        return;
    }

    let policy = ConfidencePolicy::default();
    let safety_decision = policy.evaluate_auto_open(&ranked);
    let final_decision = mode.gate(safety_decision);
    if matches!(final_decision, AutoOpenDecision::Ignore) {
        return;
    }

    let top = &ranked[0];
    let bucket = ConfidenceBucket::from_score(top.score);
    let bucket_str = match bucket {
        ConfidenceBucket::Certain => "certain",
        ConfidenceBucket::Strong => "strong",
        ConfidenceBucket::Likely => "likely",
        ConfidenceBucket::Unsafe => "unsafe",
    };
    let status_str = match final_decision {
        AutoOpenDecision::Open => "open",
        AutoOpenDecision::Prepare => "preview",
        AutoOpenDecision::RequireApproval => "approval",
        AutoOpenDecision::Ignore => return,
    };
    let reason = top
        .tiers
        .iter()
        .map(|t| format!("{t:?}"))
        .collect::<Vec<_>>()
        .join(",");

    let now = now_ms();
    let session_id = format!("service-{}", crate::utc_date_string(now));
    let candidate_id = format!("cand-{}-{}", segment.id, now);
    let reference_for_dto = top_dto.reference.clone();
    let verse_text = top_dto.snippet.clone();

    let dto = LiveScriptureCandidateDto {
        id: candidate_id.clone(),
        session_id: session_id.clone(),
        segment_id: segment.id.clone(),
        reference: reference_for_dto.clone(),
        translation_id: translation_id.clone(),
        language: segment.language.clone(),
        score: top.score,
        bucket: bucket_str.to_string(),
        status: status_str.to_string(),
        reason: reason.clone(),
        verse_text: verse_text.clone(),
        created_at_ms: now,
    };

    if let Ok(mut rt) = state.lock_runtime() {
        rt.preview = ScriptureCandidateDto {
            id: candidate_id.clone(),
            reference: reference_for_dto.clone(),
            translation: translation_id.to_uppercase(),
            language: dto.language.clone(),
            text: verse_text.clone(),
            confidence: ((top.score * 100.0).round() as i32).clamp(0, 100) as u8,
            source: "Live STT".to_string(),
            reason: reason.clone(),
            status: status_str.to_string(),
        };
    }
    if let Ok(mut hist) = state.recent_history.lock() {
        hist.record(reference_for_dto.clone());
    }

    // Persist for the operator queue. Persistence failure is non-fatal —
    // the live emit still reaches the UI.
    if let Ok(store) = state.lock_store() {
        let _ = store.insert_scripture_candidate(&aletheia_store::ScriptureCandidateRecord {
            id: candidate_id.clone(),
            session_id: session_id.clone(),
            reference: reference_for_dto.clone(),
            translation_id: translation_id.clone(),
            language: dto.language.clone(),
            score: top.score as f64,
            bucket: bucket_str.to_string(),
            status: status_str.to_string(),
            reason: reason.clone(),
            created_at_ms: now,
        });
    }

    // Gap 2 + Gap 7: when the gated decision is `Open` and the operating
    // mode allows real outputs and the operator has armed destinations,
    // dispatch the candidate straight to vMix. Failures fall through —
    // the operator UI still receives the candidate and can re-send manually.
    if matches!(final_decision, AutoOpenDecision::Open)
        && mode.allows_real_outputs()
        && destinations_armed
    {
        let candidate_dto = ScriptureCandidateDto {
            id: candidate_id.clone(),
            reference: reference_for_dto.clone(),
            translation: translation_id.to_uppercase(),
            language: dto.language.clone(),
            text: verse_text.clone(),
            confidence: ((top.score * 100.0).round() as i32).clamp(0, 100) as u8,
            source: "Live STT".to_string(),
            reason: reason.clone(),
            status: "live".to_string(),
        };
        if let Ok(scene) = scene_from_candidate(&candidate_dto) {
            let send_ok = match state.lock_vmix() {
                Ok(mut adapter) => adapter.send_live(&scene).is_ok(),
                Err(_) => false,
            };
            if send_ok {
                if let Ok(mut rt) = state.lock_runtime() {
                    rt.live = candidate_dto.clone();
                }
                // Stage C feedback: this reference is now "in flight" — feed
                // it into the recent-history buffer so subsequent candidates
                // graph-adjacent to it earn an upward prior.
                if let Ok(mut hist) = state.recent_history.lock() {
                    hist.record(reference_for_dto.clone());
                }
                let _ = state.persist_session();
                let _ = record_integration_event_state(
                    &state,
                    "vmix-main",
                    "info",
                    "live.sent",
                    &format!("auto-sent: {reference_for_dto}"),
                );
                let _ = record_audit_state(
                    &state,
                    AuditAction::LiveOutputSent,
                    &operator_name,
                    &format!(
                        "Auto-sent: {reference_for_dto} (bucket={bucket_str}, score={:.2})",
                        top.score
                    ),
                );
                let _ = handle.emit(
                    "aletheia://live-updated",
                    &serde_json::json!({
                        "reference": reference_for_dto,
                        "translation": translation_id.to_uppercase(),
                        "auditCount": state.audit_count().unwrap_or(0),
                    }),
                );
            }
        }
    }

    let _ = handle.emit("aletheia://scripture-candidate", &dto);
}

fn classify_stt_model(path: &Path) -> (Option<u32>, Option<String>, Option<String>) {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let size_mb = std::fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.len() / (1024 * 1024)) as u32);

    let quality = if filename.contains("large") {
        "Whisper large"
    } else if filename.contains("medium") {
        "Whisper medium"
    } else if filename.contains("small") {
        if filename.contains(".en") || filename.contains("-en-") {
            "Whisper small.en"
        } else {
            "Whisper small"
        }
    } else if filename.contains("base") {
        "Whisper base"
    } else if filename.contains("tiny") {
        "Whisper tiny"
    } else {
        "Whisper ggml"
    };

    let warning = match (filename.as_str(), size_mb) {
        (name, Some(size)) if name.contains("small") && size < 250 => Some(
            "This file is labelled small but is smaller than a real Whisper small model. Accuracy may be poor.".to_string(),
        ),
        (name, _) if name.contains("tiny") || name.contains("base") => Some(
            "This model is usable for testing but may miss Nigerian accents and fast sermon speech. Use small.en or better for production.".to_string(),
        ),
        _ => None,
    };

    (size_mb, Some(quality.to_string()), warning)
}

fn stt_status_from_state(state: &DesktopState, load_error: Option<String>) -> SttStatusDto {
    let asset_root = offline_asset_root(state).ok();
    let loaded_path = state.stt_adapter.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .map(|adapter| adapter.model_path().to_path_buf())
    });
    let resolved_path = loaded_path.clone().or_else(|| {
        asset_root
            .as_ref()
            .and_then(|root| find_stt_model_path(root).ok())
    });

    let (model_size_mb, model_quality, model_warning) = resolved_path
        .as_deref()
        .map(classify_stt_model)
        .unwrap_or((None, None, None));

    SttStatusDto {
        model_loaded: loaded_path.is_some(),
        model_path: resolved_path
            .as_ref()
            .map(|path| path.display().to_string()),
        model_filename: resolved_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string),
        model_size_mb,
        model_quality,
        model_warning,
        asset_root: asset_root.as_ref().map(|path| path.display().to_string()),
        load_error,
    }
}

#[tauri::command]
pub fn get_stt_status(state: State<'_, DesktopState>) -> Result<SttStatusDto, String> {
    Ok(stt_status_from_state(&state, None))
}

#[tauri::command]
pub fn get_stt_latency_profile(
    state: State<'_, DesktopState>,
) -> Result<SttLatencyProfileDto, String> {
    const TARGET_MS: u32 = 2_000;
    let mut samples: Vec<u32> = state
        .snapshot_live_transcript()
        .into_iter()
        .map(|segment| segment.latency_ms)
        .filter(|latency| *latency > 0)
        .collect();
    samples.sort_unstable();
    let sample_count = samples
        .len()
        .min(usize::try_from(u32::MAX).unwrap_or(usize::MAX)) as u32;
    let latest_ms = state
        .snapshot_live_transcript()
        .into_iter()
        .find_map(|segment| (segment.latency_ms > 0).then_some(segment.latency_ms));
    let average_ms = if samples.is_empty() {
        None
    } else {
        Some(
            (samples.iter().map(|value| u64::from(*value)).sum::<u64>()
                / u64::try_from(samples.len()).unwrap_or(1))
            .min(u64::from(u32::MAX)) as u32,
        )
    };
    let percentile = |values: &[u32], percentile: f32| -> Option<u32> {
        if values.is_empty() {
            return None;
        }
        let index = ((values.len() - 1) as f32 * percentile).round() as usize;
        values.get(index).copied()
    };
    let p50_ms = percentile(&samples, 0.50);
    let p95_ms = percentile(&samples, 0.95);
    let fastest_ms = samples.first().copied();
    let slowest_ms = samples.last().copied();
    let state_value = match p95_ms {
        None => "pending",
        Some(p95) if p95 <= TARGET_MS => "ready",
        Some(p95) if p95 <= TARGET_MS.saturating_mul(2) => "degraded",
        Some(_) => "blocked",
    };
    let detail = match (sample_count, p95_ms) {
        (0, _) => {
            "No STT latency samples yet. Start capture and speak a scripture command.".to_string()
        }
        (_, Some(p95)) if p95 <= TARGET_MS => {
            format!("P95 STT latency is {p95} ms, within the {TARGET_MS} ms target.")
        }
        (_, Some(p95)) => format!("P95 STT latency is {p95} ms, above the {TARGET_MS} ms target."),
        _ => "Latency samples unavailable.".to_string(),
    };

    Ok(SttLatencyProfileDto {
        sample_count,
        latest_ms,
        average_ms,
        p50_ms,
        p95_ms,
        fastest_ms,
        slowest_ms,
        target_ms: TARGET_MS,
        state: state_value.to_string(),
        detail,
        checked_at_ms: now_ms(),
    })
}

#[tauri::command]
pub fn reload_stt_model(state: State<'_, DesktopState>) -> Result<SttStatusDto, String> {
    let asset_root = offline_asset_root(&state)?;
    let model_path = match find_stt_model_path(&asset_root) {
        Ok(path) => path,
        Err(error) => return Ok(stt_status_from_state(&state, Some(error))),
    };

    match OfflineSttAdapter::load(&model_path) {
        Ok(adapter) => {
            let mut guard = state
                .stt_adapter
                .lock()
                .map_err(|_| "stt adapter lock".to_string())?;
            *guard = Some(adapter);
            drop(guard);
            Ok(stt_status_from_state(&state, None))
        }
        Err(error) => Ok(stt_status_from_state(&state, Some(error.to_string()))),
    }
}

#[tauri::command]
pub fn start_audio_capture(
    language_hint: Option<String>,
    device_name: Option<String>,
    mode: Option<String>,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<String, String> {
    fn emit_capture_health(
        app: &AppHandle,
        state: &str,
        detail: impl Into<String>,
        device_name: Option<String>,
    ) {
        let _ = app.emit(
            "aletheia://capture-health",
            CaptureHealthDto {
                state: state.to_string(),
                detail: detail.into(),
                device_name,
                checked_at_ms: now_ms(),
            },
        );
    }

    // Check if already running or already starting.
    {
        let shutdown = state.capture_shutdown.lock().map_err(|_| "shutdown lock")?;
        if shutdown.is_some() {
            return Ok("already-running".to_string());
        }
    }
    {
        let mut starting = state
            .capture_starting
            .lock()
            .map_err(|_| "capture starting lock".to_string())?;
        if *starting {
            return Ok("already-running".to_string());
        }
        *starting = true;
    }
    emit_capture_health(
        &app,
        "starting",
        "Starting always-on scripture command listener.",
        device_name.clone(),
    );
    // Resolve the model path so the frontend can display which model is loaded.
    // If no Whisper model file exists, refuse to start and surface a clear,
    // operator-actionable error rather than silently capturing audio whose
    // transcripts will never appear.
    let resolved_model = offline_asset_root(&state)
        .ok()
        .and_then(|root| find_stt_model_path(&root).ok());
    let model_path = match resolved_model {
        Some(path) => path.display().to_string(),
        None => {
            let detail = "No Whisper STT model found. Place a ggml-*.bin model file under the offline assets folder (or your Downloads folder) and try again.".to_string();
            if let Ok(mut starting) = state.capture_starting.lock() {
                *starting = false;
            }
            emit_capture_health(&app, "missingModel", detail.clone(), device_name.clone());
            let _ = app.emit("aletheia://capture-error", &detail);
            return Err(detail);
        }
    };
    {
        // Block capture start when the STT adapter has not been loaded yet —
        // otherwise the worker thread will spin without ever emitting transcripts.
        let mut guard = state.stt_adapter.lock().map_err(|_| "stt adapter lock")?;
        if guard.is_none() {
            match OfflineSttAdapter::load(PathBuf::from(&model_path)) {
                Ok(adapter) => {
                    *guard = Some(adapter);
                }
                Err(error) => {
                    let detail =
                        format!("Whisper STT adapter could not load {model_path}: {error}");
                    if let Ok(mut starting) = state.capture_starting.lock() {
                        *starting = false;
                    }
                    emit_capture_health(&app, "missingModel", detail.clone(), device_name.clone());
                    let _ = app.emit("aletheia://capture-error", &detail);
                    return Err(detail);
                }
            }
        }
    }

    let stt_adapter = state.stt_adapter.clone();
    let live_transcript = state.live_transcript.clone();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::sync_channel::<()>(1);

    let handle = app.clone();
    let spawn_device = device_name.clone();
    let capture_mode = mode
        .as_deref()
        .map(str::trim)
        .unwrap_or("transcript")
        .to_ascii_lowercase();
    // Spawn the capture + STT inference thread.
    std::thread::spawn(move || {
        let is_command_mode = capture_mode == "command";
        log::info!(
            "[capture] starting mode={} device={}",
            capture_mode,
            spawn_device.as_deref().unwrap_or("default")
        );
        let config = CaptureConfig {
            device_name: spawn_device.clone(),
            chunk_duration_ms: if is_command_mode { 700 } else { 4_000 },
            channel_capacity: if is_command_mode { 2 } else { 4 },
            ..CaptureConfig::default()
        };
        let (capture, mut chunk_rx) = match AudioCapture::start(config) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("[capture] failed to start: {e}");
                if let Ok(mut guard) = handle.state::<DesktopState>().capture_shutdown.lock() {
                    *guard = None;
                }
                if let Ok(mut starting) = handle.state::<DesktopState>().capture_starting.lock() {
                    *starting = false;
                }
                let _ = handle.emit(
                    "aletheia://capture-error",
                    &format!(
                        "Microphone capture could not start: {e}. Check Windows microphone permission for Aletheia and confirm the chosen input device is connected."
                    ),
                );
                let _ = handle.emit("aletheia://capture-stopped", ());
                return;
            }
        };

        if let Ok(mut guard) = handle.state::<DesktopState>().capture_shutdown.lock() {
            *guard = Some(shutdown_tx);
        }
        if let Ok(mut starting) = handle.state::<DesktopState>().capture_starting.lock() {
            *starting = false;
        }
        let selected_device = spawn_device.clone();
        emit_capture_health(
            &handle,
            "listening",
            format!(
                "Listening on {}.",
                selected_device
                    .clone()
                    .unwrap_or_else(|| "Default Microphone".to_string())
            ),
            selected_device.clone(),
        );
        let _ = handle.emit(
            "aletheia://capture-started",
            CaptureHealthDto {
                state: "listening".to_string(),
                detail: "Native audio capture active.".to_string(),
                device_name: selected_device.clone(),
                checked_at_ms: now_ms(),
            },
        );

        let mut seq: u64 = 0;
        let started_at_ms = now_ms();
        let mut first_chunk_received = false;
        let mut quiet_chunks: u32 = 0;
        let mut low_signal_alerted = false;
        let mut empty_transcript_chunks: u32 = 0;
        let mut empty_transcript_alerted = false;
        let mut level_log_samples: u32 = 0;
        let mut stt_window: Vec<f32> = Vec::new();
        let mut last_emitted_text = String::new();
        let mut last_stt_attempt_ms: u64 = 0;
        loop {
            // Check shutdown.
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            let chunk = match chunk_rx.try_recv() {
                Ok(c) => c,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    if !first_chunk_received && now_ms().saturating_sub(started_at_ms) > 2_500 {
                        emit_capture_health(
                            &handle,
                            "micUnavailable",
                            "Microphone stream opened but no audio frames arrived. Check the Windows default input device, microphone privacy, and whether another application has exclusive control of the mic.",
                            selected_device.clone(),
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    continue;
                }
                Err(_) => break,
            };
            if !first_chunk_received {
                first_chunk_received = true;
                log::info!(
                    "[capture] first audio chunk received after {} ms",
                    now_ms().saturating_sub(started_at_ms)
                );
                emit_capture_health(
                    &handle,
                    "framesReceived",
                    "Audio frames received from microphone.",
                    selected_device.clone(),
                );
            }

            let (rms, peak, level, peak_level, speech_detected) =
                summarize_audio_levels(&chunk.samples);
            if level_log_samples < 6 {
                log::info!(
                    "[capture] levels rms={:.5} peak={:.5} level={:.2} peak_level={:.2} speech_detected={}",
                    rms,
                    peak,
                    level,
                    peak_level,
                    speech_detected
                );
                level_log_samples += 1;
            }
            let _ = handle.emit(
                "aletheia://audio-level",
                AudioLevelDto {
                    level,
                    peak_level,
                    speech_detected,
                    checked_at_ms: chunk.sealed_at_ms,
                    rms,
                    peak,
                },
            );
            let signal_present = level >= 0.65 || peak_level >= 1.0 || speech_detected;
            if signal_present {
                quiet_chunks = 0;
                if low_signal_alerted {
                    low_signal_alerted = false;
                    emit_capture_health(
                        &handle,
                        "signalDetected",
                        "Mic signal active. Waiting for recognized words.",
                        selected_device.clone(),
                    );
                }
            } else {
                quiet_chunks = quiet_chunks.saturating_add(1);
                if quiet_chunks >= 6 && !low_signal_alerted {
                    low_signal_alerted = true;
                    emit_capture_health(
                        &handle,
                        "lowSignal",
                        "Audio frames are arriving, but the input level is low. Speak closer, raise the source gain, or verify mixer routing.",
                        selected_device.clone(),
                    );
                }
            }

            // Run STT if model is loaded.
            let lang = language_hint.as_deref();
            let stt_samples: Vec<f32> = if is_command_mode {
                const COMMAND_WINDOW_SAMPLES: usize = 16_000 * 2;
                const COMMAND_MIN_SAMPLES: usize = 16_000;
                const COMMAND_STT_INTERVAL_MS: u64 = 900;
                stt_window.extend_from_slice(&chunk.samples);
                if stt_window.len() > COMMAND_WINDOW_SAMPLES {
                    let excess = stt_window.len() - COMMAND_WINDOW_SAMPLES;
                    stt_window.drain(..excess);
                }
                let now = now_ms();
                if stt_window.len() < COMMAND_MIN_SAMPLES
                    || !signal_present
                    || now.saturating_sub(last_stt_attempt_ms) < COMMAND_STT_INTERVAL_MS
                {
                    continue;
                }
                last_stt_attempt_ms = now;
                stt_window.clone()
            } else {
                chunk.samples.clone()
            };
            let text = if let Ok(guard) = stt_adapter.lock() {
                if let Some(ref adapter) = *guard {
                    match adapter.transcribe(&stt_samples, lang) {
                        Ok(transcript) => transcript.text,
                        Err(e) => {
                            log::warn!("[stt] transcribe error: {e}");
                            continue;
                        }
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let trimmed = text.trim();
            if trimmed.is_empty() {
                if signal_present {
                    empty_transcript_chunks = empty_transcript_chunks.saturating_add(1);
                    if empty_transcript_chunks >= 3 && !empty_transcript_alerted {
                        empty_transcript_alerted = true;
                        emit_capture_health(
                            &handle,
                            "recognitionDegraded",
                            "Audio is reaching Aletheia, but speech is not being recognized. Reload the Whisper model or switch to a clearer microphone input.",
                            selected_device.clone(),
                        );
                    }
                }
                continue;
            }
            let normalized_text = trimmed.to_ascii_lowercase();
            if normalized_text == last_emitted_text {
                continue;
            }
            last_emitted_text = normalized_text;
            empty_transcript_chunks = 0;
            if empty_transcript_alerted {
                empty_transcript_alerted = false;
                emit_capture_health(
                    &handle,
                    "recognized",
                    "Speech recognized.",
                    selected_device.clone(),
                );
            }

            seq += 1;
            let latency_ms = ((chunk.samples.len() as f64 / 16000.0) * 1000.0) as u32;
            let segment = TranscriptSegmentDto {
                id: format!("live-{seq:06}"),
                time: format_clock_time(now_ms()),
                speaker: "Live mic".to_string(),
                language: "English".to_string(),
                text: trimmed.to_string(),
                confidence: 88,
                latency_ms,
            };

            if let Ok(mut q) = live_transcript.lock() {
                q.push_front(segment.clone());
                while q.len() > LIVE_TRANSCRIPT_CAPACITY {
                    q.pop_back();
                }
            }
            let _ = handle.emit("aletheia://transcript-segment", &segment);

            // -- Detection + safety policy + operating-mode gating ---------
            // Wrap in catch_unwind so any tier panic in the detection
            // pipeline cannot tear down the capture thread.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_live_detection(&handle, &segment);
            }));
        }

        drop(capture);
        if let Ok(mut guard) = handle.state::<DesktopState>().capture_shutdown.lock() {
            *guard = None;
        }
        if let Ok(mut starting) = handle.state::<DesktopState>().capture_starting.lock() {
            *starting = false;
        }
        log::info!("[capture] stopped");
        emit_capture_health(
            &handle,
            "idle",
            "Always-on scripture command listener stopped.",
            selected_device,
        );
        let _ = handle.emit("aletheia://capture-stopped", ());
    });

    Ok(model_path)
}

#[tauri::command]
pub fn stop_audio_capture(state: State<'_, DesktopState>) -> Result<(), String> {
    if let Ok(mut starting) = state.capture_starting.lock() {
        *starting = false;
    }
    let mut guard = state.capture_shutdown.lock().map_err(|_| "shutdown lock")?;
    if let Some(tx) = guard.take() {
        let _ = tx.send(());
    }
    Ok(())
}

#[cfg(test)]
mod voice_command_tests {
    use super::*;
    use aletheia_store::{AletheiaStore, TranslationRecord, VerseRecord};

    fn test_store() -> AletheiaStore {
        let store = AletheiaStore::open_memory().expect("memory store");
        store
            .insert_translation(&TranslationRecord {
                id: "kjv".to_string(),
                name: "King James Version".to_string(),
                language: "English".to_string(),
                license: "test".to_string(),
                offline_ready: true,
            })
            .expect("translation");
        for (book, chapter, verse, text) in [
            (
                "Genesis",
                1,
                31,
                "And God saw every thing that he had made.",
            ),
            (
                "Genesis",
                2,
                1,
                "Thus the heavens and the earth were finished.",
            ),
            (
                "Genesis",
                2,
                2,
                "And on the seventh day God ended his work.",
            ),
            (
                "Exodus",
                1,
                1,
                "Now these are the names of the children of Israel.",
            ),
        ] {
            store
                .insert_verse(&VerseRecord {
                    translation_id: "kjv".to_string(),
                    book: book.to_string(),
                    chapter,
                    verse,
                    text: text.to_string(),
                })
                .expect("verse");
        }
        store
    }

    #[test]
    fn explicit_reference_is_detected_before_context_follow_up() {
        assert_eq!(
            parse_first_explicit_reference("Matthew chapter 3 verse 4"),
            Some(("Matthew".to_string(), 3, 4))
        );
        assert_eq!(
            parse_first_explicit_reference("open Ezekiel chapter 1 verse 2"),
            Some(("Ezekiel".to_string(), 1, 2))
        );
    }

    #[test]
    fn contextual_follow_up_still_resolves_without_book_reference() {
        let current = LiveContextReference {
            book: "Ezekiel".to_string(),
            chapter: 1,
            verse: 2,
            translation_id: "kjv".to_string(),
        };
        assert_eq!(
            parse_follow_up_verse_command("go to verse 4", &current),
            Some(("Ezekiel".to_string(), 1, 4, "kjv".to_string()))
        );
    }

    #[test]
    fn explicit_reference_never_degrades_to_context_follow_up() {
        let current = LiveContextReference {
            book: "Ezekiel".to_string(),
            chapter: 1,
            verse: 2,
            translation_id: "kjv".to_string(),
        };
        assert_eq!(
            parse_first_explicit_reference("Matthew chapter 3 verse 4"),
            Some(("Matthew".to_string(), 3, 4))
        );
        assert_eq!(
            parse_follow_up_verse_command("Matthew chapter 3 verse 4", &current),
            None
        );
    }

    #[test]
    fn nigerian_style_references_parse_as_chapter_then_verse() {
        assert_eq!(
            parse_first_explicit_reference("Daniel 3 10"),
            Some(("Daniel".to_string(), 3, 10))
        );
        assert_eq!(
            parse_first_explicit_reference("Acts 11 8"),
            Some(("Acts".to_string(), 11, 8))
        );
        assert_eq!(
            parse_first_explicit_reference("1 Timothy 5 2"),
            Some(("1 Timothy".to_string(), 5, 2))
        );
    }

    #[test]
    fn follow_up_next_crosses_chapter_and_book_boundaries() {
        let store = test_store();
        let current = LiveContextReference {
            book: "Genesis".to_string(),
            chapter: 1,
            verse: 31,
            translation_id: "kjv".to_string(),
        };
        assert_eq!(
            resolve_follow_up_verse_command(&store, "next verse", &current),
            Some(("Genesis".to_string(), 2, 1, "kjv".to_string()))
        );

        let current = LiveContextReference {
            book: "Genesis".to_string(),
            chapter: 2,
            verse: 2,
            translation_id: "kjv".to_string(),
        };
        assert_eq!(
            resolve_follow_up_verse_command(&store, "continue", &current),
            Some(("Exodus".to_string(), 1, 1, "kjv".to_string()))
        );
    }

    #[test]
    fn follow_up_previous_crosses_chapter_boundary() {
        let store = test_store();
        let current = LiveContextReference {
            book: "Genesis".to_string(),
            chapter: 2,
            verse: 1,
            translation_id: "kjv".to_string(),
        };
        assert_eq!(
            resolve_follow_up_verse_command(&store, "take it back", &current),
            Some(("Genesis".to_string(), 1, 31, "kjv".to_string()))
        );
    }
}

fn summarize_audio_levels(samples: &[f32]) -> (f32, f32, f32, f32, bool) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0, 0.0, false);
    }

    let mut squared_sum = 0.0_f32;
    let mut peak = 0.0_f32;
    for &sample in samples {
        let abs = sample.abs();
        squared_sum += sample * sample;
        if abs > peak {
            peak = abs;
        }
    }

    let rms = (squared_sum / samples.len() as f32).sqrt();
    let level = (rms * 320.0).clamp(0.0, 100.0);
    let peak_level = (peak * 100.0).clamp(0.0, 100.0);
    // Laptop microphones and mixer line feeds often arrive with low RMS but
    // usable peaks. Keep the UI honest: frames + low voice should register as
    // speech activity without claiming the device is unavailable.
    let speech_detected = rms >= 0.006 || peak >= 0.035;
    (rms, peak, level, peak_level, speech_detected)
}

/// Imports a full-Bible JSON file (thiagobodruk schema) for the requested
/// translation. Operators use this to load licensed translations like NKJV,
/// NIV, NLT, MSG that cannot be bundled with the installer due to publisher
/// copyright. The file path is supplied by the user via a native file picker
/// in the frontend; we never download Bibles from the network.
#[tauri::command]
pub fn import_bible_translation(
    translation_id: String,
    translation_name: String,
    license: String,
    json_path: String,
    state: State<'_, DesktopState>,
) -> Result<BibleImportResultDto, String> {
    let path = std::path::PathBuf::from(json_path);
    if !path.exists() {
        return Err(format!("Bible file not found: {}", path.display()));
    }
    let store = state.lock_store()?;
    let inserted = import_full_bible_from_json(
        &store,
        translation_id.trim(),
        translation_name.trim(),
        license.trim(),
        &path,
    )?;
    crate::scripture_search::invalidate_translation_cache(translation_id.trim());
    crate::scripture_search::warm_translation_with_path(
        &state.database_path,
        translation_id.trim(),
    );
    Ok(BibleImportResultDto {
        translation_id,
        verses_inserted: inserted as u32,
    })
}

/// Deletes a translation and all its verses. Returns the number of verses
/// removed. Used for re-import workflows when a translation file is corrupt
/// or the operator wants to swap to a different licensed edition.
#[tauri::command]
pub fn delete_bible_translation(
    translation_id: String,
    state: State<'_, DesktopState>,
) -> Result<u32, String> {
    // Bundled translations auto-restore on next launch, so deleting them just
    // wastes ~30s of import time on relaunch. Reject at the boundary so a
    // misbehaving plugin or devtools call cannot kick off that churn.
    const BUNDLED_IDS: &[&str] = &["kjv", "bbe"];
    let id = translation_id.trim();
    if BUNDLED_IDS.contains(&id) {
        return Err(
            "Bundled translations cannot be deleted (they auto-restore on next launch)."
                .to_string(),
        );
    }
    let store = state.lock_store()?;
    store.delete_translation(id).map_err(|e| e.to_string())
}

/// Returns verse counts per translation so the UI can show which Bibles are
/// fully loaded vs. partially seeded.
#[tauri::command]
pub fn list_bible_translations(
    state: State<'_, DesktopState>,
) -> Result<Vec<BibleTranslationStatusDto>, String> {
    let store = state.lock_store()?;
    let known = known_bible_translations();
    let mut out = Vec::with_capacity(known.len());
    for (id, name) in known {
        let verses = store
            .count_verses_for_translation(id)
            .map_err(|e| e.to_string())?;
        out.push(BibleTranslationStatusDto {
            id: (*id).to_string(),
            name: (*name).to_string(),
            verses_loaded: verses as u32,
            full_canon: verses >= 30_000,
        });
    }
    Ok(out)
}

/// Build a list of book-name aliases to try in the SQLite lookup.
/// The DB may have rows under "Psalm" while the UI requests "Psalms"
/// (or vice versa), and case/whitespace variation also slips through.
/// Codex patch #615.
fn verse_book_lookup_candidates(book: &str) -> Vec<String> {
    let raw = book.trim();
    let folded = raw.to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !s.is_empty() && !out.iter().any(|x| x == &s) {
            out.push(s);
        }
    };
    // Title-case primary
    push(raw.to_string());
    match folded.as_str() {
        "psalms" | "psalm" => {
            push("Psalm".to_string());
            push("Psalms".to_string());
        }
        "song" | "song of songs" | "canticles" => push("Song of Solomon".to_string()),
        _ => {}
    }
    // Capitalise each word as a final fallback (e.g. "genesis" -> "Genesis")
    let cap: String = raw
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    push(cap);
    out
}

/// Translation fallback chain — try the requested translation first,
/// then KJV/WEB/BBE which are bundled in the seed data. Codex patch #491.
fn verse_translation_fallbacks(preferred: &str) -> Vec<String> {
    let preferred = preferred.trim().to_ascii_lowercase();
    let mut out: Vec<String> = Vec::new();
    for t in [preferred.as_str(), "kjv", "web", "bbe"] {
        if t.is_empty() || out.iter().any(|x| x == t) {
            continue;
        }
        out.push(t.to_string());
    }
    out
}

#[tauri::command]
pub fn get_bible_chapter(
    translation_id: String,
    book: String,
    chapter: u16,
    state: State<'_, DesktopState>,
) -> Result<Vec<BibleVerseDto>, String> {
    if chapter == 0 {
        return Err("Chapter must be greater than zero.".into());
    }
    if translation_id.trim().is_empty() || book.trim().is_empty() {
        return Err("Translation and book are required.".into());
    }
    let book_candidates = verse_book_lookup_candidates(book.trim());
    let translation_candidates = verse_translation_fallbacks(translation_id.trim());

    let store = state.lock_store()?;
    let mut rows = Vec::new();
    let mut hit_translation = translation_id.trim().to_ascii_lowercase();
    'outer: for candidate_translation in &translation_candidates {
        for candidate_book in &book_candidates {
            let attempt = store
                .chapter_verses(candidate_translation, candidate_book, chapter)
                .map_err(|e| e.to_string())?;
            if !attempt.is_empty() {
                rows = attempt;
                hit_translation = candidate_translation.clone();
                break 'outer;
            }
        }
    }
    let translation_upper = hit_translation.to_uppercase();
    Ok(rows
        .into_iter()
        .map(|v| BibleVerseDto {
            reference: format!("{} {}:{}", v.book, v.chapter, v.verse),
            translation: translation_upper.clone(),
            book: v.book,
            chapter: v.chapter,
            verse: v.verse,
            text: v.text,
        })
        .collect())
}

#[tauri::command]
pub fn get_capture_status(state: State<'_, DesktopState>) -> Result<CaptureStatusDto, String> {
    let running = state
        .capture_shutdown
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false);
    let model_path = offline_asset_root(&state)
        .ok()
        .and_then(|root| find_stt_model_path(&root).ok())
        .map(|p| p.display().to_string());
    Ok(CaptureStatusDto {
        running,
        model_path,
    })
}

fn vector_kb_manifest_path(state: &DesktopState) -> PathBuf {
    std::env::var("ALETHEIA_VECTOR_KB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            state
                .database_path
                .parent()
                .map(|path| path.join("vector-kb"))
                .unwrap_or_else(|| PathBuf::from("vector-kb"))
        })
        .join("manifest.json")
}

fn vector_kb_status_for_state(state: &DesktopState) -> VectorKbStatusDto {
    let manifest_path = vector_kb_manifest_path(state);
    let service_url = std::env::var("ALETHEIA_VECTOR_KB_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:47618".to_string())
        .trim_end_matches('/')
        .to_string();

    let service_online = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .ok()
        .and_then(|client| client.get(format!("{service_url}/health")).send().ok())
        .is_some_and(|response| response.status().is_success());

    let mut indexed_translations = Vec::new();
    let mut total_documents = 0_u32;
    let mut built_at_ms = None;
    let mut manifest_detail = "Vector manifest missing. Build the FAISS index.".to_string();
    let manifest_exists = manifest_path.exists();

    if manifest_exists {
        match std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        {
            Some(manifest) => {
                built_at_ms = manifest.get("builtAtMs").and_then(|v| v.as_u64());
                total_documents = manifest
                    .get("totalDocuments")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    .min(u64::from(u32::MAX)) as u32;
                if let Some(map) = manifest.get("translations").and_then(|v| v.as_object()) {
                    indexed_translations = map.keys().cloned().collect();
                    indexed_translations.sort();
                    if total_documents == 0 {
                        total_documents =
                            map.values()
                                .filter_map(|entry| {
                                    entry
                                        .get("documents")
                                        .or_else(|| entry.get("verses"))
                                        .and_then(|value| value.as_u64())
                                })
                                .sum::<u64>()
                                .min(u64::from(u32::MAX)) as u32;
                    }
                }
                manifest_detail = format!(
                    "{} indexed translations, {} documents.",
                    indexed_translations.len(),
                    total_documents
                );
            }
            None => {
                manifest_detail = "Vector manifest exists but is unreadable.".to_string();
            }
        }
    }

    let state_value = match (manifest_exists, service_online) {
        (true, true) => "ready",
        (true, false) => "index-ready-service-offline",
        (false, true) => "service-online-index-missing",
        (false, false) => "offline",
    };
    let detail = if service_online {
        format!("Vector service online. {manifest_detail}")
    } else {
        format!("Vector service offline. {manifest_detail}")
    };

    VectorKbStatusDto {
        state: state_value.to_string(),
        detail,
        manifest_path: manifest_path.display().to_string(),
        service_url,
        service_online,
        indexed_translations,
        total_documents,
        built_at_ms,
    }
}

#[tauri::command]
pub fn get_vector_kb_status(state: State<'_, DesktopState>) -> Result<VectorKbStatusDto, String> {
    Ok(vector_kb_status_for_state(&state))
}

#[tauri::command]
pub fn classify_voice_command(
    text: String,
    current_reference: Option<String>,
    translation_id: Option<String>,
) -> Result<CommandIntentDto, String> {
    let tid = translation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("kjv")
        .to_ascii_lowercase();
    Ok(classify_voice_command_inner(
        &text,
        current_reference.as_deref(),
        &tid,
    ))
}

fn classify_voice_command_inner(
    text: &str,
    current_reference: Option<&str>,
    tid: &str,
) -> CommandIntentDto {
    let normalized = TranscriptNormalizer.normalize(text);
    let mut references = GrammarReferenceParser.parse_all(&normalized.text);
    references.sort_by(|a, b| {
        b.explicit_verse
            .cmp(&a.explicit_verse)
            .then_with(|| b.confidence.total_cmp(&a.confidence))
    });

    if let Some(reference) = references.first() {
        return CommandIntentDto {
            intent: if reference.explicit_verse {
                "explicit-reference".to_string()
            } else {
                "chapter-reference".to_string()
            },
            reference: Some(reference.as_reference_string()),
            book: Some(reference.book.to_string()),
            chapter: Some(reference.chapter),
            verse: Some(reference.verse_start),
            translation_id: tid.to_string(),
            confidence: reference.confidence,
            needs_disambiguation: reference.needs_disambiguation,
            disambiguation_options: reference
                .disambiguation_options
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            detail: format!(
                "Matched explicit Bible reference via grammar alias '{}'.",
                reference.alias_matched
            ),
        };
    }

    if let Some(current_reference) = current_reference {
        if let Some((book, chapter, verse)) = parse_reference(current_reference) {
            let current = LiveContextReference {
                book,
                chapter,
                verse,
                translation_id: tid.to_string(),
            };
            if let Some((book, chapter, verse, follow_translation)) =
                parse_follow_up_verse_command(text, &current)
            {
                return CommandIntentDto {
                    intent: "context-follow-up".to_string(),
                    reference: Some(format!("{book} {chapter}:{verse}")),
                    book: Some(book),
                    chapter: Some(chapter),
                    verse: Some(verse),
                    translation_id: follow_translation,
                    confidence: 0.94,
                    needs_disambiguation: false,
                    disambiguation_options: Vec::new(),
                    detail: "Resolved next/previous/go-to-verse against current scripture context."
                        .to_string(),
                };
            }
        }
    }

    CommandIntentDto {
        intent: "semantic-query".to_string(),
        reference: None,
        book: None,
        chapter: None,
        verse: None,
        translation_id: tid.to_string(),
        confidence: 0.0,
        needs_disambiguation: false,
        disambiguation_options: Vec::new(),
        detail: "No direct reference or continuation command. Route to quote/paraphrase semantic search.".to_string(),
    }
}

fn known_bible_translations() -> &'static [(&'static str, &'static str)] {
    &[
        ("kjv", "King James Version"),
        ("web", "World English Bible"),
        ("bbe", "Bible in Basic English"),
        ("esv", "English Standard Version"),
        ("nkjv", "New King James Version"),
        ("niv", "New International Version"),
        ("nlt", "New Living Translation"),
        ("csb", "Christian Standard Bible"),
        ("amp", "Amplified Bible"),
        ("tlb", "The Living Bible"),
        ("gnb", "Good News Bible"),
        ("msg", "The Message"),
    ]
}

fn bible_integrity_for_store(
    store: &aletheia_store::AletheiaStore,
) -> Result<Vec<BibleIntegrityTranslationDto>, String> {
    let mut out = Vec::with_capacity(known_bible_translations().len());
    for (id, name) in known_bible_translations() {
        let verses = store
            .count_verses_for_translation(id)
            .map_err(|e| e.to_string())?;
        let full_canon = verses >= 30_000;
        let mut missing_books = Vec::new();
        let mut missing_chapters = Vec::new();

        if verses > 0 {
            for book in BOOKS {
                let mut book_has_any = false;
                for chapter in 1..=book.max_chapter {
                    let mut chapter_has_any = false;
                    for book_candidate in verse_book_lookup_candidates(book.canonical) {
                        if !store
                            .chapter_verses(id, &book_candidate, chapter)
                            .map_err(|e| e.to_string())?
                            .is_empty()
                        {
                            chapter_has_any = true;
                            book_has_any = true;
                            break;
                        }
                    }
                    if !chapter_has_any && full_canon && missing_chapters.len() < 32 {
                        missing_chapters.push(format!("{} {}", book.canonical, chapter));
                    }
                }
                if !book_has_any && missing_books.len() < 16 {
                    missing_books.push(book.canonical.to_string());
                }
            }
        }

        let state = if verses == 0 {
            "missing"
        } else if full_canon && missing_books.is_empty() && missing_chapters.is_empty() {
            "ready"
        } else if full_canon {
            "needs-audit"
        } else {
            "sample"
        };
        let detail = match state {
            "ready" => "Full canon appears available.".to_string(),
            "sample" => format!("{verses} verses loaded; treat as sample pack, not full Bible."),
            "needs-audit" => format!(
                "{verses} verses loaded, but {} books / {} chapters need audit.",
                missing_books.len(),
                missing_chapters.len()
            ),
            _ => "No verses loaded.".to_string(),
        };

        out.push(BibleIntegrityTranslationDto {
            id: (*id).to_string(),
            name: (*name).to_string(),
            verses_loaded: verses.max(0).min(i64::from(u32::MAX)) as u32,
            full_canon,
            missing_books,
            missing_chapters,
            state: state.to_string(),
            detail,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn audit_bible_integrity(
    state: State<'_, DesktopState>,
) -> Result<Vec<BibleIntegrityTranslationDto>, String> {
    let store = state.lock_store()?;
    bible_integrity_for_store(&store)
}

#[tauri::command]
pub fn list_display_outputs(app: AppHandle) -> Result<Vec<DisplayOutputDto>, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window unavailable".to_string())?;
    let primary_name = window
        .primary_monitor()
        .map_err(|error| format!("primary monitor lookup failed: {error}"))?
        .and_then(|monitor| monitor.name().cloned());
    let monitors = window
        .available_monitors()
        .map_err(|error| format!("display output lookup failed: {error}"))?;
    let multi_display = monitors.len() > 1;

    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let name = monitor
                .name()
                .cloned()
                .unwrap_or_else(|| format!("Display {}", index + 1));
            let size = monitor.size();
            let position = monitor.position();
            let name_folded = name.to_ascii_lowercase();
            let is_primary = primary_name.as_ref().is_some_and(|primary| primary == &name);
            let likely_hdmi = name_folded.contains("hdmi")
                || name_folded.contains("tv")
                || name_folded.contains("projector")
                || name_folded.contains("capture")
                || (multi_display && !is_primary);
            let detail = if likely_hdmi {
                "External display/capture path available for HDMI output.".to_string()
            } else if multi_display {
                "Secondary display detected.".to_string()
            } else {
                "Only one display detected. Connect an HDMI output or capture path for hardware rehearsal.".to_string()
            };

            DisplayOutputDto {
                id: format!("display-{index}"),
                name,
                width: size.width,
                height: size.height,
                position_x: position.x,
                position_y: position.y,
                scale_factor: monitor.scale_factor(),
                is_primary,
                likely_hdmi,
                detail,
            }
        })
        .collect())
}

#[tauri::command]
pub fn diagnose_backend(
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<BackendDiagnosticsDto, String> {
    let checked_at_ms = now_ms();
    let store = state.lock_store()?;
    let bibles = bible_integrity_for_store(&store)?;
    drop(store);

    let capture = get_capture_status(state.clone())?;
    let stt = stt_status_from_state(&state, None);
    let vector = vector_kb_status_for_state(&state);
    let displays = list_display_outputs(app).unwrap_or_default();

    let mut issues = Vec::new();
    if !capture.running {
        issues.push("Audio capture is not running.".to_string());
    }
    if !stt.model_loaded {
        issues.push("Offline STT model is not loaded.".to_string());
    }
    if !vector.service_online {
        issues.push("Vector semantic service is offline.".to_string());
    }
    if vector.indexed_translations.is_empty() {
        issues.push("Vector semantic index has no translations ready.".to_string());
    }
    for bible in &bibles {
        if bible.id == "kjv" && !bible.full_canon {
            issues.push(
                "KJV full canon is not loaded; scripture lookup will be incomplete.".to_string(),
            );
        }
        if bible.state == "needs-audit" {
            issues.push(format!(
                "{} needs Bible integrity audit.",
                bible.id.to_uppercase()
            ));
        }
    }
    if !displays.iter().any(|display| display.likely_hdmi) {
        issues.push("No likely HDMI/external display output detected.".to_string());
    }

    let state_value = if issues.is_empty() {
        "ready"
    } else if bibles
        .iter()
        .any(|bible| bible.id == "kjv" && bible.full_canon)
    {
        "degraded"
    } else {
        "blocked"
    };

    Ok(BackendDiagnosticsDto {
        state: state_value.to_string(),
        checked_at_ms,
        database_path: state.database_path.display().to_string(),
        capture,
        stt,
        vector,
        bibles,
        displays,
        issues,
    })
}

fn gate_check(
    id: &str,
    label: &str,
    passed: bool,
    detail: impl Into<String>,
    blocking: bool,
) -> ReleaseGateCheckDto {
    ReleaseGateCheckDto {
        id: id.to_string(),
        label: label.to_string(),
        state: if passed { "pass" } else { "fail" }.to_string(),
        detail: detail.into(),
        blocking,
    }
}

#[tauri::command]
pub fn run_production_release_gate(
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<ProductionReleaseGateDto, String> {
    let diagnostics = diagnose_backend(state, app)?;
    let kjv = diagnostics.bibles.iter().find(|bible| bible.id == "kjv");
    let full_bibles = diagnostics
        .bibles
        .iter()
        .filter(|bible| bible.full_canon)
        .count();
    let sample_exposed = diagnostics
        .bibles
        .iter()
        .any(|bible| bible.state == "sample" && bible.verses_loaded > 0);

    let mut checks = vec![
        gate_check(
            "bible-kjv-full-canon",
            "KJV full Bible integrity",
            kjv.is_some_and(|bible| bible.full_canon && bible.missing_books.is_empty()),
            kjv.map(|bible| bible.detail.clone())
                .unwrap_or_else(|| "KJV translation is missing.".to_string()),
            true,
        ),
        gate_check(
            "bible-translation-coverage",
            "Installed full Bible coverage",
            full_bibles >= 3,
            format!("{full_bibles} full-canon translations available."),
            false,
        ),
        gate_check(
            "partial-translation-policy",
            "Partial translations blocked from full-Bible UX",
            !sample_exposed,
            if sample_exposed {
                "One or more sample translations are installed; UI must label/block them as samples."
                    .to_string()
            } else {
                "No sample translation is exposed as a full Bible.".to_string()
            },
            true,
        ),
        gate_check(
            "stt-model",
            "Offline STT model",
            diagnostics.stt.model_path.is_some(),
            diagnostics
                .stt
                .model_warning
                .clone()
                .or_else(|| diagnostics.stt.model_path.clone())
                .unwrap_or_else(|| "No offline STT model found.".to_string()),
            true,
        ),
        gate_check(
            "capture-runtime",
            "Audio capture runtime",
            diagnostics.capture.running,
            if diagnostics.capture.running {
                "Native capture is running.".to_string()
            } else {
                "Native capture is not running.".to_string()
            },
            false,
        ),
        gate_check(
            "vector-service",
            "Semantic vector service",
            diagnostics.vector.service_online
                && !diagnostics.vector.indexed_translations.is_empty(),
            diagnostics.vector.detail.clone(),
            false,
        ),
        gate_check(
            "display-output",
            "HDMI/external display detection",
            diagnostics
                .displays
                .iter()
                .any(|display| display.likely_hdmi),
            format!("{} display outputs detected.", diagnostics.displays.len()),
            false,
        ),
    ];

    let passed = checks
        .iter()
        .filter(|check| check.state == "pass")
        .count()
        .min(usize::from(u16::MAX)) as u16;
    let total = checks.len().min(usize::from(u16::MAX)) as u16;
    let has_blocking_failure = checks
        .iter()
        .any(|check| check.blocking && check.state != "pass");
    let state_value = if has_blocking_failure {
        "blocked"
    } else if passed == total {
        "ready"
    } else {
        "degraded"
    };
    checks.sort_by_key(|check| (check.state == "pass", !check.blocking, check.id.clone()));

    Ok(ProductionReleaseGateDto {
        state: state_value.to_string(),
        checked_at_ms: now_ms(),
        passed,
        total,
        checks,
    })
}

#[tauri::command]
pub fn run_full_scripture_regression(
    translation_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<ScriptureRegressionReportDto, String> {
    let tid = normalize_regression_translation(translation_id.as_deref());
    let store = aletheia_store::AletheiaStore::open_read_only(&state.database_path)
        .map(aletheia_store::AletheiaStore::from_read_only_connection)
        .or_else(|_| aletheia_store::AletheiaStore::open_file(&state.database_path))
        .map_err(|error| error.to_string())?;
    run_full_scripture_regression_for_store(&store, &tid, None)
}

#[derive(Clone)]
struct ScriptureRegressionJobState {
    job_id: String,
    state: String,
    started_at_ms: u64,
    updated_at_ms: u64,
    cancel_requested: Arc<AtomicBool>,
    report: Option<ScriptureRegressionReportDto>,
    error: Option<String>,
}

static SCRIPTURE_REGRESSION_JOB: OnceLock<StdMutex<Option<ScriptureRegressionJobState>>> =
    OnceLock::new();

fn scripture_regression_job_slot() -> &'static StdMutex<Option<ScriptureRegressionJobState>> {
    SCRIPTURE_REGRESSION_JOB.get_or_init(|| StdMutex::new(None))
}

fn scripture_regression_job_to_dto(job: &ScriptureRegressionJobState) -> ScriptureRegressionJobDto {
    ScriptureRegressionJobDto {
        job_id: job.job_id.clone(),
        state: job.state.clone(),
        started_at_ms: job.started_at_ms,
        updated_at_ms: job.updated_at_ms,
        cancel_requested: job.cancel_requested.load(Ordering::SeqCst),
        report: job.report.clone(),
        error: job.error.clone(),
    }
}

#[tauri::command]
pub fn start_full_scripture_regression_job(
    translation_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<ScriptureRegressionJobDto, String> {
    let mut guard = scripture_regression_job_slot()
        .lock()
        .map_err(|_| "Scripture regression job lock poisoned.".to_string())?;
    if let Some(existing) = guard.as_ref()
        && matches!(existing.state.as_str(), "running" | "cancelling")
    {
        return Ok(scripture_regression_job_to_dto(existing));
    }

    let tid = normalize_regression_translation(translation_id.as_deref());
    let job_id = format!("scripture-regression-{}", now_ms());
    let started_at_ms = now_ms();
    let cancel_requested = Arc::new(AtomicBool::new(false));
    let db_path = state.database_path.clone();
    let job = ScriptureRegressionJobState {
        job_id: job_id.clone(),
        state: "running".to_string(),
        started_at_ms,
        updated_at_ms: started_at_ms,
        cancel_requested: cancel_requested.clone(),
        report: None,
        error: None,
    };
    *guard = Some(job.clone());
    drop(guard);

    std::thread::spawn(move || {
        let result = aletheia_store::AletheiaStore::open_read_only(&db_path)
            .map(aletheia_store::AletheiaStore::from_read_only_connection)
            .or_else(|_| aletheia_store::AletheiaStore::open_file(&db_path))
            .map_err(|error| error.to_string())
            .and_then(|store| {
                run_full_scripture_regression_for_store(&store, &tid, Some(&cancel_requested))
            });
        if let Ok(mut guard) = scripture_regression_job_slot().lock() {
            if let Some(current) = guard.as_mut()
                && current.job_id == job_id
            {
                current.updated_at_ms = now_ms();
                match result {
                    Ok(report) => {
                        current.state = if cancel_requested.load(Ordering::SeqCst) {
                            "cancelled".to_string()
                        } else {
                            "completed".to_string()
                        };
                        current.report = Some(report);
                        current.error = None;
                    }
                    Err(error) => {
                        current.state = if cancel_requested.load(Ordering::SeqCst) {
                            "cancelled".to_string()
                        } else {
                            "failed".to_string()
                        };
                        current.error = Some(error);
                    }
                }
            }
        }
    });

    Ok(scripture_regression_job_to_dto(&job))
}

#[tauri::command]
pub fn get_full_scripture_regression_job() -> Result<Option<ScriptureRegressionJobDto>, String> {
    let guard = scripture_regression_job_slot()
        .lock()
        .map_err(|_| "Scripture regression job lock poisoned.".to_string())?;
    Ok(guard.as_ref().map(scripture_regression_job_to_dto))
}

#[tauri::command]
pub fn cancel_full_scripture_regression_job() -> Result<bool, String> {
    let mut guard = scripture_regression_job_slot()
        .lock()
        .map_err(|_| "Scripture regression job lock poisoned.".to_string())?;
    if let Some(job) = guard.as_mut()
        && job.state == "running"
    {
        job.cancel_requested.store(true, Ordering::SeqCst);
        job.state = "cancelling".to_string();
        job.updated_at_ms = now_ms();
        return Ok(true);
    }
    Ok(false)
}

fn normalize_regression_translation(translation_id: Option<&str>) -> String {
    translation_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("kjv")
        .to_ascii_lowercase()
}

fn run_full_scripture_regression_for_store(
    store: &aletheia_store::AletheiaStore,
    tid: &str,
    cancel_requested: Option<&AtomicBool>,
) -> Result<ScriptureRegressionReportDto, String> {
    let started = Instant::now();
    let checked_at_ms = now_ms();

    let mut books_checked = 0u16;
    let mut chapters_checked = 0u16;
    let mut verses_checked = 0u32;
    let mut direct_lookup_checked = 0u32;
    let mut grammar_checked = 0u32;
    let mut voice_command_checked = 0u32;
    let mut search_path_checked = 0u32;
    let mut partial_quote_checked = 0u32;
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut first_failures: Vec<ScriptureRegressionFailureDto> = Vec::new();

    for book in BOOKS {
        if cancel_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            break;
        }
        books_checked = books_checked.saturating_add(1);
        for chapter in 1..=book.max_chapter {
            if cancel_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                break;
            }
            chapters_checked = chapters_checked.saturating_add(1);
            let records = chapter_records_with_fallbacks(store, book.canonical, chapter, tid);
            if records.is_empty() {
                failed = failed.saturating_add(1);
                push_scripture_regression_failure(
                    &mut first_failures,
                    format!("{} {}:1", book.canonical, chapter),
                    book.canonical.to_string(),
                    chapter,
                    1,
                    None,
                    "Chapter lookup returned no verses.".to_string(),
                );
                continue;
            }

            for record in records {
                if cancel_requested.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    break;
                }
                verses_checked = verses_checked.saturating_add(1);
                let reference_space = format!("{} {} {}", book.canonical, chapter, record.verse);
                let reference_colon = format!("{} {}:{}", book.canonical, chapter, record.verse);
                let direct_record =
                    find_verse_with_fallbacks(store, book.canonical, chapter, record.verse, tid);
                direct_lookup_checked = direct_lookup_checked.saturating_add(1);
                if direct_record.is_none() {
                    failed = failed.saturating_add(1);
                    push_scripture_regression_failure(
                        &mut first_failures,
                        format!("{} {}:{}", record.book, record.chapter, record.verse),
                        book.canonical.to_string(),
                        chapter,
                        record.verse,
                        None,
                        "Direct canonical verse lookup failed.".to_string(),
                    );
                    continue;
                }
                passed = passed.saturating_add(1);

                grammar_checked = grammar_checked.saturating_add(1);
                match parse_first_explicit_reference(&reference_space) {
                    Some((parsed_book, parsed_chapter, parsed_verse)) => {
                        let resolved = find_verse_with_fallbacks(
                            store,
                            &parsed_book,
                            parsed_chapter,
                            parsed_verse,
                            tid,
                        );
                        if resolved.is_some()
                            && parsed_chapter == chapter
                            && parsed_verse == record.verse
                        {
                            passed = passed.saturating_add(1);
                        } else {
                            failed = failed.saturating_add(1);
                            push_scripture_regression_failure(
                                &mut first_failures,
                                reference_space.clone(),
                                book.canonical.to_string(),
                                chapter,
                                record.verse,
                                Some(format!("{parsed_book} {parsed_chapter}:{parsed_verse}")),
                                "Parsed reference did not resolve back to the expected verse."
                                    .to_string(),
                            );
                        }
                    }
                    None => {
                        failed = failed.saturating_add(1);
                        push_scripture_regression_failure(
                            &mut first_failures,
                            reference_space.clone(),
                            book.canonical.to_string(),
                            chapter,
                            record.verse,
                            None,
                            "Grammar parser failed to recognize the canonical spoken reference."
                                .to_string(),
                        );
                    }
                }

                voice_command_checked = voice_command_checked.saturating_add(1);
                let intent = classify_voice_command_inner(&reference_space, None, tid);
                if !intent_matches_expected(&intent, book.canonical, chapter, record.verse) {
                    failed = failed.saturating_add(1);
                    push_scripture_regression_failure(
                        &mut first_failures,
                        reference_space.clone(),
                        book.canonical.to_string(),
                        chapter,
                        record.verse,
                        intent.reference,
                        "Voice command classifier did not return the expected verse.".to_string(),
                    );
                } else {
                    passed = passed.saturating_add(1);
                }

                search_path_checked = search_path_checked.saturating_add(1);
                let search_hits = crate::scripture_search::search_scripture_unified(
                    &reference_space,
                    tid,
                    crate::scripture_search::SearchContext::DashboardOpen,
                    store,
                );
                if !search_hits.first().is_some_and(|hit| {
                    reference_matches_expected(
                        &hit.reference,
                        book.canonical,
                        chapter,
                        record.verse,
                    )
                }) {
                    failed = failed.saturating_add(1);
                    push_scripture_regression_failure(
                        &mut first_failures,
                        reference_space.clone(),
                        book.canonical.to_string(),
                        chapter,
                        record.verse,
                        search_hits.first().map(|hit| hit.reference.clone()),
                        "User-facing scripture search did not return the expected verse first."
                            .to_string(),
                    );
                } else {
                    passed = passed.saturating_add(1);
                }

                if let Some(partial) = partial_quote_probe(&record.text) {
                    partial_quote_checked = partial_quote_checked.saturating_add(1);
                    let partial_hits = crate::scripture_search::search_scripture_unified(
                        &partial,
                        tid,
                        crate::scripture_search::SearchContext::LiveTranscript,
                        store,
                    );
                    if !partial_hits.iter().take(3).any(|hit| {
                        reference_matches_expected(
                            &hit.reference,
                            book.canonical,
                            chapter,
                            record.verse,
                        )
                    }) {
                        failed = failed.saturating_add(1);
                        push_scripture_regression_failure(
                            &mut first_failures,
                            reference_colon,
                            book.canonical.to_string(),
                            chapter,
                            record.verse,
                            partial_hits.first().map(|hit| hit.reference.clone()),
                            format!(
                                "Partial quote probe did not include expected verse in top 3: \"{partial}\""
                            ),
                        );
                    } else {
                        passed = passed.saturating_add(1);
                    }
                }
            }
        }
    }

    Ok(ScriptureRegressionReportDto {
        translation_id: tid.to_string(),
        state: if failed == 0 { "pass" } else { "fail" }.to_string(),
        checked_at_ms,
        duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        books_checked,
        chapters_checked,
        verses_checked,
        direct_lookup_checked,
        grammar_checked,
        voice_command_checked,
        search_path_checked,
        partial_quote_checked,
        passed,
        failed,
        first_failures,
    })
}

fn intent_matches_expected(
    intent: &CommandIntentDto,
    expected_book: &str,
    expected_chapter: u16,
    expected_verse: u16,
) -> bool {
    intent.chapter == Some(expected_chapter)
        && intent.verse == Some(expected_verse)
        && intent
            .book
            .as_deref()
            .is_some_and(|book| book_alias_matches(book, expected_book))
}

fn reference_matches_expected(
    reference: &str,
    expected_book: &str,
    expected_chapter: u16,
    expected_verse: u16,
) -> bool {
    parse_reference(reference).is_some_and(|(book, chapter, verse)| {
        chapter == expected_chapter
            && verse == expected_verse
            && book_alias_matches(&book, expected_book)
    })
}

fn book_alias_matches(actual: &str, expected: &str) -> bool {
    verse_book_lookup_candidates(expected)
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(actual))
}

fn partial_quote_probe(text: &str) -> Option<String> {
    let words: Vec<&str> = text
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\''))
        .filter(|word| word.len() >= 2)
        .collect();
    if words.len() < 8 {
        return None;
    }
    let width = words.len().min(10).max(8);
    let start = words.len().saturating_sub(width) / 2;
    Some(words[start..start + width].join(" "))
}

fn push_scripture_regression_failure(
    failures: &mut Vec<ScriptureRegressionFailureDto>,
    reference: String,
    expected_book: String,
    expected_chapter: u16,
    expected_verse: u16,
    actual_reference: Option<String>,
    detail: String,
) {
    if failures.len() < 100 {
        failures.push(ScriptureRegressionFailureDto {
            reference,
            expected_book,
            expected_chapter,
            expected_verse,
            actual_reference,
            detail,
        });
    }
}

// ---------------------------------------------------------------------------
// Audio device picker
// ---------------------------------------------------------------------------

/// Returns the names of all audio input devices visible to the OS.
/// The frontend uses this list to populate the audio device picker.
#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<String>, String> {
    Ok(list_input_devices())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Operator identity
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_operator_name(name: String, state: State<'_, DesktopState>) -> Result<(), String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("Operator name cannot be empty.".to_string());
    }
    {
        let mut runtime = state.lock_runtime()?;
        runtime.operator_name = trimmed;
    }
    state.persist_session()?;
    Ok(())
}

#[tauri::command]
pub fn get_operator_name(state: State<'_, DesktopState>) -> Result<String, String> {
    Ok(state.operator_name())
}

// ---------------------------------------------------------------------------
// Session persistence
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_session(state: State<'_, DesktopState>) -> Result<(), String> {
    state.persist_session()
}

// ---------------------------------------------------------------------------
// Test connection (per-adapter connectivity probe)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn test_vmix_connection(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_vmix()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "vmix".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn test_obs_connection(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_obs()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn test_propresenter_connection(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_propresenter()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn test_companion_connection(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_companion()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn test_osc_connection(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_osc()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn test_easyworship_connection(
    state: State<'_, DesktopState>,
) -> Result<AdapterDispatchResultDto, String> {
    let adapter = state.lock_easyworship()?;
    let status = adapter.status();
    let (state_str, detail) = output_health_to_state_detail(status.health);
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: state_str,
        detail,
        reference: String::new(),
        audit_count: state.audit_count()?,
    })
}

// ---------------------------------------------------------------------------
// Operator config export / import (booth migration, backup)
// ---------------------------------------------------------------------------

const REDACTED_MARKER: &str = "<redacted>";
const EXPORT_INTEGRATION_IDS: &[&str] = &[
    "vmix-main",
    "obs-main",
    "propresenter-main",
    "companion-main",
    "osc-main",
    "easyworship-main",
];

/// Returns true when the given key looks like it holds a secret value.
/// Matches common credential field names plus `*_key`, `*_secret`, `*_token`
/// suffix patterns so configs like `webhookToken` / `apiKey` / `clientSecret`
/// are caught without an exhaustive allowlist.
fn is_secret_key(lowercase_key: &str) -> bool {
    matches!(
        lowercase_key,
        "password"
            | "passwd"
            | "pwd"
            | "secret"
            | "token"
            | "api_key"
            | "apikey"
            | "auth"
            | "authorization"
            | "bearer"
            | "credential"
            | "credentials"
            | "access_key"
            | "accesskey"
            | "private_key"
            | "privatekey"
    ) || lowercase_key.ends_with("password")
        || lowercase_key.ends_with("_secret")
        || lowercase_key.ends_with("secret")
        || lowercase_key.ends_with("_token")
        || lowercase_key.ends_with("token")
        || lowercase_key.ends_with("_key")
        || lowercase_key.ends_with("apikey")
}

/// Masks credentials embedded in a URL (user:pass@host) and JWT-shaped strings
/// (three base64url segments separated by dots, long enough to be real).
fn scrub_string(s: &str) -> Option<String> {
    // URL-embedded credentials: scheme://user:pass@host/...
    // Only rewrite when both userinfo AND an '@' separator are present.
    if let Some(scheme_end) = s.find("://") {
        let rest = &s[scheme_end + 3..];
        if let Some(at) = rest.find('@') {
            let userinfo = &rest[..at];
            if userinfo.contains(':') {
                let (scheme, _) = s.split_at(scheme_end + 3);
                return Some(format!("{scheme}{REDACTED_MARKER}@{}", &rest[at + 1..]));
            }
        }
    }
    // JWT-shaped: three segments of base64url chars, each ≥10 chars.
    let segs: Vec<&str> = s.split('.').collect();
    if segs.len() == 3
        && segs.iter().all(|seg| {
            seg.len() >= 10
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        return Some(REDACTED_MARKER.to_string());
    }
    None
}

/// Walks `value` mutably and replaces any string field whose key matches a
/// known secret name with a redaction marker. Also scrubs URL-embedded
/// credentials and JWT-shaped strings found anywhere in the tree.
fn redact_secrets_in_place(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let lk = k.to_lowercase();
                if is_secret_key(&lk) && v.is_string() {
                    *v = serde_json::Value::String(REDACTED_MARKER.to_string());
                } else {
                    redact_secrets_in_place(v);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_secrets_in_place(item);
            }
        }
        serde_json::Value::String(s) => {
            if let Some(scrubbed) = scrub_string(s) {
                *s = scrubbed;
            }
        }
        _ => {}
    }
}

/// Strips redacted markers out of an inbound config so we don't overwrite the
/// operator's real credentials with the literal `"<redacted>"` placeholder.
fn drop_redacted_in_place(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(map) = value {
        let keys_to_clear: Vec<String> = map
            .iter()
            .filter_map(|(k, v)| match v {
                serde_json::Value::String(s) if s == REDACTED_MARKER => Some(k.clone()),
                _ => None,
            })
            .collect();
        for k in keys_to_clear {
            map.remove(&k);
        }
        for v in map.values_mut() {
            drop_redacted_in_place(v);
        }
    } else if let serde_json::Value::Array(items) = value {
        for item in items.iter_mut() {
            drop_redacted_in_place(item);
        }
    }
}

#[tauri::command]
pub fn export_operator_config(state: State<'_, DesktopState>) -> Result<String, String> {
    let store = state.lock_store()?;
    let mut integrations = serde_json::Map::new();
    for id in EXPORT_INTEGRATION_IDS {
        if let Some(rec) = store
            .get_integration_config(id)
            .map_err(|e| e.to_string())?
        {
            let mut cfg: serde_json::Value =
                serde_json::from_str(&rec.config_json).unwrap_or(serde_json::Value::Null);
            redact_secrets_in_place(&mut cfg);
            integrations.insert(
                (*id).to_string(),
                serde_json::json!({
                    "kind": rec.kind,
                    "displayName": rec.display_name,
                    "enabled": rec.enabled,
                    "config": cfg,
                }),
            );
        }
    }
    let runtime = state.lock_runtime()?;
    let payload = serde_json::json!({
        "version": 1,
        "exportedAtMs": now_ms(),
        "operatorName": runtime.operator_name,
        "integrations": integrations,
    });
    serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_operator_config(json: String, state: State<'_, DesktopState>) -> Result<(), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("Invalid JSON: {e}"))?;
    let version = parsed.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != 1 {
        return Err(format!("Unsupported config version: {version}"));
    }
    if let Some(name) = parsed.get("operatorName").and_then(|v| v.as_str()) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            let mut runtime = state.lock_runtime()?;
            runtime.operator_name = trimmed.to_string();
        }
    }
    if let Some(integrations) = parsed.get("integrations").and_then(|v| v.as_object()) {
        let store = state.lock_store()?;
        for (id, entry) in integrations {
            // Ignore unknown IDs entirely — an export from a future build or a
            // tampered file shouldn't be able to introduce arbitrary records.
            if !EXPORT_INTEGRATION_IDS.contains(&id.as_str()) {
                continue;
            }
            let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let display_name = entry
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(id);
            let enabled = entry
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let mut cfg = entry
                .get("config")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let existing = store
                .get_integration_config(id)
                .map_err(|e| e.to_string())?;

            // Enforce kind immutability against the current record. Prevents a
            // tampered file from swapping, say, the "obs-main" record's kind
            // to something else and confusing adapter lookups downstream.
            let effective_kind: String = if let Some(ref rec) = existing {
                if !kind.is_empty() && kind != rec.kind {
                    return Err(format!(
                        "Import rejected: integration '{id}' kind '{kind}' does not match existing '{}'.",
                        rec.kind
                    ));
                }
                rec.kind.clone()
            } else {
                kind.to_string()
            };

            // Merge: keep existing secrets when the import payload has redacted them.
            if let Some(existing) = existing {
                if let Ok(existing_cfg) =
                    serde_json::from_str::<serde_json::Value>(&existing.config_json)
                {
                    drop_redacted_in_place(&mut cfg);
                    if let (Some(new_obj), Some(old_obj)) =
                        (cfg.as_object_mut(), existing_cfg.as_object())
                    {
                        for (k, v) in old_obj {
                            new_obj.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
            } else {
                drop_redacted_in_place(&mut cfg);
            }
            let config_json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
            store
                .upsert_integration_config(&IntegrationConfigRecord {
                    id: id.clone(),
                    kind: effective_kind,
                    display_name: display_name.to_string(),
                    enabled,
                    config_json,
                    secret_ref: None,
                })
                .map_err(|e| e.to_string())?;
        }
    }
    state.persist_session()?;
    Ok(())
}
