use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aletheia_core::{ServiceSessionId, now_ms};
use aletheia_detection::{
    AccuracyFixture, KeywordLanguageDetector, LanguageDetector, ReferenceKeywordDetector,
    ScriptureDetector, SupportedLanguage, TranscriptSegment as DetectionTranscriptSegment,
    evaluate_accuracy_fixtures,
};
use aletheia_easyworship::EasyWorshipAdapter;
use aletheia_obs::ObsAdapter;
use aletheia_propresenter::ProPresenterAdapter;
use aletheia_companion::CompanionAdapter;
use aletheia_ops::{
    AcceptanceDevice, OfflineAssetManifest, VerifiedPluginManifest,
    production_offline_asset_manifest,
};
use aletheia_osc::OscAdapter;
use aletheia_output::{OutputAdapter, OutputHealth, OutputLayer, OutputScene};
use aletheia_store::ServiceProfileRecord;
use aletheia_store::{
    AletheiaStore, IntegrationConfigRecord, OfflineAssetStateRecord, TranslationRecord,
    TrustedPluginRecord, VerseRecord,
};
use aletheia_stt::SttReadiness;
use aletheia_stt::offline::OfflineSttAdapter;
use aletheia_vmix::{VmixAdapter, VmixConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::Manager;

mod vault;
pub mod dto;
pub mod commands;
pub mod audit;
pub mod health;
pub mod rehearsal;
pub mod ccli;
pub mod fleet;
pub mod stream_server;
pub mod kv;

use crate::commands::*;
use crate::dto::*;
use crate::ccli::{
    export_ccli_usage_csv, list_ccli_usage, log_ccli_usage, CcliUsageCache,
};
use crate::fleet::{get_fleet_public_key, sign_fleet_bundle, verify_fleet_bundle};
use crate::stream_server::{
    get_stream_overlay_server_status, start_stream_overlay_server,
    stop_stream_overlay_server, update_stream_overlay_state, StreamOverlayServer,
};
use crate::kv::{kv_delete, kv_get, kv_list_keys, kv_set};


pub struct DesktopState {
    store: Mutex<AletheiaStore>,
    runtime: Mutex<RuntimeState>,
    vmix: Mutex<VmixAdapter>,
    obs: Mutex<ObsAdapter>,
    propresenter: Mutex<ProPresenterAdapter>,
    companion: Mutex<CompanionAdapter>,
    osc: Mutex<OscAdapter>,
    easyworship: Mutex<EasyWorshipAdapter>,
    database_path: PathBuf,
    /// Loaded Whisper model — shared with the background transcription task.
    stt_adapter: Arc<Mutex<Option<OfflineSttAdapter>>>,
    /// Dropping the SyncSender signals the capture thread to stop the stream.
    /// cpal::Stream is !Send on Windows, so AudioCapture lives on its own
    /// dedicated std::thread rather than in this shared state struct.
    capture_shutdown: Mutex<Option<std::sync::mpsc::SyncSender<()>>>,
    /// Rolling buffer of the most recent live transcript segments (newest first).
    /// Capped to LIVE_TRANSCRIPT_CAPACITY to avoid unbounded growth during long
    /// services. Populated by the STT inference task in start_audio_capture and
    /// consumed by get_service_state and analyze_transcript.
    live_transcript: Arc<Mutex<std::collections::VecDeque<TranscriptSegmentDto>>>,
    /// CCLI usage cache — hydrated on demand from the audit log.
    pub ccli_usage: CcliUsageCache,
    /// Local HTTP server for the stream overlay browser source.
    pub stream_overlay: StreamOverlayServer,
}

const LIVE_TRANSCRIPT_CAPACITY: usize = 50;

/// Format a unix-ms timestamp as a `HH:MM:SS` UTC clock string for display in
/// the live transcript view. We deliberately avoid pulling in chrono — std is
/// enough for a wall-clock readout and keeps the binary small.
fn format_clock_time(unix_ms: u64) -> String {
    let seconds_of_day = (unix_ms / 1000) % 86_400;
    let hours = seconds_of_day / 3600;
    let minutes = (seconds_of_day % 3600) / 60;
    let seconds = seconds_of_day % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub preview: ScriptureCandidateDto,
    pub live: ScriptureCandidateDto,
    pub destinations_armed: bool,
    pub data_miser_enabled: bool,
    pub offline_mode_enabled: bool,
    /// Human-readable operator name shown in audit logs.
    #[serde(default = "default_operator_name")]
    pub operator_name: String,
}

fn default_operator_name() -> String {
    "operator:local-booth".to_string()
}

/// Returns an ISO-ish timestamp without pulling in chrono.
fn chrono_lite_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    format!("{secs}")
}

fn default_runtime_state() -> RuntimeState {
    // Operator-safe defaults:
    //   * preview / live start EMPTY — no demo verses on the program bus.
    //   * destinations DISARMED — sending live to vMix/OBS/etc. requires an
    //     explicit operator gesture. Architecture vision: "wrong scripture is
    //     worse than no scripture." Pre-arming and pre-loading demo data
    //     meant an operator could open the app, hit "Send live", and ship a
    //     demo verse to a real congregation.
    RuntimeState {
        preview: ScriptureCandidateDto::default(),
        live: ScriptureCandidateDto::default(),
        destinations_armed: false,
        data_miser_enabled: true,
        offline_mode_enabled: true,
        operator_name: default_operator_name(),
    }
}

































/// Serialised row from the `trusted_plugins` registry (v6).

/// Aggregated calibration accuracy report (v6).


