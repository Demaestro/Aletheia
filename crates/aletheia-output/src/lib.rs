//! Presentation output service boundary.

use aletheia_core::{IntegrationId, ServiceSessionId};

/// Output destination family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputKind {
    Hdmi,
    Ndi,
    Obs,
    VMix,
    ProPresenter,
    EasyWorship,
    Osc,
    Companion,
}

/// Capabilities declared by an adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputCapability {
    Preview,
    Live,
    Clear,
    Freeze,
    Blackout,
    AlphaLayer,
    DryRun,
}

/// Output layer identity. NDI uses all three layers; other adapters can flatten.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputLayer {
    Verse,
    Reference,
    ContextCard,
}

/// Rendered layer payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneLayer {
    pub layer: OutputLayer,
    pub text: String,
    pub visible: bool,
}

/// Scene ready for Preview or Live dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputScene {
    pub id: String,
    pub session_id: ServiceSessionId,
    pub reference: String,
    pub translation: String,
    pub theme_id: String,
    pub layers: Vec<SceneLayer>,
}

impl OutputScene {
    /// Creates the standard three-layer scripture scene.
    pub fn scripture(
        id: impl Into<String>,
        session_id: ServiceSessionId,
        reference: impl Into<String>,
        translation: impl Into<String>,
        verse_text: impl Into<String>,
        theme_id: impl Into<String>,
    ) -> Self {
        let reference = reference.into();
        let translation = translation.into();
        Self {
            id: id.into(),
            session_id,
            reference: reference.clone(),
            translation: translation.clone(),
            theme_id: theme_id.into(),
            layers: vec![
                SceneLayer {
                    layer: OutputLayer::Verse,
                    text: verse_text.into(),
                    visible: true,
                },
                SceneLayer {
                    layer: OutputLayer::Reference,
                    text: format!("{reference} {translation}"),
                    visible: true,
                },
                SceneLayer {
                    layer: OutputLayer::ContextCard,
                    text: "Manual live required".to_string(),
                    visible: false,
                },
            ],
        }
    }
}

/// Connection state for an output adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputHealth {
    Ready,
    Connected,
    Degraded(String),
    Offline(String),
}

/// Adapter status shown in the production rail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputAdapterStatus {
    pub id: IntegrationId,
    pub display_name: String,
    pub kind: OutputKind,
    pub capabilities: Vec<OutputCapability>,
    pub health: OutputHealth,
}

/// Production output adapter contract.
pub trait OutputAdapter {
    /// Returns adapter status without changing output.
    fn status(&self) -> OutputAdapterStatus;

    /// Runs a safe dry-run check.
    fn dry_run(&self, scene: &OutputScene) -> Result<(), OutputError>;

    /// Sends a scene to preview.
    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError>;

    /// Sends a scene to live output. Callers must enforce approval policy.
    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError>;

    /// Clears live output.
    fn clear(&mut self) -> Result<(), OutputError>;
}

/// Output errors preserve operator action wording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputError {
    MissingCapability(OutputCapability),
    NotConnected(String),
    DispatchFailed(String),
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCapability(capability) => {
                write!(formatter, "adapter is missing {capability:?} capability")
            }
            Self::NotConnected(message) | Self::DispatchFailed(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for OutputError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripture_scene_contains_ndi_layers() {
        let scene = OutputScene::scripture(
            "scene-1",
            ServiceSessionId::new("sunday-am").expect("valid session id"),
            "1 Samuel 17:45",
            "KJV",
            "Then said David to the Philistine...",
            "film-credit-lower-third",
        );

        assert_eq!(scene.layers.len(), 3);
        assert_eq!(scene.layers[0].layer, OutputLayer::Verse);
        assert_eq!(scene.layers[1].layer, OutputLayer::Reference);
        assert_eq!(scene.layers[2].layer, OutputLayer::ContextCard);
    }
}
