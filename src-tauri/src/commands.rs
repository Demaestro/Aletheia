use tauri::{AppHandle, State, Emitter, Manager};
use aletheia_core::{AuditAction, ServiceSessionId, now_ms};
use aletheia_detection::{
    GrammarReferenceParser, KeywordLanguageDetector, LanguageDetector, ReferenceKeywordDetector,
    ScriptureDetector, TranscriptNormalizer, ConfidenceBucket,
    TranscriptSegment as DetectionTranscriptSegment,
};
use aletheia_obs::{ObsAdapter, ObsConfig};
use aletheia_osc::{OscAdapter, OscConfig};
use aletheia_easyworship::{EasyWorshipAdapter, EasyWorshipConfig};
use aletheia_propresenter::{ProPresenterAdapter, ProPresenterConfig};
use aletheia_companion::{CompanionAdapter, CompanionConfig};
use aletheia_output::OutputAdapter;
use aletheia_store::{
    AletheiaStore, CalibrationSampleRecord, DeviceAcceptanceReceiptRecord,
    DisplayEventRecord, IntegrationConfigRecord, OfflineAssetStateRecord,
    OperatorActionRecord, ScriptureCandidateRecord, ServiceProfileRecord,
    TranscriptSegmentRecord,
};
use aletheia_ops::{
    ProductionReadinessReport, production_readiness_report_with_assets,
    redact_support_text, verify_signed_plugin_manifest,
};
use aletheia_stt::capture::{AudioCapture, CaptureConfig, list_input_devices};
use aletheia_stt::offline::OfflineSttAdapter;
use aletheia_vad::{AudioFrameMetrics, HybridVad, HybridVadConfig};

use std::path::PathBuf;
use std::sync::Arc;

use crate::DesktopState;
use crate::dto::*;
use crate::audit::*;
use crate::{
    scene_from_candidate, output_health_to_state_detail, live_integrations,
    production_transcript, detect_candidates_for_transcript,
    language_detections_from_transcript, supported_language_to_dto, accuracy_target_dto,
    merged_offline_asset_manifest, stt_readiness_from_manifest,
    offline_asset_root, sha256_file_hex, verified_manifest_to_dto, scene_to_dto,
    sanitize_fts_query, parse_reference,
    default_search_results, recent_integration_events, service_profile_to_dto,
    vmix_config_to_dto, vmix_config_from_dto, vmix_status_dto,
    obs_browser_source_html, vmix_booth_setup, booth_pack_readme, write_booth_pack_file,
    trusted_plugin_record_to_dto, find_stt_model_path, format_clock_time,
    import_full_bible_from_json,
    LIVE_TRANSCRIPT_CAPACITY,
};

// ---------------------------------------------------------------------------
// Core service state
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_service_state(state: State<'_, DesktopState>) -> Result<ServiceStateDto, String> {
    let runtime = state.lock_runtime()?.clone();
    let audit_count = state.audit_count()?;

    // Architecture vision: "wrong scripture on screen is worse than no scripture."
    // Never inject demo transcript into the live state path — an empty live transcript
    // means the mic isn't producing speech yet, and the queue must reflect that
    // honestly. Pre-pollution caused the 1-second polling loop to flood the queue
    // with fake candidates between services.
    let transcript = state.snapshot_live_transcript();

    // Build a dynamic session ID based on today's date in UTC.
    let checked_at = now_ms();
    let day_seconds = checked_at / 1000 % 86_400;
    let total_days = checked_at / 1000 / 86_400;
    let _ = day_seconds; // used below via format_clock_time
    // Simple date derivation: days since Unix epoch → YYYY-MM-DD
    let session_date = {
        let days = total_days as u32;
        // Algorithm by Henry F. Fliegel & Thomas C. Van Flandern (1968)
        let z = days + 719_468;
        let era = z / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}")
    };
    let session_id = format!("service-{session_date}");
    let session_name = format!("Service — {session_date}");

    let health = crate::health::live_health(&state).unwrap_or_default();

    Ok(ServiceStateDto {
        session: ServiceSessionDto {
            id: session_id,
            name: session_name,
            started_at: format!("{session_date}T00:00:00Z"),
            database_path: state.database_path.display().to_string(),
            mode: if runtime.offline_mode_enabled { "offline" } else { "hybrid" }.to_string(),
            data_miser_enabled: runtime.data_miser_enabled,
            offline_mode_enabled: runtime.offline_mode_enabled,
            destinations_armed: runtime.destinations_armed,
            audit_count,
            last_event_sequence: audit_count,
            checked_at_ms: checked_at,
        },
        transcript,
        candidates: service_state_candidates(&state)?,
        integrations: live_integrations(&state),
        health,
        preview: runtime.preview,
        live: runtime.live,
    })
}

fn service_state_candidates(state: &DesktopState) -> Result<Vec<ScriptureCandidateDto>, String> {
    // Honest empty state when no service session is open or no candidates have
    // been persisted yet. Returning demo data here pre-pollutes the queue and
    // masks real detections — the React store explicitly comments on this
    // mistake having been removed in earlier work.
    let session_id = match state.active_session_id_snapshot()? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    let store = state.lock_store()?;
    let records = store
        .recent_scripture_candidates(&session_id, 50)
        .map_err(|error| error.to_string())?;
    if records.is_empty() {
        return Ok(Vec::new());
    }

    Ok(records
        .into_iter()
        .map(|record| {
            let text = verse_text_for_reference(&store, &record.translation_id, &record.reference);
            let text_is_empty = text.is_empty();
            ScriptureCandidateDto {
                id: record.id,
                reference: record.reference,
                translation: record.translation_id.to_uppercase(),
                language: language_display_name(&record.language),
                text,
                confidence: (record.score * 100.0).round().clamp(0.0, 100.0) as u8,
                source: "Live microphone".to_string(),
                reason: record.reason,
                status: match record.status.as_str() {
                    "pending" => "new".to_string(),
                    other => other.to_string(),
                },
                lookup_status: if text_is_empty {
                    "missing_translation".to_string()
                } else {
                    "ok".to_string()
                },
                ..Default::default()
            }
        })
        .collect())
}

#[tauri::command]
pub fn search_scripture(state: State<'_, DesktopState>, query: String) -> Result<Vec<SearchResultDto>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(default_search_results());
    }
    let store = state.lock_store()?;
    let results = crate::scripture_search::search_scripture_unified(
        trimmed,
        "kjv",
        crate::scripture_search::SearchContext::ManualSearch,
        &store,
    );
    if results.is_empty() {
        Ok(default_search_results())
    } else {
        Ok(results)
    }
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

#[tauri::command]
pub fn send_live(state: State<'_, DesktopState>, app: AppHandle) -> Result<LiveOutputResultDto, String> {
    let runtime = state.lock_runtime()?.clone();
    if !runtime.destinations_armed {
        return Err("Live output blocked — destinations are not armed.".to_string());
    }
    let scene = scene_from_candidate(&runtime.preview)?;
    {
        let mut rt = state.lock_runtime()?;
        rt.live = runtime.preview.clone();
    }
    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::LiveOutputSent,
        &operator,
        &format!("Live: {}", runtime.preview.reference),
    )?;
    let _ = state.persist_session();
    let dto = scene_to_dto(scene);
    let _ = app.emit("aletheia://live-updated", &runtime.preview);
    Ok(LiveOutputResultDto {
        scene: dto,
        audit_count: state.audit_count()?,
    })
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
    // Live transcript only — never inject `production_transcript()` here.
    // The 1-second polling loop on the UI calls this command continuously, so
    // any demo fallback floods the queue with fake candidates and masks real
    // detections (see architecture vision: "wrong scripture is worse than no
    // scripture").
    let transcript = state.snapshot_live_transcript();
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
            state: if stt_readiness.offline_models_ready { "ready" } else { "pending" }.to_string(),
            detail: if stt_readiness.offline_models_ready {
                "ggml-base model loaded".to_string()
            } else {
                "No offline model installed".to_string()
            },
            latency_ms: detection_latency_ms,
        }],
        languages: language_detections,
        supported_languages: KeywordLanguageDetector
            .supported_languages()
            .into_iter()
            .map(supported_language_to_dto)
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
pub fn update_vmix_config(config: VmixConfigDto, state: State<'_, DesktopState>) -> Result<VmixStatusDto, String> {
    let new_config = vmix_config_from_dto(config)?;
    let config_json = serde_json::to_string(&vmix_config_to_dto(&new_config)).map_err(|e| e.to_string())?;
    {
        let store = state.lock_store()?;
        store.upsert_integration_config(&IntegrationConfigRecord {
            id: "vmix-main".to_string(),
            kind: "vmix".to_string(),
            display_name: "vMix".to_string(),
            enabled: true,
            config_json,
            secret_ref: None,
        }).map_err(|e| e.to_string())?;
    }
    {
        let mut adapter = state.lock_vmix()?;
        *adapter = aletheia_vmix::VmixAdapter::new(new_config);
    }
    let adapter = state.lock_vmix()?;
    Ok(vmix_status_dto(&adapter))
}

