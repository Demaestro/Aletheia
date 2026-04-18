//! ProPresenter 7+ REST API output adapter.
//!
//! ProPresenter exposes a REST/SSE control surface on TCP `1025` (default).
//! The endpoints we care about for worship scripture display are:
//!
//! | Endpoint                                            | Purpose                          |
//! |-----------------------------------------------------|----------------------------------|
//! | `GET  /version`                                     | Liveness + version probe         |
//! | `GET  /v1/presentations`                            | Discover the slate to drive      |
//! | `PUT  /v1/messages/{id}`                            | Update an on-screen message      |
//! | `POST /v1/messages/{id}/trigger`                    | Show the message live            |
//! | `POST /v1/messages/{id}/clear`                      | Hide the message                 |
//! | `POST /v1/clear/layer/messages`                     | Hard-clear all messages          |
//!
//! Aletheia drives ProPresenter via the **Messages** subsystem rather than
//! editing presentation slides — this avoids touching the operator's planned
//! sermon order. The operator pre-creates a `Scripture` message in
//! ProPresenter Messages with two tokens:
//!
//! ```text
//! Token name: Verse        (multi-line)
//! Token name: Reference    (single-line)
//! ```
//!
//! Aletheia rewrites those tokens on every Preview/Live cycle.
//!
//! ## Security model
//! Loopback by default. Private-LAN hosts allowed only via explicit opt-in,
//! identical to the vMix and OBS adapters.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use aletheia_core::IntegrationId;
use aletheia_output::{
    OutputAdapter, OutputAdapterStatus, OutputCapability, OutputError, OutputHealth, OutputKind,
    OutputLayer, OutputScene,
};

// ---------------------------------------------------------------------------
// Public defaults
// ---------------------------------------------------------------------------

pub const DEFAULT_PROPRESENTER_HOST: &str = "127.0.0.1";
pub const DEFAULT_PROPRESENTER_PORT: u16 = 1025;
pub const DEFAULT_MESSAGE_NAME: &str = "Scripture";
pub const DEFAULT_VERSE_TOKEN: &str = "Verse";
pub const DEFAULT_REFERENCE_TOKEN: &str = "Reference";
pub const DEFAULT_TIMEOUT_MS: u64 = 1_500;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// ProPresenter REST adapter configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProPresenterConfig {
    pub integration_id: String,
    pub host: String,
    pub port: u16,
    /// Name of the pre-created Message in ProPresenter (Messages tab).
    pub message_name: String,
    /// Token name for the verse text token inside the message.
    pub verse_token: String,
    /// Token name for the reference text token inside the message.
    pub reference_token: String,
    pub timeout_ms: u64,
    pub allow_private_network: bool,
}

impl Default for ProPresenterConfig {
    fn default() -> Self {
        Self {
            integration_id: "propresenter-main".to_string(),
            host: DEFAULT_PROPRESENTER_HOST.to_string(),
            port: DEFAULT_PROPRESENTER_PORT,
            message_name: DEFAULT_MESSAGE_NAME.to_string(),
            verse_token: DEFAULT_VERSE_TOKEN.to_string(),
            reference_token: DEFAULT_REFERENCE_TOKEN.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            allow_private_network: false,
        }
    }
}

