use tauri::{AppHandle, State, Emitter, Manager};
use aletheia_core::{AuditAction, now_ms};
use aletheia_detection::{KeywordLanguageDetector, LanguageDetector};
use aletheia_obs::{ObsAdapter, ObsConfig};
use aletheia_osc::{OscAdapter, OscConfig};
use aletheia_easyworship::{EasyWorshipAdapter, EasyWorshipConfig};
use aletheia_propresenter::{ProPresenterAdapter, ProPresenterConfig};
use aletheia_companion::{CompanionAdapter, CompanionConfig};
use aletheia_output::OutputAdapter;
use aletheia_store::{
    CalibrationSampleRecord, DeviceAcceptanceReceiptRecord, IntegrationConfigRecord,
    OfflineAssetStateRecord, ServiceProfileRecord,
};
use aletheia_ops::{
    ProductionReadinessReport, production_readiness_report_with_assets,
    redact_support_text, verify_signed_plugin_manifest,
};
use aletheia_stt::capture::{AudioCapture, CaptureConfig, list_input_devices};

use std::path::PathBuf;

use crate::DesktopState;
use crate::dto::*;
use crate::audit::*;
use crate::{
    scene_from_candidate, output_health_to_state_detail, live_integrations,
    production_transcript, production_candidates, detect_candidates_for_transcript,
    language_detections_from_transcript, supported_language_to_dto, accuracy_target_dto,
    merged_offline_asset_manifest, stt_readiness_from_manifest,
    offline_asset_root, sha256_file_hex, verified_manifest_to_dto, scene_to_dto,
    verse_to_search_result, sanitize_fts_query, parse_reference,
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

    let live_segments = state.snapshot_live_transcript();
    let transcript = if live_segments.is_empty() {
        production_transcript()
    } else {
        live_segments
    };

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
        candidates: production_candidates(),
        integrations: live_integrations(&state),
        health,
        preview: runtime.preview,
        live: runtime.live,
    })
}

#[tauri::command]
pub fn search_scripture(state: State<'_, DesktopState>, query: String) -> Result<Vec<SearchResultDto>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(default_search_results());
    }

    let store = state.lock_store()?;

    // Try exact reference parse first.
    if let Some((book, chapter, verse)) = parse_reference(trimmed) {
        if let Ok(Some(record)) = store.find_verse("kjv", &book, chapter, verse) {
            return Ok(vec![verse_to_search_result(record, "Exact reference")]);
        }
    }

    // Fall back to FTS phrase search.
    let sanitized = sanitize_fts_query(trimmed);
    if !sanitized.is_empty() {
        if let Ok(results) = store.search_phrase(&sanitized, 10) {
            if !results.is_empty() {
                return Ok(results
                    .into_iter()
                    .map(|record| verse_to_search_result(record, "Offline phrase match"))
                    .collect());
            }
        }
    }

    Ok(default_search_results())
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
            state: if stt_readiness.offline_models_ready { "ready" } else { "pending" }.to_string(),
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
    let resolved_model = offline_asset_root(&state)
        .ok()
        .and_then(|root| find_stt_model_path(&root).ok());
    let model_path = match resolved_model {
        Some(path) => path.display().to_string(),
        None => {
            let detail = "No Whisper STT model found. Place a ggml-*.bin model file under the offline assets folder (or your Downloads folder) and try again.".to_string();
            let _ = app.emit("aletheia://capture-error", &detail);
            return Err(detail);
        }
    };
    {
        // Block capture start when the STT adapter has not been loaded yet —
        // otherwise the worker thread will spin without ever emitting transcripts.
        let guard = state.stt_adapter.lock().map_err(|_| "stt adapter lock")?;
        if guard.is_none() {
            let detail = format!(
                "Whisper STT adapter is not loaded. Open Settings → STT and load the model at: {model_path}"
            );
            let _ = app.emit("aletheia://capture-error", &detail);
            return Err(detail);
        }
    }

    let stt_adapter = state.stt_adapter.clone();
    let live_transcript = state.live_transcript.clone();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::sync_channel::<()>(1);

    // Store the shutdown handle.
    {
        let mut guard = state.capture_shutdown.lock().map_err(|_| "shutdown lock")?;
        *guard = Some(shutdown_tx);
    }

    let handle = app.clone();
    let spawn_device = device_name.clone();
    // Spawn the capture + STT inference thread.
    std::thread::spawn(move || {
        let config = CaptureConfig { device_name: spawn_device, ..CaptureConfig::default() };
        let (capture, mut chunk_rx) = match AudioCapture::start(config) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("[capture] failed to start: {e}");
                let _ = handle.emit(
                    "aletheia://capture-error",
                    &format!(
                        "Microphone capture could not start: {e}. Check Windows microphone permission for Aletheia and confirm the chosen input device is connected."
                    ),
                );
                return;
            }
        };

        let mut seq: u64 = 0;
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

            // Run STT if model is loaded.
            let lang = language_hint.as_deref();
            let text = if let Ok(guard) = stt_adapter.lock() {
                if let Some(ref adapter) = *guard {
                    match adapter.transcribe(&chunk.samples, lang) {
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
                continue;
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
        }

        drop(capture);
        log::info!("[capture] stopped");
    });

    Ok(model_path)
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
