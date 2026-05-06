//! Bitfocus Companion HTTP control output adapter.
//!
//! Companion exposes an HTTP control surface (default port `8000`) that lets
//! external systems press buttons, change button styles, and run custom
//! variables. Aletheia drives Companion to push scripture verse text into a
//! pre-configured button, then "press" it so any downstream automation
//! (lighting cues, lower-third triggers, video switcher macros) wired up in
//! Companion fires alongside the verse going live in the operator's other
//! presentation tool.
//!
//! Endpoints used:
//!
//! | Endpoint                                              | Purpose                          |
//! |-------------------------------------------------------|----------------------------------|
//! | `GET  /api/location/{page}/{row}/{column}/press`      | Trigger button press             |
//! | `POST /api/location/{page}/{row}/{column}/style`      | Update button text/colors        |
//! | `POST /api/custom-variable/{name}/value`              | Push verse text into a variable  |
//!
//! ## Security model
//! Loopback by default, identical to vMix / OBS / ProPresenter adapters. A
//! production LAN host is allowed only with `allow_private_network = true`.

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

pub const DEFAULT_COMPANION_HOST: &str = "127.0.0.1";
pub const DEFAULT_COMPANION_PORT: u16 = 8000;
pub const DEFAULT_PAGE: u16 = 1;
pub const DEFAULT_ROW: u16 = 0;
pub const DEFAULT_COLUMN: u16 = 0;
pub const DEFAULT_VERSE_VARIABLE: &str = "aletheia_verse";
pub const DEFAULT_REFERENCE_VARIABLE: &str = "aletheia_reference";
pub const DEFAULT_TIMEOUT_MS: u64 = 1_500;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Companion HTTP adapter configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionConfig {
    pub integration_id: String,
    pub host: String,
    pub port: u16,
    /// Companion button location for trigger.
    pub page: u16,
    pub row: u16,
    pub column: u16,
    /// Custom variable name receiving verse text.
    pub verse_variable: String,
    /// Custom variable name receiving reference text.
    pub reference_variable: String,
    pub timeout_ms: u64,
    pub allow_private_network: bool,
}

impl Default for CompanionConfig {
    fn default() -> Self {
        Self {
            integration_id: "companion-main".to_string(),
            host: DEFAULT_COMPANION_HOST.to_string(),
            port: DEFAULT_COMPANION_PORT,
            page: DEFAULT_PAGE,
            row: DEFAULT_ROW,
            column: DEFAULT_COLUMN,
            verse_variable: DEFAULT_VERSE_VARIABLE.to_string(),
            reference_variable: DEFAULT_REFERENCE_VARIABLE.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            allow_private_network: false,
        }
    }
}

