//! Production operations policies for Aletheia.
//!
//! This crate deliberately keeps operational safety logic outside the UI. The
//! desktop shell can expose these reports, tests can verify them, and support
//! tooling can reuse them without reaching into React state.

use std::collections::BTreeSet;
use std::convert::TryInto;
use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result type for production operations policies.
pub type OpsResult<T> = Result<T, OpsError>;

/// Production operations errors.
#[derive(Debug)]
pub enum OpsError {
    InvalidManifest(String),
    InvalidSignature(String),
    Json(serde_json::Error),
}

impl std::fmt::Display for OpsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidManifest(message) => {
                write!(formatter, "invalid plugin manifest: {message}")
            }
            Self::InvalidSignature(message) => {
                write!(formatter, "invalid plugin signature: {message}")
            }
            Self::Json(error) => write!(formatter, "json error: {error}"),
        }
    }
}

impl std::error::Error for OpsError {}

impl From<serde_json::Error> for OpsError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Status for a secure secret-storage boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretVaultStatus {
    pub provider: String,
    pub state: String,
    pub detail: String,
    pub stored_secret_count: u16,
    pub release_required: bool,
    pub policy: Vec<String>,
}

/// Non-secret pointer to a credential held outside SQLite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub created_at_ms: u64,
}

/// Creates the default secure-storage status for the current platform.
pub fn default_secret_vault_status(stored_secret_count: u16) -> SecretVaultStatus {
    let provider = if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else {
        "Linux Secret Service or Tauri Stronghold"
    };

    SecretVaultStatus {
        provider: provider.to_string(),
        state: if stored_secret_count == 0 {
            "ready".to_string()
        } else {
            "healthy".to_string()
        },
        detail: "SQLite stores only secret references. Raw provider keys must live in the OS vault boundary.".to_string(),
        stored_secret_count,
        release_required: true,
        policy: vec![
            "No secret values in SQLite, logs, support bundles, or plugin manifests.".to_string(),
            "Credential reads are adapter-scoped and audited by secret reference id.".to_string(),
            "Support bundles include credential labels only after redaction.".to_string(),
        ],
    }
}

/// A signed plugin manifest payload. This is the part that is signed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifestPayload {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub entrypoint: String,
    pub capabilities: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub minimum_aletheia_version: String,
}

/// Ed25519 signature envelope for a plugin manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSignatureEnvelope {
    pub algorithm: String,
    pub key_id: String,
    pub public_key_b64: String,
    pub signature_b64: String,
}

/// Full signed plugin manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedPluginManifest {
    pub payload: PluginManifestPayload,
    pub signature: PluginSignatureEnvelope,
}

/// Verified plugin manifest summary exposed to the UI and logs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedPluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub digest_sha256: String,
    pub key_id: String,
    pub capability_count: u16,
}

/// Current plugin policy status for production readiness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPolicyStatus {
    pub state: String,
    pub detail: String,
    pub trusted_key_count: u16,
    pub required_controls: Vec<String>,
}

/// Canonical JSON bytes used for manifest signing.
pub fn canonical_plugin_payload(payload: &PluginManifestPayload) -> OpsResult<Vec<u8>> {
    Ok(serde_json::to_vec(payload)?)
}

