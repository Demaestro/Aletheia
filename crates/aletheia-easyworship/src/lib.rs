//! EasyWorship 7+ watch-folder output adapter for Aletheia worship production.
//!
//! EasyWorship 7.4+ reads a "Data Feed" file from a configured watch directory.
//! Aletheia writes two files on every preview/live dispatch:
//!
//! - `NowPlaying.txt`  — plain-text fallback (reference, translation, verse text).
//! - `NowPlaying.xml`  — structured feed EasyWorship can import directly.
//!
//! The operator points EasyWorship's "Data Feed" watcher at the configured folder.
//! EasyWorship polls for changes roughly every 100 ms.
//!
//! ## State file
//!
//! `AletheiaState.json` in the same folder records the last dispatched scene and
//! the dispatch type (`preview` | `live` | `clear`). It is machine-readable and
//! useful for health checks.

use std::fs;
use std::path::{Path, PathBuf};

use aletheia_core::IntegrationId;
use aletheia_output::{
    OutputAdapter, OutputAdapterStatus, OutputCapability, OutputError, OutputHealth, OutputKind,
    OutputLayer, OutputScene,
};

/// File names written into the watch directory.
pub const FILE_TXT: &str = "NowPlaying.txt";
pub const FILE_XML: &str = "NowPlaying.xml";
pub const FILE_STATE: &str = "AletheiaState.json";

/// Returns the platform-appropriate default watch-folder path.
///
/// EasyWorship is Windows-only software, but Aletheia itself may run on macOS
/// or Linux during development.  The returned path is in the OS-standard
/// application-data location so it does not require elevated permissions.
pub fn default_watch_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        // %LOCALAPPDATA%\Aletheia\easyworship-feed, e.g.
        // C:\Users\Alice\AppData\Local\Aletheia\easyworship-feed
        let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| r"C:\Aletheia".to_string());
        std::path::PathBuf::from(base)
            .join("Aletheia")
            .join("easyworship-feed")
    }
    #[cfg(target_os = "macos")]
    {
        // ~/Library/Application Support/Aletheia/easyworship-feed
        let base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(base)
            .join("Library")
            .join("Application Support")
            .join("Aletheia")
            .join("easyworship-feed")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // ~/.local/share/Aletheia/easyworship-feed  (XDG on Linux)
        let base = std::env::var("XDG_DATA_HOME")
            .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.local/share")))
            .unwrap_or_else(|_| "/tmp".to_string());
        std::path::PathBuf::from(base)
            .join("Aletheia")
            .join("easyworship-feed")
    }
}

/// EasyWorship adapter configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EasyWorshipConfig {
    pub integration_id: String,
    /// Absolute path to the directory EasyWorship monitors for data-feed files.
    pub watch_dir: PathBuf,
}

impl Default for EasyWorshipConfig {
    fn default() -> Self {
        Self {
            integration_id: "easyworship-main".to_string(),
            watch_dir: default_watch_dir(),
        }
    }
}

impl EasyWorshipConfig {
    /// Validates that `watch_dir` is an absolute path.
    pub fn validate(&self) -> Result<(), EasyWorshipError> {
        if !self.watch_dir.is_absolute() {
            return Err(EasyWorshipError::InvalidConfig(format!(
                "EasyWorship watch directory '{}' must be an absolute path",
                self.watch_dir.display()
            )));
        }
        Ok(())
    }
}

/// Dispatch kind recorded in `AletheiaState.json`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchKind {
    Preview,
    Live,
    Clear,
}

impl DispatchKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Live => "live",
            Self::Clear => "clear",
        }
    }
}

/// EasyWorship watch-folder adapter.
pub struct EasyWorshipAdapter {
    config: EasyWorshipConfig,
}

impl EasyWorshipAdapter {
    /// Creates an adapter from explicit configuration.
    pub fn new(config: EasyWorshipConfig) -> Self {
        Self { config }
    }

    /// Returns immutable configuration for diagnostics.
    pub fn config(&self) -> &EasyWorshipConfig {
        &self.config
    }

    /// Checks whether the watch directory exists and is writable.
    pub fn check_status(&self) -> Result<(), EasyWorshipError> {
        self.config.validate()?;
        let dir = &self.config.watch_dir;

        // Try to create the directory. If it already exists this is a no-op.
        fs::create_dir_all(dir).map_err(|e| {
            EasyWorshipError::DirectoryError(format!(
                "cannot create EasyWorship watch directory '{}': {e}",
                dir.display()
            ))
        })?;

        // Verify write access with a probe file.
        let probe = dir.join(".aletheia-probe");
        fs::write(&probe, b"probe").map_err(|e| {
            EasyWorshipError::WriteError(format!(
                "EasyWorship watch directory '{}' is not writable: {e}",
                dir.display()
            ))
        })?;
        let _ = fs::remove_file(&probe);
        Ok(())
    }