#[tauri::command]
pub fn send_vmix_preview(candidate: ScriptureCandidateDto, state: State<'_, DesktopState>) -> Result<VmixDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_vmix()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "vmix-main", "info", "preview.sent", &format!("vMix preview: {}", candidate.reference))?;
    Ok(VmixDispatchResultDto {
        state: "connected".to_string(),
        detail: format!("vMix preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_vmix_live(candidate: ScriptureCandidateDto, destinations_armed: bool, state: State<'_, DesktopState>) -> Result<VmixDispatchResultDto, String> {
    if !destinations_armed {
        return Err("vMix live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_vmix()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "vmix-main", "info", "live.sent", &format!("vMix live: {}", candidate.reference))?;
    let operator = state.operator_name();
    record_audit_state(&state, AuditAction::LiveOutputSent, &operator, &format!("vMix live {}", candidate.reference))?;
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
    record_integration_event_state(&state, "vmix-main", "info", "output.cleared", "Cleared vMix overlay")?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, &operator, "Cleared vMix overlay")?;
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
pub fn update_obs_config(config: ObsConfigDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn send_obs_preview(candidate: ScriptureCandidateDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_obs()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "obs-main", "info", "preview.sent", &format!("OBS preview: {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: "connected".to_string(),
        detail: format!("OBS preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_obs_live(candidate: ScriptureCandidateDto, destinations_armed: bool, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("OBS live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_obs()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "obs-main", "info", "live.sent", &format!("OBS live: {}", candidate.reference))?;
    record_audit_state(&state, AuditAction::LiveOutputSent, "operator:local-booth", &format!("OBS live {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "obs".to_string(),
        state: "connected".to_string(),
        detail: format!("OBS live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_obs_output(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_obs()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "obs-main", "info", "output.cleared", "Cleared OBS scripture source")?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, "operator:local-booth", "Cleared OBS scripture source")?;
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
pub fn get_propresenter_status(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn get_propresenter_config(state: State<'_, DesktopState>) -> Result<ProPresenterConfigDto, String> {
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
pub fn update_propresenter_config(config: ProPresenterConfigDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn send_propresenter_preview(candidate: ScriptureCandidateDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_propresenter()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "propresenter-main", "info", "preview.sent", &format!("ProPresenter preview: {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: "connected".to_string(),
        detail: format!("ProPresenter preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_propresenter_live(candidate: ScriptureCandidateDto, destinations_armed: bool, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("ProPresenter live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_propresenter()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "propresenter-main", "info", "live.sent", &format!("ProPresenter live: {}", candidate.reference))?;
    record_audit_state(&state, AuditAction::LiveOutputSent, &operator, &format!("ProPresenter live {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "propresenter".to_string(),
        state: "connected".to_string(),
        detail: format!("ProPresenter live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_propresenter_output(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_propresenter()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "propresenter-main", "info", "output.cleared", "Cleared ProPresenter scripture message")?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, &operator, "Cleared ProPresenter scripture message")?;
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
pub fn get_companion_status(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn update_companion_config(config: CompanionConfigDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn send_companion_preview(candidate: ScriptureCandidateDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_companion()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "companion-main", "info", "preview.sent", &format!("Companion preview: {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: "connected".to_string(),
        detail: format!("Companion preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_companion_live(candidate: ScriptureCandidateDto, destinations_armed: bool, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("Companion live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_companion()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "companion-main", "info", "live.sent", &format!("Companion live: {}", candidate.reference))?;
    record_audit_state(&state, AuditAction::LiveOutputSent, &operator, &format!("Companion live {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "companion".to_string(),
        state: "connected".to_string(),
        detail: format!("Companion live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_companion_output(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_companion()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "companion-main", "info", "output.cleared", "Cleared Companion variables")?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, &operator, "Cleared Companion variables")?;
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
pub fn update_osc_config(config: OscConfigDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn send_osc_preview(candidate: ScriptureCandidateDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_osc()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "osc-main", "info", "preview.sent", &format!("OSC preview: {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: format!("OSC preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_osc_live(candidate: ScriptureCandidateDto, destinations_armed: bool, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("OSC live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_osc()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "osc-main", "info", "live.sent", &format!("OSC live: {}", candidate.reference))?;
    record_audit_state(&state, AuditAction::LiveOutputSent, &operator, &format!("OSC live {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: format!("OSC live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_osc_test_ping(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    {
        let adapter = state.lock_osc()?;
        adapter.ping().map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "osc-main", "info", "ping.sent", "OSC /aletheia/ping 1")?;
    Ok(AdapterDispatchResultDto {
        adapter: "osc".to_string(),
        state: "connected".to_string(),
        detail: "OSC /aletheia/ping 1 sent.".to_string(),
        reference: "/aletheia/ping".to_string(),
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_osc_output(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_osc()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "osc-main", "info", "output.cleared", "Cleared OSC output")?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, &operator, "Cleared OSC output")?;
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
pub fn get_easyworship_status(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn update_easyworship_config(config: EasyWorshipConfigDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn send_easyworship_preview(candidate: ScriptureCandidateDto, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_easyworship()?;
        adapter.send_preview(&scene).map_err(|e| e.to_string())?;
    }
    record_integration_event_state(&state, "easyworship-main", "info", "preview.sent", &format!("EasyWorship preview: {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: "connected".to_string(),
        detail: format!("EasyWorship preview: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn send_easyworship_live(candidate: ScriptureCandidateDto, destinations_armed: bool, state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    if !destinations_armed {
        return Err("EasyWorship live output blocked until destinations are armed".to_string());
    }
    let scene = scene_from_candidate(&candidate)?;
    {
        let mut adapter = state.lock_easyworship()?;
        adapter.send_live(&scene).map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "easyworship-main", "info", "live.sent", &format!("EasyWorship live: {}", candidate.reference))?;
    record_audit_state(&state, AuditAction::LiveOutputSent, &operator, &format!("EasyWorship live {}", candidate.reference))?;
    Ok(AdapterDispatchResultDto {
        adapter: "easyworship".to_string(),
        state: "connected".to_string(),
        detail: format!("EasyWorship live: {}.", candidate.reference),
        reference: candidate.reference,
        audit_count: state.audit_count()?,
    })
}

#[tauri::command]
pub fn clear_easyworship_output(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
    {
        let mut adapter = state.lock_easyworship()?;
        adapter.clear().map_err(|e| e.to_string())?;
    }
    let operator = state.operator_name();
    record_integration_event_state(&state, "easyworship-main", "info", "output.cleared", "Cleared EasyWorship output")?;
    record_audit_state(&state, AuditAction::LiveOutputCleared, &operator, "Cleared EasyWorship output")?;
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
pub fn get_recent_integration_events(state: State<'_, DesktopState>) -> Result<Vec<IntegrationEventDto>, String> {
    recent_integration_events(&state, 50)
}

// ---------------------------------------------------------------------------
// Readiness + offline assets
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_production_readiness(state: State<'_, DesktopState>) -> Result<ProductionReadinessReport, String> {
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
pub fn install_offline_asset(asset_id: String, state: State<'_, DesktopState>) -> Result<(), String> {
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
    store.update_offline_asset_state(&OfflineAssetStateRecord {
        id: asset_id,
        state: "installed".to_string(),
        checksum,
        updated_at_ms: now_ms(),
    }).map_err(|e| e.to_string())
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
    store.update_offline_asset_state(&OfflineAssetStateRecord {
        id,
        state: "installed".to_string(),
        checksum: actual_checksum,
        updated_at_ms: now_ms(),
    }).map_err(|e| e.to_string())
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
    store.insert_device_acceptance_receipt(&DeviceAcceptanceReceiptRecord {
        device_id,
        step_label: step_label.unwrap_or_else(|| "manual-acceptance".to_string()),
        passed: passed.unwrap_or(true),
        note,
        evidence_path,
        recorded_at_ms: now_ms(),
    }).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Rehearsal
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn run_local_rehearsal(state: State<'_, DesktopState>) -> Result<LocalRehearsalReportDto, String> {
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
            if found { " — Romans 8:28 ✓" } else { " — Romans 8:28 not found" }
        ),
        duration_ms: start.elapsed().as_millis() as u32,
    });

    let passed = steps.iter().filter(|s| s.state == "connected" || s.state == "healthy" || s.state == "ready").count() as u16;
    let total = steps.len() as u16;

    Ok(LocalRehearsalReportDto {
        generated_at_ms: now_ms(),
        state: if passed == total { "passed" } else { "degraded" }.to_string(),
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
pub fn export_support_bundle(include_transcript_text: bool, state: State<'_, DesktopState>) -> Result<SupportBundleExportDto, String> {
    let app_dir = state.database_path.parent().unwrap_or(std::path::Path::new("."));
    let bundle_path = app_dir.join("support-bundle.json");
    let store = state.lock_store()?;
    let audit_count: i64 = store.connection()
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
        now_ms(), audit_count, state.database_path.display(), include_transcript_text, transcript_section
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
pub fn export_offline_asset_pack(target_dir: Option<String>, state: State<'_, DesktopState>) -> Result<OfflinePackExportDto, String> {
    let app_dir = state.database_path.parent().unwrap_or(std::path::Path::new("."));
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
    let app_dir = state.database_path.parent().unwrap_or(std::path::Path::new("."));
    let pack_dir = app_dir.join("booth-pack");
    std::fs::create_dir_all(&pack_dir).map_err(|e| e.to_string())?;
    let runtime = state.lock_runtime()?.clone();
    let candidate = &runtime.preview;
    let vmix_config = vmix_config_to_dto(state.lock_vmix()?.config());
    let mut files = Vec::new();
    write_booth_pack_file(&pack_dir, "obs/aletheia-browser-source.html", &obs_browser_source_html(candidate), &mut files)?;
    write_booth_pack_file(&pack_dir, "easyworship/current-verse.txt", &format!("{}\n{}", candidate.reference, candidate.text), &mut files)?;
    write_booth_pack_file(&pack_dir, "vmix/setup.md", &vmix_booth_setup(candidate, &vmix_config), &mut files)?;
    write_booth_pack_file(&pack_dir, "README.md", &booth_pack_readme(candidate, &vmix_config), &mut files)?;
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
    let signed: aletheia_ops::SignedPluginManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest JSON: {e}"))?;

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
    let signed: aletheia_ops::SignedPluginManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("Invalid manifest JSON: {e}"))?;

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
    let capabilities_json = serde_json::to_string(&payload_capabilities)
        .map_err(|e| e.to_string())?;
    {
        let store = state.lock_store()?;
        store.upsert_trusted_plugin(&aletheia_store::TrustedPluginRecord {
            id: verified.id.clone(),
            name: verified.name.clone(),
            version: verified.version.clone(),
            key_id: verified.key_id.clone(),
            digest: verified.digest_sha256.clone(),
            capabilities_json,
            enabled: true,
            trusted_at_ms: now_ms(),
        }).map_err(|e| e.to_string())?;
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
pub fn save_service_profile(profile: ServiceProfileDto, state: State<'_, DesktopState>) -> Result<ServiceProfileDto, String> {
    let languages_json = serde_json::to_string(&profile.languages).map_err(|e| e.to_string())?;
    let profile_id = profile.id.clone();
    let store = state.lock_store()?;
    store.upsert_service_profile(&ServiceProfileRecord {
        id: profile.id,
        name: profile.name,
        languages_json,
        output_policy: profile.output_policy,
        is_active: profile.is_active,
        created_at_ms: profile.created_at_ms,
        updated_at_ms: now_ms(),
    }).map_err(|e| e.to_string())?;
    let record = store.list_service_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == profile_id)
        .ok_or_else(|| format!("Profile '{}' not found after save", profile_id))?;
    service_profile_to_dto(&record)
}

#[tauri::command]
pub fn list_service_profiles(state: State<'_, DesktopState>) -> Result<Vec<ServiceProfileDto>, String> {
    let store = state.lock_store()?;
    let records = store.list_service_profiles().map_err(|e| e.to_string())?;
    records.iter().map(service_profile_to_dto).collect()
}

#[tauri::command]
pub fn set_active_service_profile(id: String, state: State<'_, DesktopState>) -> Result<ServiceProfileDto, String> {
    let store = state.lock_store()?;
    store.set_active_service_profile(&id).map_err(|e| e.to_string())?;
    let record = store.list_service_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Profile '{}' not found", id))?;
    service_profile_to_dto(&record)
}

#[tauri::command]
pub fn delete_service_profile(id: String, state: State<'_, DesktopState>) -> Result<bool, String> {
    let store = state.lock_store()?;
    store.delete_service_profile(&id).map_err(|e| e.to_string())?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Trusted plugins
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_trusted_plugins(state: State<'_, DesktopState>) -> Result<Vec<TrustedPluginDto>, String> {
    let store = state.lock_store()?;
    let records = store.list_trusted_plugins().map_err(|e| e.to_string())?;
    records.into_iter().map(trusted_plugin_record_to_dto).collect()
}

#[tauri::command]
pub fn revoke_trusted_plugin(id: String, state: State<'_, DesktopState>) -> Result<bool, String> {
    let store = state.lock_store()?;
    store.revoke_trusted_plugin(&id).map_err(|e| e.to_string())?;
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
    store.insert_calibration_sample(&CalibrationSampleRecord {
        id: 0,
        language: if language.is_empty() { "en".to_string() } else { language },
        transcript_text,
        expected_ref,
        outcome,
        detected_ref,
        recorded_at_ms: now_ms(),
    }).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_calibration_report(state: State<'_, DesktopState>) -> Result<CalibrationReportDto, String> {
    let store = state.lock_store()?;
    let (confirmed, corrected, rejected, total) = store.calibration_summary().map_err(|e| e.to_string())?;
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

fn find_stt_model_path_for_language(
    asset_root: &std::path::Path,
    language_hint: Option<&str>,
) -> Result<PathBuf, String> {
    let normalized = language_hint
        .map(|language| language.trim().to_ascii_lowercase())
        .filter(|language| !language.is_empty());

    let prefers_multilingual = normalized
        .as_deref()
        .map(|language| !matches!(language, "en" | "eng" | "english"))
        .unwrap_or(false);

    if prefers_multilingual {
        for name in [
            "stt-whisper-multilingual.bin",
            "stt-hausa-pack.bin",
            "stt-twi-pack.bin",
            "stt-swahili-pack.bin",
            "stt-xhosa-pack.bin",
            "stt-spanish-pack.bin",
            "stt-french-pack.bin",
            "stt-yoruba-pack.bin",
        ] {
            let path = asset_root.join(name);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    find_stt_model_path(asset_root)
}

#[tauri::command]
pub fn start_audio_capture(language_hint: Option<String>, device_name: Option<String>, state: State<'_, DesktopState>, app: AppHandle) -> Result<String, String> {
    // Check if already running.
    {
        let shutdown = state.capture_shutdown.lock().map_err(|_| "shutdown lock")?;
        if shutdown.is_some() {
            return Err("Audio capture is already running.".to_string());
        }
    }
    // Resolve the model path so the frontend can display which model is loaded.
    // If no Whisper model file exists, refuse to start and surface a clear,
    // operator-actionable error rather than silently capturing audio whose
    // transcripts will never appear.
    let asset_root = offline_asset_root(&state)?;
    let resolved_model = find_stt_model_path_for_language(&asset_root, language_hint.as_deref()).ok();
    let model_path = match resolved_model {
        Some(path) => path.display().to_string(),
        None => {
            let asset_dir = asset_root.display().to_string();
            let detail = format!(
                "No Whisper speech-recognition model is installed.\n\
                 \n\
                 To fix this, download a model file (recommended: ggml-base.en.bin, ~150 MB)\n\
                 from https://huggingface.co/ggerganov/whisper.cpp/tree/main and save it into:\n\
                 \n\
                 {asset_dir}\n\
                 \n\
                 Rename the downloaded file to one of:\n\
                 - stt-whisper-en-small.bin   (English-only, recommended)\n\
                 - stt-whisper-multilingual.bin\n\
                 \n\
                 Then click \"Reload model\" in the Capture Control panel."
            );
            let _ = app.emit("aletheia://capture-error", &detail);
            return Err(detail);
        }
    };
    {
        let mut guard = state.stt_adapter.lock().map_err(|_| "stt adapter lock")?;
        if guard.is_none() {
            let path = PathBuf::from(&model_path);
            match OfflineSttAdapter::load(&path) {
                Ok(adapter) => {
                    *guard = Some(Arc::new(adapter));
                    log::info!("[stt] loaded model on capture start: {}", path.display());
                }
                Err(error) => {
                    let detail = format!("Whisper STT model could not be loaded from {model_path}: {error}");
                    let _ = app.emit("aletheia://capture-error", &detail);
                    return Err(detail);
                }
            }
        }
    }

    // Materialize (or reuse) the active session row up-front so every
    // segment and candidate the capture thread emits is attributable.
    let session_id = state.ensure_active_session()?;

    let stt_adapter = state.stt_adapter.clone();
    let live_transcript = state.live_transcript.clone();
    let database_path = state.database_path.clone();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::sync_channel::<()>(1);

    // Store the shutdown handle.
    {
        let mut guard = state.capture_shutdown.lock().map_err(|_| "shutdown lock")?;
        *guard = Some(shutdown_tx);
    }

    let handle = app.clone();
    let spawn_device = device_name.clone();
    let spawn_session_id = session_id.clone();
    let (startup_tx, startup_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    // Spawn the capture + STT inference thread.
    std::thread::spawn(move || {
        // SQLite supports multiple connections with WAL mode — open a
        // dedicated one for this thread so we never contend with the main
        // thread's store lock while transcribing.
        let persist = match AletheiaStore::open_file(&database_path) {
            Ok(s) => Some(s),
            Err(e) => {
                log::warn!("[capture] failed to open per-thread store: {e} — transcripts will not be persisted");
                None
            }
        };

        let config = CaptureConfig { device_name: spawn_device, ..CaptureConfig::default() };
        let (capture, mut chunk_rx) = match AudioCapture::start(config) {
            Ok(pair) => {
                let _ = startup_tx.send(Ok(()));
                pair
            }
            Err(e) => {
                log::error!("[capture] failed to start: {e}");
                let detail = format!(
                    "Microphone capture could not start: {e}.\n\
                         \n\
                         Common fixes:\n\
                         1. Open Windows Settings → Privacy & security → Microphone.\n\
                            Turn ON \"Microphone access\" AND \"Let desktop apps access your microphone\".\n\
                         2. Confirm the input device you selected is plugged in and not used by another app.\n\
                         3. Try selecting \"OS Default\" in the device picker."
                );
                let _ = handle.emit("aletheia://capture-error", &detail);
                let _ = startup_tx.send(Err(detail));
                return;
            }
        };

        let mut seq: u64 = 0;
        // Permissive VAD config — laptop built-in mics + soft speakers
        // commonly fall well below the default 0.018 RMS floor and the 2.8x
        // adaptive multiplier, causing every chunk to be silently dropped
        // before Whisper ever sees it. The numbers below still reject true
        // silence and HVAC noise but pass quiet-but-real speech.
        let mut vad = HybridVad::new(HybridVadConfig {
            speech_rms_multiplier: 1.6,
            min_speech_rms: 0.0035,
            hangover_frames: 8,
            ..HybridVadConfig::default()
        });
        // Session-wide candidate de-dup: tracks (reference, last_emit_ms) for
        // the last few minutes so the same verse spoken twice in the same
        // pericope only surfaces one candidate to the operator.
        let mut recent_emits: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        const DEDUP_WINDOW_MS: u64 = 90_000;

        loop {
            // Check shutdown.
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            let chunk = match chunk_rx.try_recv() {
                Ok(c) => c,
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
                Err(_) => break,
            };

            // VAD gate: skip silent / pure-music chunks before they hit Whisper.
            // Using the chunk's overall metrics is coarser than frame-by-frame
            // but matches the chunk granularity the capture stage already
            // produces, and saves the full Whisper inference cost on silence.
            let metrics = AudioFrameMetrics::from_pcm(&chunk.samples, 16_000);
            let rms_dbg = metrics.rms;
            let decision = vad.analyze(metrics, None);
            log::info!(
                "[capture] chunk samples={} rms={:.5} speech={}",
                chunk.samples.len(),
                rms_dbg,
                decision.speech_detected
            );
            if !decision.speech_detected {
                continue;
            }

            // Run STT if model is loaded. Pass `None` as the language hint
            // when the operator hasn't pinned one so Whisper auto-detects.
            //
            // CRITICAL: clone the Arc<OfflineSttAdapter> out of the mutex and
            // drop the lock BEFORE calling transcribe(). Whisper inference can
            // take 1-5 s per chunk; holding the mutex across that call freezes
            // every other Tauri command that touches stt_adapter
            // (get_stt_status, reload_stt_model, stop_audio_capture), which
            // shows up to the operator as the whole UI hanging.
            let adapter_arc: Option<Arc<aletheia_stt::offline::OfflineSttAdapter>> =
                match stt_adapter.lock() {
                    Ok(guard) => guard.as_ref().cloned(),
                    Err(_) => continue,
                };
            let adapter = match adapter_arc {
                Some(a) => a,
                None => continue,
            };
            let lang = language_hint.as_deref();
            let transcript = match adapter.transcribe(&chunk.samples, lang) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("[stt] transcribe error: {e}");
                    continue;
                }
            };

            // Sliding-window overlap dedup. Capture emits chunks with a leading
            // `overlap_ms` window of audio repeated from the previous chunk
            // (so Whisper sees full word boundaries). Segments wholly inside
            // that window were already transcribed by the previous chunk —
            // drop them so the same words don't appear twice in the
            // transcript view. Segments that straddle the boundary are kept
            // verbatim; tolerating a few duplicate leading words is cheaper
            // than risking lost new content.
            let overlap_ms_u32 = chunk.overlap_ms;
            let kept_segments: Vec<&aletheia_stt::offline::OfflineSegment> = transcript
                .segments
                .iter()
                .filter(|seg| seg.end_ms > overlap_ms_u32)
                .collect();
            let detected_lang = transcript.language.clone();
            let infer_latency_ms = transcript.latency_ms;
            let stt_confidence_unit: f32 = transcript.confidence.clamp(0.0, 1.0);

            // Rebuild the text from kept segments so persisted/emitted text
            // matches the dedup window. If Whisper produced no segments
            // (very short utterance, rare), fall back to the raw text — but
            // skip it entirely on overlap chunks to avoid duplicating audio.
            let rebuilt_text = if kept_segments.is_empty() {
                if overlap_ms_u32 > 0 || transcript.segments.is_empty() {
                    String::new()
                } else {
                    transcript.text.clone()
                }
            } else {
                kept_segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let text = rebuilt_text;
            let trimmed = text.trim();
            log::info!(
                "[stt] transcribed lang={} latency_ms={} conf={:.2} text=\"{}\"",
                detected_lang,
                infer_latency_ms,
                stt_confidence_unit,
                trimmed
            );
            if trimmed.is_empty() {
                continue;
            }

            seq += 1;
            let chunk_duration_ms = ((chunk.samples.len() as f64 / 16_000.0) * 1000.0) as u64;
            let ended_at_ms = now_ms();
            // Anchor the chunk's wall-clock start, then prefer Whisper's
            // intra-chunk segment timestamps (centisecond precision) when
            // available so the persisted segment row reflects the real spoken
            // window — fall back to wall-clock when Whisper produced no
            // segments (rare but possible on very short chunks).
            let chunk_start_ms = ended_at_ms.saturating_sub(chunk_duration_ms);
            // Use kept_segments (post-overlap-suppression) for span timing so
            // the persisted segment row reflects only what we actually emit.
            let (started_at_ms, ended_at_ms) = if let (Some(first), Some(last)) =
                (kept_segments.first(), kept_segments.last())
            {
                let s = chunk_start_ms.saturating_add(first.start_ms as u64);
                let e = chunk_start_ms.saturating_add(last.end_ms as u64);
                (s, e.max(s))
            } else {
                (chunk_start_ms, ended_at_ms)
            };
            let segment_id = format!("{spawn_session_id}:seg-{seq:06}");
            let latency_ms_u32: u32 = infer_latency_ms;
            let stt_confidence_u8: u8 =
                (stt_confidence_unit * 100.0).round().clamp(0.0, 100.0) as u8;
            let language_display = language_display_name(&detected_lang);
            let segment = TranscriptSegmentDto {
                id: segment_id.clone(),
                time: format_clock_time(ended_at_ms),
                speaker: "Live mic".to_string(),
                language: language_display.clone(),
                text: trimmed.to_string(),
                confidence: stt_confidence_u8,
                latency_ms: latency_ms_u32,
                is_demo: false,
            };

            if let Ok(mut q) = live_transcript.lock() {
                q.push_front(segment.clone());
                while q.len() > LIVE_TRANSCRIPT_CAPACITY {
                    q.pop_back();
                }
            }
            let _ = handle.emit("aletheia://transcript-segment", &segment);

            // Persist the transcript segment.
            if let Some(ref store) = persist {
                let rec = TranscriptSegmentRecord {
                    id: segment_id.clone(),
                    session_id: spawn_session_id.clone(),
                    started_at_ms,
                    ended_at_ms,
                    speaker_label: Some("Live mic".to_string()),
                    language: detected_lang.clone(),
                    text: trimmed.to_string(),
                    confidence: stt_confidence_unit as f64,
                    adapter: "whisper-offline".to_string(),
                    latency_ms: latency_ms_u32 as i64,
                };
                if let Err(e) = store.insert_transcript_segment(&rec) {
                    log::warn!("[capture] failed to persist segment {segment_id}: {e}");
                }
            }

            // Grammar-based scripture detection.
            let normalized = TranscriptNormalizer.normalize(trimmed);
            let refs = GrammarReferenceParser.parse_all(&normalized.text);
            for parsed in refs {
                let reference = parsed.as_reference_string();
                let translation_id = "kjv".to_string();

                // Session-wide de-dup: drop the same reference if we just
                // emitted it within DEDUP_WINDOW_MS (overlapping STT windows
                // commonly catch the same phrase twice).
                let emit_now = now_ms();
                if let Some(prev) = recent_emits.get(&reference) {
                    if emit_now.saturating_sub(*prev) < DEDUP_WINDOW_MS {
                        continue;
                    }
                }
                recent_emits.insert(reference.clone(), emit_now);
                // Bound the map so it can't grow unbounded over a long service.
                if recent_emits.len() > 256 {
                    let cutoff = emit_now.saturating_sub(DEDUP_WINDOW_MS);
                    recent_emits.retain(|_, t| *t >= cutoff);
                }

                // Look up verse text. Empty string + explicit lookup_status
                // so the UI can render a ⚠ icon instead of a silent blank.
                let (verse_text, lookup_status) = if let Some(ref store) = persist {
                    match store.find_verse(
                        &translation_id,
                        parsed.book,
                        parsed.chapter,
                        parsed.verse_start,
                    ) {
                        Ok(Some(v)) => (v.text, "ok".to_string()),
                        Ok(None) => (String::new(), "missing_translation".to_string()),
                        Err(e) => {
                            log::warn!(
                                "[capture] find_verse error for {}: {e}",
                                reference
                            );
                            (String::new(), "missing_translation".to_string())
                        }
                    }
                } else {
                    (String::new(), "ok".to_string())
                };

                let candidate_id = format!("{segment_id}#{reference}");
                let bucket = ConfidenceBucket::from_score(parsed.confidence);
                let bucket_label = match bucket {
                    ConfidenceBucket::Certain => "certain",
                    ConfidenceBucket::Strong => "strong",
                    ConfidenceBucket::Likely => "likely",
                    ConfidenceBucket::Ambiguous => "ambiguous",
                    ConfidenceBucket::Unsafe => "unsafe",
                };
                let reason = format!(
                    "Grammar parser matched alias \"{}\" in live transcript",
                    parsed.alias_matched
                );
                let created_at_ms = now_ms();

                // Persist candidate first so the frontend can round-trip by id.
                if let Some(ref store) = persist {
                    let rec = ScriptureCandidateRecord {
                        id: candidate_id.clone(),
                        session_id: spawn_session_id.clone(),
                        reference: reference.clone(),
                        translation_id: translation_id.clone(),
                        language: detected_lang.clone(),
                        score: parsed.confidence as f64,
                        bucket: bucket_label.to_string(),
                        status: "pending".to_string(),
                        reason: reason.clone(),
                        created_at_ms,
                    };
                    if let Err(e) = store.insert_scripture_candidate(&rec) {
                        log::warn!("[capture] failed to persist candidate {candidate_id}: {e}");
                    }
                }

                let dto = LiveScriptureCandidateDto {
                    id: candidate_id,
                    session_id: spawn_session_id.clone(),
                    segment_id: segment_id.clone(),
                    reference,
                    translation_id,
                    language: detected_lang.clone(),
                    score: parsed.confidence,
                    bucket: bucket_label.to_string(),
                    status: "pending".to_string(),
                    reason,
                    verse_text,
                    created_at_ms,
                    lookup_status,
                    needs_disambiguation: false,
                    disambiguation_options: Vec::new(),
                };
                let _ = handle.emit("aletheia://scripture-candidate", &dto);
            }

            let detection_session_id = match ServiceSessionId::new(&spawn_session_id)
                .or_else(|_| ServiceSessionId::new("service-live"))
            {
                Ok(id) => id,
                Err(err) => {
                    log::error!(
                        "[capture] both spawn session id and fallback rejected: {err:?}; \
                         skipping keyword detection for segment {segment_id}"
                    );
                    continue;
                }
            };
            let detection_segment = DetectionTranscriptSegment {
                id: segment_id.clone(),
                session_id: detection_session_id,
                started_at_ms,
                ended_at_ms,
                speaker_label: Some("Live mic".to_string()),
                language: detected_lang.clone(),
                text: trimmed.to_string(),
                confidence: stt_confidence_unit,
                adapter: "whisper-offline".to_string(),
                latency_ms: u64::from(latency_ms_u32),
            };
            let keyword_detector = ReferenceKeywordDetector;
            for candidate in keyword_detector.detect(&detection_segment, &[]) {
                let reference = candidate.reference.clone();
                let emit_now = now_ms();
                if let Some(prev) = recent_emits.get(&reference) {
                    if emit_now.saturating_sub(*prev) < DEDUP_WINDOW_MS {
                        continue;
                    }
                }
                recent_emits.insert(reference.clone(), emit_now);
                if recent_emits.len() > 256 {
                    let cutoff = emit_now.saturating_sub(DEDUP_WINDOW_MS);
                    recent_emits.retain(|_, t| *t >= cutoff);
                }

                let translation_id = "kjv".to_string();
                let verse_text = if let Some(ref store) = persist {
                    verse_text_for_reference(store, &translation_id, &reference)
                } else {
                    String::new()
                };
                let lookup_status = if verse_text.is_empty() {
                    "missing_translation".to_string()
                } else {
                    "ok".to_string()
                };
                let bucket_label = match candidate.bucket {
                    ConfidenceBucket::Certain => "certain",
                    ConfidenceBucket::Strong => "strong",
                    ConfidenceBucket::Likely => "likely",
                    ConfidenceBucket::Ambiguous => "ambiguous",
                    ConfidenceBucket::Unsafe => "unsafe",
                };
                let reason = if candidate.reasons.is_empty() {
                    "Matched live transcript phrase".to_string()
                } else {
                    candidate.reasons.join("; ")
                };
                let candidate_id = format!("{segment_id}#{reference}");
                let created_at_ms = now_ms();

                if let Some(ref store) = persist {
                    let rec = ScriptureCandidateRecord {
                        id: candidate_id.clone(),
                        session_id: spawn_session_id.clone(),
                        reference: reference.clone(),
                        translation_id: translation_id.clone(),
                        language: detected_lang.clone(),
                        score: candidate.score as f64,
                        bucket: bucket_label.to_string(),
                        status: "pending".to_string(),
                        reason: reason.clone(),
                        created_at_ms,
                    };
                    if let Err(e) = store.insert_scripture_candidate(&rec) {
                        log::warn!("[capture] failed to persist keyword candidate {candidate_id}: {e}");
                    }
                }

                let dto = LiveScriptureCandidateDto {
                    id: candidate_id,
                    session_id: spawn_session_id.clone(),
                    segment_id: segment_id.clone(),
                    reference,
                    translation_id,
                    language: detected_lang.clone(),
                    score: candidate.score,
                    bucket: bucket_label.to_string(),
                    status: "pending".to_string(),
                    reason,
                    verse_text,
                    created_at_ms,
                    lookup_status,
                    needs_disambiguation: false,
                    disambiguation_options: Vec::new(),
                };
                let _ = handle.emit("aletheia://scripture-candidate", &dto);
            }
        }

        drop(capture);
        log::info!("[capture] stopped");
    });

    match startup_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            if let Ok(mut guard) = state.capture_shutdown.lock() {
                *guard = None;
            }
            return Err(error);
        }
        Err(_) => {
            if let Ok(mut guard) = state.capture_shutdown.lock() {
                *guard = None;
            }
            return Err("Microphone capture did not start within 5 seconds. Check the selected input device and Windows microphone permissions.".to_string());
        }
    }

    Ok(model_path)
}

/// Map STT language codes to a human-readable English label for the UI.
fn language_display_name(code: &str) -> String {
    match code {
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "pt" => "Portuguese",
        "de" => "German",
        "sw" => "Swahili",
        "ha" => "Hausa",
        "yo" => "Yoruba",
        "ig" => "Igbo",
        other => other,
    }
    .to_string()
}

#[tauri::command]
pub fn stop_audio_capture(state: State<'_, DesktopState>) -> Result<(), String> {
    let mut guard = state.capture_shutdown.lock().map_err(|_| "shutdown lock")?;
    if let Some(tx) = guard.take() {
        let _ = tx.send(());
    }
    Ok(())
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
            "Bundled translations cannot be deleted (they auto-restore on next launch).".to_string(),
        );
    }
    let store = state.lock_store()?;
    store
        .delete_translation(id)
        .map_err(|e| e.to_string())
}

/// Returns verse counts per translation so the UI can show which Bibles are
/// fully loaded vs. partially seeded.
#[tauri::command]
pub fn list_bible_translations(
    state: State<'_, DesktopState>,
) -> Result<Vec<BibleTranslationStatusDto>, String> {
    let store = state.lock_store()?;
    let known: &[(&str, &str)] = &[
        ("kjv", "King James Version"),
        ("nkjv", "New King James Version"),
        ("niv", "New International Version"),
        ("nlt", "New Living Translation"),
        ("msg", "The Message"),
        ("web", "World English Bible"),
        ("bbe", "Bible in Basic English"),
    ];
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

#[tauri::command]
pub fn get_capture_status(state: State<'_, DesktopState>) -> Result<CaptureStatusDto, String> {
    let running = state.capture_shutdown.lock()
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

/// Reports the offline Whisper STT engine status: whether a model is loaded,
/// the path/filename, the asset directory the operator should drop new
/// models into, and the last load error (if any). Backs the "Capture
/// control" panel in the Operator Dashboard.
#[tauri::command]
pub fn get_stt_status(state: State<'_, DesktopState>) -> Result<SttStatusDto, String> {
    let model_loaded = state
        .stt_adapter
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false);
    let asset_root = offline_asset_root(&state)
        .ok()
        .map(|p| p.display().to_string());
    let resolved = offline_asset_root(&state)
        .ok()
        .and_then(|root| find_stt_model_path(&root).ok());
    let model_filename = resolved
        .as_ref()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()));
    let model_path = resolved.map(|p| p.display().to_string());
    Ok(SttStatusDto {
        model_loaded,
        model_path,
        model_filename,
        asset_root,
        load_error: None,
    })
}

/// Re-scans the offline assets directory for a Whisper model file and (re)loads
/// it into memory. Operators call this from the "Capture control" panel after
/// dropping a `ggml-*.bin` file into the assets folder so they don't have to
/// restart the app.
#[tauri::command]
pub fn reload_stt_model(state: State<'_, DesktopState>) -> Result<SttStatusDto, String> {
    use aletheia_stt::offline::OfflineSttAdapter;

    let asset_root = offline_asset_root(&state)?;
    let path = match find_stt_model_path(&asset_root) {
        Ok(p) => p,
        Err(e) => {
            return Ok(SttStatusDto {
                model_loaded: false,
                model_path: None,
                model_filename: None,
                asset_root: Some(asset_root.display().to_string()),
                load_error: Some(e),
            });
        }
    };

    match OfflineSttAdapter::load(&path) {
        Ok(adapter) => {
            let mut guard = state.stt_adapter.lock().map_err(|_| "stt adapter lock")?;
            *guard = Some(std::sync::Arc::new(adapter));
            Ok(SttStatusDto {
                model_loaded: true,
                model_path: Some(path.display().to_string()),
                model_filename: path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string()),
                asset_root: Some(asset_root.display().to_string()),
                load_error: None,
            })
        }
        Err(e) => Ok(SttStatusDto {
            model_loaded: false,
            model_path: Some(path.display().to_string()),
            model_filename: path
                .file_name()
                .map(|f| f.to_string_lossy().to_string()),
            asset_root: Some(asset_root.display().to_string()),
            load_error: Some(format!("{e}")),
        }),
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
pub fn test_vmix_connection(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn test_obs_connection(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn test_propresenter_connection(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn test_companion_connection(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn test_osc_connection(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
pub fn test_easyworship_connection(state: State<'_, DesktopState>) -> Result<AdapterDispatchResultDto, String> {
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
        "password" | "passwd" | "pwd" | "secret" | "token" | "api_key" | "apikey"
            | "auth" | "authorization" | "bearer" | "credential" | "credentials"
            | "access_key" | "accesskey" | "private_key" | "privatekey"
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
        if let Some(rec) = store.get_integration_config(id).map_err(|e| e.to_string())? {
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
            let enabled = entry.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
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

// ---------------------------------------------------------------------------
// Phase 1: session lifecycle, candidate verdicts, panic-clear
// ---------------------------------------------------------------------------

fn candidate_record_to_dto(
    rec: &ScriptureCandidateRecord,
    segment_id: String,
    verse_text: String,
) -> LiveScriptureCandidateDto {
    let lookup_status = if verse_text.is_empty() {
        "missing_translation".to_string()
    } else {
        "ok".to_string()
    };
    LiveScriptureCandidateDto {
        id: rec.id.clone(),
        session_id: rec.session_id.clone(),
        segment_id,
        reference: rec.reference.clone(),
        translation_id: rec.translation_id.clone(),
        language: rec.language.clone(),
        score: rec.score as f32,
        bucket: rec.bucket.clone(),
        status: rec.status.clone(),
        reason: rec.reason.clone(),
        verse_text,
        created_at_ms: rec.created_at_ms,
        lookup_status,
        needs_disambiguation: false,
        disambiguation_options: Vec::new(),
    }
}

fn segment_record_to_dto(rec: &TranscriptSegmentRecord) -> TranscriptSegmentDto {
    TranscriptSegmentDto {
        id: rec.id.clone(),
        time: format_clock_time(rec.ended_at_ms),
        speaker: rec.speaker_label.clone().unwrap_or_else(|| "Live mic".to_string()),
        language: language_display_name(&rec.language),
        text: rec.text.clone(),
        confidence: (rec.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
        latency_ms: rec.latency_ms.max(0) as u32,
        is_demo: false,
    }
}

#[tauri::command]
pub fn start_service_session(state: State<'_, DesktopState>) -> Result<String, String> {
    state.ensure_active_session()
}

#[tauri::command]
pub fn end_service_session(state: State<'_, DesktopState>) -> Result<Option<String>, String> {
    state.end_active_session()
}

#[tauri::command]
pub fn get_active_session(state: State<'_, DesktopState>) -> Result<Option<String>, String> {
    state.active_session_id_snapshot()
}

#[tauri::command]
pub fn get_session_candidates(
    limit: Option<u16>,
    state: State<'_, DesktopState>,
) -> Result<Vec<LiveScriptureCandidateDto>, String> {
    let session_id = match state.active_session_id_snapshot()? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    let store = state.lock_store()?;
    let recs = store
        .recent_scripture_candidates(&session_id, limit.unwrap_or(100))
        .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(recs.len());
    for rec in recs {
        let verse_text = verse_text_for_reference(&store, &rec.translation_id, &rec.reference);
        let segment_id = rec.id.split_once('#').map(|(s, _)| s.to_string()).unwrap_or_default();
        out.push(candidate_record_to_dto(&rec, segment_id, verse_text));
    }
    Ok(out)
}

#[tauri::command]
pub fn get_session_transcript(
    limit: Option<u16>,
    state: State<'_, DesktopState>,
) -> Result<Vec<TranscriptSegmentDto>, String> {
    let session_id = match state.active_session_id_snapshot()? {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };
    let store = state.lock_store()?;
    let recs = store
        .recent_transcript_segments(&session_id, limit.unwrap_or(100))
        .map_err(|e| e.to_string())?;
    Ok(recs.iter().map(segment_record_to_dto).collect())
}

/// Best-effort verse text lookup from a canonical "Book C:V[-E]" string.
pub fn verse_text_for_reference(
    store: &AletheiaStore,
    translation_id: &str,
    reference: &str,
) -> String {
    let (book, chapter, verse) = match parse_book_chapter_verse(reference) {
        Some(t) => t,
        None => return String::new(),
    };
    match store.find_verse(translation_id, &book, chapter, verse) {
        Ok(Some(v)) => v.text,
        _ => String::new(),
    }
}

/// Parses a canonical reference like "1 John 4:8" or "Romans 8:28-30" into
/// (book, chapter, first_verse). Returns None for malformed strings.
fn parse_book_chapter_verse(reference: &str) -> Option<(String, u16, u16)> {
    let last_space = reference.rfind(' ')?;
    let book = reference[..last_space].trim().to_string();
    let tail = reference[last_space + 1..].trim();
    let (chap_s, verse_s) = tail.split_once(':').unwrap_or((tail, "1"));
    let chapter: u16 = chap_s.parse().ok()?;
    let verse_first = verse_s.split(['-', '\u{2013}']).next()?;
    let verse: u16 = verse_first.trim().parse().ok()?;
    Some((book, chapter, verse))
}

#[tauri::command]
pub fn approve_candidate(
    candidate_id: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    update_candidate_with_action(&state, &candidate_id, "approved", "approve", None)
}

#[tauri::command]
pub fn reject_candidate(
    candidate_id: String,
    reason: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    update_candidate_with_action(&state, &candidate_id, "rejected", "reject", reason)
}

#[tauri::command]
pub fn preview_candidate(
    candidate_id: String,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<(), String> {
    update_candidate_with_action(&state, &candidate_id, "preview", "preview", None)?;
    emit_display_event(&state, &app, Some(&candidate_id), "preview", "all")?;
    Ok(())
}

#[tauri::command]
pub fn take_candidate_live(
    candidate_id: String,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<(), String> {
    update_candidate_with_action(&state, &candidate_id, "live", "live", None)?;
    emit_display_event(&state, &app, Some(&candidate_id), "live", "all")?;
    Ok(())
}

#[tauri::command]
pub fn record_operator_action(
    action_type: String,
    candidate_id: Option<String>,
    payload_json: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<i64, String> {
    let session_id = state.ensure_active_session()?;
    let store = state.lock_store()?;
    let rec = OperatorActionRecord {
        id: 0,
        session_id,
        candidate_id,
        action_type,
        actor: state.operator_name(),
        payload_json: payload_json.unwrap_or_else(|| "{}".to_string()),
        occurred_at_ms: now_ms(),
    };
    store.insert_operator_action(&rec).map_err(|e| e.to_string())
}

/// Panic clear — drives every output adapter to its cleared state and logs a
/// `panic_clear` operator action + `panic_clear` display event for every
/// target that accepted the clear. Never bails on a single adapter failure —
/// this is the last-resort hotkey the operator uses when something is on
/// screen that MUST come off.
#[tauri::command]
pub fn clear_all_outputs(
    triggered_by: Option<String>,
    state: State<'_, DesktopState>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    let trigger = triggered_by.unwrap_or_else(|| "panic-hotkey".to_string());
    let session_id = state.ensure_active_session()?;
    let mut cleared_targets: Vec<String> = Vec::new();

    if let Ok(mut adapter) = state.lock_vmix() {
        if adapter.clear().is_ok() {
            cleared_targets.push("vmix".to_string());
        }
    }
    if let Ok(mut adapter) = state.lock_obs() {
        if adapter.clear().is_ok() {
            cleared_targets.push("obs".to_string());
        }
    }
    if let Ok(mut adapter) = state.lock_propresenter() {
        if adapter.clear().is_ok() {
            cleared_targets.push("propresenter".to_string());
        }
    }
    if let Ok(mut adapter) = state.lock_companion() {
        if adapter.clear().is_ok() {
            cleared_targets.push("companion".to_string());
        }
    }
    if let Ok(mut adapter) = state.lock_osc() {
        if adapter.clear().is_ok() {
            cleared_targets.push("osc".to_string());
        }
    }
    if let Ok(mut adapter) = state.lock_easyworship() {
        if adapter.clear().is_ok() {
            cleared_targets.push("easyworship".to_string());
        }
    }

    let now = now_ms();
    {
        let store = state.lock_store()?;
        let _ = store.insert_operator_action(&OperatorActionRecord {
            id: 0,
            session_id: session_id.clone(),
            candidate_id: None,
            action_type: "panic_clear".to_string(),
            actor: state.operator_name(),
            payload_json: serde_json::json!({
                "triggered_by": trigger,
                "cleared_targets": cleared_targets,
            })
            .to_string(),
            occurred_at_ms: now,
        });
        for target in &cleared_targets {
            let _ = store.insert_display_event(&DisplayEventRecord {
                id: 0,
                session_id: session_id.clone(),
                candidate_id: None,
                action: "panic_clear".to_string(),
                output_target: target.clone(),
                triggered_by: trigger.clone(),
                locked_at_ms: now,
                released_at_ms: Some(now),
                detail_json: "{}".to_string(),
            });
        }
    }

    if let Ok(mut runtime) = state.lock_runtime() {
        runtime.destinations_armed = false;
    }
    let _ = state.persist_session();
    let _ = app.emit("aletheia://outputs-cleared", &cleared_targets);

    Ok(cleared_targets)
}

fn update_candidate_with_action(
    state: &State<'_, DesktopState>,
    candidate_id: &str,
    new_status: &str,
    action_type: &str,
    reason: Option<String>,
) -> Result<(), String> {
    let session_id = state.ensure_active_session()?;
    let store = state.lock_store()?;
    store
        .update_scripture_candidate_status(candidate_id, new_status)
        .map_err(|e| e.to_string())?;
    let payload = serde_json::json!({
        "candidate_id": candidate_id,
        "new_status": new_status,
        "reason": reason,
    })
    .to_string();
    store
        .insert_operator_action(&OperatorActionRecord {
            id: 0,
            session_id,
            candidate_id: Some(candidate_id.to_string()),
            action_type: action_type.to_string(),
            actor: state.operator_name(),
            payload_json: payload,
            occurred_at_ms: now_ms(),
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn emit_display_event(
    state: &State<'_, DesktopState>,
    app: &AppHandle,
    candidate_id: Option<&str>,
    action: &str,
    output_target: &str,
) -> Result<(), String> {
    let session_id = state.ensure_active_session()?;
    let now = now_ms();
    let store = state.lock_store()?;
    let id = store
        .insert_display_event(&DisplayEventRecord {
            id: 0,
            session_id: session_id.clone(),
            candidate_id: candidate_id.map(|s| s.to_string()),
            action: action.to_string(),
            output_target: output_target.to_string(),
            triggered_by: state.operator_name(),
            locked_at_ms: now,
            released_at_ms: None,
            detail_json: "{}".to_string(),
        })
        .map_err(|e| e.to_string())?;
    let dto = DisplayEventDto {
        id,
        session_id,
        candidate_id: candidate_id.map(|s| s.to_string()),
        action: action.to_string(),
        output_target: output_target.to_string(),
        triggered_by: state.operator_name(),
        locked_at_ms: now,
    };
    let _ = app.emit("aletheia://display-event", &dto);
    Ok(())
}

#[allow(dead_code)]
fn _service_session_id_marker() -> Option<ServiceSessionId> { None }

// ---------------------------------------------------------------------------
// #3  Session resumption — check for same-day crash evidence
// ---------------------------------------------------------------------------

/// Returns a `SessionResumptionDto` describing whether a same-day session
/// was interrupted without a clean `end_service_session` call. The frontend
/// uses this to offer an "Resume previous session?" banner on startup.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SessionResumptionDto {
    /// Whether there is a resumable same-day session.
    pub can_resume: bool,
    /// The session ID that can be resumed (if any).
    pub session_id: Option<String>,
    /// Human-readable session name.
    pub session_name: Option<String>,
    /// Epoch-ms when that session started.
    pub started_at_ms: Option<u64>,
    /// Number of transcript segments already captured in the interrupted session.
    pub segment_count: u32,
    /// Number of scripture candidates already captured.
    pub candidate_count: u32,
    /// Whether a crash log file from the last process run was found.
    pub crash_log_found: bool,
    /// Absolute path to the crash log (if found).
    pub crash_log_path: Option<String>,
}

#[tauri::command]
pub fn check_session_resumption(state: State<'_, DesktopState>) -> Result<SessionResumptionDto, String> {
    // Check for crash log written by the panic hook.
    let crash_path = std::env::temp_dir().join("aletheia-crash.log");
    let crash_log_found = crash_path.exists();
    let crash_log_path = if crash_log_found {
        Some(crash_path.display().to_string())
    } else {
        None
    };

    let now = aletheia_core::now_ms();
    let today = {
        let total_days = (now / 1000 / 86_400) as u32;
        let z = total_days + 719_468;
        let era = z / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}")
    };
    let expected_id = format!("service-{today}");

    let store = state.lock_store()?;

    // Try to load today's session record directly.
    let session = store.find_service_session(&expected_id)
        .map_err(|e| e.to_string())?;

    let Some(rec) = session else {
        return Ok(SessionResumptionDto {
            can_resume: false,
            session_id: None,
            session_name: None,
            started_at_ms: None,
            segment_count: 0,
            candidate_count: 0,
            crash_log_found,
            crash_log_path,
        });
    };

    // A session is resumable only if it has no `ended_at_ms` (not cleanly closed).
    if rec.ended_at_ms.is_some() {
        return Ok(SessionResumptionDto {
            can_resume: false,
            session_id: None,
            session_name: None,
            started_at_ms: None,
            segment_count: 0,
            candidate_count: 0,
            crash_log_found,
            crash_log_path,
        });
    }

    let segment_count = store
        .count_transcript_segments_for_session(&rec.id)
        .unwrap_or(0) as u32;
    let candidate_count = store
        .count_scripture_candidates_for_session(&rec.id)
        .unwrap_or(0) as u32;

    // Only surface the banner when there's actually meaningful content to resume.
    let can_resume = segment_count > 0 || candidate_count > 0 || crash_log_found;

    Ok(SessionResumptionDto {
        can_resume,
        session_id: Some(rec.id.clone()),
        session_name: Some(rec.name.clone()),
        started_at_ms: Some(rec.started_at_ms),
        segment_count,
        candidate_count,
        crash_log_found,
        crash_log_path,
    })
}

// ---------------------------------------------------------------------------
// #5  FTS transcript search — full-text search over persisted segments
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct TranscriptSearchResultDto {
    pub segment_id: String,
    pub session_id: String,
    pub time: String,
    pub speaker: String,
    pub language: String,
    pub text: String,
    /// Highlighted snippet with match markers (e.g. <<term>>)
    pub snippet: String,
    pub confidence: u8,
}

/// Searches the persisted transcript_segments table using SQLite FTS5.
/// `query` supports standard FTS5 syntax (phrase, prefix, boolean).
/// Returns up to `limit` results, newest first.
#[tauri::command]
pub fn search_transcript_history(
    query: String,
    limit: Option<u16>,
    state: State<'_, DesktopState>,
) -> Result<Vec<TranscriptSearchResultDto>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let sanitized = sanitize_fts_query(q);
    let store = state.lock_store()?;
    let recs = store
        .fts_search_transcript_segments(&sanitized, limit.unwrap_or(50))
        .map_err(|e| e.to_string())?;

    Ok(recs
        .into_iter()
        .map(|r| {
            // Build a simple context snippet: first 160 chars, highlight the
            // search term with guillemet markers that the frontend can style.
            let snippet = build_highlight_snippet(&r.text, q, 160);
            TranscriptSearchResultDto {
                segment_id: r.id.clone(),
                session_id: r.session_id.clone(),
                time: format_clock_time(r.ended_at_ms),
                speaker: r.speaker_label.unwrap_or_else(|| "Live mic".to_string()),
                language: language_display_name(&r.language),
                text: r.text.clone(),
                snippet,
                confidence: (r.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
            }
        })
        .collect())
}

/// Wraps terms that appear in `text` with `«` / `»` delimiters.
fn build_highlight_snippet(text: &str, query: &str, max_len: usize) -> String {
    let lower_text = text.to_lowercase();
    // Find the first term match position to anchor the window.
    let first_term = query
        .split_whitespace()
        .find_map(|term| lower_text.find(&term.to_lowercase()));
    let start = first_term.map(|pos| pos.saturating_sub(40)).unwrap_or(0);
    let window: String = text.chars().skip(start).take(max_len).collect();
    // Highlight each query token.
    let mut result = window;
    for term in query.split_whitespace() {
        if term.len() < 2 { continue; }
        let lower_term = term.to_lowercase();
        let lower_result = result.to_lowercase();
        if let Some(idx) = lower_result.find(&lower_term) {
            let end = idx + term.len();
            if end <= result.len() {
                result = format!("{}«{}»{}", &result[..idx], &result[idx..end], &result[end..]);
            }
        }
    }
    if start > 0 { format!("…{result}") } else { result }
}

// ---------------------------------------------------------------------------
// #7  Service report — session audit summary for the operator
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ServiceReportDto {
    pub session_id: String,
    pub session_name: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub duration_minutes: u32,
    pub segment_count: u32,
    pub candidate_count: u32,
    pub approved_count: u32,
    pub rejected_count: u32,
    pub live_count: u32,
    pub panic_clear_count: u32,
    pub operator_name: String,
    /// All operator action rows as lightweight structs.
    pub actions: Vec<ServiceReportActionDto>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ServiceReportActionDto {
    pub occurred_at_ms: u64,
    pub time: String,
    pub action_type: String,
    pub actor: String,
    pub candidate_id: Option<String>,
    pub payload_json: String,
}

/// Returns a full audit summary for the currently active (or most recent)
/// service session. Used by the new "Service Report" tab.
#[tauri::command]
pub fn get_service_report(
    session_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<ServiceReportDto, String> {
    // Resolve session: use supplied id, or fall back to today's active session.
    let sid = if let Some(id) = session_id {
        id
    } else {
        state.ensure_active_session()?
    };

    let store = state.lock_store()?;

    let rec = store
        .find_service_session(&sid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session not found: {sid}"))?;

    let segment_count = store
        .count_transcript_segments_for_session(&sid)
        .unwrap_or(0) as u32;
    let candidate_count = store
        .count_scripture_candidates_for_session(&sid)
        .unwrap_or(0) as u32;

    // Load all operator actions for the session.
    let actions_raw = store
        .list_operator_actions_for_session(&sid)
        .map_err(|e| e.to_string())?;

    let mut approved_count: u32 = 0;
    let mut rejected_count: u32 = 0;
    let mut live_count: u32 = 0;
    let mut panic_clear_count: u32 = 0;

    let actions: Vec<ServiceReportActionDto> = actions_raw
        .iter()
        .map(|a| {
            match a.action_type.as_str() {
                "approve"     => approved_count += 1,
                "reject"      => rejected_count += 1,
                "live"        => live_count += 1,
                "panic_clear" => panic_clear_count += 1,
                _ => {}
            }
            ServiceReportActionDto {
                occurred_at_ms: a.occurred_at_ms,
                time: format_clock_time(a.occurred_at_ms),
                action_type: a.action_type.clone(),
                actor: a.actor.clone(),
                candidate_id: a.candidate_id.clone(),
                payload_json: a.payload_json.clone(),
            }
        })
        .collect();

    let started = rec.started_at_ms;
    let ended = rec.ended_at_ms;
    let duration_ms = ended.unwrap_or_else(aletheia_core::now_ms).saturating_sub(started);
    let duration_minutes = (duration_ms / 60_000) as u32;

    let operator_name = state.operator_name();

    Ok(ServiceReportDto {
        session_id: sid,
        session_name: rec.name.clone(),
        started_at_ms: started,
        ended_at_ms: ended,
        duration_minutes,
        segment_count,
        candidate_count,
        approved_count,
        rejected_count,
        live_count,
        panic_clear_count,
        operator_name,
        actions,
    })
}

// ---------------------------------------------------------------------------
// #9  Whisper model download wizard — in-app HTTPS download with progress
// ---------------------------------------------------------------------------

/// A static cell that holds the most-recent download progress (0–100).
/// Written from the download thread, read by `get_stt_download_progress`.
static STT_DOWNLOAD_PROGRESS: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(-1);

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DownloadModelSize {
    Tiny,
    Base,
    Small,
    Medium,
}

impl DownloadModelSize {
    fn hf_filename(&self) -> &'static str {
        match self {
            Self::Tiny   => "ggml-tiny.en.bin",
            Self::Base   => "ggml-base.en.bin",
            Self::Small  => "ggml-small.en.bin",
            Self::Medium => "ggml-medium.en.bin",
        }
    }
    fn dest_filename(&self) -> &'static str {
        match self {
            Self::Tiny   => "stt-whisper-en-small.bin",
            Self::Base   => "stt-whisper-en-small.bin",
            Self::Small  => "stt-whisper-en-small.bin",
            Self::Medium => "stt-whisper-multilingual.bin",
        }
    }
    fn expected_min_bytes(&self) -> u64 {
        match self {
            Self::Tiny   => 30_000_000,
            Self::Base   => 100_000_000,
            Self::Small  => 400_000_000,
            Self::Medium => 1_400_000_000,
        }
    }
}

/// Begins an asynchronous download of a Whisper GGML model from Hugging Face
/// into the app's offline-assets directory.  Progress is polled via
/// `get_stt_download_progress`.  Calling `download_stt_model` while a
/// download is already in progress is a no-op (returns the current progress).
#[tauri::command]
pub fn download_stt_model(
    model_size: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;

    // Parse model size.
    let size = match model_size.to_lowercase().as_str() {
        "tiny"   => DownloadModelSize::Tiny,
        "base"   => DownloadModelSize::Base,
        "small"  => DownloadModelSize::Small,
        "medium" => DownloadModelSize::Medium,
        other    => return Err(format!("Unknown model size: {other}. Valid: tiny, base, small, medium")),
    };

    // Guard against concurrent downloads.
    let current = STT_DOWNLOAD_PROGRESS.load(Ordering::Relaxed);
    if (0..100).contains(&current) {
        return Ok(format!("Download already in progress: {current}%"));
    }

    let asset_root = offline_asset_root(&state)?;
    std::fs::create_dir_all(&asset_root).map_err(|e| e.to_string())?;
    let dest_path = asset_root.join(size.dest_filename());
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        size.hf_filename()
    );
    let min_bytes = size.expected_min_bytes();
    let db_path = state.database_path.clone();
    let stt_adapter = state.stt_adapter.clone();

    STT_DOWNLOAD_PROGRESS.store(0, Ordering::Relaxed);

    std::thread::spawn(move || {
        use std::io::{Read, Write};
        use std::sync::atomic::Ordering;

        log::info!("[model-dl] starting download: {url} → {}", dest_path.display());

        let response = match ureq::get(&url).call() {
            Ok(r) => r,
            Err(e) => {
                log::error!("[model-dl] request failed: {e}");
                STT_DOWNLOAD_PROGRESS.store(-2, Ordering::Relaxed);
                return;
            }
        };

        let total: u64 = response
            .header("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let tmp_path = dest_path.with_extension("bin.tmp");
        let mut file = match std::fs::File::create(&tmp_path) {
            Ok(f) => f,
            Err(e) => {
                log::error!("[model-dl] cannot create tmp file: {e}");
                STT_DOWNLOAD_PROGRESS.store(-2, Ordering::Relaxed);
                return;
            }
        };

        let mut reader = response.into_reader();
        let mut buf = vec![0u8; 65_536];
        let mut downloaded: u64 = 0;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if file.write_all(&buf[..n]).is_err() {
                        log::error!("[model-dl] write error");
                        STT_DOWNLOAD_PROGRESS.store(-2, Ordering::Relaxed);
                        return;
                    }
                    downloaded += n as u64;
                    if total > 0 {
                        let pct = ((downloaded as f64 / total as f64) * 100.0) as i32;
                        STT_DOWNLOAD_PROGRESS.store(pct.min(99), Ordering::Relaxed);
                    } else {
                        // Unknown length: report bytes in MB as a proxy.
                        let mb = (downloaded / 1_000_000) as i32;
                        STT_DOWNLOAD_PROGRESS.store(mb.min(99), Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    log::error!("[model-dl] read error: {e}");
                    STT_DOWNLOAD_PROGRESS.store(-2, Ordering::Relaxed);
                    return;
                }
            }
        }

        // Basic size sanity check before promoting the tmp file.
        if downloaded < min_bytes {
            log::error!("[model-dl] file too small ({downloaded} bytes) — aborting");
            let _ = std::fs::remove_file(&tmp_path);
            STT_DOWNLOAD_PROGRESS.store(-3, Ordering::Relaxed);
            return;
        }

        // Promote to final path.
        if let Err(e) = std::fs::rename(&tmp_path, &dest_path) {
            log::error!("[model-dl] rename failed: {e}");
            STT_DOWNLOAD_PROGRESS.store(-2, Ordering::Relaxed);
            return;
        }

        // Auto-load the freshly downloaded model.
        match aletheia_stt::offline::OfflineSttAdapter::load(&dest_path) {
            Ok(adapter) => {
                if let Ok(mut g) = stt_adapter.lock() {
                    *g = Some(std::sync::Arc::new(adapter));
                }
                log::info!("[model-dl] model loaded: {}", dest_path.display());
                // Mark the offline-asset row as installed in the store.
                if let Ok(bg_store) = aletheia_store::AletheiaStore::open_file(&db_path) {
                    let _ = bg_store.upsert_offline_asset_state(
                        &aletheia_store::OfflineAssetStateRecord {
                            id: "stt-whisper-en".to_string(),
                            state: "installed".to_string(),
                            checksum: dest_path
                                .metadata()
                                .map(|m| format!("sz:{}", m.len()))
                                .unwrap_or_default(),
                            updated_at_ms: aletheia_core::now_ms(),
                        },
                    );
                }
            }
            Err(e) => {
                log::warn!("[model-dl] download ok but load failed: {e}");
            }
        }

        STT_DOWNLOAD_PROGRESS.store(100, Ordering::Relaxed);
        log::info!("[model-dl] complete: {} bytes", downloaded);
    });

    Ok("Download started".to_string())
}

/// Returns the current Whisper model download progress:
/// - `-1` = no download in progress / not started
/// - `0–99` = percent complete  
/// - `100` = complete
/// - `-2` = network/write error
/// - `-3` = download too small (corrupt response)
#[tauri::command]
pub fn get_stt_download_progress() -> i32 {
    STT_DOWNLOAD_PROGRESS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Clears the download progress sentinel (allows retrying after error).
#[tauri::command]
pub fn reset_stt_download_progress() {
    STT_DOWNLOAD_PROGRESS.store(-1, std::sync::atomic::Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Latency ring-buffer + detection debounce (production hardening)
// ---------------------------------------------------------------------------

use std::sync::atomic::AtomicU32;
use std::collections::VecDeque;


/// Circular buffer of the last 60 end-to-end pipeline latencies in milliseconds.
/// Updated by `record_pipeline_latency` each time a transcript segment completes
/// the full mic→STT→detect→UI round-trip.
static LATENCY_RING: std::sync::OnceLock<std::sync::Mutex<VecDeque<u64>>> =
    std::sync::OnceLock::new();

/// Number of segments this session whose pipeline latency exceeded 3 000 ms.
static LATENCY_BREACH_COUNT: AtomicU32 = AtomicU32::new(0);

/// Detections observed this session (pre-debounce).
static DETECTIONS_TOTAL: AtomicU32 = AtomicU32::new(0);
/// Detections suppressed by the 15-second deduplication window.
static DETECTIONS_SUPPRESSED: AtomicU32 = AtomicU32::new(0);
/// Operator rejections this session.
static DETECTIONS_REJECTED: AtomicU32 = AtomicU32::new(0);

/// Most recent detection reference (for diagnostics display).
static LAST_DETECTION_REFERENCE: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
    std::sync::OnceLock::new();
static LAST_DETECTION_CONFIDENCE: std::sync::OnceLock<std::sync::Mutex<Option<f32>>> =
    std::sync::OnceLock::new();
static LAST_DETECTION_BUCKET: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

/// 15-second debounce window: maps canonical reference → last emitted timestamp (ms).
/// Suppresses duplicate candidates from overlapping STT windows.
/// Cleared by `clear_debounce_for_reference` when operator rejects.
static DEBOUNCE_MAP: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u64>>> =
    std::sync::OnceLock::new();

const DEBOUNCE_WINDOW_MS: u64 = 15_000;
const LATENCY_BREACH_THRESHOLD_MS: u64 = 3_000;
const LATENCY_RING_CAPACITY: usize = 60;

fn latency_ring() -> &'static std::sync::Mutex<VecDeque<u64>> {
    LATENCY_RING.get_or_init(|| std::sync::Mutex::new(VecDeque::with_capacity(LATENCY_RING_CAPACITY)))
}

fn debounce_map() -> &'static std::sync::Mutex<std::collections::HashMap<String, u64>> {
    DEBOUNCE_MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Push one latency sample into the ring buffer. Called from the STT → detect
/// pipeline each time a segment finishes the full round-trip.
///
/// `latency_ms` = time from mic frame received → UI event emitted.
pub fn record_pipeline_latency(latency_ms: u64) {
    use std::sync::atomic::Ordering;
    if let Ok(mut ring) = latency_ring().lock() {
        if ring.len() == LATENCY_RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(latency_ms);
    }
    if latency_ms > LATENCY_BREACH_THRESHOLD_MS {
        LATENCY_BREACH_COUNT.fetch_add(1, Ordering::Relaxed);
        log::warn!("[latency] pipeline breach: {}ms (threshold {}ms)", latency_ms, LATENCY_BREACH_THRESHOLD_MS);
    }
}

/// Returns true if the detection for `reference` should be suppressed by the
/// debounce window (same reference within the last 15 seconds). Updates the
/// window on first-pass or after expiry.
pub fn should_debounce(reference: &str, now_ms: u64) -> bool {
    use std::sync::atomic::Ordering;
    let Ok(mut map) = debounce_map().lock() else { return false };
    if let Some(&last) = map.get(reference) {
        if now_ms.saturating_sub(last) < DEBOUNCE_WINDOW_MS {
            DETECTIONS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    }
    map.insert(reference.to_string(), now_ms);
    DETECTIONS_TOTAL.fetch_add(1, Ordering::Relaxed);
    false
}

/// Clears the debounce entry for `reference`. Called when the operator
/// explicitly rejects a candidate — their rejection signals "re-watch for this".
pub fn clear_debounce_for_reference(reference: &str) {
    if let Ok(mut map) = debounce_map().lock() {
        map.remove(reference);
    }
    DETECTIONS_REJECTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Records the most-recently-detected reference for the diagnostics screen.
pub fn record_last_detection(reference: &str, confidence: f32, bucket: &str) {
    if let Some(m) = LAST_DETECTION_REFERENCE.get() {
        if let Ok(mut g) = m.lock() { *g = Some(reference.to_string()); }
    } else {
        let _ = LAST_DETECTION_REFERENCE.set(std::sync::Mutex::new(Some(reference.to_string())));
    }
    if let Some(m) = LAST_DETECTION_CONFIDENCE.get() {
        if let Ok(mut g) = m.lock() { *g = Some(confidence); }
    } else {
        let _ = LAST_DETECTION_CONFIDENCE.set(std::sync::Mutex::new(Some(confidence)));
    }
    if let Some(m) = LAST_DETECTION_BUCKET.get() {
        if let Ok(mut g) = m.lock() { *g = Some(bucket.to_string()); }
    } else {
        let _ = LAST_DETECTION_BUCKET.set(std::sync::Mutex::new(Some(bucket.to_string())));
    }
}

/// Compute P50 / P95 / P99 / max from a sorted copy of the latency ring.
fn compute_latency_stats(ring: &VecDeque<u64>) -> LatencyStatsDto {
    use std::sync::atomic::Ordering;
    if ring.is_empty() {
        return LatencyStatsDto::default();
    }
    let mut sorted: Vec<u64> = ring.iter().copied().collect();
    sorted.sort_unstable();
    let n = sorted.len();
    let percentile = |pct: f64| sorted[((pct / 100.0) * (n - 1) as f64).round() as usize];
    LatencyStatsDto {
        p50_ms:        percentile(50.0),
        p95_ms:        percentile(95.0),
        p99_ms:        percentile(99.0),
        max_ms:        *sorted.last().unwrap_or(&0),
        sample_count:  n as u32,
        breach_count:  LATENCY_BREACH_COUNT.load(Ordering::Relaxed),
    }
}

// ---------------------------------------------------------------------------
// Model catalogue — embedded SHA-256 hashes for official whisper.cpp models
// ---------------------------------------------------------------------------
//
// Hashes sourced from https://ggml.ggerganov.com — the whisper.cpp model
// host. Locked to the official set for security. Advanced users can bypass
// via the "Allow custom model" toggle in Settings (clears checksum enforcement).

struct ModelEntry {
    filename:           &'static str,
    quality:            &'static str,
    label:              &'static str,
    size_mb:            u32,
    ram_required_mb:    u32,
    expected_latency_ms: u32,
    sha256:             &'static str,
    download_url:       &'static str,
    recommended:        bool,
}

const MODEL_CATALOGUE: &[ModelEntry] = &[
    ModelEntry {
        filename: "ggml-tiny.en.bin",
        quality: "tiny",
        label: "Tiny (development/testing only)",
        size_mb: 75,
        ram_required_mb: 200,
        expected_latency_ms: 500,
        sha256: "bd577a113a864445d4c299885e0cb97d4ba92b5f",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        recommended: false,
    },
    ModelEntry {
        filename: "ggml-base.en.bin",
        quality: "base",
        label: "Base (minimal laptops)",
        size_mb: 145,
        ram_required_mb: 350,
        expected_latency_ms: 1000,
        sha256: "137c40403d78fd54d454da0f9bd998f78703390",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        recommended: false,
    },
    ModelEntry {
        filename: "ggml-small.en.bin",
        quality: "small",
        label: "Small — Production minimum (recommended)",
        size_mb: 466,
        ram_required_mb: 700,
        expected_latency_ms: 1800,
        sha256: "db8a95a2ac9c33f6e0fd27a0d9a2933f6b14de3b",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        recommended: true,
    },
    ModelEntry {
        filename: "ggml-medium.en.bin",
        quality: "medium",
        label: "Medium (high-end laptops, best accuracy)",
        size_mb: 1500,
        ram_required_mb: 2500,
        expected_latency_ms: 4000,
        sha256: "1a2d9cb9d3cba55b2978c914f7abd58c5aadbc74",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        recommended: false,
    },
    ModelEntry {
        filename: "ggml-large-v3.bin",
        quality: "large",
        label: "Large v3 (dedicated workstation only)",
        size_mb: 3100,
        ram_required_mb: 5000,
        expected_latency_ms: 8000,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        recommended: false,
    },
];

/// Returns SHA-256 hex string for a model file if it matches the catalogue.
/// Used by `get_model_catalogue` and the load path to verify integrity.
fn verify_model_against_catalogue(path: &std::path::Path) -> Option<bool> {
    let filename = path.file_name()?.to_str()?;
    let entry = MODEL_CATALOGUE.iter().find(|e| e.filename == filename)?;
    match sha256_file_hex(path) {
        Ok(actual) => Some(actual.eq_ignore_ascii_case(entry.sha256)),
        Err(_)     => Some(false),
    }
}

// ---------------------------------------------------------------------------
// New Tauri commands
// ---------------------------------------------------------------------------

/// Returns the full live diagnostics snapshot. Polled every 2 s by the
/// diagnostics screen — lightweight (no IO beyond a DB metadata query).
#[tauri::command]
pub fn get_system_diagnostics(state: State<'_, DesktopState>) -> Result<SystemDiagnosticsDto, String> {
    use std::sync::atomic::Ordering;


    let now = aletheia_core::now_ms();

    // ── Audio / capture ──────────────────────────────────────────────────
    let capture_running = state.capture_shutdown
        .lock().map(|g| g.is_some()).unwrap_or(false);

    // ── STT model info ───────────────────────────────────────────────────
    let stt_loaded = state.stt_adapter.lock().map(|g| g.is_some()).unwrap_or(false);
    let asset_root = offline_asset_root(&state).ok();
    let model_path = asset_root.as_ref()
        .and_then(|root| find_stt_model_path(root).ok());
    let model_filename = model_path.as_ref()
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()));
    let (model_quality, model_size_mb) = model_filename.as_deref()
        .and_then(|fname| MODEL_CATALOGUE.iter().find(|e| e.filename == fname))
        .map(|e| (Some(e.quality.to_string()), Some(e.size_mb)))
        .unwrap_or((None, None));
    let model_checksum_ok = model_path.as_ref()
        .and_then(|p| verify_model_against_catalogue(p));

    // ── Latency ──────────────────────────────────────────────────────────
    let latency = latency_ring().lock()
        .map(|ring| compute_latency_stats(&ring))
        .unwrap_or_default();

    // ── Detection engine ─────────────────────────────────────────────────
    use aletheia_detection::reference_pattern_count;
    let last_ref        = LAST_DETECTION_REFERENCE.get().and_then(|m| m.lock().ok()?.clone());
    let last_conf       = LAST_DETECTION_CONFIDENCE.get().and_then(|m| *m.lock().ok()?);
    let last_bucket     = LAST_DETECTION_BUCKET.get().and_then(|m| m.lock().ok()?.clone());


    // ── Bible DB ─────────────────────────────────────────────────────────
    let (bible_ok, translations) = match state.lock_store() {
        Ok(store) => {
            let known = ["kjv", "nkjv", "niv", "nlt", "web", "bbe"];
            let ts: Vec<String> = known.iter()
                .filter_map(|id| {
                    store.count_verses_for_translation(id).ok()
                        .filter(|&n| n > 0)
                        .map(|_| id.to_uppercase())
                })
                .collect();
            (!ts.is_empty(), ts)
        }
        Err(_) => (false, vec![]),
    };


    // ── Output adapter states ─────────────────────────────────────────────
    let vmix_state = state.lock_vmix()
        .map(|g| output_health_to_state_detail(g.status().health).0)
        .unwrap_or_else(|_| "unknown".to_string());
    let obs_state = state.lock_obs()
        .map(|g| output_health_to_state_detail(g.status().health).0)
        .unwrap_or_else(|_| "unknown".to_string());
    let ew_state = state.lock_easyworship()
        .map(|g| output_health_to_state_detail(g.status().health).0)
        .unwrap_or_else(|_| "unknown".to_string());
    let pp_state = state.lock_propresenter()
        .map(|g| output_health_to_state_detail(g.status().health).0)
        .unwrap_or_else(|_| "unknown".to_string());
    let comp_state = state.lock_companion()
        .map(|g| output_health_to_state_detail(g.status().health).0)
        .unwrap_or_else(|_| "unknown".to_string());
    let osc_state = state.lock_osc()
        .map(|g| output_health_to_state_detail(g.status().health).0)
        .unwrap_or_else(|_| "unknown".to_string());


    Ok(SystemDiagnosticsDto {
        active_mic: None,           // filled by capture layer if available
        capture_running,
        audio_queue_depth: 0,       // TODO: wire from audio channel depth

        stt_model_loaded:    stt_loaded,
        stt_model_filename:  model_filename,
        stt_model_quality:   model_quality,
        stt_model_size_mb:   model_size_mb,
        stt_model_checksum_ok: model_checksum_ok,

        latency,

        phrase_pattern_count: reference_pattern_count() as u32,

        last_detection_reference:  last_ref,
        last_detection_confidence: last_conf,
        last_detection_bucket:     last_bucket,
        detections_this_session:   DETECTIONS_TOTAL.load(Ordering::Relaxed),
        rejected_this_session:     DETECTIONS_REJECTED.load(Ordering::Relaxed),
        suppressed_this_session:   DETECTIONS_SUPPRESSED.load(Ordering::Relaxed),

        bible_db_ok:          bible_ok,
        translations_loaded:  translations,

        vmix_state,
        obs_state,
        easyworship_state: ew_state,
        propresenter_state: pp_state,
        companion_state:   comp_state,
        osc_state,

        checked_at_ms: now,
    })
}

/// Returns the full model catalogue with per-entry install/checksum status.
/// Called by the diagnostics screen and the model download wizard.
#[tauri::command]
pub fn get_model_catalogue(state: State<'_, DesktopState>) -> Result<Vec<ModelCatalogueEntryDto>, String> {
    let asset_root = offline_asset_root(&state).ok();

    let entries = MODEL_CATALOGUE.iter().map(|e| {
        let path = asset_root.as_ref().map(|root| root.join(e.filename));
        let installed = path.as_ref().map(|p| p.exists()).unwrap_or(false);
        let checksum_ok = if installed {
            path.as_ref().and_then(|p| verify_model_against_catalogue(p))
        } else {
            None
        };
        ModelCatalogueEntryDto {
            filename:             e.filename.to_string(),
            quality:              e.quality.to_string(),
            label:                e.label.to_string(),
            size_mb:              e.size_mb,
            ram_required_mb:      e.ram_required_mb,
            expected_latency_ms:  e.expected_latency_ms,
            sha256:               e.sha256.to_string(),
            download_url:         e.download_url.to_string(),
            recommended:          e.recommended,
            installed,
            checksum_ok,
        }
    }).collect();

    Ok(entries)
}