/// Verifies a signed plugin manifest against trusted key ids and manifest policy.
pub fn verify_signed_plugin_manifest(
    manifest: &SignedPluginManifest,
    trusted_key_ids: &[String],
) -> OpsResult<VerifiedPluginManifest> {
    validate_plugin_policy(&manifest.payload)?;
    if manifest.signature.algorithm != "ed25519" {
        return Err(OpsError::InvalidSignature(
            "only ed25519 plugin signatures are accepted".to_string(),
        ));
    }
    if !trusted_key_ids
        .iter()
        .any(|trusted| trusted == &manifest.signature.key_id)
    {
        return Err(OpsError::InvalidSignature(format!(
            "key id {} is not trusted",
            manifest.signature.key_id
        )));
    }

    let public_key_bytes = BASE64
        .decode(&manifest.signature.public_key_b64)
        .map_err(|error| {
            OpsError::InvalidSignature(format!("public key is not base64: {error}"))
        })?;
    let signature_bytes = BASE64
        .decode(&manifest.signature.signature_b64)
        .map_err(|error| OpsError::InvalidSignature(format!("signature is not base64: {error}")))?;
    let public_key_array: [u8; 32] = public_key_bytes.as_slice().try_into().map_err(|_| {
        OpsError::InvalidSignature("ed25519 public key must be 32 bytes".to_string())
    })?;
    let signature_array: [u8; 64] = signature_bytes.as_slice().try_into().map_err(|_| {
        OpsError::InvalidSignature("ed25519 signature must be 64 bytes".to_string())
    })?;

    let verifying_key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|error| OpsError::InvalidSignature(format!("public key rejected: {error}")))?;
    let signature = Signature::from_bytes(&signature_array);
    let payload_bytes = canonical_plugin_payload(&manifest.payload)?;
    verifying_key
        .verify(&payload_bytes, &signature)
        .map_err(|error| {
            OpsError::InvalidSignature(format!("signature verification failed: {error}"))
        })?;

    Ok(VerifiedPluginManifest {
        id: manifest.payload.id.clone(),
        name: manifest.payload.name.clone(),
        version: manifest.payload.version.clone(),
        digest_sha256: sha256_hex(&payload_bytes),
        key_id: manifest.signature.key_id.clone(),
        capability_count: manifest.payload.capabilities.len() as u16,
    })
}

fn validate_plugin_policy(payload: &PluginManifestPayload) -> OpsResult<()> {
    if payload.id.trim().is_empty() || payload.name.trim().is_empty() {
        return Err(OpsError::InvalidManifest(
            "plugin id and name are required".to_string(),
        ));
    }
    if payload.entrypoint.starts_with('/')
        || payload.entrypoint.contains("..")
        || payload.entrypoint.trim().is_empty()
    {
        return Err(OpsError::InvalidManifest(
            "plugin entrypoint must be a relative path inside the signed plugin package"
                .to_string(),
        ));
    }
    if payload.capabilities.is_empty() {
        return Err(OpsError::InvalidManifest(
            "plugin must declare at least one capability".to_string(),
        ));
    }
    if payload
        .allowed_hosts
        .iter()
        .any(|host| host == "*" || host == "0.0.0.0/0")
    {
        return Err(OpsError::InvalidManifest(
            "wildcard network hosts are not allowed".to_string(),
        ));
    }

    let unique_capabilities = payload.capabilities.iter().collect::<BTreeSet<_>>();
    if unique_capabilities.len() != payload.capabilities.len() {
        return Err(OpsError::InvalidManifest(
            "plugin capabilities must be unique".to_string(),
        ));
    }
    Ok(())
}

/// Offline asset installed/readiness state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineAsset {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub language: String,
    pub license: String,
    pub state: String,
    pub size_mb: u32,
    pub checksum_sha256: String,
    pub required_for_release: bool,
}

/// Offline asset manifest summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineAssetManifest {
    pub state: String,
    pub installed_count: u16,
    pub required_count: u16,
    pub assets: Vec<OfflineAsset>,
}