impl CompanionConfig {
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}/", self.host, self.port)
    }

    pub fn validate(&self) -> Result<(), CompanionError> {
        if self.port == 0 {
            return Err(CompanionError::InvalidConfig(
                "Companion port must be between 1 and 65535".to_string(),
            ));
        }
        if self.verse_variable.trim().is_empty() || self.reference_variable.trim().is_empty() {
            return Err(CompanionError::InvalidConfig(
                "Companion custom variable names are required".to_string(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct CompanionAdapter {
    config: CompanionConfig,
}

impl CompanionAdapter {
    pub fn new(config: CompanionConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &CompanionConfig {
        &self.config
    }

    /// Probes Companion liveness with a benign GET against the variables list.
    pub fn check_status(&self) -> Result<CompanionVersionInfo, CompanionError> {
        self.config.validate()?;
        // Companion's HTTP control API does not expose a /version endpoint, but
        // GET /api/custom-variable/{name}/value returns a 200 (or 404) without
        // side effects and is sufficient as a liveness probe.
        // Probing internal/current_time (which usually exists)
        // prevents a valid 404 on an unconfigured custom variable from failing health checks.
        let path = "/api/variables/internal/current_time";
        let response = self.request("GET", path, None)?;
        // 404 still proves the HTTP server is alive; treat 4xx other than 401/403
        // as "reachable but the variable is not configured yet".
        if response.status_code == 401 || response.status_code == 403 {
            return Err(CompanionError::HttpStatus {
                code: response.status_code,
                detail: "Companion HTTP API rejected request — check authentication".to_string(),
            });
        }
        if response.status_code >= 500 {
            return Err(CompanionError::HttpStatus {
                code: response.status_code,
                detail: "Companion HTTP API returned a server error".to_string(),
            });
        }

        Ok(CompanionVersionInfo {
            endpoint: self.config.endpoint(),
            status_code: response.status_code,
        })
    }

    fn set_variable(&self, name: &str, value: &str) -> Result<(), CompanionError> {
        // Companion 3.x: POST /api/custom-variable/{name}/value with the raw
        // text body (Content-Type: text/plain).
        let path = format!(
            "/api/custom-variable/{}/value",
            url_encode_path_segment(name)
        );
        let response = self.request_with_content_type("POST", &path, Some(value), "text/plain")?;
        if !(200..300).contains(&response.status_code) {
            return Err(CompanionError::HttpStatus {
                code: response.status_code,
                detail: format!("Companion rejected variable update for {name}"),
            });
        }
        Ok(())
    }

    fn press_button(&self) -> Result<(), CompanionError> {
        let path = format!(
            "/api/location/{}/{}/{}/press",
            self.config.page, self.config.row, self.config.column
        );
        let response = self.request("GET", &path, None)?;
        if !(200..300).contains(&response.status_code) {
            return Err(CompanionError::HttpStatus {
                code: response.status_code,
                detail: format!(
                    "Companion rejected press for page {}, row {}, col {}",
                    self.config.page, self.config.row, self.config.column
                ),
            });
        }
        Ok(())
    }

    fn update_scene(&self, scene: &OutputScene) -> Result<(), CompanionError> {
        self.config.validate()?;
        let verse = layer_text(scene, OutputLayer::Verse);
        let reference = layer_text(scene, OutputLayer::Reference);
        self.set_variable(&self.config.verse_variable, verse)?;
        self.set_variable(&self.config.reference_variable, reference)?;
        Ok(())
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<CompanionHttpResponse, CompanionError> {
        self.request_with_content_type(method, path, body, "application/json")
    }

    fn request_with_content_type(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        content_type: &str,
    ) -> Result<CompanionHttpResponse, CompanionError> {
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let stream = connect_checked(&self.config, timeout)?;
        request_over_stream(
            stream,
            &self.config.host,
            method,
            path,
            body,
            content_type,
            timeout,
        )
    }
}

impl Default for CompanionAdapter {
    fn default() -> Self {
        Self::new(CompanionConfig::default())
    }
}

impl OutputAdapter for CompanionAdapter {
    fn status(&self) -> OutputAdapterStatus {
        let health = match self.check_status() {
            Ok(_) => OutputHealth::Connected,
            Err(CompanionError::InvalidConfig(m)) => OutputHealth::Offline(m),
            Err(e) => OutputHealth::Offline(e.to_string()),
        };

        OutputAdapterStatus {
            id: integration_id_or_fallback(&self.config.integration_id),
            display_name: "Bitfocus Companion".to_string(),
            kind: OutputKind::Companion,
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
                "Companion scene has no verse text".to_string(),
            ));
        }
        Ok(())
    }

    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        // Pushing variables without pressing the button is the staging step.
        self.update_scene(scene).map_err(output_error_from)
    }

    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.update_scene(scene).map_err(output_error_from)?;
        self.press_button().map_err(output_error_from)
    }

    fn clear(&mut self) -> Result<(), OutputError> {
        // Clear by emptying both variables. Companion does not have a "release
        // button" semantic for simple presses; users should wire a second button
        // for clear if they want a hard hide.
        self.set_variable(&self.config.verse_variable, "")
            .map_err(output_error_from)?;
        self.set_variable(&self.config.reference_variable, "")
            .map_err(output_error_from)
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionVersionInfo {
    pub endpoint: String,
    pub status_code: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompanionError {
    InvalidConfig(String),
    AddressBlocked(String),
    ConnectionFailed(String),
    HttpStatus { code: u16, detail: String },
    MalformedResponse(String),
}

impl std::fmt::Display for CompanionError {
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

impl std::error::Error for CompanionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionHttpResponse {
    pub status_code: u16,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

fn output_error_from(error: CompanionError) -> OutputError {
    match error {
        CompanionError::InvalidConfig(m) | CompanionError::AddressBlocked(m) => {
            OutputError::DispatchFailed(m)
        }
        CompanionError::ConnectionFailed(m) => OutputError::NotConnected(m),
        CompanionError::HttpStatus { detail, .. } | CompanionError::MalformedResponse(detail) => {
            OutputError::DispatchFailed(detail)
        }
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
    config: &CompanionConfig,
    timeout: Duration,
) -> Result<TcpStream, CompanionError> {
    let mut last_error = None;
    let addresses = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|e| {
            CompanionError::ConnectionFailed(format!("could not resolve Companion endpoint: {e}"))
        })?;

    for address in addresses {
        if !address_allowed(config, address) {
            return Err(CompanionError::AddressBlocked(format!(
                "blocked Companion endpoint {}. Use loopback by default, or explicitly allow a private production LAN address.",
                address.ip()
            )));
        }
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e.to_string()),
        }
    }
    Err(CompanionError::ConnectionFailed(format!(
        "Companion is not reachable at {}:{}{}",
        config.host,
        config.port,
        last_error.map(|e| format!(": {e}")).unwrap_or_default()
    )))
}

fn address_allowed(config: &CompanionConfig, address: SocketAddr) -> bool {
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
    content_type: &str,
    timeout: Duration,
) -> Result<CompanionHttpResponse, CompanionError> {
    stream.set_read_timeout(Some(timeout)).map_err(|e| {
        CompanionError::ConnectionFailed(format!("could not set read timeout: {e}"))
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|e| {
        CompanionError::ConnectionFailed(format!("could not set write timeout: {e}"))
    })?;

    let body_str = body.unwrap_or("");
    let mut request = String::new();
    request.push_str(method);
    request.push(' ');
    request.push_str(path);
    request.push_str(" HTTP/1.1\r\n");
    request.push_str(&format!("Host: {host}\r\n"));
    request.push_str("User-Agent: Aletheia-Companion/0.1\r\n");
    request.push_str("Accept: */*\r\n");
    request.push_str("Connection: close\r\n");
    if body.is_some() {
        request.push_str(&format!("Content-Type: {content_type}\r\n"));
        request.push_str(&format!("Content-Length: {}\r\n", body_str.len()));
    }
    request.push_str("\r\n");
    request.push_str(body_str);

    stream.write_all(request.as_bytes()).map_err(|e| {
        CompanionError::ConnectionFailed(format!("could not send Companion request: {e}"))
    })?;

    let mut bytes = Vec::new();
    stream
        .take(1_048_576)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            CompanionError::ConnectionFailed(format!("could not read Companion response: {e}"))
        })?;
    parse_http_response(&bytes)
}

fn parse_http_response(bytes: &[u8]) -> Result<CompanionHttpResponse, CompanionError> {
    let response = String::from_utf8_lossy(bytes);
    let mut split = response.splitn(2, "\r\n\r\n");
    let headers = split.next().ok_or_else(|| {
        CompanionError::MalformedResponse("Companion response had no headers".to_string())
    })?;
    let body = split.next().unwrap_or_default().to_string();
    let status_line = headers.lines().next().ok_or_else(|| {
        CompanionError::MalformedResponse("Companion response had no status line".to_string())
    })?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| {
            CompanionError::MalformedResponse("Companion response had no status code".to_string())
        })?
        .parse::<u16>()
        .map_err(|_| {
            CompanionError::MalformedResponse(
                "Companion response status code was invalid".to_string(),
            )
        })?;

    Ok(CompanionHttpResponse { status_code, body })
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

fn integration_id_or_fallback(value: &str) -> IntegrationId {
    if let Ok(id) = IntegrationId::new(value.to_string()) {
        return id;
    }
    if let Ok(id) = IntegrationId::new("companion") {
        return id;
    }
    unreachable!("static Companion integration id is valid")
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
        let config = CompanionConfig {
            host: "8.8.8.8".to_string(),
            ..CompanionConfig::default()
        };
        assert!(!address_allowed(
            &config,
            SocketAddr::from(([8, 8, 8, 8], 8000))
        ));
    }

    #[test]
    fn allows_loopback_by_default() {
        let config = CompanionConfig::default();
        assert!(address_allowed(
            &config,
            SocketAddr::from(([127, 0, 0, 1], 8000))
        ));
    }

    #[test]
    fn validate_rejects_empty_variable_names() {
        let config = CompanionConfig {
            verse_variable: "  ".to_string(),
            ..CompanionConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(CompanionError::InvalidConfig(_))
        ));
    }

    #[test]
    fn dry_run_requires_verse_text() {
        let session = ServiceSessionId::new("sunday-am").expect("valid session");
        let scene = OutputScene::scripture("s1", session, "Romans 8:28", "KJV", "", "default");
        let adapter = CompanionAdapter::default();
        assert!(adapter.dry_run(&scene).is_err());
    }

    #[test]
    fn url_encode_keeps_unreserved() {
        assert_eq!(url_encode_path_segment("aletheia_verse"), "aletheia_verse");
        assert_eq!(url_encode_path_segment("my var"), "my%20var");
    }
}