impl ProPresenterConfig {
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}/", self.host, self.port)
    }

    pub fn validate(&self) -> Result<(), ProPresenterError> {
        if self.port == 0 {
            return Err(ProPresenterError::InvalidConfig(
                "ProPresenter port must be between 1 and 65535".to_string(),
            ));
        }
        if self.message_name.trim().is_empty() {
            return Err(ProPresenterError::InvalidConfig(
                "ProPresenter message name is required".to_string(),
            ));
        }
        if self.verse_token.trim().is_empty() || self.reference_token.trim().is_empty() {
            return Err(ProPresenterError::InvalidConfig(
                "ProPresenter token names are required".to_string(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct ProPresenterAdapter {
    config: ProPresenterConfig,
}

impl ProPresenterAdapter {
    pub fn new(config: ProPresenterConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ProPresenterConfig {
        &self.config
    }

    /// Calls `GET /version` to verify the API is reachable and report version info.
    pub fn check_status(&self) -> Result<ProPresenterVersionInfo, ProPresenterError> {
        self.config.validate()?;
        let response = self.request("GET", "/version", None)?;
        if response.status_code != 200 {
            return Err(ProPresenterError::HttpStatus {
                code: response.status_code,
                detail: "ProPresenter /version did not return HTTP 200".to_string(),
            });
        }

        // Body is JSON: { "name": "ProPresenter", "platform": "macOS", ... }
        let name = extract_json_string(&response.body, "name").unwrap_or_else(|| "ProPresenter".to_string());
        let host_description = extract_json_string(&response.body, "host_description")
            .or_else(|| extract_json_string(&response.body, "platform"))
            .unwrap_or_else(|| "unknown".to_string());

        Ok(ProPresenterVersionInfo {
            endpoint: self.config.endpoint(),
            name,
            host_description,
        })
    }

    /// Updates the configured message tokens with new verse/reference text.
    fn update_message(&self, scene: &OutputScene) -> Result<(), ProPresenterError> {
        self.config.validate()?;
        let verse = layer_text(scene, OutputLayer::Verse);
        let reference = layer_text(scene, OutputLayer::Reference);

        let payload = serde_json::json!([
            {
                "name": &self.config.verse_token,
                "text": { "text": verse }
            },
            {
                "name": &self.config.reference_token,
                "text": { "text": reference }
            }
        ]);
        let body = serde_json::to_string(&payload).map_err(|e| ProPresenterError::InvalidConfig(format!("Failed to serialize message payload: {e}")))?;

        let path = format!(
            "/v1/messages/{}",
            url_encode_path_segment(&self.config.message_name)
        );
        let response = self.request("PUT", &path, Some(&body))?;
        if !(200..300).contains(&response.status_code) {
            return Err(ProPresenterError::HttpStatus {
                code: response.status_code,
                detail: format!("ProPresenter rejected message update for {}", self.config.message_name),
            });
        }
        Ok(())
    }

    fn trigger_message(&self) -> Result<(), ProPresenterError> {
        let path = format!(
            "/v1/messages/{}/trigger",
            url_encode_path_segment(&self.config.message_name)
        );
        let response = self.request("POST", &path, Some("[]"))?;
        if !(200..300).contains(&response.status_code) {
            return Err(ProPresenterError::HttpStatus {
                code: response.status_code,
                detail: "ProPresenter rejected message trigger".to_string(),
            });
        }
        Ok(())
    }

    fn clear_message(&self) -> Result<(), ProPresenterError> {
        let path = format!(
            "/v1/messages/{}/clear",
            url_encode_path_segment(&self.config.message_name)
        );
        let response = self.request("POST", &path, None)?;
        if !(200..300).contains(&response.status_code) {
            return Err(ProPresenterError::HttpStatus {
                code: response.status_code,
                detail: "ProPresenter rejected message clear".to_string(),
            });
        }
        Ok(())
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<ProPresenterHttpResponse, ProPresenterError> {
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let stream = connect_checked(&self.config, timeout)?;
        request_over_stream(stream, &self.config.host, method, path, body, timeout)
    }
}

impl Default for ProPresenterAdapter {
    fn default() -> Self {
        Self::new(ProPresenterConfig::default())
    }
}

impl OutputAdapter for ProPresenterAdapter {
    fn status(&self) -> OutputAdapterStatus {
        let health = match self.check_status() {
            Ok(_) => OutputHealth::Connected,
            Err(ProPresenterError::InvalidConfig(m)) => OutputHealth::Offline(m),
            Err(e) => OutputHealth::Offline(e.to_string()),
        };

        OutputAdapterStatus {
            id: integration_id_or_fallback(&self.config.integration_id),
            display_name: "ProPresenter".to_string(),
            kind: OutputKind::ProPresenter,
            capabilities: vec![
                OutputCapability::Preview,
                OutputCapability::Live,
                OutputCapability::Clear,
                OutputCapability::DryRun,
            ],
            health,
        }
    }

    fn dry_run(&self, scene: &OutputScene) -> Result<(), OutputError> {
        self.check_status().map_err(output_error_from)?;
        if layer_text(scene, OutputLayer::Verse).trim().is_empty() {
            return Err(OutputError::DispatchFailed(
                "ProPresenter scene has no verse text".to_string(),
            ));
        }
        Ok(())
    }

    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        // ProPresenter has no separate preview bus — staging the message
        // (without triggering) is the operational equivalent.
        self.update_message(scene).map_err(output_error_from)
    }

    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.update_message(scene).map_err(output_error_from)?;
        self.trigger_message().map_err(output_error_from)
    }

    fn clear(&mut self) -> Result<(), OutputError> {
        self.clear_message().map_err(output_error_from)
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProPresenterVersionInfo {
    pub endpoint: String,
    pub name: String,
    pub host_description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProPresenterError {
    InvalidConfig(String),
    AddressBlocked(String),
    ConnectionFailed(String),
    HttpStatus { code: u16, detail: String },
    MalformedResponse(String),
}

impl std::fmt::Display for ProPresenterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(m)
            | Self::AddressBlocked(m)
            | Self::ConnectionFailed(m)
            | Self::MalformedResponse(m) => f.write_str(m),
            Self::HttpStatus { code, detail } => write!(f, "{detail} (HTTP {code})"),
        }
    }
}

impl std::error::Error for ProPresenterError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProPresenterHttpResponse {
    pub status_code: u16,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

fn output_error_from(error: ProPresenterError) -> OutputError {
    match error {
        ProPresenterError::InvalidConfig(m) | ProPresenterError::AddressBlocked(m) => {
            OutputError::DispatchFailed(m)
        }
        ProPresenterError::ConnectionFailed(m) => OutputError::NotConnected(m),
        ProPresenterError::HttpStatus { detail, .. }
        | ProPresenterError::MalformedResponse(detail) => OutputError::DispatchFailed(detail),
    }
}

fn layer_text(scene: &OutputScene, output_layer: OutputLayer) -> &str {
    scene
        .layers
        .iter()
        .find(|layer| layer.layer == output_layer && layer.visible)
        .map(|layer| layer.text.as_str())
        .unwrap_or("")
}

fn connect_checked(
    config: &ProPresenterConfig,
    timeout: Duration,
) -> Result<TcpStream, ProPresenterError> {
    let mut last_error = None;
    let addresses = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|e| {
            ProPresenterError::ConnectionFailed(format!(
                "could not resolve ProPresenter endpoint: {e}"
            ))
        })?;

    for address in addresses {
        if !address_allowed(config, address) {
            return Err(ProPresenterError::AddressBlocked(format!(
                "blocked ProPresenter endpoint {}. Use loopback by default, or explicitly allow a private production LAN address.",
                address.ip()
            )));
        }
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e.to_string()),
        }
    }
    Err(ProPresenterError::ConnectionFailed(format!(
        "ProPresenter is not reachable at {}:{}{}",
        config.host,
        config.port,
        last_error.map(|e| format!(": {e}")).unwrap_or_default()
    )))
}

fn address_allowed(config: &ProPresenterConfig, address: SocketAddr) -> bool {
    let ip = address.ip();
    ip.is_loopback() || (config.allow_private_network && is_private_ip(ip))
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => a.is_private() || a.is_link_local(),
        IpAddr::V6(a) => a.is_loopback() || ((a.segments()[0] & 0xfe00) == 0xfc00),
    }
}

fn request_over_stream(
    mut stream: TcpStream,
    host: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<ProPresenterHttpResponse, ProPresenterError> {
    stream.set_read_timeout(Some(timeout)).map_err(|e| {
        ProPresenterError::ConnectionFailed(format!("could not set read timeout: {e}"))
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|e| {
        ProPresenterError::ConnectionFailed(format!("could not set write timeout: {e}"))
    })?;

    let body_str = body.unwrap_or("");
    let mut request = String::new();
    request.push_str(method);
    request.push(' ');
    request.push_str(path);
    request.push_str(" HTTP/1.1\r\n");
    request.push_str(&format!("Host: {host}\r\n"));
    request.push_str("User-Agent: Aletheia-ProPresenter/0.1\r\n");
    request.push_str("Accept: application/json\r\n");
    request.push_str("Connection: close\r\n");
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
        request.push_str(&format!("Content-Length: {}\r\n", body_str.len()));
    }
    request.push_str("\r\n");
    request.push_str(body_str);

    stream.write_all(request.as_bytes()).map_err(|e| {
        ProPresenterError::ConnectionFailed(format!("could not send ProPresenter request: {e}"))
    })?;

    let mut bytes = Vec::new();
    stream
        .take(1_048_576)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            ProPresenterError::ConnectionFailed(format!(
                "could not read ProPresenter response: {e}"
            ))
        })?;
    parse_http_response(&bytes)
}

fn parse_http_response(bytes: &[u8]) -> Result<ProPresenterHttpResponse, ProPresenterError> {
    let response = String::from_utf8_lossy(bytes);
    let mut split = response.splitn(2, "\r\n\r\n");
    let headers = split.next().ok_or_else(|| {
        ProPresenterError::MalformedResponse("ProPresenter response had no headers".to_string())
    })?;
    let body = split.next().unwrap_or_default().to_string();
    let status_line = headers.lines().next().ok_or_else(|| {
        ProPresenterError::MalformedResponse("ProPresenter response had no status line".to_string())
    })?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| {
            ProPresenterError::MalformedResponse(
                "ProPresenter response had no status code".to_string(),
            )
        })?
        .parse::<u16>()
        .map_err(|_| {
            ProPresenterError::MalformedResponse(
                "ProPresenter response status code was invalid".to_string(),
            )
        })?;

    Ok(ProPresenterHttpResponse { status_code, body })
}

