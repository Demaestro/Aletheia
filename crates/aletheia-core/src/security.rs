//! Security primitives shared by local adapters and IPC.

/// Permission scope granted to a local service or plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityScope {
    /// Can observe service state but cannot mutate output.
    ReadServiceState,
    /// Can read transcript segments held locally for detection.
    ReadTranscript,
    /// Can send a scene to preview only.
    WritePreview,
    /// Can send an already-approved scene to live output.
    WriteLiveOutput,
    /// Can manage credentials through the secure vault.
    ManageSecrets,
    /// Can bind a loopback-only automation endpoint.
    BindLocalAutomationEndpoint,
}

/// Secret wrapper that refuses to print the underlying value.
#[derive(Clone, Eq, PartialEq)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    /// Creates a new redacted secret.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Exposes the secret only to trusted Rust-side adapters.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RedactedSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RedactedSecret(***)")
    }
}