/// Default packaged asset plan for the Nigeria-first local build.
pub fn production_offline_asset_manifest() -> OfflineAssetManifest {
    struct AssetConfig<'a> {
        id: &'a str,
        kind: &'a str,
        label: &'a str,
        language: &'a str,
        license: &'a str,
        state: &'a str,
        size_mb: u32,
        required_for_release: bool,
        // Real SHA-256 the operator must supply when installing from file.
        // Empty string means "not yet known / not a file-based asset".
        expected_checksum: &'a str,
    }

    fn asset(config: AssetConfig<'_>) -> OfflineAsset {
        OfflineAsset {
            id: config.id.to_string(),
            kind: config.kind.to_string(),
            label: config.label.to_string(),
            language: config.language.to_string(),
            license: config.license.to_string(),
            state: config.state.to_string(),
            size_mb: config.size_mb,
            checksum_sha256: if !config.expected_checksum.is_empty() {
                config.expected_checksum.to_string()
            } else if config.state == "installed" {
                "packaged-at-build".to_string()
            } else {
                "pending-download".to_string()
            },
            required_for_release: config.required_for_release,
        }
    }

    macro_rules! asset {
        ($id:expr, $kind:expr, $label:expr, $language:expr, $license:expr, $state:expr, $size_mb:expr, $required_for_release:expr, $expected_checksum:expr $(,)?) => {
            asset(AssetConfig {
                id: $id,
                kind: $kind,
                label: $label,
                language: $language,
                license: $license,
                state: $state,
                size_mb: $size_mb,
                required_for_release: $required_for_release,
                expected_checksum: $expected_checksum,
            })
        };
    }

    // Real SHA-256 checksums computed from the downloaded ggml model files:
    //   ggml-small.en.bin     (466 MB, English-only)
    //   ggml-base.bin         (142 MB, multilingual — covers all 7 languages)
    const CHECKSUM_EN: &str = "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d";
    const CHECKSUM_MULTI: &str = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";

    let assets = vec![
        // Scripture assets — bundled at build time, no file install required.
        asset!(
            "bible-kjv",
            "scripture",
            "King James Version",
            "English",
            "public-domain",
            "installed",
            5,
            true,
            "",
        ),
        asset!(
            "bible-web",
            "scripture",
            "World English Bible",
            "English",
            "public-domain",
            "installed",
            7,
            true,
            "",
        ),
        // Scripture alias bundles — compiled into the binary.
        asset!(
            "aliases-yoruba",
            "scripture-aliases",
            "Yoruba book aliases",
            "Yoruba",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-igbo",
            "scripture-aliases",
            "Igbo book aliases",
            "Igbo",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-hausa",
            "scripture-aliases",
            "Hausa book aliases",
            "Hausa",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-twi",
            "scripture-aliases",
            "Twi book aliases",
            "Twi",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-swahili",
            "scripture-aliases",
            "Swahili book aliases",
            "Swahili",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-xhosa",
            "scripture-aliases",
            "Xhosa book aliases",
            "Xhosa",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-spanish",
            "scripture-aliases",
            "Spanish book aliases",
            "Spanish",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        asset!(
            "aliases-french",
            "scripture-aliases",
            "French book aliases",
            "French",
            "internal-index",
            "installed",
            1,
            true,
            "",
        ),
        // STT model: Whisper small.en (English-only, 466 MB).
        // File: ggml-small.en.bin  —  install via Health → Offline Model Packs.
        asset!(
            "stt-whisper-en-small",
            "stt-model",
            "Offline English STT (Whisper small.en)",
            "English",
            "operator-provided-model",
            "pending",
            466,
            true,
            CHECKSUM_EN,
        ),
        // STT model: Whisper base multilingual (142 MB).
        // ONE physical file — ggml-base.bin — handles every non-English locale
        // we care about. Listing eight aliased packs of the same file misled
        // operators into thinking each language was a separate download. The
        // multilingual pack is now surfaced once; per-language behaviour is
        // selected at inference time via the `language_hint` argument to
        // start_audio_capture.
        asset!(
            "stt-whisper-multilingual",
            "stt-model",
            "Offline multilingual STT (Whisper base, 99 languages)",
            "Multilingual",
            "operator-provided-model",
            "pending",
            142,
            false,
            CHECKSUM_MULTI,
        ),
    ];
    let installed_count = assets
        .iter()
        .filter(|asset| asset.state == "installed")
        .count() as u16;
    let required_count = assets
        .iter()
        .filter(|asset| asset.required_for_release)
        .count() as u16;
    let missing_required = assets
        .iter()
        .any(|asset| asset.required_for_release && asset.state != "installed");

    OfflineAssetManifest {
        state: if missing_required { "blocked" } else { "ready" }.to_string(),
        installed_count,
        required_count,
        assets,
    }
}

/// Device acceptance state for production integration rehearsals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceStep {
    pub label: String,
    pub expected: String,
    pub required: bool,
}

/// Device acceptance plan item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceDevice {
    pub id: String,
    pub name: String,
    pub category: String,
    pub state: String,
    pub required_for_release: bool,
    pub steps: Vec<AcceptanceStep>,
}