impl DesktopState {
    fn open(database_path: impl AsRef<Path>) -> Result<Self, String> {
        let database_path = database_path.as_ref().to_path_buf();
        // open_file applies migrations and guards against schema-too-new errors.
        let store = AletheiaStore::open_file(&database_path).map_err(|error| {
            format!(
                "Cannot open Aletheia database at '{}': {error}",
                database_path.display()
            )
        })?;

        // Cold-start guard: only run the synchronous critical-path seed when
        // the local KJV table is essentially empty. On warm starts this skip
        // saves ~400 SQLite commits (≈ 2–8 s on local SSD, much worse on
        // OneDrive-synced or HDD storage). The full ~31 000 verse import is
        // already done lazily on a background thread by `import_bundled_bibles`.
        let kjv_count = store
            .count_verses_for_translation("kjv")
            .unwrap_or(0);
        if kjv_count < 100 {
            seed_scripture_library(&store)?;
        }
        seed_offline_assets(&store)?;

        // Auto-install STT model files if found in the user's Downloads folder.
        // SHA256-hashing and copying 141 MB files MUST happen off the main thread
        // or Tauri's setup() blocks and Windows labels the window "Not Responding".
        let asset_root = database_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("offline-assets");
        {
            let db_bg   = database_path.clone();
            let root_bg = asset_root.clone();
            std::thread::spawn(move || {
                match AletheiaStore::open_file(&db_bg) {
                    Ok(bg_store) => auto_seed_stt_models(&bg_store, &root_bg),
                    Err(e) => log::warn!("[stt-seed] bg store open failed: {e}"),
                }
            });
        }

        // Try to restore persisted runtime state; fall back to defaults.
        let runtime = match store.load_runtime_state().ok().flatten() {
            Some(json) => {
                serde_json::from_str::<RuntimeState>(&json).unwrap_or_else(|_| {
                    log::warn!("[session] saved state is invalid — starting fresh");
                    default_runtime_state()
                })
            }
            None => default_runtime_state(),
        };

        let vmix_config = load_vmix_config(&store)?;

        // Pre-load the Whisper STT model in a background thread so it is
        // ready the moment the operator opens the Transcript screen.
        // The 142 MB ggml file typically loads in 1-2 s on an SSD.
        let stt_adapter: Arc<Mutex<Option<OfflineSttAdapter>>> = Arc::new(Mutex::new(None));
        {
            let stt_bg  = stt_adapter.clone();
            let root_bg = asset_root.clone();
            std::thread::spawn(move || {
                match find_stt_model_path(&root_bg) {
                    Ok(path) => match OfflineSttAdapter::load(&path) {
                        Ok(adapter) => {
                            if let Ok(mut g) = stt_bg.lock() {
                                *g = Some(adapter);
                            }
                            log::info!("[stt] auto-loaded model: {}", path.display());
                        }
                        Err(e) => log::warn!("[stt] auto-load failed: {e}"),
                    },
                    Err(_) => log::info!("[stt] no installed model found for auto-load"),
                }
            });
        }

        Ok(Self {
            store: Mutex::new(store),
            runtime: Mutex::new(runtime),
            vmix: Mutex::new(VmixAdapter::new(vmix_config)),
            obs: Mutex::new(ObsAdapter::default()),
            propresenter: Mutex::new(ProPresenterAdapter::default()),
            companion: Mutex::new(CompanionAdapter::default()),
            osc: Mutex::new(OscAdapter::default()),
            easyworship: Mutex::new(EasyWorshipAdapter::default()),
            database_path,
            stt_adapter,
            capture_shutdown: Mutex::new(None),
            live_transcript: Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(
                LIVE_TRANSCRIPT_CAPACITY,
            ))),
            ccli_usage: CcliUsageCache::default(),
            stream_overlay: StreamOverlayServer::default(),
        })
    }

    /// Return a snapshot of the live transcript ring buffer (newest-first).
    /// Lock is held only for the duration of the clone so callers can safely
    /// hold other locks before/after without risking deadlock.
    pub fn snapshot_live_transcript(&self) -> Vec<TranscriptSegmentDto> {
        match self.live_transcript.lock() {
            Ok(g) => g.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn audit_count(&self) -> Result<i64, String> {
        let store = self.lock_store()?;
        store
            .connection()
            .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
            .map_err(|error| error.to_string())
    }

    pub fn lock_store(&self) -> Result<std::sync::MutexGuard<'_, AletheiaStore>, String> {
        self.store
            .lock()
            .map_err(|_| "local store lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_runtime(&self) -> Result<std::sync::MutexGuard<'_, RuntimeState>, String> {
        self.runtime
            .lock()
            .map_err(|_| "runtime state lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_vmix(&self) -> Result<std::sync::MutexGuard<'_, VmixAdapter>, String> {
        self.vmix
            .lock()
            .map_err(|_| "vMix adapter lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_obs(&self) -> Result<std::sync::MutexGuard<'_, ObsAdapter>, String> {
        self.obs
            .lock()
            .map_err(|_| "OBS adapter lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_propresenter(&self) -> Result<std::sync::MutexGuard<'_, ProPresenterAdapter>, String> {
        self.propresenter
            .lock()
            .map_err(|_| "ProPresenter adapter lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_companion(&self) -> Result<std::sync::MutexGuard<'_, CompanionAdapter>, String> {
        self.companion
            .lock()
            .map_err(|_| "Companion adapter lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_osc(&self) -> Result<std::sync::MutexGuard<'_, OscAdapter>, String> {
        self.osc
            .lock()
            .map_err(|_| "OSC adapter lock is unavailable; restart Aletheia".to_string())
    }

    pub fn lock_easyworship(&self) -> Result<std::sync::MutexGuard<'_, EasyWorshipAdapter>, String> {
        self.easyworship
            .lock()
            .map_err(|_| "EasyWorship adapter lock is unavailable; restart Aletheia".to_string())
    }

    /// Persists current runtime state to the SQLite database so it survives
    /// an app restart. Called after meaningful state changes (arm, preview, live).
    pub fn persist_session(&self) -> Result<(), String> {
        let runtime = self.lock_runtime()?.clone();
        let json = serde_json::to_string(&runtime)
            .map_err(|e| format!("failed to serialize runtime state: {e}"))?;
        let store = self.lock_store()?;
        store.save_runtime_state(&json).map_err(|e| e.to_string())
    }

    /// Returns the current operator name for audit attribution.
    pub fn operator_name(&self) -> String {
        self.lock_runtime()
            .map(|r| r.operator_name.clone())
            .unwrap_or_else(|_| default_operator_name())
    }
}


























pub fn recent_integration_events(
    state: &DesktopState,
    limit: u16,
) -> Result<Vec<IntegrationEventDto>, String> {
    let store = state.lock_store()?;
    let events = store
        .recent_integration_events(limit)
        .map_err(|error| error.to_string())?;
    Ok(events
        .into_iter()
        .map(|event| IntegrationEventDto {
            timestamp_ms: event.timestamp_ms,
            integration_id: event.integration_id,
            severity: event.severity,
            action: event.action,
            detail: event.detail,
        })
        .collect())
}




// ---------------------------------------------------------------------------
// Service profile commands
// ---------------------------------------------------------------------------





pub fn service_profile_to_dto(record: &ServiceProfileRecord) -> Result<ServiceProfileDto, String> {
    let languages: Vec<String> = serde_json::from_str(&record.languages_json)
        .map_err(|e| format!("invalid languages JSON in profile '{}': {e}", record.id))?;
    Ok(ServiceProfileDto {
        id: record.id.clone(),
        name: record.name.clone(),
        languages,
        output_policy: record.output_policy.clone(),
        is_active: record.is_active,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    })
}

// ---------------------------------------------------------------------------
// OBS adapter commands
// ---------------------------------------------------------------------------






// ---------------------------------------------------------------------------
// ProPresenter adapter commands
// ---------------------------------------------------------------------------







// ---------------------------------------------------------------------------
// Bitfocus Companion adapter commands
// ---------------------------------------------------------------------------







// ---------------------------------------------------------------------------
// OSC adapter commands
// ---------------------------------------------------------------------------






// ---------------------------------------------------------------------------
// EasyWorship adapter commands
// ---------------------------------------------------------------------------






fn load_vmix_config(store: &AletheiaStore) -> Result<VmixConfig, String> {
    if let Some(record) = store
        .get_integration_config("vmix-main")
        .map_err(|error| error.to_string())?
    {
        let dto: VmixConfigDto = serde_json::from_str(&record.config_json)
            .map_err(|error| format!("saved vMix config is invalid: {error}"))?;
        return vmix_config_from_dto(dto);
    }

    let config = VmixConfig::default();
    let config_json =
        serde_json::to_string(&vmix_config_to_dto(&config)).map_err(|error| error.to_string())?;
    store
        .upsert_integration_config(&IntegrationConfigRecord {
            id: "vmix-main".to_string(),
            kind: "vmix".to_string(),
            display_name: "vMix".to_string(),
            enabled: true,
            config_json,
            secret_ref: None,
        })
        .map_err(|error| error.to_string())?;
    Ok(config)
}

pub fn vmix_config_to_dto(config: &VmixConfig) -> VmixConfigDto {
    VmixConfigDto {
        host: config.host.clone(),
        port: config.port,
        title_input: config.title_input.clone(),
        verse_field: config.verse_field.clone(),
        reference_field: config.reference_field.clone(),
        overlay_channel: config.overlay_channel,
        allow_private_network: config.allow_private_network,
        username: config.username.clone().unwrap_or_default(),
        password: config.password.clone().unwrap_or_default(),
    }
}

pub fn vmix_config_from_dto(dto: VmixConfigDto) -> Result<VmixConfig, String> {
    let username = dto.username.trim();
    let password = dto.password;
    let config = VmixConfig {
        integration_id: "vmix-main".to_string(),
        host: dto.host.trim().to_string(),
        port: dto.port,
        title_input: dto.title_input.trim().to_string(),
        verse_field: dto.verse_field.trim().to_string(),
        reference_field: dto.reference_field.trim().to_string(),
        overlay_channel: dto.overlay_channel,
        timeout_ms: 2500,
        allow_private_network: dto.allow_private_network,
        username: if username.is_empty() {
            None
        } else {
            Some(username.to_string())
        },
        password: if username.is_empty() {
            None
        } else {
            Some(password)
        },
    };
    config.validate().map_err(|error| error.to_string())?;
    Ok(config)
}

pub fn vmix_status_dto(adapter: &VmixAdapter) -> VmixStatusDto {
    let status = adapter.status();
    let (state, detail) = output_health_to_state_detail(status.health);
    let config = adapter.config();
    VmixStatusDto {
        state,
        detail,
        endpoint: config.endpoint(),
        host: config.host.clone(),
        port: config.port,
        title_input: config.title_input.clone(),
        verse_field: config.verse_field.clone(),
        reference_field: config.reference_field.clone(),
        overlay_channel: config.overlay_channel,
        allow_private_network: config.allow_private_network,
        checked_at_ms: now_ms(),
        username: config.username.clone().unwrap_or_default(),
        auth_enabled: config.username.as_deref().map(|u| !u.is_empty()).unwrap_or(false),
    }
}

pub fn output_health_to_state_detail(health: OutputHealth) -> (String, String) {
    match health {
        OutputHealth::Ready => ("ready".to_string(), "Adapter is ready.".to_string()),
        OutputHealth::Connected => (
            "connected".to_string(),
            "Adapter is reachable and configured correctly.".to_string(),
        ),
        OutputHealth::Degraded(detail) => ("degraded".to_string(), detail),
        OutputHealth::Offline(detail) => ("offline".to_string(), detail),
    }
}


/// Append an audit event using an already-acquired store handle.
///
/// CRITICAL: This function does NOT lock `state.store` itself — the caller
/// must already hold the store mutex (or be in a context where re-locking
/// would deadlock). `std::sync::Mutex` on Windows is NOT reentrant, so any
/// command that already called `state.lock_store()?` MUST pass that handle
/// directly to this function instead of calling `record_audit_state`.

/// Convenience wrapper for callers that do NOT already hold the store lock.
/// Acquires the lock, calls `record_audit`, and releases. Safe to call only
/// from contexts where the store mutex is currently free.

pub fn write_booth_pack_file(
    root: &Path,
    relative_path: &str,
    contents: &str,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&path, contents.as_bytes()).map_err(|error| error.to_string())?;
    files.push(relative_path.to_string());
    Ok(())
}

pub fn obs_browser_source_html(candidate: &ScriptureCandidateDto) -> String {
    let reference = escape_html(&format!(
        "{} {}",
        candidate.reference, candidate.translation
    ));
    let verse = escape_html(&candidate.text);
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Aletheia OBS Scripture Source</title>
  <style>
    :root {{
      color-scheme: dark;
      font-family: Inter, Arial, sans-serif;
      background: transparent;
    }}
    html, body {{
      width: 1920px;
      height: 1080px;
      margin: 0;
      overflow: hidden;
      background: transparent;
    }}
    .lower-third {{
      position: absolute;
      left: 96px;
      right: 96px;
      bottom: 76px;
      padding: 26px 32px 28px;
      border-left: 6px solid #117A5B;
      background: rgba(5, 5, 5, 0.86);
      box-shadow: 0 28px 80px rgba(0, 0, 0, 0.34);
    }}
    .reference {{
      color: #A7B3AD;
      font-size: 30px;
      font-weight: 700;
      letter-spacing: 0;
      text-transform: uppercase;
    }}
    .verse {{
      margin-top: 14px;
      color: #F6F8F7;
      font-size: 56px;
      font-weight: 700;
      line-height: 1.12;
      letter-spacing: 0;
    }}
  </style>
</head>
<body>
  <main class="lower-third" aria-label="Current scripture">
    <div class="reference">{reference}</div>
    <div class="verse">{verse}</div>
  </main>
</body>
</html>
"#
    )
}

pub fn vmix_booth_setup(candidate: &ScriptureCandidateDto, config: &VmixConfigDto) -> String {
    format!(
        "# vMix Setup\n\n\
Create or import a title named `{}`.\n\n\
Fields:\n\n\
- `{}`: {}\n\
- `{}`: {} {}\n\n\
Recommended rehearsal:\n\n\
1. Enable vMix Web Controller/API on `{}:{}`.\n\
2. Keep `Private LAN` off unless the vMix machine is on a trusted booth network.\n\
3. Press **Check vMix** in Aletheia.\n\
4. Press **Preview title** and verify the text changes without going program.\n\
5. Arm destinations, press **Take live**, then press **Clear**.\n\n\
Current preview source: {}.\n",
        config.title_input,
        config.verse_field,
        candidate.text,
        config.reference_field,
        candidate.reference,
        candidate.translation,
        config.host,
        config.port,
        candidate.reason
    )
}

pub fn booth_pack_readme(candidate: &ScriptureCandidateDto, config: &VmixConfigDto) -> String {
    format!(
        "# Aletheia Booth Compatibility Pack\n\n\
This pack was generated from the current Aletheia preview candidate.\n\n\
Current scripture: **{} {}**\n\n\
{}\n\n\
Use these files during rehearsal:\n\n\
- `obs/aletheia-browser-source.html`: add as an OBS browser source at 1920x1080.\n\
- `easyworship/current-verse.txt`: import or watch as a text handoff for EasyWorship.\n\
- `propresenter/playlist-cue.json`: use as the cue payload for ProPresenter API or manual import.\n\
- `vmix/setup.md`: configure the vMix title input `{}` with fields `{}` and `{}`.\n\
- `ndi/layers.json` and `hdmi/output-window.json`: verify output layer expectations on the real production machine.\n\
- `companion/buttons.json` and `osc/cues.json`: wire Stream Deck, Companion, or OSC controls without granting live output by default.\n\n\
Safety policy:\n\n\
- Aletheia may prepare preview output automatically.\n\
- Live output requires the operator to arm destinations first.\n\
- Keep Data Miser enabled on 3G/4G unless cloud enhancement has been rehearsed.\n\
- Do not store provider keys in these files; use the operating system secret vault.\n",
        candidate.reference,
        candidate.translation,
        candidate.text,
        config.title_input,
        config.verse_field,
        config.reference_field
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn seed_scripture_library(store: &AletheiaStore) -> Result<(), String> {
    store
        .insert_translation(&TranslationRecord {
            id: "kjv".to_string(),
            name: "King James Version".to_string(),
            language: "English".to_string(),
            license: "public-domain".to_string(),
            offline_ready: true,
        })
        .map_err(|error| error.to_string())?;

    #[rustfmt::skip]
    let verses: &[(&str, u16, u16, &str)] = &[
        // Genesis
        ("Genesis", 1, 1, "In the beginning God created the heaven and the earth."),
        ("Genesis", 1, 26, "And God said, Let us make man in our image, after our likeness."),
        ("Genesis", 28, 15, "And, behold, I am with thee, and will keep thee in all places whither thou goest."),
        ("Genesis", 50, 20, "But as for you, ye thought evil against me; but God meant it unto good."),
        // Exodus
        ("Exodus", 14, 14, "The LORD shall fight for you, and ye shall hold your peace."),
        ("Exodus", 20, 3, "Thou shalt have no other gods before me."),
        // Deuteronomy
        ("Deuteronomy", 31, 6, "Be strong and of a good courage, fear not, nor be afraid of them: for the LORD thy God, he it is that doth go with thee."),
        ("Deuteronomy", 31, 8, "And the LORD, he it is that doth go before thee; he will be with thee, he will not fail thee, neither forsake thee."),
        // Joshua
        ("Joshua", 1, 8, "This book of the law shall not depart out of thy mouth; but thou shalt meditate therein day and night."),
        ("Joshua", 1, 9, "Have not I commanded thee? Be strong and of a good courage; be not afraid, neither be thou dismayed: for the LORD thy God is with thee whithersoever thou goest."),
        // 1 Samuel
        ("1 Samuel", 16, 7, "For the LORD seeth not as man seeth; for man looketh on the outward appearance, but the LORD looketh on the heart."),
        ("1 Samuel", 17, 45, "Thou comest to me with a sword, and with a spear, and with a shield: but I come to thee in the name of the LORD of hosts."),
        // Psalm
        ("Psalm", 1, 1, "Blessed is the man that walketh not in the counsel of the ungodly, nor standeth in the way of sinners."),
        ("Psalm", 1, 3, "And he shall be like a tree planted by the rivers of water, that bringeth forth his fruit in his season."),
        ("Psalm", 23, 1, "The LORD is my shepherd; I shall not want."),
        ("Psalm", 23, 2, "He maketh me to lie down in green pastures: he leadeth me beside the still waters."),
        ("Psalm", 23, 3, "He restoreth my soul: he leadeth me in the paths of righteousness for his name's sake."),
        ("Psalm", 23, 4, "Yea, though I walk through the valley of the shadow of death, I will fear no evil: for thou art with me."),
        ("Psalm", 23, 5, "Thou preparest a table before me in the presence of mine enemies: thou anointest my head with oil; my cup runneth over."),
        ("Psalm", 23, 6, "Surely goodness and mercy shall follow me all the days of my life: and I will dwell in the house of the LORD for ever."),
        ("Psalm", 27, 1, "The LORD is my light and my salvation; whom shall I fear? the LORD is the strength of my life; of whom shall I be afraid?"),
        ("Psalm", 34, 8, "O taste and see that the LORD is good: blessed is the man that trusteth in him."),
        ("Psalm", 37, 4, "Delight thyself also in the LORD: and he shall give thee the desires of thine heart."),
        ("Psalm", 46, 1, "God is our refuge and strength, a very present help in trouble."),
        ("Psalm", 46, 10, "Be still, and know that I am God: I will be exalted among the heathen, I will be exalted in the earth."),
        ("Psalm", 51, 10, "Create in me a clean heart, O God; and renew a right spirit within me."),
        ("Psalm", 91, 1, "He that dwelleth in the secret place of the most High shall abide under the shadow of the Almighty."),
        ("Psalm", 91, 2, "I will say of the LORD, He is my refuge and my fortress: my God; in him will I trust."),
        ("Psalm", 91, 11, "For he shall give his angels charge over thee, to keep thee in all thy ways."),
        ("Psalm", 100, 4, "Enter into his gates with thanksgiving, and into his courts with praise: be thankful unto him, and bless his name."),
        ("Psalm", 103, 2, "Bless the LORD, O my soul, and forget not all his benefits."),
        ("Psalm", 119, 9, "Wherewithal shall a young man cleanse his way? by taking heed thereto according to thy word."),
        ("Psalm", 119, 11, "Thy word have I hid in mine heart, that I might not sin against thee."),
        ("Psalm", 119, 105, "Thy word is a lamp unto my feet, and a light unto my path."),
        ("Psalm", 121, 1, "I will lift up mine eyes unto the hills, from whence cometh my help."),
        ("Psalm", 121, 2, "My help cometh from the LORD, which made heaven and earth."),
        ("Psalm", 121, 8, "The LORD shall preserve thy going out and thy coming in from this time forth, and even for evermore."),
        ("Psalm", 139, 14, "I will praise thee; for I am fearfully and wonderfully made."),
        // Proverbs
        ("Proverbs", 3, 5, "Trust in the LORD with all thine heart; and lean not unto thine own understanding."),
        ("Proverbs", 3, 6, "In all thy ways acknowledge him, and he shall direct thy paths."),
        ("Proverbs", 4, 23, "Keep thy heart with all diligence; for out of it are the issues of life."),
        ("Proverbs", 18, 10, "The name of the LORD is a strong tower: the righteous runneth into it, and is safe."),
        ("Proverbs", 22, 6, "Train up a child in the way he should go: and when he is old, he will not depart from it."),
        ("Proverbs", 29, 18, "Where there is no vision, the people perish: but he that keepeth the law, happy is he."),
        // Ecclesiastes
        ("Ecclesiastes", 3, 1, "To every thing there is a season, and a time to every purpose under the heaven."),
        ("Ecclesiastes", 3, 11, "He hath made every thing beautiful in his time."),
        // Isaiah
        ("Isaiah", 6, 8, "Also I heard the voice of the Lord, saying, Whom shall I send, and who will go for us? Then said I, Here am I; send me."),
        ("Isaiah", 9, 6, "For unto us a child is born, unto us a son is given: and the government shall be upon his shoulder."),
        ("Isaiah", 26, 3, "Thou wilt keep him in perfect peace, whose mind is stayed on thee: because he trusteth in thee."),
        ("Isaiah", 40, 28, "Hast thou not known? hast thou not heard, that the everlasting God, the LORD, the Creator of the ends of the earth, fainteth not, neither is weary?"),
        ("Isaiah", 40, 29, "He giveth power to the faint; and to them that have no might he increaseth strength."),
        ("Isaiah", 40, 31, "But they that wait upon the LORD shall renew their strength; they shall mount up with wings as eagles; they shall run, and not be weary."),
        ("Isaiah", 41, 10, "Fear thou not; for I am with thee: be not dismayed; for I am thy God: I will strengthen thee; yea, I will help thee."),
        ("Isaiah", 43, 2, "When thou passest through the waters, I will be with thee; and through the rivers, they shall not overflow thee."),
        ("Isaiah", 53, 5, "But he was wounded for our transgressions, he was bruised for our iniquities: the chastisement of our peace was upon him; and with his stripes we are healed."),
        ("Isaiah", 55, 8, "For my thoughts are not your thoughts, neither are your ways my ways, saith the LORD."),
        ("Isaiah", 55, 11, "So shall my word be that goeth forth out of my mouth: it shall not return unto me void."),
        ("Isaiah", 60, 1, "Arise, shine; for thy light is come, and the glory of the LORD is risen upon thee."),
        // Jeremiah
        ("Jeremiah", 29, 11, "For I know the thoughts that I think toward you, saith the LORD, thoughts of peace, and not of evil, to give you an expected end."),
        ("Jeremiah", 29, 12, "Then shall ye call upon me, and ye shall go and pray unto me, and I will hearken unto you."),
        ("Jeremiah", 29, 13, "And ye shall seek me, and find me, when ye shall search for me with all your heart."),
        ("Jeremiah", 33, 3, "Call unto me, and I will answer thee, and shew thee great and mighty things, which thou knowest not."),
        // Lamentations
        ("Lamentations", 3, 22, "It is of the LORD's mercies that we are not consumed, because his compassions fail not."),
        ("Lamentations", 3, 23, "They are new every morning: great is thy faithfulness."),
        // Ezekiel
        ("Ezekiel", 36, 26, "A new heart also will I give you, and a new spirit will I put within you."),
        // Joel
        ("Joel", 2, 28, "And it shall come to pass afterward, that I will pour out my spirit upon all flesh; and your sons and your daughters shall prophesy."),
        // Micah
        ("Micah", 6, 8, "He hath shewed thee, O man, what is good; and what doth the LORD require of thee, but to do justly, and to love mercy, and to walk humbly with thy God?"),
        // Habakkuk
        ("Habakkuk", 2, 4, "The just shall live by his faith."),
        // Zechariah
        ("Zechariah", 4, 6, "Not by might, nor by power, but by my spirit, saith the LORD of hosts."),
        // Malachi
        ("Malachi", 3, 10, "Bring ye all the tithes into the storehouse, that there may be meat in mine house, and prove me now herewith, saith the LORD of hosts."),
        // Matthew
        ("Matthew", 5, 3, "Blessed are the poor in spirit: for theirs is the kingdom of heaven."),
        ("Matthew", 5, 4, "Blessed are they that mourn: for they shall be comforted."),
        ("Matthew", 5, 14, "Ye are the light of the world. A city that is set on an hill cannot be hid."),
        ("Matthew", 6, 9, "After this manner therefore pray ye: Our Father which art in heaven, Hallowed be thy name."),
        ("Matthew", 6, 33, "But seek ye first the kingdom of God, and his righteousness; and all these things shall be added unto you."),
        ("Matthew", 11, 28, "Come unto me, all ye that labour and are heavy laden, and I will give you rest."),
        ("Matthew", 11, 29, "Take my yoke upon you, and learn of me; for I am meek and lowly in heart: and ye shall find rest unto your souls."),
        ("Matthew", 16, 18, "And I say also unto thee, That thou art Peter, and upon this rock I will build my church; and the gates of hell shall not prevail against it."),
        ("Matthew", 18, 20, "For where two or three are gathered together in my name, there am I in the midst of them."),
        ("Matthew", 19, 26, "With men this is impossible; but with God all things are possible."),
        ("Matthew", 28, 19, "Go ye therefore, and teach all nations, baptizing them in the name of the Father, and of the Son, and of the Holy Ghost."),
        ("Matthew", 28, 20, "Teaching them to observe all things whatsoever I have commanded you: and, lo, I am with you alway, even unto the end of the world."),
        // Mark
        ("Mark", 10, 45, "For even the Son of man came not to be ministered unto, but to minister, and to give his life a ransom for many."),
        ("Mark", 11, 24, "Therefore I say unto you, What things soever ye desire, when ye pray, believe that ye receive them, and ye shall have them."),
        ("Mark", 16, 15, "And he said unto them, Go ye into all the world, and preach the gospel to every creature."),
        // Luke
        ("Luke", 1, 37, "For with God nothing shall be impossible."),
        ("Luke", 4, 18, "The Spirit of the Lord is upon me, because he hath anointed me to preach the gospel to the poor."),
        ("Luke", 15, 7, "I say unto you, that likewise joy shall be in heaven over one sinner that repenteth, more than over ninety and nine just persons, which need no repentance."),
        // John
        ("John", 1, 1, "In the beginning was the Word, and the Word was with God, and the Word was God."),
        ("John", 1, 14, "And the Word was made flesh, and dwelt among us, (and we beheld his glory, the glory as of the only begotten of the Father,) full of grace and truth."),
        ("John", 3, 16, "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."),
        ("John", 3, 17, "For God sent not his Son into the world to condemn the world; but that the world through him might be saved."),
        ("John", 10, 10, "The thief cometh not, but for to steal, and to kill, and to destroy: I am come that they might have life, and that they might have it more abundantly."),
        ("John", 11, 25, "Jesus said unto her, I am the resurrection, and the life: he that believeth in me, though he were dead, yet shall he live."),
        ("John", 11, 35, "Jesus wept."),
        ("John", 14, 1, "Let not your heart be troubled: ye believe in God, believe also in me."),
        ("John", 14, 6, "Jesus saith unto him, I am the way, the truth, and the life: no man cometh unto the Father, but by me."),
        ("John", 14, 27, "Peace I leave with you, my peace I give unto you: not as the world giveth, give I unto you."),
        ("John", 15, 5, "I am the vine, ye are the branches: He that abideth in me, and I in him, the same bringeth forth much fruit: for without me ye can do nothing."),
        ("John", 15, 13, "Greater love hath no man than this, that a man lay down his life for his friends."),
        ("John", 16, 33, "These things I have spoken unto you, that in me ye might have peace. In the world ye shall have tribulation: but be of good cheer; I have overcome the world."),
        // Acts
        ("Acts", 1, 8, "But ye shall receive power, after that the Holy Ghost is come upon you: and ye shall be witnesses unto me both in Jerusalem, and in all Judaea, and in Samaria, and unto the uttermost part of the earth."),
        ("Acts", 2, 38, "Then Peter said unto them, Repent, and be baptized every one of you in the name of Jesus Christ for the remission of sins, and ye shall receive the gift of the Holy Ghost."),
        // Romans
        ("Romans", 1, 16, "For I am not ashamed of the gospel of Christ: for it is the power of God unto salvation to every one that believeth."),
        ("Romans", 3, 23, "For all have sinned, and come short of the glory of God."),
        ("Romans", 5, 8, "But God commendeth his love toward us, in that, while we were yet sinners, Christ died for us."),
        ("Romans", 6, 23, "For the wages of sin is death; but the gift of God is eternal life through Jesus Christ our Lord."),
        ("Romans", 8, 1, "There is therefore now no condemnation to them which are in Christ Jesus, who walk not after the flesh, but after the Spirit."),
        ("Romans", 8, 28, "And we know that all things work together for good to them that love God, to them who are the called according to his purpose."),
        ("Romans", 8, 31, "What shall we then say to these things? If God be for us, who can be against us?"),
        ("Romans", 8, 37, "Nay, in all these things we are more than conquerors through him that loved us."),
        ("Romans", 8, 38, "For I am persuaded, that neither death, nor life, nor angels, nor principalities, nor powers, nor things present, nor things to come, nor height, nor depth, nor any other creature, shall be able to separate us from the love of God."),
        ("Romans", 10, 9, "That if thou shalt confess with thy mouth the Lord Jesus, and shalt believe in thine heart that God hath raised him from the dead, thou shalt be saved."),
        ("Romans", 10, 17, "So then faith cometh by hearing, and hearing by the word of God."),
        ("Romans", 12, 1, "I beseech you therefore, brethren, by the mercies of God, that ye present your bodies a living sacrifice, holy, acceptable unto God, which is your reasonable service."),
        ("Romans", 12, 2, "And be not conformed to this world: but be ye transformed by the renewing of your mind."),
        // 1 Corinthians
        ("1 Corinthians", 6, 19, "What? know ye not that your body is the temple of the Holy Ghost which is in you, which ye have of God, and ye are not your own?"),
        ("1 Corinthians", 10, 13, "There hath no temptation taken you but such as is common to man: but God is faithful, who will not suffer you to be tempted above that ye are able."),
        ("1 Corinthians", 13, 4, "Charity suffereth long, and is kind; charity envieth not; charity vaunteth not itself, is not puffed up."),
        ("1 Corinthians", 13, 13, "And now abideth faith, hope, charity, these three; but the greatest of these is charity."),
        // 2 Corinthians
        ("2 Corinthians", 4, 17, "For our light affliction, which is but for a moment, worketh for us a far more exceeding and eternal weight of glory."),
        ("2 Corinthians", 5, 7, "For we walk by faith, not by sight."),
        ("2 Corinthians", 5, 17, "Therefore if any man be in Christ, he is a new creature: old things are passed away; behold, all things are become new."),
        ("2 Corinthians", 5, 21, "For he hath made him to be sin for us, who knew no sin; that we might be made the righteousness of God in him."),
        ("2 Corinthians", 9, 8, "And God is able to make all grace abound toward you; that ye, always having all sufficiency in all things, may abound to every good work."),
        ("2 Corinthians", 12, 9, "And he said unto me, My grace is sufficient for thee: for my strength is made perfect in weakness."),
        // Galatians
        ("Galatians", 2, 20, "I am crucified with Christ: nevertheless I live; yet not I, but Christ liveth in me."),
        ("Galatians", 5, 22, "But the fruit of the Spirit is love, joy, peace, longsuffering, gentleness, goodness, faith."),
        ("Galatians", 5, 23, "Meekness, temperance: against such there is no law."),
        ("Galatians", 6, 7, "Be not deceived; God is not mocked: for whatsoever a man soweth, that shall he also reap."),
        // Ephesians
        ("Ephesians", 2, 8, "For by grace are ye saved through faith; and that not of yourselves: it is the gift of God."),
        ("Ephesians", 2, 10, "For we are his workmanship, created in Christ Jesus unto good works, which God hath before ordained that we should walk in them."),
        ("Ephesians", 3, 20, "Now unto him that is able to do exceeding abundantly above all that we ask or think, according to the power that worketh in us."),
        ("Ephesians", 4, 32, "And be ye kind one to another, tenderhearted, forgiving one another, even as God for Christ's sake hath forgiven you."),
        ("Ephesians", 6, 10, "Finally, my brethren, be strong in the Lord, and in the power of his might."),
        ("Ephesians", 6, 11, "Put on the whole armour of God, that ye may be able to stand against the wiles of the devil."),
        // Philippians
        ("Philippians", 1, 6, "Being confident of this very thing, that he which hath begun a good work in you will perform it until the day of Jesus Christ."),
        ("Philippians", 4, 4, "Rejoice in the Lord alway: and again I say, Rejoice."),
        ("Philippians", 4, 6, "Be careful for nothing; but in every thing by prayer and supplication with thanksgiving let your requests be made known unto God."),
        ("Philippians", 4, 7, "And the peace of God, which passeth all understanding, shall keep your hearts and minds through Christ Jesus."),
        ("Philippians", 4, 13, "I can do all things through Christ which strengtheneth me."),
        ("Philippians", 4, 19, "But my God shall supply all your need according to his riches in glory by Christ Jesus."),
        // Colossians
        ("Colossians", 3, 2, "Set your affection on things above, not on things on the earth."),
        ("Colossians", 3, 23, "And whatsoever ye do, do it heartily, as to the Lord, and not unto men."),
        // 1 Thessalonians
        ("1 Thessalonians", 5, 17, "Pray without ceasing."),
        ("1 Thessalonians", 5, 18, "In every thing give thanks: for this is the will of God in Christ Jesus concerning you."),
        // 2 Timothy
        ("2 Timothy", 1, 7, "For God hath not given us the spirit of fear; but of power, and of love, and of a sound mind."),
        ("2 Timothy", 2, 15, "Study to shew thyself approved unto God, a workman that needeth not to be ashamed, rightly dividing the word of truth."),
        ("2 Timothy", 3, 16, "All scripture is given by inspiration of God, and is profitable for doctrine, for reproof, for correction, for instruction in righteousness."),
        // Hebrews
        ("Hebrews", 4, 12, "For the word of God is quick, and powerful, and sharper than any twoedged sword."),
        ("Hebrews", 11, 1, "Now faith is the substance of things hoped for, the evidence of things not seen."),
        ("Hebrews", 11, 6, "But without faith it is impossible to please him: for he that cometh to God must believe that he is, and that he is a rewarder of them that diligently seek him."),
        ("Hebrews", 13, 5, "Let your conversation be without covetousness; and be content with such things as ye have: for he hath said, I will never leave thee, nor forsake thee."),
        ("Hebrews", 13, 8, "Jesus Christ the same yesterday, and to day, and for ever."),
        // James
        ("James", 1, 2, "My brethren, count it all joy when ye fall into divers temptations."),
        ("James", 1, 3, "Knowing this, that the trying of your faith worketh patience."),
        ("James", 4, 7, "Submit yourselves therefore to God. Resist the devil, and he will flee from you."),
        ("James", 5, 16, "Confess your faults one to another, and pray one for another, that ye may be healed. The effectual fervent prayer of a righteous man availeth much."),
        // 1 Peter
        ("1 Peter", 2, 9, "But ye are a chosen generation, a royal priesthood, an holy nation, a peculiar people; that ye should shew forth the praises of him who hath called you out of darkness into his marvellous light."),
        ("1 Peter", 5, 7, "Casting all your care upon him; for he careth for you."),
        // 1 John
        ("1 John", 1, 9, "If we confess our sins, he is faithful and just to forgive us our sins, and to cleanse us from all unrighteousness."),
        ("1 John", 4, 8, "He that loveth not knoweth not God; for God is love."),
        ("1 John", 4, 19, "We love him, because he first loved us."),
        // Revelation
        ("Revelation", 3, 20, "Behold, I stand at the door, and knock: if any man hear my voice, and open the door, I will come in to him, and will sup with him, and he with me."),
        ("Revelation", 21, 4, "And God shall wipe away all tears from their eyes; and there shall be no more death, neither sorrow, nor crying, neither shall there be any more pain."),
        ("Revelation", 21, 5, "And he that sat upon the throne said, Behold, I make all things new."),
    ];

    // Wrap the ~150 KJV inserts plus the seed_extra_translations call (another
    // ~250 inserts across NKJV/NIV/NLT/MSG/WEB) in a SINGLE write transaction.
    // Without this each `insert_verse` autocommits — three statements per
    // verse (verse table + FTS delete + FTS insert) means roughly 1 200 commit
    // fsyncs, which on OneDrive-synced or HDD-backed storage adds 5–20 seconds
    // to startup. One commit at the end keeps cold-start seeding under ~200 ms
    // even on slow disks.
    let conn = store.connection();
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| format!("could not start scripture seed transaction: {error}"))?;

    let seed_result = (|| -> Result<(), String> {
        for (book, chapter, verse, text) in verses {
            store
                .insert_verse(&VerseRecord {
                    translation_id: "kjv".to_string(),
                    book: book.to_string(),
                    chapter: *chapter,
                    verse: *verse,
                    text: text.to_string(),
                })
                .map_err(|error| error.to_string())?;
        }
        seed_extra_translations(store)
    })();

    match seed_result {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(|error| {
                format!("scripture seed commit failed: {error}")
            })?;
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

/// Seeds the additional English translations the operator can pick from in the
/// scripture panel: NKJV, NIV, NLT, MSG (The Message), and WEB (World English
/// Bible). NKJV/NIV/NLT/MSG carry publisher copyrights, so only a curated set
/// of high-frequency preaching references is bundled — operators are expected
/// to honour their own publisher licence agreements when pushing live. WEB is
/// public-domain and ships with the same coverage as KJV's most-used set.
fn seed_extra_translations(store: &AletheiaStore) -> Result<(), String> {
    let translations = [
        ("nkjv", "New King James Version", "publisher-thomas-nelson"),
        ("niv", "New International Version", "publisher-biblica"),
        ("nlt", "New Living Translation", "publisher-tyndale"),
        ("msg", "The Message", "publisher-navpress"),
        ("web", "World English Bible", "public-domain"),
    ];
    for (id, name, license) in translations {
        store
            .insert_translation(&TranslationRecord {
                id: id.to_string(),
                name: name.to_string(),
                language: "English".to_string(),
                license: license.to_string(),
                offline_ready: true,
            })
            .map_err(|error| error.to_string())?;
    }

    // (translation_id, book, chapter, verse, text)
    #[rustfmt::skip]
    let verses: &[(&str, &str, u16, u16, &str)] = &[
        // ---------- NKJV ----------
        ("nkjv", "Genesis", 1, 1, "In the beginning God created the heavens and the earth."),
        ("nkjv", "Joshua", 1, 9, "Have I not commanded you? Be strong and of good courage; do not be afraid, nor be dismayed, for the Lord your God is with you wherever you go."),
        ("nkjv", "Psalm", 23, 1, "The Lord is my shepherd; I shall not want."),
        ("nkjv", "Psalm", 23, 4, "Yea, though I walk through the valley of the shadow of death, I will fear no evil; for You are with me; Your rod and Your staff, they comfort me."),
        ("nkjv", "Psalm", 27, 1, "The Lord is my light and my salvation; whom shall I fear? The Lord is the strength of my life; of whom shall I be afraid?"),
        ("nkjv", "Psalm", 46, 1, "God is our refuge and strength, a very present help in trouble."),
        ("nkjv", "Psalm", 46, 10, "Be still, and know that I am God; I will be exalted among the nations, I will be exalted in the earth!"),
        ("nkjv", "Psalm", 91, 1, "He who dwells in the secret place of the Most High shall abide under the shadow of the Almighty."),
        ("nkjv", "Psalm", 119, 105, "Your word is a lamp to my feet and a light to my path."),
        ("nkjv", "Proverbs", 3, 5, "Trust in the Lord with all your heart, and lean not on your own understanding."),
        ("nkjv", "Proverbs", 3, 6, "In all your ways acknowledge Him, and He shall direct your paths."),
        ("nkjv", "Isaiah", 26, 3, "You will keep him in perfect peace, whose mind is stayed on You, because he trusts in You."),
        ("nkjv", "Isaiah", 40, 31, "But those who wait on the Lord shall renew their strength; they shall mount up with wings like eagles, they shall run and not be weary, they shall walk and not faint."),
        ("nkjv", "Isaiah", 41, 10, "Fear not, for I am with you; be not dismayed, for I am your God. I will strengthen you, yes, I will help you, I will uphold you with My righteous right hand."),
        ("nkjv", "Isaiah", 53, 5, "But He was wounded for our transgressions, He was bruised for our iniquities; the chastisement for our peace was upon Him, and by His stripes we are healed."),
        ("nkjv", "Jeremiah", 29, 11, "For I know the thoughts that I think toward you, says the Lord, thoughts of peace and not of evil, to give you a future and a hope."),
        ("nkjv", "Jeremiah", 33, 3, "Call to Me, and I will answer you, and show you great and mighty things, which you do not know."),
        ("nkjv", "Lamentations", 3, 22, "Through the Lord's mercies we are not consumed, because His compassions fail not."),
        ("nkjv", "Lamentations", 3, 23, "They are new every morning; great is Your faithfulness."),
        ("nkjv", "Matthew", 6, 33, "But seek first the kingdom of God and His righteousness, and all these things shall be added to you."),
        ("nkjv", "Matthew", 11, 28, "Come to Me, all you who labor and are heavy laden, and I will give you rest."),
        ("nkjv", "Matthew", 28, 19, "Go therefore and make disciples of all the nations, baptizing them in the name of the Father and of the Son and of the Holy Spirit."),
        ("nkjv", "Matthew", 28, 20, "Teaching them to observe all things that I have commanded you; and lo, I am with you always, even to the end of the age."),
        ("nkjv", "John", 3, 16, "For God so loved the world that He gave His only begotten Son, that whoever believes in Him should not perish but have everlasting life."),
        ("nkjv", "John", 10, 10, "The thief does not come except to steal, and to kill, and to destroy. I have come that they may have life, and that they may have it more abundantly."),
        ("nkjv", "John", 14, 6, "Jesus said to him, 'I am the way, the truth, and the life. No one comes to the Father except through Me.'"),
        ("nkjv", "John", 14, 27, "Peace I leave with you, My peace I give to you; not as the world gives do I give to you. Let not your heart be troubled, neither let it be afraid."),
        ("nkjv", "John", 16, 33, "These things I have spoken to you, that in Me you may have peace. In the world you will have tribulation; but be of good cheer, I have overcome the world."),
        ("nkjv", "Romans", 8, 28, "And we know that all things work together for good to those who love God, to those who are the called according to His purpose."),
        ("nkjv", "Romans", 8, 31, "What then shall we say to these things? If God is for us, who can be against us?"),
        ("nkjv", "Romans", 8, 37, "Yet in all these things we are more than conquerors through Him who loved us."),
        ("nkjv", "Romans", 10, 9, "That if you confess with your mouth the Lord Jesus and believe in your heart that God has raised Him from the dead, you will be saved."),
        ("nkjv", "Romans", 12, 2, "And do not be conformed to this world, but be transformed by the renewing of your mind."),
        ("nkjv", "2 Corinthians", 5, 17, "Therefore, if anyone is in Christ, he is a new creation; old things have passed away; behold, all things have become new."),
        ("nkjv", "Galatians", 2, 20, "I have been crucified with Christ; it is no longer I who live, but Christ lives in me; and the life which I now live in the flesh I live by faith in the Son of God, who loved me and gave Himself for me."),
        ("nkjv", "Ephesians", 2, 8, "For by grace you have been saved through faith, and that not of yourselves; it is the gift of God."),
        ("nkjv", "Ephesians", 6, 10, "Finally, my brethren, be strong in the Lord and in the power of His might."),
        ("nkjv", "Philippians", 4, 6, "Be anxious for nothing, but in everything by prayer and supplication, with thanksgiving, let your requests be made known to God."),
        ("nkjv", "Philippians", 4, 7, "And the peace of God, which surpasses all understanding, will guard your hearts and minds through Christ Jesus."),
        ("nkjv", "Philippians", 4, 13, "I can do all things through Christ who strengthens me."),
        ("nkjv", "Philippians", 4, 19, "And my God shall supply all your need according to His riches in glory by Christ Jesus."),
        ("nkjv", "Hebrews", 11, 1, "Now faith is the substance of things hoped for, the evidence of things not seen."),
        ("nkjv", "Hebrews", 13, 5, "Let your conduct be without covetousness; be content with such things as you have. For He Himself has said, 'I will never leave you nor forsake you.'"),
        ("nkjv", "Hebrews", 13, 8, "Jesus Christ is the same yesterday, today, and forever."),
        ("nkjv", "James", 1, 2, "My brethren, count it all joy when you fall into various trials."),
        ("nkjv", "1 Peter", 5, 7, "Casting all your care upon Him, for He cares for you."),
        ("nkjv", "1 John", 1, 9, "If we confess our sins, He is faithful and just to forgive us our sins and to cleanse us from all unrighteousness."),
        ("nkjv", "1 John", 4, 8, "He who does not love does not know God, for God is love."),
        ("nkjv", "Revelation", 3, 20, "Behold, I stand at the door and knock. If anyone hears My voice and opens the door, I will come in to him and dine with him, and he with Me."),

        // ---------- NIV ----------
        ("niv", "Genesis", 1, 1, "In the beginning God created the heavens and the earth."),
        ("niv", "Joshua", 1, 9, "Have I not commanded you? Be strong and courageous. Do not be afraid; do not be discouraged, for the Lord your God will be with you wherever you go."),
        ("niv", "Psalm", 23, 1, "The Lord is my shepherd, I lack nothing."),
        ("niv", "Psalm", 23, 4, "Even though I walk through the darkest valley, I will fear no evil, for you are with me; your rod and your staff, they comfort me."),
        ("niv", "Psalm", 27, 1, "The Lord is my light and my salvation— whom shall I fear? The Lord is the stronghold of my life— of whom shall I be afraid?"),
        ("niv", "Psalm", 46, 1, "God is our refuge and strength, an ever-present help in trouble."),
        ("niv", "Psalm", 46, 10, "He says, 'Be still, and know that I am God; I will be exalted among the nations, I will be exalted in the earth.'"),
        ("niv", "Psalm", 91, 1, "Whoever dwells in the shelter of the Most High will rest in the shadow of the Almighty."),
        ("niv", "Psalm", 119, 105, "Your word is a lamp for my feet, a light on my path."),
        ("niv", "Proverbs", 3, 5, "Trust in the Lord with all your heart and lean not on your own understanding;"),
        ("niv", "Proverbs", 3, 6, "in all your ways submit to him, and he will make your paths straight."),
        ("niv", "Isaiah", 26, 3, "You will keep in perfect peace those whose minds are steadfast, because they trust in you."),
        ("niv", "Isaiah", 40, 31, "but those who hope in the Lord will renew their strength. They will soar on wings like eagles; they will run and not grow weary, they will walk and not be faint."),
        ("niv", "Isaiah", 41, 10, "So do not fear, for I am with you; do not be dismayed, for I am your God. I will strengthen you and help you; I will uphold you with my righteous right hand."),
        ("niv", "Isaiah", 53, 5, "But he was pierced for our transgressions, he was crushed for our iniquities; the punishment that brought us peace was on him, and by his wounds we are healed."),
        ("niv", "Jeremiah", 29, 11, "For I know the plans I have for you, declares the Lord, plans to prosper you and not to harm you, plans to give you hope and a future."),
        ("niv", "Jeremiah", 33, 3, "Call to me and I will answer you and tell you great and unsearchable things you do not know."),
        ("niv", "Lamentations", 3, 22, "Because of the Lord's great love we are not consumed, for his compassions never fail."),
        ("niv", "Lamentations", 3, 23, "They are new every morning; great is your faithfulness."),
        ("niv", "Matthew", 6, 33, "But seek first his kingdom and his righteousness, and all these things will be given to you as well."),
        ("niv", "Matthew", 11, 28, "Come to me, all you who are weary and burdened, and I will give you rest."),
        ("niv", "Matthew", 28, 19, "Therefore go and make disciples of all nations, baptizing them in the name of the Father and of the Son and of the Holy Spirit,"),
        ("niv", "Matthew", 28, 20, "and teaching them to obey everything I have commanded you. And surely I am with you always, to the very end of the age."),
        ("niv", "John", 3, 16, "For God so loved the world that he gave his one and only Son, that whoever believes in him shall not perish but have eternal life."),
        ("niv", "John", 10, 10, "The thief comes only to steal and kill and destroy; I have come that they may have life, and have it to the full."),
        ("niv", "John", 14, 6, "Jesus answered, 'I am the way and the truth and the life. No one comes to the Father except through me.'"),
        ("niv", "John", 14, 27, "Peace I leave with you; my peace I give you. I do not give to you as the world gives. Do not let your hearts be troubled and do not be afraid."),
        ("niv", "John", 16, 33, "I have told you these things, so that in me you may have peace. In this world you will have trouble. But take heart! I have overcome the world."),
        ("niv", "Romans", 8, 28, "And we know that in all things God works for the good of those who love him, who have been called according to his purpose."),
        ("niv", "Romans", 8, 31, "What, then, shall we say in response to these things? If God is for us, who can be against us?"),
        ("niv", "Romans", 8, 37, "No, in all these things we are more than conquerors through him who loved us."),
        ("niv", "Romans", 10, 9, "If you declare with your mouth, 'Jesus is Lord,' and believe in your heart that God raised him from the dead, you will be saved."),
        ("niv", "Romans", 12, 2, "Do not conform to the pattern of this world, but be transformed by the renewing of your mind."),
        ("niv", "2 Corinthians", 5, 17, "Therefore, if anyone is in Christ, the new creation has come: The old has gone, the new is here!"),
        ("niv", "Galatians", 2, 20, "I have been crucified with Christ and I no longer live, but Christ lives in me. The life I now live in the body, I live by faith in the Son of God, who loved me and gave himself for me."),
        ("niv", "Ephesians", 2, 8, "For it is by grace you have been saved, through faith—and this is not from yourselves, it is the gift of God—"),
        ("niv", "Ephesians", 6, 10, "Finally, be strong in the Lord and in his mighty power."),
        ("niv", "Philippians", 4, 6, "Do not be anxious about anything, but in every situation, by prayer and petition, with thanksgiving, present your requests to God."),
        ("niv", "Philippians", 4, 7, "And the peace of God, which transcends all understanding, will guard your hearts and your minds in Christ Jesus."),
        ("niv", "Philippians", 4, 13, "I can do all this through him who gives me strength."),
        ("niv", "Philippians", 4, 19, "And my God will meet all your needs according to the riches of his glory in Christ Jesus."),
        ("niv", "Hebrews", 11, 1, "Now faith is confidence in what we hope for and assurance about what we do not see."),
        ("niv", "Hebrews", 13, 5, "Keep your lives free from the love of money and be content with what you have, because God has said, 'Never will I leave you; never will I forsake you.'"),
        ("niv", "Hebrews", 13, 8, "Jesus Christ is the same yesterday and today and forever."),
        ("niv", "James", 1, 2, "Consider it pure joy, my brothers and sisters, whenever you face trials of many kinds,"),
        ("niv", "1 Peter", 5, 7, "Cast all your anxiety on him because he cares for you."),
        ("niv", "1 John", 1, 9, "If we confess our sins, he is faithful and just and will forgive us our sins and purify us from all unrighteousness."),
        ("niv", "1 John", 4, 8, "Whoever does not love does not know God, because God is love."),
        ("niv", "Revelation", 3, 20, "Here I am! I stand at the door and knock. If anyone hears my voice and opens the door, I will come in and eat with that person, and they with me."),

        // ---------- NLT ----------
        ("nlt", "Genesis", 1, 1, "In the beginning God created the heavens and the earth."),
        ("nlt", "Joshua", 1, 9, "This is my command—be strong and courageous! Do not be afraid or discouraged. For the Lord your God is with you wherever you go."),
        ("nlt", "Psalm", 23, 1, "The Lord is my shepherd; I have all that I need."),
        ("nlt", "Psalm", 23, 4, "Even when I walk through the darkest valley, I will not be afraid, for you are close beside me. Your rod and your staff protect and comfort me."),
        ("nlt", "Psalm", 27, 1, "The Lord is my light and my salvation—so why should I be afraid? The Lord is my fortress, protecting me from danger, so why should I tremble?"),
        ("nlt", "Psalm", 46, 1, "God is our refuge and strength, always ready to help in times of trouble."),
        ("nlt", "Psalm", 46, 10, "Be still, and know that I am God! I will be honored by every nation. I will be honored throughout the world."),
        ("nlt", "Psalm", 91, 1, "Those who live in the shelter of the Most High will find rest in the shadow of the Almighty."),
        ("nlt", "Psalm", 119, 105, "Your word is a lamp to guide my feet and a light for my path."),
        ("nlt", "Proverbs", 3, 5, "Trust in the Lord with all your heart; do not depend on your own understanding."),
        ("nlt", "Proverbs", 3, 6, "Seek his will in all you do, and he will show you which path to take."),
        ("nlt", "Isaiah", 26, 3, "You will keep in perfect peace all who trust in you, all whose thoughts are fixed on you!"),
        ("nlt", "Isaiah", 40, 31, "But those who trust in the Lord will find new strength. They will soar high on wings like eagles. They will run and not grow weary. They will walk and not faint."),
        ("nlt", "Isaiah", 41, 10, "Don't be afraid, for I am with you. Don't be discouraged, for I am your God. I will strengthen you and help you. I will hold you up with my victorious right hand."),
        ("nlt", "Isaiah", 53, 5, "But he was pierced for our rebellion, crushed for our sins. He was beaten so we could be whole. He was whipped so we could be healed."),
        ("nlt", "Jeremiah", 29, 11, "For I know the plans I have for you, says the Lord. They are plans for good and not for disaster, to give you a future and a hope."),
        ("nlt", "Jeremiah", 33, 3, "Ask me and I will tell you remarkable secrets you do not know about things to come."),
        ("nlt", "Lamentations", 3, 22, "The faithful love of the Lord never ends! His mercies never cease."),
        ("nlt", "Lamentations", 3, 23, "Great is his faithfulness; his mercies begin afresh each morning."),
        ("nlt", "Matthew", 6, 33, "Seek the Kingdom of God above all else, and live righteously, and he will give you everything you need."),
        ("nlt", "Matthew", 11, 28, "Then Jesus said, 'Come to me, all of you who are weary and carry heavy burdens, and I will give you rest.'"),
        ("nlt", "Matthew", 28, 19, "Therefore, go and make disciples of all the nations, baptizing them in the name of the Father and the Son and the Holy Spirit."),
        ("nlt", "Matthew", 28, 20, "Teach these new disciples to obey all the commands I have given you. And be sure of this: I am with you always, even to the end of the age."),
        ("nlt", "John", 3, 16, "For this is how God loved the world: He gave his one and only Son, so that everyone who believes in him will not perish but have eternal life."),
        ("nlt", "John", 10, 10, "The thief's purpose is to steal and kill and destroy. My purpose is to give them a rich and satisfying life."),
        ("nlt", "John", 14, 6, "Jesus told him, 'I am the way, the truth, and the life. No one can come to the Father except through me.'"),
        ("nlt", "John", 14, 27, "I am leaving you with a gift—peace of mind and heart. And the peace I give is a gift the world cannot give. So don't be troubled or afraid."),
        ("nlt", "John", 16, 33, "I have told you all this so that you may have peace in me. Here on earth you will have many trials and sorrows. But take heart, because I have overcome the world."),
        ("nlt", "Romans", 8, 28, "And we know that God causes everything to work together for the good of those who love God and are called according to his purpose for them."),
        ("nlt", "Romans", 8, 31, "What shall we say about such wonderful things as these? If God is for us, who can ever be against us?"),
        ("nlt", "Romans", 8, 37, "No, despite all these things, overwhelming victory is ours through Christ, who loved us."),
        ("nlt", "Romans", 10, 9, "If you openly declare that Jesus is Lord and believe in your heart that God raised him from the dead, you will be saved."),
        ("nlt", "Romans", 12, 2, "Don't copy the behavior and customs of this world, but let God transform you into a new person by changing the way you think."),
        ("nlt", "2 Corinthians", 5, 17, "This means that anyone who belongs to Christ has become a new person. The old life is gone; a new life has begun!"),
        ("nlt", "Galatians", 2, 20, "My old self has been crucified with Christ. It is no longer I who live, but Christ lives in me. So I live in this earthly body by trusting in the Son of God, who loved me and gave himself for me."),
        ("nlt", "Ephesians", 2, 8, "God saved you by his grace when you believed. And you can't take credit for this; it is a gift from God."),
        ("nlt", "Ephesians", 6, 10, "A final word: Be strong in the Lord and in his mighty power."),
        ("nlt", "Philippians", 4, 6, "Don't worry about anything; instead, pray about everything. Tell God what you need, and thank him for all he has done."),
        ("nlt", "Philippians", 4, 7, "Then you will experience God's peace, which exceeds anything we can understand. His peace will guard your hearts and minds as you live in Christ Jesus."),
        ("nlt", "Philippians", 4, 13, "For I can do everything through Christ, who gives me strength."),
        ("nlt", "Philippians", 4, 19, "And this same God who takes care of me will supply all your needs from his glorious riches, which have been given to us in Christ Jesus."),
        ("nlt", "Hebrews", 11, 1, "Faith is the confidence that what we hope for will actually happen; it gives us assurance about things we cannot see."),
        ("nlt", "Hebrews", 13, 5, "Don't love money; be satisfied with what you have. For God has said, 'I will never fail you. I will never abandon you.'"),
        ("nlt", "Hebrews", 13, 8, "Jesus Christ is the same yesterday, today, and forever."),
        ("nlt", "James", 1, 2, "Dear brothers and sisters, when troubles of any kind come your way, consider it an opportunity for great joy."),
        ("nlt", "1 Peter", 5, 7, "Give all your worries and cares to God, for he cares about you."),
        ("nlt", "1 John", 1, 9, "But if we confess our sins to him, he is faithful and just to forgive us our sins and to cleanse us from all wickedness."),
        ("nlt", "1 John", 4, 8, "But anyone who does not love does not know God, for God is love."),
        ("nlt", "Revelation", 3, 20, "Look! I stand at the door and knock. If you hear my voice and open the door, I will come in, and we will share a meal together as friends."),

        // ---------- MSG (The Message) ----------
        ("msg", "Genesis", 1, 1, "First this: God created the Heavens and Earth—all you see, all you don't see."),
        ("msg", "Joshua", 1, 9, "Haven't I commanded you? Strength! Courage! Don't be timid; don't get discouraged. God, your God, is with you every step you take."),
        ("msg", "Psalm", 23, 1, "God, my shepherd! I don't need a thing."),
        ("msg", "Psalm", 23, 4, "Even when the way goes through Death Valley, I'm not afraid when you walk at my side. Your trusty shepherd's crook makes me feel secure."),
        ("msg", "Psalm", 46, 10, "Step out of the traffic! Take a long, loving look at me, your High God, above politics, above everything."),
        ("msg", "Psalm", 91, 1, "You who sit down in the High God's presence, spend the night in Shaddai's shadow,"),
        ("msg", "Psalm", 119, 105, "By your words I can see where I'm going; they throw a beam of light on my dark path."),
        ("msg", "Proverbs", 3, 5, "Trust God from the bottom of your heart; don't try to figure out everything on your own."),
        ("msg", "Proverbs", 3, 6, "Listen for God's voice in everything you do, everywhere you go; he's the one who will keep you on track."),
        ("msg", "Isaiah", 40, 31, "But those who wait upon God get fresh strength. They spread their wings and soar like eagles, they run and don't get tired, they walk and don't lag behind."),
        ("msg", "Isaiah", 41, 10, "Don't panic. I'm with you. There's no need to fear for I'm your God. I'll give you strength. I'll help you. I'll hold you steady, keep a firm grip on you."),
        ("msg", "Jeremiah", 29, 11, "I know what I'm doing. I have it all planned out—plans to take care of you, not abandon you, plans to give you the future you hope for."),
        ("msg", "Matthew", 6, 33, "Steep your life in God-reality, God-initiative, God-provisions. Don't worry about missing out. You'll find all your everyday human concerns will be met."),
        ("msg", "Matthew", 11, 28, "Are you tired? Worn out? Burned out on religion? Come to me. Get away with me and you'll recover your life. I'll show you how to take a real rest."),
        ("msg", "John", 3, 16, "This is how much God loved the world: He gave his Son, his one and only Son. And this is why: so that no one need be destroyed; by believing in him, anyone can have a whole and lasting life."),
        ("msg", "John", 10, 10, "A thief is only there to steal and kill and destroy. I came so they can have real and eternal life, more and better life than they ever dreamed of."),
        ("msg", "John", 14, 6, "Jesus said, 'I am the Road, also the Truth, also the Life. No one gets to the Father apart from me.'"),
        ("msg", "John", 14, 27, "I'm leaving you well and whole. That's my parting gift to you. Peace. I don't leave you the way you're used to being left—feeling abandoned, bereft. So don't be upset. Don't be distraught."),
        ("msg", "Romans", 8, 28, "That's why we can be so sure that every detail in our lives of love for God is worked into something good."),
        ("msg", "Romans", 8, 31, "So, what do you think? With God on our side like this, how can we lose?"),
        ("msg", "Romans", 12, 2, "Don't become so well-adjusted to your culture that you fit into it without even thinking. Instead, fix your attention on God. You'll be changed from the inside out."),
        ("msg", "2 Corinthians", 5, 17, "Now we look inside, and what we see is that anyone united with the Messiah gets a fresh start, is created new. The old life is gone; a new life burgeons!"),
        ("msg", "Ephesians", 2, 8, "Saving is all his idea, and all his work. All we do is trust him enough to let him do it. It's God's gift from start to finish!"),
        ("msg", "Philippians", 4, 6, "Don't fret or worry. Instead of worrying, pray. Let petitions and praises shape your worries into prayers, letting God know your concerns."),
        ("msg", "Philippians", 4, 7, "Before you know it, a sense of God's wholeness, everything coming together for good, will come and settle you down."),
        ("msg", "Philippians", 4, 13, "Whatever I have, wherever I am, I can make it through anything in the One who makes me who I am."),
        ("msg", "Philippians", 4, 19, "You can be sure that God will take care of everything you need, his generosity exceeding even yours in the glory that pours from Jesus."),
        ("msg", "Hebrews", 11, 1, "The fundamental fact of existence is that this trust in God, this faith, is the firm foundation under everything that makes life worth living."),
        ("msg", "Hebrews", 13, 5, "Don't be obsessed with getting more material things. Be relaxed with what you have. Since God assured us, 'I'll never let you down, never walk off and leave you.'"),
        ("msg", "Hebrews", 13, 8, "For Jesus doesn't change—yesterday, today, tomorrow, he's always totally himself."),
        ("msg", "James", 1, 2, "Consider it a sheer gift, friends, when tests and challenges come at you from all sides."),
        ("msg", "1 Peter", 5, 7, "Live carefree before God; he is most careful with you."),
        ("msg", "1 John", 1, 9, "On the other hand, if we admit our sins—make a clean breast of them—he won't let us down; he'll be true to himself. He'll forgive our sins and purge us of all wrongdoing."),
        ("msg", "1 John", 4, 8, "The person who refuses to love doesn't know the first thing about God, because God is love—so you can't know him if you don't love."),
        ("msg", "Revelation", 3, 20, "Look at me. I stand at the door. I knock. If you hear me call and open the door, I'll come right in and sit down to supper with you."),

        // ---------- WEB (World English Bible — public domain) ----------
        ("web", "Genesis", 1, 1, "In the beginning, God created the heavens and the earth."),
        ("web", "Joshua", 1, 9, "Haven't I commanded you? Be strong and courageous. Don't be afraid. Don't be dismayed, for Yahweh your God is with you wherever you go."),
        ("web", "Psalm", 23, 1, "Yahweh is my shepherd: I shall lack nothing."),
        ("web", "Psalm", 23, 4, "Even though I walk through the valley of the shadow of death, I will fear no evil, for you are with me. Your rod and your staff, they comfort me."),
        ("web", "Psalm", 27, 1, "Yahweh is my light and my salvation. Whom shall I fear? Yahweh is the strength of my life. Of whom shall I be afraid?"),
        ("web", "Psalm", 46, 1, "God is our refuge and strength, a very present help in trouble."),
        ("web", "Psalm", 46, 10, "Be still, and know that I am God. I will be exalted among the nations. I will be exalted in the earth."),
        ("web", "Psalm", 91, 1, "He who dwells in the secret place of the Most High will rest in the shadow of the Almighty."),
        ("web", "Psalm", 119, 105, "Your word is a lamp to my feet, and a light for my path."),
        ("web", "Proverbs", 3, 5, "Trust in Yahweh with all your heart, and don't lean on your own understanding."),
        ("web", "Proverbs", 3, 6, "In all your ways acknowledge him, and he will make your paths straight."),
        ("web", "Isaiah", 26, 3, "You will keep whoever's mind is steadfast in perfect peace, because he trusts in you."),
        ("web", "Isaiah", 40, 31, "but those who wait for Yahweh will renew their strength. They will mount up with wings like eagles. They will run, and not be weary. They will walk, and not faint."),
        ("web", "Isaiah", 41, 10, "Don't you be afraid, for I am with you. Don't be dismayed, for I am your God. I will strengthen you. I will help you. I will uphold you with the right hand of my righteousness."),
        ("web", "Jeremiah", 29, 11, "For I know the thoughts that I think toward you, says Yahweh, thoughts of peace, and not of evil, to give you hope and a future."),
        ("web", "Matthew", 6, 33, "But seek first God's Kingdom, and his righteousness; and all these things will be given to you as well."),
        ("web", "Matthew", 11, 28, "Come to me, all you who labor and are heavily burdened, and I will give you rest."),
        ("web", "John", 3, 16, "For God so loved the world, that he gave his one and only Son, that whoever believes in him should not perish, but have eternal life."),
        ("web", "John", 14, 6, "Jesus said to him, 'I am the way, the truth, and the life. No one comes to the Father, except through me.'"),
        ("web", "Romans", 8, 28, "We know that all things work together for good for those who love God, for those who are called according to his purpose."),
        ("web", "Romans", 8, 31, "What then shall we say about these things? If God is for us, who can be against us?"),
        ("web", "Romans", 10, 9, "that if you will confess with your mouth that Jesus is Lord, and believe in your heart that God raised him from the dead, you will be saved."),
        ("web", "2 Corinthians", 5, 17, "Therefore if anyone is in Christ, he is a new creation. The old things have passed away. Behold, all things have become new."),
        ("web", "Ephesians", 2, 8, "for by grace you have been saved through faith, and that not of yourselves; it is the gift of God,"),
        ("web", "Philippians", 4, 6, "In nothing be anxious, but in everything, by prayer and petition with thanksgiving, let your requests be made known to God."),
        ("web", "Philippians", 4, 7, "And the peace of God, which surpasses all understanding, will guard your hearts and your thoughts in Christ Jesus."),
        ("web", "Philippians", 4, 13, "I can do all things through Christ, who strengthens me."),
        ("web", "Hebrews", 11, 1, "Now faith is assurance of things hoped for, proof of things not seen."),
        ("web", "Hebrews", 13, 8, "Jesus Christ is the same yesterday, today, and forever."),
        ("web", "1 Peter", 5, 7, "casting all your worries on him, because he cares for you."),
        ("web", "1 John", 1, 9, "If we confess our sins, he is faithful and righteous to forgive us the sins, and to cleanse us from all unrighteousness."),
        ("web", "1 John", 4, 8, "He who doesn't love doesn't know God, for God is love."),
        ("web", "Revelation", 3, 20, "Behold, I stand at the door and knock. If anyone hears my voice and opens the door, then I will come in to him, and will dine with him, and he with me."),
    ];

    for (translation, book, chapter, verse, text) in verses {
        store
            .insert_verse(&VerseRecord {
                translation_id: translation.to_string(),
                book: book.to_string(),
                chapter: *chapter,
                verse: *verse,
                text: text.to_string(),
            })
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// One book entry in the bundled JSON Bible (thiagobodruk format).
#[derive(serde::Deserialize)]
struct BundledBibleBook {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    abbrev: Option<String>,
    chapters: Vec<Vec<String>>,
}

/// Imports a full Bible from a JSON file matching the thiagobodruk schema —
/// `[{"name":"Genesis","abbrev":"gn","chapters":[["v1","v2",...],...]}, ...]`.
/// Wraps the entire insert in a single transaction for ~50× speedup over
/// per-row autocommit (4.5 MB file → ~31k inserts in ~2–4 s).
///
/// Returns the number of verses inserted.
pub fn import_full_bible_from_json(
    store: &AletheiaStore,
    translation_id: &str,
    translation_name: &str,
    license: &str,
    json_path: &Path,
) -> Result<usize, String> {
    let bytes = std::fs::read(json_path)
        .map_err(|error| format!("could not read Bible JSON {}: {error}", json_path.display()))?;
    // Some sources prepend a UTF-8 BOM; strip it before serde parses.
    let json_text = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        std::str::from_utf8(&bytes[3..])
    } else {
        std::str::from_utf8(&bytes)
    }
    .map_err(|error| format!("Bible JSON is not valid UTF-8: {error}"))?;
    let books: Vec<BundledBibleBook> = serde_json::from_str(json_text)
        .map_err(|error| format!("Bible JSON parse failed: {error}"))?;

    store
        .insert_translation(&TranslationRecord {
            id: translation_id.to_string(),
            name: translation_name.to_string(),
            language: "English".to_string(),
            license: license.to_string(),
            offline_ready: true,
        })
        .map_err(|error| error.to_string())?;

    let conn = store.connection();
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| format!("could not start Bible import transaction: {error}"))?;

    let mut inserted: usize = 0;
    let result = (|| -> Result<(), String> {
        for book in &books {
            let book_name = book
                .name
                .clone()
                .or_else(|| book.abbrev.as_deref().map(canonical_book_name))
                .unwrap_or_else(|| "Unknown".to_string());
            for (chapter_index, chapter) in book.chapters.iter().enumerate() {
                let chapter_no = (chapter_index + 1) as u16;
                for (verse_index, text) in chapter.iter().enumerate() {
                    let verse_no = (verse_index + 1) as u16;
                    store
                        .insert_verse(&VerseRecord {
                            translation_id: translation_id.to_string(),
                            book: book_name.clone(),
                            chapter: chapter_no,
                            verse: verse_no,
                            text: text.clone(),
                        })
                        .map_err(|error| error.to_string())?;
                    inserted += 1;
                }
            }
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")
                .map_err(|error| format!("Bible import commit failed: {error}"))?;
            log::info!(
                "[bible-import] {translation_id}: inserted {inserted} verses from {}",
                json_path.display()
            );
            Ok(inserted)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

/// Maps the lowercase abbreviations used by the thiagobodruk Bible JSON to the
/// canonical book names used elsewhere in Aletheia. Falls back to the raw
/// abbreviation if unknown so imports never silently drop verses.
fn canonical_book_name(abbrev: &str) -> String {
    match abbrev.to_ascii_lowercase().as_str() {
        "gn" | "gen" => "Genesis",
        "ex" | "exo" => "Exodus",
        "lv" | "lev" => "Leviticus",
        "nm" | "num" => "Numbers",
        "dt" | "deu" => "Deuteronomy",
        "js" | "jos" => "Joshua",
        "jud" | "jdg" => "Judges",
        "rt" | "rut" => "Ruth",
        "1sm" | "1sa" => "1 Samuel",
        "2sm" | "2sa" => "2 Samuel",
        "1kgs" | "1ki" => "1 Kings",
        "2kgs" | "2ki" => "2 Kings",
        "1ch" | "1chr" => "1 Chronicles",
        "2ch" | "2chr" => "2 Chronicles",
        "ezr" => "Ezra",
        "ne" | "neh" => "Nehemiah",
        "et" | "est" => "Esther",
        "job" => "Job",
        "ps" | "psa" => "Psalm",
        "prv" | "pro" => "Proverbs",
        "ec" | "ecc" => "Ecclesiastes",
        "ss" | "sng" => "Song of Solomon",
        "is" | "isa" => "Isaiah",
        "jr" | "jer" => "Jeremiah",
        "lm" | "lam" => "Lamentations",
        "ez" | "ezk" => "Ezekiel",
        "dn" | "dan" => "Daniel",
        "ho" | "hos" => "Hosea",
        "jl" | "joe" => "Joel",
        "am" | "amo" => "Amos",
        "ob" | "oba" => "Obadiah",
        "jn" | "jon" => "Jonah",
        "mi" | "mic" => "Micah",
        "na" | "nam" => "Nahum",
        "hk" | "hab" => "Habakkuk",
        "zp" | "zep" => "Zephaniah",
        "hg" | "hag" => "Haggai",
        "zc" | "zec" => "Zechariah",
        "ml" | "mal" => "Malachi",
        "mt" | "mat" => "Matthew",
        "mk" | "mar" => "Mark",
        "lk" | "luk" => "Luke",
        "jo" | "joh" => "John",
        "act" => "Acts",
        "rm" | "rom" => "Romans",
        "1co" => "1 Corinthians",
        "2co" => "2 Corinthians",
        "gl" | "gal" => "Galatians",
        "eph" => "Ephesians",
        "ph" | "php" => "Philippians",
        "cl" | "col" => "Colossians",
        "1ts" | "1th" => "1 Thessalonians",
        "2ts" | "2th" => "2 Thessalonians",
        "1tm" | "1ti" => "1 Timothy",
        "2tm" | "2ti" => "2 Timothy",
        "tt" | "tit" => "Titus",
        "phm" => "Philemon",
        "hb" | "heb" => "Hebrews",
        "jm" | "jas" => "James",
        "1pe" => "1 Peter",
        "2pe" => "2 Peter",
        "1jo" | "1jn" => "1 John",
        "2jo" | "2jn" => "2 John",
        "3jo" | "3jn" => "3 John",
        "jd" | "jde" => "Jude",
        "re" | "rev" => "Revelation",
        other => return other.to_string(),
    }
    .to_string()
}

/// Looks for bundled full-Bible JSON files in the Tauri resources folder and
/// imports any translation whose verse count is below the full-canon threshold
/// (~31 000). Run on a background thread so cold start doesn't block the UI.
pub fn import_bundled_bibles(store: &AletheiaStore, resources_dir: &Path) {
    const FULL_BIBLE_VERSE_THRESHOLD: i64 = 30_000;
    let bundles: &[(&str, &str, &str, &str)] = &[
        ("kjv", "King James Version", "public-domain", "kjv-full.json"),
        (
            "bbe",
            "Bible in Basic English",
            "public-domain",
            "bbe-full.json",
        ),
    ];
    for (id, name, license, filename) in bundles {
        let count = store.count_verses_for_translation(id).unwrap_or(0);
        if count >= FULL_BIBLE_VERSE_THRESHOLD {
            continue;
        }
        let path = resources_dir.join("bibles").join(filename);
        if !path.exists() {
            log::warn!(
                "[bible-import] bundled file missing: {} (verse count was {count})",
                path.display()
            );
            continue;
        }
        match import_full_bible_from_json(store, id, name, license, &path) {
            Ok(n) => log::info!("[bible-import] {id}: {n} verses imported"),
            Err(error) => log::error!("[bible-import] {id} failed: {error}"),
        }
    }
}

fn seed_offline_assets(store: &AletheiaStore) -> Result<(), String> {
    let now = now_ms();
    let manifest = production_offline_asset_manifest();
    for asset in manifest.assets {
        let record = OfflineAssetStateRecord {
            id: asset.id,
            state: asset.state,
            checksum: asset.checksum_sha256,
            updated_at_ms: now,
        };
        store
            .ensure_offline_asset_state(&record)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Returns the current service session ID string based on today's UTC date.
/// Format: `"service-YYYY-MM-DD"` — matches the pattern used in `get_service_state`.
fn current_service_session_id() -> String {
    use aletheia_core::now_ms;
    let total_days = now_ms() / 1_000 / 86_400;
    let z = total_days as u32 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("service-{y:04}-{m:02}-{d:02}")
}

pub fn scene_from_candidate(candidate: &ScriptureCandidateDto) -> Result<OutputScene, String> {
    let session_id = ServiceSessionId::new(&current_service_session_id())
        .map_err(|error| format!("invalid service session id: {error}"))?;
    Ok(OutputScene::scripture(
        format!("scene-{}", candidate.id),
        session_id,
        candidate.reference.clone(),
        candidate.translation.clone(),
        candidate.text.clone(),
        "broadcast-lower",
    ))
}


#[cfg(test)]
fn detect_production_transcript_candidates() -> Result<Vec<ScriptureCandidateDto>, String> {
    detect_candidates_for_transcript(production_transcript())
}

pub fn detect_candidates_for_transcript(
    transcript: Vec<TranscriptSegmentDto>,
) -> Result<Vec<ScriptureCandidateDto>, String> {
    let session_id = ServiceSessionId::new(&current_service_session_id())
        .map_err(|error| format!("invalid service session id: {error}"))?;
    let detector = ReferenceKeywordDetector;
    let mut context = Vec::new();
    let mut candidates = Vec::new();

    for (index, segment) in transcript.into_iter().enumerate() {
        let started_at_ms = (index as u64) * 5_000;
        let detection_segment = DetectionTranscriptSegment {
            id: segment.id,
            session_id: session_id.clone(),
            started_at_ms,
            ended_at_ms: started_at_ms + 4_000,
            speaker_label: Some(segment.speaker),
            language: segment.language,
            text: segment.text,
            confidence: f32::from(segment.confidence) / 100.0,
            adapter: "offline-whisper-local".to_string(),
            latency_ms: u64::from(segment.latency_ms),
        };

        candidates.extend(
            detector
                .detect(&detection_segment, &context)
                .into_iter()
                .map(detected_candidate_to_dto),
        );
        context.push(detection_segment);
    }

    candidates.sort_by(|left, right| right.confidence.cmp(&left.confidence));
    let mut seen_references = HashSet::new();
    candidates.retain(|candidate| seen_references.insert(candidate.reference.clone()));
    Ok(candidates)
}

pub fn language_detections_from_transcript(
    detector: &KeywordLanguageDetector,
    transcript: &[TranscriptSegmentDto],
) -> Vec<LanguageDetectionDto> {
    let mut detections: Vec<LanguageDetectionDto> = Vec::new();

    for segment in transcript {
        let detection = detector.detect_language(&segment.text, Some(&segment.language));
        if let Some(index) = detections
            .iter()
            .position(|existing| existing.code == detection.code)
        {
            let existing: &mut LanguageDetectionDto = &mut detections[index];
            existing.confidence = existing.confidence.max(percent_score(detection.confidence));
            for term in detection.matched_terms {
                if !existing.matched_terms.contains(&term) {
                    existing.matched_terms.push(term);
                }
            }
        } else {
            detections.push(LanguageDetectionDto {
                code: detection.code.to_string(),
                name: detection.name.to_string(),
                confidence: percent_score(detection.confidence),
                matched_terms: detection.matched_terms,
            });
        }
    }

    detections.sort_by(|left, right| right.confidence.cmp(&left.confidence));
    detections
}

pub fn supported_language_to_dto(language: SupportedLanguage) -> SupportedLanguageDto {
    SupportedLanguageDto {
        code: language.code.to_string(),
        name: language.name.to_string(),
        stt_locale: language.stt_locale.to_string(),
        scripture_aliases_ready: language.scripture_aliases_ready,
        offline_stt_ready: language.offline_stt_ready,
        cloud_stt_ready: language.cloud_stt_ready,
    }
}

pub fn accuracy_target_dto() -> AccuracyTargetDto {
    let detector = ReferenceKeywordDetector;
    let evaluation = evaluate_accuracy_fixtures(&detector, &accuracy_validation_fixtures());

    AccuracyTargetDto {
        target_precision: 95,
        target_recall: 90,
        auto_preview_threshold: 95,
        validated_precision: percent_score(evaluation.precision),
        validated_recall: percent_score(evaluation.recall),
        validation_sample_count: evaluation.total,
        live_requires_operator: true,
        strategy: vec![
            "Use language detection to route STT and scripture aliases before matching.".to_string(),
            "Fuse exact reference, verse quotation, language alias, service-plan context, and operator feedback evidence.".to_string(),
            "Calibrate confidence per language pack; do not auto-preview below 95% precision proof.".to_string(),
            "Keep cloud reranking optional and never blocking in low-bandwidth mode.".to_string(),
        ],
    }
}

fn accuracy_validation_fixtures() -> Vec<AccuracyFixture> {
    vec![
        AccuracyFixture {
            id: "english-romans",
            language: "English",
            text: "Please open Romans 8:28.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "hausa-romans",
            language: "Hausa",
            text: "Mu bude Romawa 8:28 tare da ikilisiya.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "twi-romans",
            language: "Twi",
            text: "Momma yenhwɛ Romafo 8:28 ansa na yebɔ mpae.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "swahili-romans",
            language: "Swahili",
            text: "Tufungue Warumi 8:28 pamoja na kanisa.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "xhosa-romans",
            language: "Xhosa",
            text: "Masivule KwabaseRoma 8:28 namhlanje.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "spanish-romans",
            language: "Spanish",
            text: "Abramos Romanos 8:28 juntos.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "french-romans",
            language: "French",
            text: "Ouvrons Romains 8:28 ensemble.",
            expected_reference: Some("Romans 8:28"),
        },
        AccuracyFixture {
            id: "negative-prayer",
            language: "English",
            text: "We will pray after the song.",
            expected_reference: None,
        },
    ]
}

pub fn merged_offline_asset_manifest(store: &AletheiaStore) -> Result<OfflineAssetManifest, String> {
    let mut manifest = production_offline_asset_manifest();
    let states = store
        .list_offline_asset_states()
        .map_err(|error| error.to_string())?;
    let state_map: HashMap<String, OfflineAssetStateRecord> = states
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect();

    for asset in &mut manifest.assets {
        if let Some(state) = state_map.get(&asset.id) {
            asset.state = state.state.clone();
            asset.checksum_sha256 = state.checksum.clone();
        }
    }

    let installed_count = manifest
        .assets
        .iter()
        .filter(|asset| asset.state == "installed")
        .count() as u16;
    let required_count = manifest
        .assets
        .iter()
        .filter(|asset| asset.required_for_release)
        .count() as u16;
    let missing_required = manifest
        .assets
        .iter()
        .any(|asset| asset.required_for_release && asset.state != "installed");

    manifest.installed_count = installed_count;
    manifest.required_count = required_count;
    manifest.state = if missing_required { "blocked" } else { "ready" }.to_string();
    Ok(manifest)
}

pub fn stt_readiness_from_manifest(
    manifest: &OfflineAssetManifest,
    data_miser_enabled: bool,
) -> SttReadiness {
    let stt_assets: Vec<_> = manifest
        .assets
        .iter()
        .filter(|asset| asset.kind == "stt-model")
        .collect();
    let required_ready = stt_assets
        .iter()
        .filter(|asset| asset.required_for_release)
        .all(|asset| asset.state == "installed");

    SttReadiness {
        offline_models_ready: !stt_assets.is_empty() && required_ready,
        cloud_ready: !data_miser_enabled,
        last_cloud_latency_ms: if data_miser_enabled { None } else { Some(140) },
    }
}

pub fn merge_acceptance_receipts(
    store: &AletheiaStore,
    devices: Vec<AcceptanceDevice>,
) -> Result<Vec<AcceptanceDevice>, String> {
    let mut merged = Vec::with_capacity(devices.len());

    for mut device in devices {
        let receipts = store
            .list_device_acceptance_receipts(&device.id)
            .map_err(|error| error.to_string())?;
        if receipts.is_empty() {
            device.state = "not-run".to_string();
            merged.push(device);
            continue;
        }

        let mut latest_by_step: HashMap<String, bool> = HashMap::new();
        for receipt in receipts {
            latest_by_step
                .entry(receipt.step_label)
                .or_insert(receipt.passed);
        }

        let mut missing_required = false;
        let mut failed_required = false;
        for step in &device.steps {
            if !step.required {
                continue;
            }
            match latest_by_step.get(&step.label) {
                Some(passed) => {
                    if !*passed {
                        failed_required = true;
                    }
                }
                None => {
                    missing_required = true;
                }
            }
        }

        device.state = if failed_required || missing_required {
            "degraded".to_string()
        } else {
            "healthy".to_string()
        };

        merged.push(device);
    }

    Ok(merged)
}

pub fn offline_asset_root(state: &DesktopState) -> Result<PathBuf, String> {
    Ok(state
        .database_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("offline-assets"))
}

pub fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verified_manifest_to_dto(verified: VerifiedPluginManifest) -> VerifiedPluginManifestDto {
    VerifiedPluginManifestDto {
        id: verified.id,
        name: verified.name,
        version: verified.version,
        digest_sha256: verified.digest_sha256,
        key_id: verified.key_id,
        capability_count: verified.capability_count,
    }
}

fn percent_score(score: f32) -> u8 {
    (score * 100.0).round().clamp(0.0, 100.0) as u8
}

fn detected_candidate_to_dto(
    candidate: aletheia_detection::ScriptureCandidate,
) -> ScriptureCandidateDto {
    let confidence = percent_score(candidate.score);
    ScriptureCandidateDto {
        id: stable_candidate_id(&candidate.reference),
        reference: candidate.reference.clone(),
        translation: candidate.translation,
        language: candidate.language,
        text: clean_verse_text_for_display(verse_text_for_reference(&candidate.reference)),
        confidence,
        source: "Local AI assist".to_string(),
        reason: candidate.reasons.join("; "),
        status: if confidence >= 85 { "preview" } else { "new" }.to_string(),
        ..Default::default()
    }
}

fn stable_candidate_id(reference: &str) -> String {
    reference
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn verse_text_for_reference(reference: &str) -> &'static str {
    match reference {
        "Romans 8:28" => {
            "And we know that all things work together for good to them that love God."
        }
        "Psalm 23:4" => {
            "Yea, though I walk through the valley of the shadow of death, I will fear no evil."
        }
        "John 11:35" => "Jesus wept.",
        "Isaiah 40:31" => "They that wait upon the LORD shall renew their strength.",
        "1 Samuel 17:45" => {
            "Then said David to the Philistine, Thou comest to me with a sword, and with a spear."
        }
        _ => "Detected scripture candidate requires operator review.",
    }
}

pub fn scene_to_dto(scene: OutputScene) -> OutputSceneDto {
    OutputSceneDto {
        id: scene.id,
        reference: scene.reference,
        translation: scene.translation,
        theme_id: scene.theme_id,
        layers: scene
            .layers
            .into_iter()
            .map(|layer| SceneLayerDto {
                layer: match layer.layer {
                    OutputLayer::Verse => "verse".to_string(),
                    OutputLayer::Reference => "reference".to_string(),
                    OutputLayer::ContextCard => "contextCard".to_string(),
                },
                text: layer.text,
                visible: layer.visible,
            })
            .collect(),
    }
}

pub fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

pub fn verse_to_search_result(record: VerseRecord, source: &str) -> SearchResultDto {
    SearchResultDto {
        reference: format!("{} {}:{}", record.book, record.chapter, record.verse),
        translation: record.translation_id.to_uppercase(),
        snippet: clean_verse_text_for_display(&record.text),
        source: source.to_string(),
        language: "English".to_string(),
    }
}

/// Removes KJV translator-italics markers (`{is}`, `{the}`, etc.) and other
/// editorial brackets from verse text before it goes on screen. The bundled
/// public-domain KJV JSON wraps words supplied by the translator (not present
/// in the underlying Hebrew/Greek) in curly braces; the convention is fine for
/// scholarly reading but jarring on a projection screen and in operator UI.
///
/// The function is intentionally tolerant: it handles `{is}`, `{ is }`, and
/// also strips the surrounding braces from any short alphabetic insertion so
/// new bracketed editorial conventions don't slip through.
pub fn clean_verse_text_for_display(text: &str) -> String {
    // Fast path — no braces means nothing to clean.
    if !text.contains('{') {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            // Collect the bracketed content (max 32 chars to avoid swallowing
            // unrelated punctuation if a stray '{' appears in the source).
            let mut inner = String::new();
            let mut closed = false;
            for _ in 0..32 {
                match chars.next() {
                    Some('}') => {
                        closed = true;
                        break;
                    }
                    Some(other) => inner.push(other),
                    None => break,
                }
            }
            if closed {
                // Replace the entire `{…}` span with the bracketed text only,
                // dropping the braces themselves. KJV italics are still
                // visually conveyed by surrounding context — the operator
                // doesn't need typographic italics on a projection screen.
                out.push_str(inner.trim());
            } else {
                // Unbalanced — fall back to original characters so we don't
                // silently corrupt non-KJV translations that legitimately use
                // a single brace (very rare).
                out.push('{');
                out.push_str(&inner);
            }
        } else {
            out.push(c);
        }
    }
    // Collapse runs of whitespace introduced by stripping inline brackets.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

#[cfg(test)]
mod text_clean_tests {
    use super::clean_verse_text_for_display;

    #[test]
    fn strips_kjv_italics_braces() {
        assert_eq!(
            clean_verse_text_for_display("But his delight {is} in the law of the LORD"),
            "But his delight is in the law of the LORD"
        );
    }

    #[test]
    fn handles_multiple_braces_in_one_verse() {
        let input = "And the Spirit {of God} moved upon the face of {the} waters.";
        assert_eq!(
            clean_verse_text_for_display(input),
            "And the Spirit of God moved upon the face of the waters."
        );
    }

    #[test]
    fn passes_through_text_with_no_braces_unchanged() {
        let input = "For God so loved the world.";
        assert_eq!(clean_verse_text_for_display(input), input);
    }

    #[test]
    fn tolerates_unclosed_brace_without_corruption() {
        // Should preserve original characters rather than silently swallow.
        let input = "Verse with stray { open brace and no close.";
        let out = clean_verse_text_for_display(input);
        assert!(out.contains("stray"));
        assert!(out.contains("open brace"));
    }
}

pub fn sanitize_fts_query(input: &str) -> String {
    input
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character.is_whitespace() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_reference(input: &str) -> Option<(String, u16, u16)> {
    let normalized = input
        .to_lowercase()
        .replace(':', " ")
        .replace('.', " ")
        .replace(',', " ");
    let parts = normalized.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let verse = parts.last()?.parse::<u16>().ok()?;
    let chapter = parts
        .get(parts.len().saturating_sub(2))?
        .parse::<u16>()
        .ok()?;
    let book_key = parts[..parts.len() - 2].join(" ");
    canonical_book(&book_key).map(|book| (book.to_string(), chapter, verse))
}

fn canonical_book(input: &str) -> Option<&'static str> {
    match input.trim() {
        // Old Testament
        "gen" | "genesis" => Some("Genesis"),
        "ex" | "exo" | "exod" | "exodus" => Some("Exodus"),
        "lev" | "leviticus" => Some("Leviticus"),
        "num" | "numbers" => Some("Numbers"),
        "deut" | "deu" | "dt" | "deuteronomy" => Some("Deuteronomy"),
        "josh" | "jos" | "joshua" => Some("Joshua"),
        "judg" | "jdg" | "judges" => Some("Judges"),
        "ruth" => Some("Ruth"),
        "1 sam" | "1sam" | "1 samuel" | "first samuel" => Some("1 Samuel"),
        "2 sam" | "2sam" | "2 samuel" | "second samuel" => Some("2 Samuel"),
        "1 kgs" | "1kgs" | "1 kings" | "first kings" => Some("1 Kings"),
        "2 kgs" | "2kgs" | "2 kings" | "second kings" => Some("2 Kings"),
        "1 chr" | "1chr" | "1 chronicles" | "first chronicles" => Some("1 Chronicles"),
        "2 chr" | "2chr" | "2 chronicles" | "second chronicles" => Some("2 Chronicles"),
        "ezra" => Some("Ezra"),
        "neh" | "nehemiah" => Some("Nehemiah"),
        "est" | "esther" => Some("Esther"),
        "job" => Some("Job"),
        "ps" | "psa" | "psalm" | "psalms" => Some("Psalm"),
        "prov" | "pro" | "proverbs" => Some("Proverbs"),
        "eccl" | "ecc" | "ecclesiastes" => Some("Ecclesiastes"),
        "song" | "sos" | "song of solomon" | "song of songs" => Some("Song of Solomon"),
        "isa" | "is" | "isaiah" => Some("Isaiah"),
        "jer" | "jeremiah" => Some("Jeremiah"),
        "lam" | "lamentations" => Some("Lamentations"),
        "ezek" | "eze" | "ezekiel" => Some("Ezekiel"),
        "dan" | "daniel" => Some("Daniel"),
        "hos" | "hosea" => Some("Hosea"),
        "joel" => Some("Joel"),
        "amos" => Some("Amos"),
        "obad" | "obadiah" => Some("Obadiah"),
        "jonah" | "jon" => Some("Jonah"),
        "mic" | "micah" => Some("Micah"),
        "nah" | "nahum" => Some("Nahum"),
        "hab" | "habakkuk" => Some("Habakkuk"),
        "zeph" | "zep" | "zephaniah" => Some("Zephaniah"),
        "hag" | "haggai" => Some("Haggai"),
        "zech" | "zec" | "zechariah" => Some("Zechariah"),
        "mal" | "malachi" => Some("Malachi"),
        // New Testament
        "matt" | "mat" | "mt" | "matthew" => Some("Matthew"),
        "mark" | "mrk" | "mk" | "mar" => Some("Mark"),
        "luke" | "luk" | "lk" => Some("Luke"),
        "jn" | "jhn" | "john" => Some("John"),
        "acts" | "act" => Some("Acts"),
        "rom" | "romans" => Some("Romans"),
        "1 cor" | "1cor" | "1 corinthians" | "first corinthians" => Some("1 Corinthians"),
        "2 cor" | "2cor" | "2 corinthians" | "second corinthians" => Some("2 Corinthians"),
        "gal" | "galatians" => Some("Galatians"),
        "eph" | "ephesians" => Some("Ephesians"),
        "phil" | "php" | "philippians" => Some("Philippians"),
        "col" | "colossians" => Some("Colossians"),
        "1 thess" | "1thess" | "1 thessalonians" | "first thessalonians" => Some("1 Thessalonians"),
        "2 thess" | "2thess" | "2 thessalonians" | "second thessalonians" => Some("2 Thessalonians"),
        "1 tim" | "1tim" | "1 timothy" | "first timothy" => Some("1 Timothy"),
        "2 tim" | "2tim" | "2 timothy" | "second timothy" => Some("2 Timothy"),
        "titus" | "tit" => Some("Titus"),
        "philem" | "phm" | "philemon" => Some("Philemon"),
        "heb" | "hebrews" => Some("Hebrews"),
        "jas" | "james" => Some("James"),
        "1 pet" | "1pet" | "1 peter" | "first peter" => Some("1 Peter"),
        "2 pet" | "2pet" | "2 peter" | "second peter" => Some("2 Peter"),
        "1 jn" | "1jn" | "1 john" | "first john" => Some("1 John"),
        "2 jn" | "2jn" | "2 john" | "second john" => Some("2 John"),
        "3 jn" | "3jn" | "3 john" | "third john" => Some("3 John"),
        "jude" => Some("Jude"),
        "rev" | "revelation" | "revelations" => Some("Revelation"),
        _ => None,
    }
}

pub fn default_search_results() -> Vec<SearchResultDto> {
    vec![
        SearchResultDto {
            reference: "John 3:16".to_string(),
            translation: "KJV".to_string(),
            snippet: "For God so loved the world, that he gave his only begotten Son.".to_string(),
            source: "Exact reference".to_string(),
            language: "English".to_string(),
        },
        SearchResultDto {
            reference: "Psalm 23:1".to_string(),
            translation: "KJV".to_string(),
            snippet: "The LORD is my shepherd; I shall not want.".to_string(),
            source: "Recent service plan".to_string(),
            language: "English".to_string(),
        },
        SearchResultDto {
            reference: "1 Samuel 17:45".to_string(),
            translation: "KJV".to_string(),
            snippet: "Then said David to the Philistine, Thou comest to me with a sword."
                .to_string(),
            source: "Offline phrase match".to_string(),
            language: "English".to_string(),
        },
    ]
}

pub fn production_transcript() -> Vec<TranscriptSegmentDto> {
    vec![
        TranscriptSegmentDto {
            id: "seg-1842".to_string(),
            time: "00:18:42".to_string(),
            speaker: "Pastor Daniel".to_string(),
            language: "English".to_string(),
            text: "Turn with me to Romans chapter eight. We will read verse twenty eight together."
                .to_string(),
            confidence: 94,
            latency_ms: 410,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1847".to_string(),
            time: "00:18:47".to_string(),
            speaker: "Pastor Daniel".to_string(),
            language: "English".to_string(),
            text: "And we know that all things work together for good to them that love God."
                .to_string(),
            confidence: 91,
            latency_ms: 438,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1855".to_string(),
            time: "00:18:55".to_string(),
            speaker: "Interpreter".to_string(),
            language: "Yoruba".to_string(),
            text: "A mo pe ohun gbogbo n sise po fun rere fun awon ti won fe Olorun.".to_string(),
            confidence: 83,
            latency_ms: 620,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1912".to_string(),
            time: "00:19:12".to_string(),
            speaker: "Pastor Daniel".to_string(),
            language: "English".to_string(),
            text: "If you are writing notes, add Isaiah forty verse thirty one for later."
                .to_string(),
            confidence: 89,
            latency_ms: 455,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1920-ha".to_string(),
            time: "00:19:20".to_string(),
            speaker: "Interpreter".to_string(),
            language: "Hausa".to_string(),
            text: "Mu bude Romawa 8:28 tare da ikilisiya.".to_string(),
            confidence: 86,
            latency_ms: 610,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1928-tw".to_string(),
            time: "00:19:28".to_string(),
            speaker: "Interpreter".to_string(),
            language: "Twi".to_string(),
            text: "Momma yenhwɛ Romafo 8:28 ansa na yebɔ mpae.".to_string(),
            confidence: 84,
            latency_ms: 640,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1936-sw".to_string(),
            time: "00:19:36".to_string(),
            speaker: "Interpreter".to_string(),
            language: "Swahili".to_string(),
            text: "Tufungue Warumi 8:28 pamoja na kanisa.".to_string(),
            confidence: 88,
            latency_ms: 590,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1944-xh".to_string(),
            time: "00:19:44".to_string(),
            speaker: "Interpreter".to_string(),
            language: "Xhosa".to_string(),
            text: "Masivule KwabaseRoma 8:28 namhlanje.".to_string(),
            confidence: 82,
            latency_ms: 670,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-1952-es".to_string(),
            time: "00:19:52".to_string(),
            speaker: "Interpreter".to_string(),
            language: "Spanish".to_string(),
            text: "Abramos Romanos 8:28 juntos.".to_string(),
            confidence: 90,
            latency_ms: 520,
            ..Default::default()
        },
        TranscriptSegmentDto {
            id: "seg-2000-fr".to_string(),
            time: "00:20:00".to_string(),
            speaker: "Interpreter".to_string(),
            language: "French".to_string(),
            text: "Ouvrons Romains 8:28 ensemble.".to_string(),
            confidence: 90,
            latency_ms: 530,
            ..Default::default()
        },
    ]
}

pub fn production_candidates() -> Vec<ScriptureCandidateDto> {
    vec![
        ScriptureCandidateDto {
            id: "romans-828".to_string(),
            reference: "Romans 8:28".to_string(),
            translation: "KJV".to_string(),
            language: "English".to_string(),
            text: "And we know that all things work together for good to them that love God."
                .to_string(),
            confidence: 92,
            source: "Pastor mic".to_string(),
            reason: "Exact reference plus quoted phrase in the last 12 seconds.".to_string(),
            status: "preview".to_string(),
            ..Default::default()
        },
        ScriptureCandidateDto {
            id: "isaiah-4031".to_string(),
            reference: "Isaiah 40:31".to_string(),
            translation: "KJV".to_string(),
            language: "English".to_string(),
            text: "They that wait upon the LORD shall renew their strength.".to_string(),
            confidence: 76,
            source: "Transcript context".to_string(),
            reason: "Reference was spoken, but no verse text has been quoted yet.".to_string(),
            status: "new".to_string(),
            ..Default::default()
        },
        ScriptureCandidateDto {
            id: "psalm-231".to_string(),
            reference: "Psalm 23:1".to_string(),
            translation: "KJV".to_string(),
            language: "English".to_string(),
            text: "The LORD is my shepherd; I shall not want.".to_string(),
            confidence: 68,
            source: "Manual fallback".to_string(),
            reason: "Recent service plan contains Psalm 23 and the phrase matched softly."
                .to_string(),
            status: "approved".to_string(),
            ..Default::default()
        },
        ScriptureCandidateDto {
            id: "john-316".to_string(),
            reference: "John 3:16".to_string(),
            translation: "KJV".to_string(),
            language: "English".to_string(),
            text: "For God so loved the world, that he gave his only begotten Son.".to_string(),
            confidence: 64,
            source: "Phrase search".to_string(),
            reason: "Phrase match only. Operator review required.".to_string(),
            status: "new".to_string(),
            ..Default::default()
        },
    ]
}

/// Build the integration list from the actual adapter state held in
/// DesktopState rather than the hard-coded demo fixture. Each adapter exposes
/// an OutputHealth value via its `status()` method which we map to the same
/// state strings the React UI already renders.
pub fn live_integrations(state: &DesktopState) -> Vec<IntegrationDto> {
    let mut items = Vec::with_capacity(5);

    if let Ok(adapter) = state.lock_vmix() {
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(IntegrationDto {
            id: "vmix".to_string(),
            name: "vMix".to_string(),
            kind: "HTTP API".to_string(),
            state: s,
            detail: d,
            capability: "SetText fields, preview overlay, live overlay, clear".to_string(),
        });
    }

    if let Ok(adapter) = state.obs.lock() {
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(IntegrationDto {
            id: "obs-main".to_string(),
            name: "OBS Studio".to_string(),
            kind: "WebSocket".to_string(),
            state: s,
            detail: d,
            capability: "Preview, live text source, clear".to_string(),
        });
    }

    if let Ok(adapter) = state.propresenter.lock() {
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(IntegrationDto {
            id: "propresenter-main".to_string(),
            name: "ProPresenter".to_string(),
            kind: "REST API".to_string(),
            state: s,
            detail: d,
            capability: "Messages subsystem: verse + reference tokens, trigger, clear".to_string(),
        });
    }

    if let Ok(adapter) = state.companion.lock() {
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(IntegrationDto {
            id: "companion-main".to_string(),
            name: "Bitfocus Companion".to_string(),
            kind: "HTTP control".to_string(),
            state: s,
            detail: d,
            capability: "Custom variables + button press for cue automation".to_string(),
        });
    }

    if let Ok(adapter) = state.osc.lock() {
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(IntegrationDto {
            id: "osc-main".to_string(),
            name: "OSC bridge".to_string(),
            kind: "UDP / TouchOSC".to_string(),
            state: s,
            detail: d,
            capability: "Cue triggers, scripture text bus".to_string(),
        });
    }

    if let Ok(adapter) = state.easyworship.lock() {
        let (s, d) = output_health_to_state_detail(adapter.status().health);
        items.push(IntegrationDto {
            id: "easyworship".to_string(),
            name: "EasyWorship".to_string(),
            kind: "Watch folder".to_string(),
            state: s,
            detail: d,
            capability: "Slide export, schedule handoff".to_string(),
        });
    }

    items
}

/// Build a health-card list from real disk + DB state. Used by get_service_state
/// in place of the static demo fixture so the operator dashboard reflects the
/// machine they are actually about to lead a service from.


// ---------------------------------------------------------------------------
// Trusted plugin registry commands (v6)
// ---------------------------------------------------------------------------

/// Returns all plugins currently in the trust registry, enabled or disabled.

/// Permanently removes a plugin from the trust registry.
/// The plugin will no longer be allowed to dispatch outputs.

// ---------------------------------------------------------------------------
// Calibration dataset commands (v6)
// ---------------------------------------------------------------------------

/// Records one operator-reviewed transcript/reference pair for accuracy tracking.
///
/// `outcome` must be `"confirmed"` | `"corrected"` | `"rejected"`.

/// Returns aggregate accuracy statistics computed from stored calibration samples.

// ---------------------------------------------------------------------------
// Helper: TrustedPluginRecord → DTO
// ---------------------------------------------------------------------------

pub fn trusted_plugin_record_to_dto(
    record: TrustedPluginRecord,
) -> Result<TrustedPluginDto, String> {
    let capabilities: Vec<String> =
        serde_json::from_str(&record.capabilities_json).map_err(|error| {
            format!(
                "invalid capabilities JSON for plugin '{}': {error}",
                record.id
            )
        })?;
    Ok(TrustedPluginDto {
        id: record.id,
        name: record.name,
        version: record.version,
        key_id: record.key_id,
        digest: record.digest,
        capabilities,
        enabled: record.enabled,
        trusted_at_ms: record.trusted_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aletheia_core::AuditAction;
    use crate::audit::record_audit;

    #[test]
    fn production_detection_finds_high_confidence_romans_candidate() {
        let candidates = detect_production_transcript_candidates().expect("detection should run");
        assert!(candidates
            .iter()
            .any(|candidate| candidate.reference == "Romans 8:28" && candidate.confidence >= 85));
    }

    /// After the deadlock fix, `record_audit` was refactored to take a borrowed
    /// store instead of acquiring the lock itself. This regression test makes
    /// sure the chained-hash invariants still hold: every row's `previous_hash`
    /// must equal the prior row's `event_hash`, and the genesis row points at
    /// the literal string "genesis".
    #[test]
    fn record_audit_chains_hashes_correctly() {
        let store = AletheiaStore::open_memory().expect("in-memory store");

        record_audit(
            &store,
            AuditAction::ServiceStarted,
            "operator:test",
            "first event",
        )
        .expect("first audit write");
        record_audit(
            &store,
            AuditAction::PreviewRendered,
            "operator:test",
            "second event",
        )
        .expect("second audit write");
        record_audit(
            &store,
            AuditAction::LiveOutputSent,
            "operator:test",
            "third event",
        )
        .expect("third audit write");

        // Read all rows back in append order and verify the chain.
        let rows: Vec<(String, String)> = store
            .connection()
            .prepare("SELECT previous_hash, event_hash FROM audit_log ORDER BY id ASC")
            .expect("prepare select")
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();

        assert_eq!(rows.len(), 3, "three audit rows expected");
        assert_eq!(rows[0].0, "genesis", "first row links to genesis");
        assert_eq!(
            rows[1].0, rows[0].1,
            "second row's previous_hash equals first row's event_hash"
        );
        assert_eq!(
            rows[2].0, rows[1].1,
            "third row's previous_hash equals second row's event_hash"
        );
        // Each event_hash must be a 64-char SHA-256 hex.
        for (_, h) in &rows {
            assert_eq!(h.len(), 64, "event_hash is sha256 hex");
        }
    }

    #[test]
    fn obs_export_escapes_scripture_html() {
        let candidate = ScriptureCandidateDto {
            id: "escape-proof".to_string(),
            reference: "John 11:35".to_string(),
            translation: "KJV".to_string(),
            language: "English".to_string(),
            text: "<script>alert('x')</script> & Jesus wept.".to_string(),
            confidence: 98,
            source: "test".to_string(),
            reason: "escape proof".to_string(),
            status: "preview".to_string(),
            ..Default::default()
        };

        let html = obs_browser_source_html(&candidate);
        assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; Jesus wept."));
        assert!(!html.contains("<script>alert"));
    }
}

// ---------------------------------------------------------------------------
// STT model auto-seeding
// ---------------------------------------------------------------------------

/// Copies model files found in the user's `Downloads\aletheia-models` folder
/// into the offline-assets directory and records them as installed.
/// Runs silently — never blocks startup or returns a hard error.
fn auto_seed_stt_models(store: &AletheiaStore, asset_root: &Path) {
    if let Err(e) = std::fs::create_dir_all(asset_root) {
        log::warn!("[stt-seed] could not create asset dir: {e}");
        return;
    }

    // Locate the model download directory via USERPROFILE / HOME.
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let model_dir = PathBuf::from(base)
        .join("Downloads")
        .join("aletheia-models");
    if !model_dir.exists() {
        return;
    }

    const CHECKSUM_EN: &str =
        "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002";
    const CHECKSUM_MULTI: &str =
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";

    let packs: &[(&str, &str, &str)] = &[
        ("stt-whisper-en-small",     "ggml-base.en.bin", CHECKSUM_EN),
        ("stt-whisper-multilingual", "ggml-base.bin",    CHECKSUM_MULTI),
    ];

    // Collect already-installed asset IDs to avoid redundant copies.
    let installed: HashSet<String> = store
        .list_offline_asset_states()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.state == "installed")
        .map(|r| r.id)
        .collect();

    for (asset_id, filename, expected_checksum) in packs {
        if installed.contains(*asset_id) {
            continue;
        }
        // Fast path: if the destination file is already in asset_root, just mark
        // it installed — skip the expensive SHA256 hash and 141 MB copy entirely.
        let target = asset_root.join(format!("{asset_id}.bin"));
        if target.exists() {
            let record = OfflineAssetStateRecord {
                id: asset_id.to_string(),
                state: "installed".to_string(),
                checksum: String::new(),
                updated_at_ms: now_ms(),
            };
            if let Err(e) = store.update_offline_asset_state(&record) {
                log::warn!("[stt-seed] db update failed for {asset_id}: {e}");
            } else {
                log::info!("[stt-seed] already present, marked installed: {asset_id}");
            }
            continue;
        }
        let source = model_dir.join(filename);
        if !source.exists() {
            continue;
        }
        let actual = match sha256_file_hex(&source) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("[stt-seed] checksum failed for {asset_id}: {e}");
                continue;
            }
        };
        if actual != *expected_checksum {
            log::warn!("[stt-seed] checksum mismatch for {asset_id}, skipping");
            continue;
        }
        if let Err(e) = std::fs::copy(&source, &target) {
            log::warn!("[stt-seed] copy failed for {asset_id}: {e}");
            continue;
        }
        let record = OfflineAssetStateRecord {
            id: asset_id.to_string(),
            state: "installed".to_string(),
            checksum: actual,
            updated_at_ms: now_ms(),
        };
        if let Err(e) = store.update_offline_asset_state(&record) {
            log::warn!("[stt-seed] store update failed for {asset_id}: {e}");
        } else {
            log::info!("[stt-seed] installed {asset_id}");
        }
    }
}

// ---------------------------------------------------------------------------
// Audio capture + offline STT commands
// ---------------------------------------------------------------------------

/// Returns the path to the best available STT model.
///
/// Checks the bundled installer-resource directory first (for the ggml-base.en
/// model dropped into `src-tauri/resources/models/` at build time), then falls
/// back to user-installed model packs in the offline-assets directory.
pub fn find_stt_model_path(asset_root: &Path) -> Result<PathBuf, String> {
    // Bundled-resource search hint, set once at app start by `setup()`.
    if let Some(resource_dir) = bundled_resource_dir() {
        let bundled = resource_dir.join("resources/models/ggml-base.en.bin");
        if bundled.exists() {
            return Ok(bundled);
        }
    }
    // Prefer English-only model, then fall back to any multilingual pack.
    let candidates = [
        "stt-whisper-en-small.bin",
        "stt-whisper-multilingual.bin",
        // Legacy aliases — kept so installs from older builds keep working.
        "stt-yoruba-pack.bin",
        "stt-hausa-pack.bin",
        "stt-twi-pack.bin",
        "stt-swahili-pack.bin",
        "stt-xhosa-pack.bin",
        "stt-spanish-pack.bin",
        "stt-french-pack.bin",
    ];
    for name in candidates {
        let p = asset_root.join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    Err("No offline STT model installed. Install model packs via Health → Offline Model Packs.".to_string())
}

// `OnceLock` cell for the installer's bundled-resource directory. Populated
// once at startup from `setup()` so `find_stt_model_path` can locate the
// auto-bundled `ggml-base.en.bin` without threading the path through every
// call site.
static BUNDLED_RESOURCE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_bundled_resource_dir(path: PathBuf) {
    let _ = BUNDLED_RESOURCE_DIR.set(path);
}

pub fn bundled_resource_dir() -> Option<&'static Path> {
    BUNDLED_RESOURCE_DIR.get().map(|p| p.as_path())
}





// ---------------------------------------------------------------------------

/// Called by the frontend immediately after React mounts.
/// Because the window starts hidden (`"visible": false` in tauri.conf.json),
/// WebView2 initialises off-screen and never causes a Win32 "(Not Responding)"
/// flash.  Calling this command makes the window appear only once the JS
/// engine is alive and the message pump is fully responsive.

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install a global panic hook that writes crash details to a file
    // before the process aborts. This file survives the crash and can
    // be included in support bundles.
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let crash_msg = format!(
            "[CRASH] Aletheia panicked at {location}\n  payload: {payload}\n  time: {}\n",
            chrono_lite_now()
        );
        eprintln!("{crash_msg}");
        // Best-effort write to the app data dir or temp dir.
        let crash_path = std::env::temp_dir().join("aletheia-crash.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_path)
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{crash_msg}")
            });
    }));

    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;
            let database_path = app_dir.join("aletheia.sqlite3");

            // Populate the bundled-resource directory hint BEFORE state open /
            // manage so any early path that resolves the STT model (audit
            // seeding, health checks triggered during hydration) can find the
            // bundled `ggml-base.en.bin` from the installer resources dir.
            if let Ok(resource_dir_early) = app.path().resource_dir() {
                set_bundled_resource_dir(resource_dir_early);
            }

            let desktop_state = DesktopState::open(database_path.clone())
                .map_err(|message| std::io::Error::new(std::io::ErrorKind::Other, message))?;
            app.manage(desktop_state);

            // vMix auto-reconnect: every N seconds (exponential backoff 10s..5min)
            // probe the adapter; on offline/degraded re-evaluate via check_status,
            // which re-runs the HTTP probe and effectively "reconnects". Emit
            // `aletheia://vmix-reconnect` so the UI can surface attempts.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use std::time::Duration;
                    const MIN_DELAY_MS: u64 = 10_000;
                    const MAX_DELAY_MS: u64 = 5 * 60 * 1000;
                    let mut delay_ms: u64 = MIN_DELAY_MS;
                    let mut attempt: u32 = 0;
                    let mut last_state = String::new();
                    loop {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        let probe_handle = app_handle.clone();
                        let result = tauri::async_runtime::spawn_blocking(move || {
                            let state = probe_handle.state::<DesktopState>();
                            let adapter = state.lock_vmix().ok()?;
                            Some(aletheia_output::OutputAdapter::status(&*adapter).health)
                        }).await.ok().flatten();
                        let (state_str, _detail) = match result {
                            Some(h) => crate::output_health_to_state_detail(h),
                            None => ("offline".to_string(), "vMix probe unavailable".to_string()),
                        };
                        let needs_retry = matches!(state_str.as_str(), "offline" | "degraded");
                        if needs_retry {
                            attempt = attempt.saturating_add(1);
                            delay_ms = (delay_ms.saturating_mul(2)).min(MAX_DELAY_MS);
                        } else {
                            attempt = 0;
                            delay_ms = MIN_DELAY_MS;
                        }
                        // Emit only on state transition or while actively retrying.
                        // Avoids a steady stream of "still healthy" events to the UI.
                        if state_str != last_state || needs_retry {
                            let payload = serde_json::json!({
                                "state": state_str,
                                "attempt": attempt,
                                "nextRetryMs": delay_ms,
                            });
                            let _ = tauri::Emitter::emit(&app_handle, "aletheia://vmix-reconnect", payload);
                            last_state = state_str;
                        }
                    }
                });
            }

            // Auto-import bundled full-Bible JSON files on first launch (and on
            // upgrades that ship updated translations). Runs in the background
            // so the UI is interactive while ~31 000 verses are inserted.
            //
            // Emits `aletheia://library-imported` when the import finishes so
            // the dashboard's "Scripture library offline" / "degraded" banner
            // can clear immediately instead of waiting for the next poll.
            if let Ok(resource_dir) = app.path().resource_dir() {
                let db_for_bibles = database_path.clone();
                let app_handle = app.handle().clone();
                std::thread::spawn(move || match AletheiaStore::open_file(&db_for_bibles) {
                    Ok(bg_store) => {
                        import_bundled_bibles(&bg_store, &resource_dir);
                        // Final verse count after the import — drives the UI
                        // banner state. > 30 000 → healthy. > 0 → degraded.
                        // 0 → still offline (bundle missing or corrupt).
                        let kjv_count = bg_store
                            .count_verses_for_translation("kjv")
                            .unwrap_or(0);
                        let payload = serde_json::json!({
                            "translation": "kjv",
                            "verseCount": kjv_count,
                            "state": if kjv_count > 30_000 {
                                "healthy"
                            } else if kjv_count > 0 {
                                "degraded"
                            } else {
                                "offline"
                            },
                        });
                        let _ = tauri::Emitter::emit(
                            &app_handle,
                            "aletheia://library-imported",
                            payload,
                        );
                    }
                    Err(e) => log::warn!("[bible-import] bg store open failed: {e}"),
                });
            }

            // Log at Debug in development, Warn in production so critical messages
            // are never silently dropped in release builds.
            let log_level = if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Warn
            };
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log_level)
                    .build(),
            )?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_service_state,
            search_scripture,
            render_preview,
            set_destinations_armed,
            set_data_miser,
            send_live,
            run_pre_service_check,
            analyze_transcript,
            get_vmix_status,
            get_vmix_config,
            update_vmix_config,
            get_recent_integration_events,
            get_production_readiness,
            install_offline_asset,
            install_offline_asset_from_path,
            record_device_acceptance,
            run_local_rehearsal,
            export_support_bundle,
            export_offline_asset_pack,
            export_booth_pack,
            vault_store_secret,
            vault_read_secret,
            vault_delete_secret,
            verify_plugin_manifest,
            enable_plugin_manifest,
            send_vmix_preview,
            send_vmix_live,
            clear_vmix_overlay,
            save_service_profile,
            list_service_profiles,
            set_active_service_profile,
            delete_service_profile,
            get_obs_status,
            update_obs_config,
            send_obs_preview,
            send_obs_live,
            clear_obs_output,
            get_propresenter_status,
            get_propresenter_config,
            update_propresenter_config,
            send_propresenter_preview,
            send_propresenter_live,
            clear_propresenter_output,
            get_companion_status,
            get_companion_config,
            update_companion_config,
            send_companion_preview,
            send_companion_live,
            clear_companion_output,
            get_osc_status,
            update_osc_config,
            send_osc_preview,
            send_osc_live,
            send_osc_test_ping,
            clear_osc_output,
            get_easyworship_status,
            update_easyworship_config,
            send_easyworship_preview,
            send_easyworship_live,
            clear_easyworship_output,
            list_trusted_plugins,
            revoke_trusted_plugin,
            record_calibration_sample,
            get_calibration_report,
            start_audio_capture,
            stop_audio_capture,
            get_capture_status,
            import_bible_translation,
            list_bible_translations,
            show_main_window,
            set_operator_name,
            get_operator_name,
            save_session,
            test_vmix_connection,
            test_obs_connection,
            test_propresenter_connection,
            test_companion_connection,
            test_osc_connection,
            test_easyworship_connection,
            log_ccli_usage,
            list_ccli_usage,
            export_ccli_usage_csv,
            sign_fleet_bundle,
            verify_fleet_bundle,
            get_fleet_public_key,
            start_stream_overlay_server,
            stop_stream_overlay_server,
            update_stream_overlay_state,
            get_stream_overlay_server_status,
            kv_get,
            kv_set,
            kv_delete,
            kv_list_keys,
            list_audio_devices,
            delete_bible_translation,
            export_operator_config,
            import_operator_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aletheia desktop shell");
}
