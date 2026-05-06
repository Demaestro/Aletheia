use aletheia_ops::RedactionSummary;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct BibleImportResultDto {
    pub translation_id: String,
    pub verses_inserted: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct BibleVerseDto {
    pub translation: String,
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    pub text: String,
    pub reference: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct BibleTranslationStatusDto {
    pub id: String,
    pub name: String,
    pub verses_loaded: u32,
    pub full_canon: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ScriptureCandidateDto {
    pub id: String,
    pub reference: String,
    pub translation: String,
    pub language: String,
    pub text: String,
    pub confidence: u8,
    pub source: String,
    pub reason: String,
    pub status: String,
}

/// Live candidate surfaced from the capture/detection pipeline.
/// Emitted over the `aletheia://scripture-candidate` Tauri event and also
/// persisted in the `scripture_candidates` table.
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct LiveScriptureCandidateDto {
    pub id: String,
    pub session_id: String,
    pub segment_id: String,
    pub reference: String,
    pub translation_id: String,
    pub language: String,
    pub score: f32,
    /// `"certain" | "strong" | "likely" | "unsafe"`.
    pub bucket: String,
    /// Operator-facing decision: `"open" | "preview" | "approval" | "ignored"`.
    pub status: String,
    pub reason: String,
    pub verse_text: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegmentDto {
    pub id: String,
    pub time: String,
    pub speaker: String,
    pub language: String,
    pub text: String,
    pub confidence: u8,
    pub latency_ms: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct IntegrationDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub state: String,
    pub detail: String,
    pub capability: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct HealthItemDto {
    pub label: String,
    pub state: String,
    pub detail: String,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ServiceSessionDto {
    pub id: String,
    pub name: String,
    pub started_at: String,
    pub database_path: String,
    pub mode: String,
    pub data_miser_enabled: bool,
    pub offline_mode_enabled: bool,
    pub destinations_armed: bool,
    pub audit_count: i64,
    pub last_event_sequence: i64,
    pub checked_at_ms: u64,
    /// Operating mode: "manual" | "assisted" | "auto" | "rehearsal" | "mock".
    /// Defaults to "assisted" — the architectural-vision safe default.
    #[serde(default = "default_operating_mode_str")]
    pub operating_mode: String,
}

fn default_operating_mode_str() -> String {
    "assisted".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ServiceStateDto {
    pub session: ServiceSessionDto,
    pub transcript: Vec<TranscriptSegmentDto>,
    pub candidates: Vec<ScriptureCandidateDto>,
    pub integrations: Vec<IntegrationDto>,
    pub health: Vec<HealthItemDto>,
    pub preview: ScriptureCandidateDto,
    pub live: ScriptureCandidateDto,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub reference: String,
    pub translation: String,
    pub snippet: String,
    pub source: String,
    pub language: String,
    /// Canonical verse identity: `"<book>|<chapter>|<verse>"` with an optional
    /// `|<endVerse>` suffix when the result spans an inclusive range. Empty
    /// when the result has no scripture identity (e.g. UI placeholder rows).
    ///
    /// The architectural-vision quote matcher emits this so a quote like "for
    /// God so loved the world" can be re-rendered in any operator-configured
    /// translation without re-running the detection pipeline. See
    /// `fetch_verse_in_translations`.
    #[serde(default)]
    pub verse_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SceneLayerDto {
    pub layer: String,
    pub text: String,
    pub visible: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct OutputSceneDto {
    pub id: String,
    pub reference: String,
    pub translation: String,
    pub theme_id: String,
    pub layers: Vec<SceneLayerDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct LiveOutputResultDto {
    pub scene: OutputSceneDto,
    pub audit_count: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct VmixStatusDto {
    pub state: String,
    pub detail: String,
    pub endpoint: String,
    pub host: String,
    pub port: u16,
    pub title_input: String,
    pub verse_field: String,
    pub reference_field: String,
    pub overlay_channel: u8,
    pub allow_private_network: bool,
    pub checked_at_ms: u64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub auth_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct VmixDispatchResultDto {
    pub state: String,
    pub detail: String,
    pub reference: String,
    pub audit_count: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct VmixConfigDto {
    pub host: String,
    pub port: u16,
    pub title_input: String,
    pub verse_field: String,
    pub reference_field: String,
    pub overlay_channel: u8,
    pub allow_private_network: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ObsConfigDto {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub scene_name: String,
    pub source_name: String,
    pub allow_private_network: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct CompanionConfigDto {
    pub host: String,
    pub port: u16,
    pub page: u16,
    pub row: u16,
    pub column: u16,
    pub verse_variable: String,
    pub reference_variable: String,
    pub allow_private_network: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ProPresenterConfigDto {
    pub host: String,
    pub port: u16,
    pub message_name: String,
    pub verse_token: String,
    pub reference_token: String,
    pub allow_private_network: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct OscConfigDto {
    pub host: String,
    pub port: u16,
    pub namespace: String,
    pub allow_private_network: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct EasyWorshipConfigDto {
    pub watch_dir: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct AdapterDispatchResultDto {
    pub adapter: String,
    pub state: String,
    pub detail: String,
    pub reference: String,
    pub audit_count: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ServiceProfileDto {
    pub id: String,
    pub name: String,
    pub languages: Vec<String>,
    pub output_policy: String,
    pub is_active: bool,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct IntegrationEventDto {
    pub timestamp_ms: u64,
    pub integration_id: String,
    pub severity: String,
    pub action: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SupportBundleExportDto {
    pub path: String,
    pub size_bytes: u64,
    #[ts(type = "any")]
    pub redaction_summary: RedactionSummary,
    pub included_files: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct OfflinePackExportDto {
    pub path: String,
    pub manifest_path: String,
    pub checksum_path: String,
    pub asset_count: u16,
    pub bytes_written: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct BoothPackExportDto {
    pub path: String,
    pub generated_at_ms: u64,
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct LocalRehearsalStepDto {
    pub label: String,
    pub state: String,
    pub detail: String,
    pub duration_ms: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct LocalRehearsalReportDto {
    pub generated_at_ms: u64,
    pub state: String,
    pub passed: u16,
    pub total: u16,
    pub proof_path: String,
    pub steps: Vec<LocalRehearsalStepDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct AiAdapterStatusDto {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub state: String,
    pub detail: String,
    pub latency_ms: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct LanguageDetectionDto {
    pub code: String,
    pub name: String,
    pub confidence: u8,
    pub matched_terms: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SupportedLanguageDto {
    pub code: String,
    pub name: String,
    pub stt_locale: String,
    pub scripture_aliases_ready: bool,
    pub offline_stt_ready: bool,
    pub cloud_stt_ready: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct AccuracyTargetDto {
    pub target_precision: u8,
    pub target_recall: u8,
    pub auto_preview_threshold: u8,
    pub validated_precision: u8,
    pub validated_recall: u8,
    pub validation_sample_count: u16,
    pub live_requires_operator: bool,
    pub strategy: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct PluginVerificationResultDto {
    pub state: String,
    pub detail: String,
    pub manifest: Option<VerifiedPluginManifestDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct VerifiedPluginManifestDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub digest_sha256: String,
    pub key_id: String,
    pub capability_count: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct TrustedPluginDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub key_id: String,
    pub digest: String,
    pub capabilities: Vec<String>,
    pub enabled: bool,
    pub trusted_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct CalibrationReportDto {
    pub confirmed: i64,
    pub corrected: i64,
    pub rejected: i64,
    pub total: i64,
    /// Operator-confirmed precision estimate (0â€“100), or `None` if fewer than 5 samples.
    pub precision: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct AiDetectionResultDto {
    pub mode: String,
    pub decision_policy: String,
    pub processed_segments: u16,
    pub candidates: Vec<ScriptureCandidateDto>,
    pub adapters: Vec<AiAdapterStatusDto>,
    pub languages: Vec<LanguageDetectionDto>,
    pub supported_languages: Vec<SupportedLanguageDto>,
    pub accuracy_target: AccuracyTargetDto,
    pub checked_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
pub struct CaptureStatusDto {
    pub running: bool,
    pub model_path: Option<String>,
}

/// Snapshot of the offline Whisper STT engine for the operator-facing capture
/// controls. This command is intentionally cheap: it only inspects the loaded
/// adapter and model file metadata, while `reload_stt_model` performs the
/// expensive model load.
#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SttStatusDto {
    pub model_loaded: bool,
    pub model_path: Option<String>,
    pub model_filename: Option<String>,
    pub model_size_mb: Option<u32>,
    pub model_quality: Option<String>,
    pub model_warning: Option<String>,
    pub asset_root: Option<String>,
    pub load_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SttLatencyProfileDto {
    pub sample_count: u32,
    pub latest_ms: Option<u32>,
    pub average_ms: Option<u32>,
    pub p50_ms: Option<u32>,
    pub p95_ms: Option<u32>,
    pub fastest_ms: Option<u32>,
    pub slowest_ms: Option<u32>,
    pub target_ms: u32,
    pub state: String,
    pub detail: String,
    pub checked_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct DisplayOutputDto {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub position_x: i32,
    pub position_y: i32,
    pub scale_factor: f64,
    pub is_primary: bool,
    pub likely_hdmi: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct AudioLevelDto {
    pub level: f32,
    pub peak_level: f32,
    pub speech_detected: bool,
    pub checked_at_ms: u64,
    pub rms: f32,
    pub peak: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct CaptureHealthDto {
    pub state: String,
    pub detail: String,
    pub device_name: Option<String>,
    pub checked_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct CommandIntentDto {
    pub intent: String,
    pub reference: Option<String>,
    pub book: Option<String>,
    pub chapter: Option<u16>,
    pub verse: Option<u16>,
    pub translation_id: String,
    pub confidence: f32,
    pub needs_disambiguation: bool,
    pub disambiguation_options: Vec<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct BibleIntegrityTranslationDto {
    pub id: String,
    pub name: String,
    pub verses_loaded: u32,
    pub full_canon: bool,
    pub missing_books: Vec<String>,
    pub missing_chapters: Vec<String>,
    pub state: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct VectorKbStatusDto {
    pub state: String,
    pub detail: String,
    pub manifest_path: String,
    pub service_url: String,
    pub service_online: bool,
    pub indexed_translations: Vec<String>,
    pub total_documents: u32,
    pub built_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct BackendDiagnosticsDto {
    pub state: String,
    pub checked_at_ms: u64,
    pub database_path: String,
    pub capture: CaptureStatusDto,
    pub stt: SttStatusDto,
    pub vector: VectorKbStatusDto,
    pub bibles: Vec<BibleIntegrityTranslationDto>,
    pub displays: Vec<DisplayOutputDto>,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ReleaseGateCheckDto {
    pub id: String,
    pub label: String,
    pub state: String,
    pub detail: String,
    pub blocking: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ProductionReleaseGateDto {
    pub state: String,
    pub checked_at_ms: u64,
    pub passed: u16,
    pub total: u16,
    pub checks: Vec<ReleaseGateCheckDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ScriptureRegressionFailureDto {
    pub reference: String,
    pub expected_book: String,
    pub expected_chapter: u16,
    pub expected_verse: u16,
    pub actual_reference: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ScriptureRegressionReportDto {
    pub translation_id: String,
    pub state: String,
    pub checked_at_ms: u64,
    pub duration_ms: u64,
    pub books_checked: u16,
    pub chapters_checked: u16,
    pub verses_checked: u32,
    pub direct_lookup_checked: u32,
    pub grammar_checked: u32,
    pub voice_command_checked: u32,
    pub search_path_checked: u32,
    pub partial_quote_checked: u32,
    pub passed: u32,
    pub failed: u32,
    pub first_failures: Vec<ScriptureRegressionFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct ScriptureRegressionJobDto {
    pub job_id: String,
    pub state: String,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub cancel_requested: bool,
    pub report: Option<ScriptureRegressionReportDto>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// CCLI usage (persisted through the chained-hash audit log)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct CcliUsageEntryDto {
    pub id: String,
    pub ccli_number: String,
    pub song_title: String,
    pub sent_live_at_ms: u64,
    pub service_session_id: String,
    pub operator: String,
}

// ---------------------------------------------------------------------------
// Fleet bundle signing (ed25519)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct SignedFleetBundleDto {
    /// Hex-encoded ed25519 signature over the UTF-8 bytes of `payload_json`.
    pub signature_hex: String,
    /// Hex-encoded ed25519 public key for verification.
    pub public_key_hex: String,
    /// Hex-encoded SHA-256 digest of the payload, for integrity checks.
    pub payload_sha256: String,
    /// The original bundle JSON (pass-through).
    pub payload_json: String,
    /// Milliseconds since the Unix epoch when the bundle was signed.
    pub signed_at_ms: u64,
    pub signer_label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct FleetVerifyResultDto {
    pub valid: bool,
    pub detail: String,
    pub public_key_hex: String,
    pub payload_sha256: String,
}

// ---------------------------------------------------------------------------
// Stream overlay HTTP server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct StreamOverlayStateDto {
    pub ticker_text: String,
    pub armed: bool,
    pub live_reference: Option<String>,
    pub live_text: Option<String>,
    /// Translations keyed by BCP-47 language code → translated string.
    pub translations: std::collections::HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/gen/")]
#[serde(rename_all = "camelCase")]
pub struct StreamOverlayServerStatusDto {
    pub running: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub started_at_ms: Option<u64>,
}