/// Device-level acceptance plan across live production integrations.
pub fn production_acceptance_plan() -> Vec<AcceptanceDevice> {
    vec![
        device(
            "vmix",
            "vMix",
            "broadcast",
            true,
            &[
                (
                    "Check API",
                    "Configured title input appears in vMix XML",
                    true,
                ),
                (
                    "Preview title",
                    "Overlay preview updates without going live",
                    true,
                ),
                (
                    "Take live",
                    "Overlay 2 enters program only after destinations are armed",
                    true,
                ),
                (
                    "Clear",
                    "Overlay 2 exits program without affecting lyrics or lower thirds",
                    true,
                ),
            ],
        ),
        device(
            "obs",
            "OBS Studio",
            "broadcast",
            true,
            &[
                ("Connect", "WebSocket auth succeeds on loopback", true),
                ("Preview text", "Text source updates in preview scene", true),
                (
                    "Live text",
                    "Program scene updates only after explicit operator action",
                    true,
                ),
            ],
        ),
        device(
            "easyworship",
            "EasyWorship",
            "presentation",
            true,
            &[
                (
                    "Export",
                    "Aletheia writes a slide file into the configured watch folder",
                    true,
                ),
                (
                    "Schedule handoff",
                    "EasyWorship imports the slide without admin elevation",
                    true,
                ),
            ],
        ),
        device(
            "propresenter",
            "ProPresenter",
            "presentation",
            false,
            &[
                (
                    "API health",
                    "Configured ProPresenter host is allowlisted",
                    true,
                ),
                (
                    "Playlist cue",
                    "Stage display cue lands in rehearsal playlist",
                    true,
                ),
            ],
        ),
        device(
            "hdmi",
            "HDMI output",
            "display",
            true,
            &[
                (
                    "Display detect",
                    "Projector display appears at 1920x1080 or configured fallback",
                    true,
                ),
                (
                    "Safe area",
                    "Verse and reference remain inside church projector safe area",
                    true,
                ),
            ],
        ),
        device(
            "ndi",
            "NDI output",
            "video",
            false,
            &[
                (
                    "Discovery",
                    "Aletheia Lower Third is visible on the production network",
                    true,
                ),
                (
                    "Alpha",
                    "Verse layer preserves transparency in receiver",
                    true,
                ),
            ],
        ),
    ]
}

fn device(
    id: &str,
    name: &str,
    category: &str,
    required_for_release: bool,
    steps: &[(&str, &str, bool)],
) -> AcceptanceDevice {
    AcceptanceDevice {
        id: id.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        state: "not-run".to_string(),
        required_for_release,
        steps: steps
            .iter()
            .map(|(label, expected, required)| AcceptanceStep {
                label: (*label).to_string(),
                expected: (*expected).to_string(),
                required: *required,
            })
            .collect(),
    }
}

/// Release gate status for product-readiness reporting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseGateStatus {
    pub label: String,
    pub state: String,
    pub detail: String,
}

/// Support bundle plan exposed to operators before export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBundlePlan {
    pub state: String,
    pub detail: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

/// Redaction counts for a support bundle export.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionSummary {
    pub emails: u16,
    pub ip_addresses: u16,
    pub windows_paths: u16,
    pub unix_paths: u16,
    pub secret_like_values: u16,
}

/// Production readiness report returned to the desktop UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionReadinessReport {
    pub generated_at_ms: u64,
    pub score: u8,
    pub state: String,
    pub blockers: Vec<String>,
    pub secret_vault: SecretVaultStatus,
    pub plugin_policy: PluginPolicyStatus,
    pub support_bundle: SupportBundlePlan,
    pub offline_assets: OfflineAssetManifest,
    pub acceptance_devices: Vec<AcceptanceDevice>,
    pub release_gates: Vec<ReleaseGateStatus>,
}

/// Builds the default readiness report. Hardware steps remain not-run until an operator rehearses them.
pub fn production_readiness_report(
    generated_at_ms: u64,
    trusted_plugin_key_count: u16,
    stored_secret_count: u16,
) -> ProductionReadinessReport {
    production_readiness_report_with_assets(
        generated_at_ms,
        trusted_plugin_key_count,
        stored_secret_count,
        production_offline_asset_manifest(),
    )
}