    /// Writes all three feed files to the watch directory.
    fn dispatch(
        &self,
        scene: Option<&OutputScene>,
        kind: DispatchKind,
    ) -> Result<(), EasyWorshipError> {
        self.config.validate()?;
        let dir = &self.config.watch_dir;
        fs::create_dir_all(dir).map_err(|e| {
            EasyWorshipError::DirectoryError(format!(
                "cannot create EasyWorship watch directory '{}': {e}",
                dir.display()
            ))
        })?;

        match scene {
            Some(scene) => {
                let reference = layer_text(scene, OutputLayer::Reference);
                let verse = layer_text(scene, OutputLayer::Verse);

                write_file(
                    &dir.join(FILE_TXT),
                    &plain_text_feed(reference, &scene.translation, verse),
                )?;
                write_file(
                    &dir.join(FILE_XML),
                    &xml_feed(reference, &scene.translation, verse, kind),
                )?;
                write_file(
                    &dir.join(FILE_STATE),
                    &state_json(&scene.reference, &scene.translation, verse, kind),
                )?;
            }
            None => {
                // Clear: write blank files so EasyWorship clears its display.
                write_file(&dir.join(FILE_TXT), "Aletheia\n\n\n")?;
                write_file(
                    &dir.join(FILE_XML),
                    &xml_feed("", "", "", DispatchKind::Clear),
                )?;
                write_file(&dir.join(FILE_STATE), &state_json("", "", "", kind))?;
            }
        }

        Ok(())
    }
}

impl Default for EasyWorshipAdapter {
    fn default() -> Self {
        Self::new(EasyWorshipConfig::default())
    }
}

impl OutputAdapter for EasyWorshipAdapter {
    fn status(&self) -> OutputAdapterStatus {
        let health = match self.check_status() {
            Ok(()) => OutputHealth::Connected,
            Err(EasyWorshipError::InvalidConfig(m)) => OutputHealth::Offline(m),
            Err(e) => OutputHealth::Degraded(e.to_string()),
        };

        OutputAdapterStatus {
            id: integration_id_or_fallback(&self.config.integration_id),
            display_name: "EasyWorship".to_string(),
            kind: OutputKind::EasyWorship,
            capabilities: vec![
                OutputCapability::Preview,
                OutputCapability::Live,
                OutputCapability::Clear,
                OutputCapability::DryRun,
            ],
            health,
        }
    }

    fn dry_run(&self, _scene: &OutputScene) -> Result<(), OutputError> {
        self.check_status().map_err(ew_to_output_error)
    }

    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.dispatch(Some(scene), DispatchKind::Preview)
            .map_err(ew_to_output_error)
    }

    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.dispatch(Some(scene), DispatchKind::Live)
            .map_err(ew_to_output_error)
    }

    fn clear(&mut self) -> Result<(), OutputError> {
        self.dispatch(None, DispatchKind::Clear)
            .map_err(ew_to_output_error)
    }
}

// ---------------------------------------------------------------------------
// File content builders
// ---------------------------------------------------------------------------

fn plain_text_feed(reference: &str, translation: &str, verse: &str) -> String {
    format!("Aletheia Scripture\n{reference} {translation}\n\n{verse}\n")
}

fn xml_feed(reference: &str, translation: &str, verse: &str, kind: DispatchKind) -> String {
    // EasyWorship Data Feed XML schema (compatible with EW 7.4+).
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DataFeed xmlns="http://easyworship.com/datafeed/1.0" type="scripture" dispatch="{kind}">
  <Reference>{ref_escaped}</Reference>
  <Translation>{trans_escaped}</Translation>
  <VerseText><![CDATA[{verse}]]></VerseText>
  <Source>Aletheia</Source>
</DataFeed>
"#,
        kind = kind.as_str(),
        ref_escaped = xml_escape(reference),
        trans_escaped = xml_escape(translation),
    )
}

fn state_json(reference: &str, translation: &str, verse: &str, kind: DispatchKind) -> String {
    // Use manual JSON serialization to avoid adding serde_json to this crate's hot path.
    format!(
        r#"{{"source":"Aletheia","dispatch":"{kind}","reference":"{ref_j}","translation":"{trans_j}","verse":"{verse_j}"}}"#,
        kind = kind.as_str(),
        ref_j = json_escape(reference),
        trans_j = json_escape(translation),
        verse_j = json_escape(verse),
    )
}

fn write_file(path: &Path, content: &str) -> Result<(), EasyWorshipError> {
    fs::write(path, content.as_bytes()).map_err(|e| {
        EasyWorshipError::WriteError(format!("failed to write '{}': {e}", path.display()))
    })
}

/// Minimal XML entity escaping for reference/translation fields.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Minimal JSON string escaping.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn layer_text(scene: &OutputScene, layer: OutputLayer) -> &str {
    scene
        .layers
        .iter()
        .find(|l| l.layer == layer && l.visible)
        .map(|l| l.text.as_str())
        .unwrap_or("")
}