fn url_encode_path_segment(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(*byte));
            }
            _ => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'A' + value - 10),
    }
}


/// Tiny JSON helper: pull a top-level string field by key. Avoids dragging in a
/// JSON parser for the one thing /version returns.
fn extract_json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let colon = rest.find(':')?;
    let after = &rest[colon + 1..];
    let quote = after.find('"')?;
    let value_start = quote + 1;
    let value_end = after[value_start..].find('"')? + value_start;
    Some(after[value_start..value_end].to_string())
}

fn integration_id_or_fallback(value: &str) -> IntegrationId {
    if let Ok(id) = IntegrationId::new(value.to_string()) {
        return id;
    }
    if let Ok(id) = IntegrationId::new("propresenter") {
        return id;
    }
    unreachable!("static ProPresenter integration id is valid")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aletheia_core::ServiceSessionId;

    #[test]
    fn rejects_public_remote_addresses_by_default() {
        let config = ProPresenterConfig {
            host: "8.8.8.8".to_string(),
            ..ProPresenterConfig::default()
        };
        assert!(!address_allowed(
            &config,
            SocketAddr::from(([8, 8, 8, 8], 1025))
        ));
    }


    #[test]
    fn url_encode_keeps_unreserved_and_percent_encodes_spaces() {
        assert_eq!(url_encode_path_segment("Scripture"), "Scripture");
        assert_eq!(url_encode_path_segment("Sunday Verse"), "Sunday%20Verse");
    }

    #[test]
    fn extract_json_string_pulls_top_level_field() {
        let body =
            r#"{"name":"ProPresenter","host_description":"Mac mini","platform":"macOS"}"#;
        assert_eq!(
            extract_json_string(body, "name"),
            Some("ProPresenter".to_string())
        );
        assert_eq!(
            extract_json_string(body, "host_description"),
            Some("Mac mini".to_string())
        );
    }

    #[test]
    fn validate_rejects_empty_token_names() {
        let config = ProPresenterConfig {
            verse_token: "  ".to_string(),
            ..ProPresenterConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ProPresenterError::InvalidConfig(_))
        ));
    }

    #[test]
    fn dry_run_requires_verse_text() {
        let session = ServiceSessionId::new("sunday-am").expect("valid session");
        let scene = OutputScene::scripture("s1", session, "Romans 8:28", "KJV", "", "default");
        let adapter = ProPresenterAdapter::default();
        assert!(adapter.dry_run(&scene).is_err());
    }
}