/// Builds a readiness report using a caller-provided offline asset manifest.
pub fn production_readiness_report_with_assets(
    generated_at_ms: u64,
    trusted_plugin_key_count: u16,
    stored_secret_count: u16,
    offline_assets: OfflineAssetManifest,
) -> ProductionReadinessReport {
    let secret_vault = default_secret_vault_status(stored_secret_count);
    let acceptance_devices = production_acceptance_plan();
    let plugin_policy = PluginPolicyStatus {
        state: if trusted_plugin_key_count > 0 {
            "ready"
        } else {
            "blocked"
        }
        .to_string(),
        detail: if trusted_plugin_key_count > 0 {
            "Plugin manifests require Ed25519 signatures from trusted release keys.".to_string()
        } else {
            "Add a production plugin signing key before enabling third-party plugins.".to_string()
        },
        trusted_key_count: trusted_plugin_key_count,
        required_controls: vec![
            "Ed25519 signature over canonical manifest payload.".to_string(),
            "No wildcard network hosts.".to_string(),
            "Explicit capability list and relative package entrypoint.".to_string(),
            "Plugin failure must degrade only its destination.".to_string(),
        ],
    };
    let support_bundle = SupportBundlePlan {
        state: "ready".to_string(),
        detail: "Exports diagnostics as redacted JSON. Transcript text is excluded unless the operator opts in.".to_string(),
        includes: vec![
            "Version and local runtime mode".to_string(),
            "Health summary and adapter delivery receipts".to_string(),
            "Redacted integration configuration".to_string(),
            "Device acceptance checklist".to_string(),
        ],
        excludes: vec![
            "Raw provider keys".to_string(),
            "Unredacted local usernames and absolute home paths".to_string(),
            "Transcript text by default".to_string(),
        ],
    };

    let mut blockers = Vec::new();
    if plugin_policy.state == "blocked" {
        blockers.push("Production plugin signing key is not configured.".to_string());
    }
    if offline_assets.state != "ready" {
        blockers.push("Required offline assets are missing.".to_string());
    }
    blockers
        .push("Device-level acceptance rehearsals still need real hardware evidence.".to_string());

    let score = if blockers.is_empty() {
        96
    } else if trusted_plugin_key_count == 0 {
        78
    } else {
        86
    };

    ProductionReadinessReport {
        generated_at_ms,
        score,
        state: if blockers.is_empty() { "ready" } else { "degraded" }.to_string(),
        blockers,
        secret_vault,
        plugin_policy,
        support_bundle,
        offline_assets,
        acceptance_devices,
        release_gates: vec![
            ReleaseGateStatus {
                label: "Build verification".to_string(),
                state: "healthy".to_string(),
                detail: "Run npm run verify:production before every signed build.".to_string(),
            },
            ReleaseGateStatus {
                label: "Signed installer".to_string(),
                state: "degraded".to_string(),
                detail: "Release key and updater endpoint must be configured outside source control.".to_string(),
            },
            ReleaseGateStatus {
                label: "Rollback drill".to_string(),
                state: "degraded".to_string(),
                detail: "Install, upgrade, rollback, and offline reinstall must be rehearsed on Windows.".to_string(),
            },
            ReleaseGateStatus {
                label: "Asset licensing".to_string(),
                state: "degraded".to_string(),
                detail: "Bible and STT model licenses must be approved per distribution region.".to_string(),
            },
        ],
    }
}