fn integration_id_or_fallback(value: &str) -> IntegrationId {
    IntegrationId::new(value.to_string())
        .or_else(|_| IntegrationId::new("easyworship".to_string()))
        .expect("static EasyWorship integration id is valid")
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// EasyWorship adapter errors with operator-safe wording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EasyWorshipError {
    InvalidConfig(String),
    DirectoryError(String),
    WriteError(String),
}

impl std::fmt::Display for EasyWorshipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(m) | Self::DirectoryError(m) | Self::WriteError(m) => {
                f.write_str(m)
            }
        }
    }
}

impl std::error::Error for EasyWorshipError {}

fn ew_to_output_error(e: EasyWorshipError) -> OutputError {
    match e {
        EasyWorshipError::InvalidConfig(m) => OutputError::DispatchFailed(m),
        EasyWorshipError::DirectoryError(m) | EasyWorshipError::WriteError(m) => {
            OutputError::NotConnected(m)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aletheia_core::ServiceSessionId;
    use aletheia_output::OutputScene;

    fn test_scene() -> OutputScene {
        let session = ServiceSessionId::new("test-session").expect("valid session id");
        OutputScene::scripture(
            "scene-john-316",
            session,
            "John 3:16",
            "NIV",
            "For God so loved the world that he gave his one and only Son.",
            "broadcast-lower",
        )
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "aletheia-ew-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ))
    }

    #[test]
    fn plain_text_feed_contains_reference_and_verse() {
        let text = plain_text_feed("John 3:16 NIV", "NIV", "For God so loved...");
        assert!(text.contains("John 3:16 NIV"));
        assert!(text.contains("For God so loved..."));
        assert!(text.starts_with("Aletheia Scripture\n"));
    }

    #[test]
    fn xml_feed_escapes_ampersand_in_reference() {
        let xml = xml_feed("Isaiah 7:14 & 9:6", "KJV", "verse text", DispatchKind::Live);
        assert!(xml.contains("Isaiah 7:14 &amp; 9:6"));
        assert!(!xml.contains("Isaiah 7:14 & 9:6"));
    }

    #[test]
    fn xml_feed_wraps_verse_in_cdata() {
        let xml = xml_feed(
            "John 3:16",
            "NIV",
            "For God so loved <the world>",
            DispatchKind::Preview,
        );
        assert!(xml.contains("<![CDATA[For God so loved <the world>]]>"));
    }

    #[test]
    fn xml_feed_clear_uses_clear_dispatch() {
        let xml = xml_feed("", "", "", DispatchKind::Clear);
        assert!(xml.contains(r#"dispatch="clear""#));
    }

    #[test]
    fn state_json_escapes_special_chars() {
        let json = state_json(
            r#"Psalm 23 "The Lord""#,
            "KJV",
            "line1\nline2",
            DispatchKind::Live,
        );
        assert!(json.contains(r#"Psalm 23 \"The Lord\""#));
        assert!(json.contains(r#"line1\nline2"#));
    }

    #[test]
    fn config_validates_absolute_path() {
        let config = EasyWorshipConfig {
            watch_dir: PathBuf::from("relative/path"),
            ..EasyWorshipConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn default_config_is_valid() {
        assert!(EasyWorshipConfig::default().validate().is_ok());
    }

    #[test]
    fn dispatch_writes_all_three_files() {
        let dir = temp_dir();
        let config = EasyWorshipConfig {
            watch_dir: dir.clone(),
            ..EasyWorshipConfig::default()
        };
        let mut adapter = EasyWorshipAdapter::new(config);
        let scene = test_scene();

        adapter
            .send_live(&scene)
            .expect("live dispatch should succeed");

        assert!(dir.join(FILE_TXT).exists(), "NowPlaying.txt should exist");
        assert!(dir.join(FILE_XML).exists(), "NowPlaying.xml should exist");
        assert!(
            dir.join(FILE_STATE).exists(),
            "AletheiaState.json should exist"
        );

        let txt = fs::read_to_string(dir.join(FILE_TXT)).expect("readable");
        assert!(txt.contains("John 3:16"));

        let xml = fs::read_to_string(dir.join(FILE_XML)).expect("readable");
        assert!(xml.contains(r#"dispatch="live""#));

        let state = fs::read_to_string(dir.join(FILE_STATE)).expect("readable");
        assert!(state.contains(r#""dispatch":"live""#));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_writes_blank_files() {
        let dir = temp_dir();
        let config = EasyWorshipConfig {
            watch_dir: dir.clone(),
            ..EasyWorshipConfig::default()
        };
        let mut adapter = EasyWorshipAdapter::new(config);

        adapter.clear().expect("clear should succeed");

        let xml = fs::read_to_string(dir.join(FILE_XML)).expect("readable");
        assert!(xml.contains(r#"dispatch="clear""#));

        let _ = fs::remove_dir_all(&dir);
    }
}