/// Redacts support-bundle text. This is intentionally conservative and deterministic.
pub fn redact_support_text(input: &str) -> (String, RedactionSummary) {
    let mut output = input.to_string();
    let mut summary = RedactionSummary::default();

    let patterns = [
        (
            Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("email regex"),
            "[redacted-email]",
            RedactionKind::Email,
        ),
        (
            Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("ipv4 regex"),
            "[redacted-ip]",
            RedactionKind::IpAddress,
        ),
        (
            Regex::new(r#"[A-Za-z]:\\[^\s",}]+"#).expect("windows path regex"),
            "[redacted-windows-path]",
            RedactionKind::WindowsPath,
        ),
        (
            Regex::new(r#"/(?:home|Users|mnt/[a-z]/Users)/[^\s",}]+"#).expect("unix path regex"),
            "[redacted-local-path]",
            RedactionKind::UnixPath,
        ),
        (
            Regex::new(r#"(?i)(api[_-]?key|token|secret|password)["'=:\s]+[A-Za-z0-9_\-./+]{8,}"#)
                .expect("secret-like regex"),
            "[redacted-secret-like-value]",
            RedactionKind::SecretLike,
        ),
    ];

    for (regex, replacement, kind) in patterns {
        let count = regex.find_iter(&output).count() as u16;
        if count > 0 {
            output = regex.replace_all(&output, replacement).to_string();
            match kind {
                RedactionKind::Email => summary.emails = summary.emails.saturating_add(count),
                RedactionKind::IpAddress => {
                    summary.ip_addresses = summary.ip_addresses.saturating_add(count)
                }
                RedactionKind::WindowsPath => {
                    summary.windows_paths = summary.windows_paths.saturating_add(count)
                }
                RedactionKind::UnixPath => {
                    summary.unix_paths = summary.unix_paths.saturating_add(count)
                }
                RedactionKind::SecretLike => {
                    summary.secret_like_values = summary.secret_like_values.saturating_add(count)
                }
            }
        }
    }

    (output, summary)
}

enum RedactionKind {
    Email,
    IpAddress,
    WindowsPath,
    UnixPath,
    SecretLike,
}

fn sha256_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_signed_plugin_manifest() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let payload = PluginManifestPayload {
            id: "obs-main".to_string(),
            name: "OBS Studio Adapter".to_string(),
            vendor: "Aletheia".to_string(),
            version: "1.0.0".to_string(),
            entrypoint: "adapter.wasm".to_string(),
            capabilities: vec!["write-preview".to_string(), "write-live-output".to_string()],
            allowed_hosts: vec!["127.0.0.1".to_string()],
            minimum_aletheia_version: "0.1.0".to_string(),
        };
        let payload_bytes = canonical_plugin_payload(&payload).expect("payload serializes");
        let signature = signing_key.sign(&payload_bytes);
        let manifest = SignedPluginManifest {
            payload,
            signature: PluginSignatureEnvelope {
                algorithm: "ed25519".to_string(),
                key_id: "aletheia-test".to_string(),
                public_key_b64: BASE64.encode(signing_key.verifying_key().as_bytes()),
                signature_b64: BASE64.encode(signature.to_bytes()),
            },
        };

        let verified = verify_signed_plugin_manifest(&manifest, &["aletheia-test".to_string()])
            .expect("valid signature");

        assert_eq!(verified.id, "obs-main");
        assert_eq!(verified.capability_count, 2);
    }

    #[test]
    fn rejects_wildcard_plugin_hosts() {
        let payload = PluginManifestPayload {
            id: "bad".to_string(),
            name: "Bad".to_string(),
            vendor: "Unknown".to_string(),
            version: "1.0.0".to_string(),
            entrypoint: "adapter.wasm".to_string(),
            capabilities: vec!["write-live-output".to_string()],
            allowed_hosts: vec!["*".to_string()],
            minimum_aletheia_version: "0.1.0".to_string(),
        };

        assert!(validate_plugin_policy(&payload).is_err());
    }

    #[test]
    fn support_bundle_redacts_sensitive_text() {
        let (redacted, summary) = redact_support_text(
            r#"operator@example.com host=192.168.1.44 path=C:\Users\USER\AppData token=abc123456789 /home/mastapraise/.config"#,
        );

        assert!(redacted.contains("[redacted-email]"));
        assert!(redacted.contains("[redacted-ip]"));
        assert!(redacted.contains("[redacted-windows-path]"));
        assert!(redacted.contains("[redacted-local-path]"));
        assert!(redacted.contains("[redacted-secret-like-value]"));
        assert_eq!(summary.emails, 1);
        assert_eq!(summary.ip_addresses, 1);
    }

    #[test]
    fn readiness_report_surfaces_release_blockers() {
        // Build a manifest where every required asset is installed so we can
        // isolate the plugin-signing-key blocker without the offline-asset
        // blocker also firing. (The default manifest reports `blocked` because
        // ggml-small.en.bin must be operator-installed.)
        let mut assets = production_offline_asset_manifest();
        for asset in assets.assets.iter_mut() {
            if asset.required_for_release {
                asset.state = "installed".to_string();
            }
        }
        assets.state = "ready".to_string();
        assets.installed_count = assets.assets.len() as u16;

        let report = production_readiness_report_with_assets(100, 0, 0, assets);

        assert_eq!(report.state, "degraded");
        assert!(
            report
                .blockers
                .iter()
                .any(|blocker| blocker.contains("plugin signing key"))
        );
        assert_eq!(report.offline_assets.state, "ready");
    }
}
