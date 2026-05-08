//! vMix HTTP API adapter.
//!
//! The adapter defaults to loopback-only control of vMix's HTTP API. Remote private-network
//! hosts can be enabled explicitly in configuration, but public hosts are rejected to avoid
//! turning the operator workstation into a generic HTTP control surface.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use aletheia_core::IntegrationId;
use aletheia_output::{
    OutputAdapter, OutputAdapterStatus, OutputCapability, OutputError, OutputHealth, OutputKind,
    OutputLayer, OutputScene,
};

/// vMix Web API defaults for a local operator machine.
pub const DEFAULT_VMIX_HOST: &str = "127.0.0.1";
pub const DEFAULT_VMIX_PORT: u16 = 8088;
pub const DEFAULT_VMIX_TITLE_INPUT: &str = "Aletheia Scripture.gtzip";
pub const DEFAULT_VERSE_FIELD: &str = "Headline.Text";
pub const DEFAULT_REFERENCE_FIELD: &str = "Description.Text";
pub const DEFAULT_OVERLAY_CHANNEL: u8 = 2;
pub const DEFAULT_TIMEOUT_MS: u64 = 2500;

/// Configuration for the vMix HTTP API adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmixConfig {
    pub integration_id: String,
    pub host: String,
    pub port: u16,
    pub title_input: String,
    pub verse_field: String,
    pub reference_field: String,
    pub overlay_channel: u8,
    pub timeout_ms: u64,
    pub allow_private_network: bool,
    /// Optional HTTP Basic username for vMix Web Controller "Enhanced security".
    pub username: Option<String>,
    /// Optional HTTP Basic password matching `username`.
    pub password: Option<String>,
}

impl Default for VmixConfig {
    fn default() -> Self {
        Self {
            integration_id: "vmix-main".to_string(),
            host: DEFAULT_VMIX_HOST.to_string(),
            port: DEFAULT_VMIX_PORT,
            title_input: DEFAULT_VMIX_TITLE_INPUT.to_string(),
            verse_field: DEFAULT_VERSE_FIELD.to_string(),
            reference_field: DEFAULT_REFERENCE_FIELD.to_string(),
            overlay_channel: DEFAULT_OVERLAY_CHANNEL,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            allow_private_network: false,
            username: None,
            password: None,
        }
    }
}

impl VmixConfig {
    /// Returns a display endpoint without credentials or query values.
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}/api/", self.host, self.port)
    }

    /// Validates static adapter settings without opening a network connection.
    pub fn validate(&self) -> Result<(), VmixError> {
        if self.port == 0 {
            return Err(VmixError::InvalidConfig(
                "vMix port must be between 1 and 65535".to_string(),
            ));
        }
        if !(1..=4).contains(&self.overlay_channel) {
            return Err(VmixError::InvalidConfig(
                "vMix overlay channel must be between 1 and 4".to_string(),
            ));
        }
        if self.title_input.trim().is_empty() {
            return Err(VmixError::InvalidConfig(
                "vMix title input is required".to_string(),
            ));
        }
        if self.verse_field.trim().is_empty() || self.reference_field.trim().is_empty() {
            return Err(VmixError::InvalidConfig(
                "vMix title field names are required".to_string(),
            ));
        }
        Ok(())
    }
}

/// Adapter facade used by Tauri and future integration workers.
pub struct VmixAdapter {
    config: VmixConfig,
}

impl VmixAdapter {
    /// Creates a vMix adapter from explicit configuration.
    pub fn new(config: VmixConfig) -> Self {
        Self { config }
    }

    /// Returns immutable configuration for UI status and diagnostics.
    pub fn config(&self) -> &VmixConfig {
        &self.config
    }

    /// Checks the vMix API and whether the configured title input is visible in state XML.
    pub fn check_status(&self) -> Result<VmixConnectionStatus, VmixError> {
        self.config.validate()?;
        let response = self.request_api(&[])?;
        if response.status_code != 200 {
            return Err(VmixError::HttpStatus {
                code: response.status_code,
                detail: "vMix status request did not return HTTP 200".to_string(),
            });
        }

        let title_found = title_input_present(&response.body, &self.config.title_input);
        Ok(VmixConnectionStatus {
            endpoint: self.config.endpoint(),
            title_input: self.config.title_input.clone(),
            overlay_channel: self.config.overlay_channel,
            title_input_found: title_found,
            version: extract_xml_text(&response.body, "version"),
        })
    }

    fn update_title_fields(&self, scene: &OutputScene) -> Result<(), VmixError> {
        self.config.validate()?;
        self.run_function("PauseRender", &[Param::input(&self.config.title_input)])?;
        let update_result = self
            .set_title_text(
                &self.config.verse_field,
                layer_text(scene, OutputLayer::Verse),
            )
            .and_then(|_| {
                self.set_title_text(
                    &self.config.reference_field,
                    layer_text(scene, OutputLayer::Reference),
                )
            });
        let resume_result =
            self.run_function("ResumeRender", &[Param::input(&self.config.title_input)]);
        update_result?;
        resume_result
    }

    fn set_title_text(&self, field_name: &str, value: &str) -> Result<(), VmixError> {
        self.run_function(
            "SetText",
            &[
                Param::input(&self.config.title_input),
                Param::new("SelectedName", field_name),
                Param::new("Value", value),
            ],
        )
    }

    fn run_function(&self, function: &str, params: &[Param<'_>]) -> Result<(), VmixError> {
        let mut owned_params = Vec::with_capacity(params.len() + 1);
        owned_params.push(("Function".to_string(), function.to_string()));
        for param in params {
            owned_params.push((param.key.to_string(), param.value.to_string()));
        }
        let response = self.request_api(&owned_params)?;
        if response.status_code == 200 {
            Ok(())
        } else {
            Err(VmixError::HttpStatus {
                code: response.status_code,
                detail: format!("vMix rejected {function}"),
            })
        }
    }

    fn request_api(&self, params: &[(String, String)]) -> Result<VmixHttpResponse, VmixError> {
        self.config.validate()?;
        let path = api_path(params);
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let stream = connect_checked(&self.config, timeout)?;
        let auth_header = build_basic_auth_header(
            self.config.username.as_deref(),
            self.config.password.as_deref(),
        );
        request_over_stream(
            stream,
            &self.config.host,
            &path,
            timeout,
            auth_header.as_deref(),
        )
    }
}

impl Default for VmixAdapter {
    fn default() -> Self {
        Self::new(VmixConfig::default())
    }
}

impl OutputAdapter for VmixAdapter {
    fn status(&self) -> OutputAdapterStatus {
        let health = match self.check_status() {
            Ok(status) if status.title_input_found => OutputHealth::Connected,
            Ok(status) => OutputHealth::Degraded(format!(
                "API reachable at {}, but title input '{}' was not found",
                status.endpoint, status.title_input
            )),
            Err(error) => OutputHealth::Offline(error.to_string()),
        };

        OutputAdapterStatus {
            id: integration_id_or_fallback(&self.config.integration_id),
            display_name: "vMix".to_string(),
            kind: OutputKind::VMix,
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
        self.check_status().map_err(output_error_from_vmix)?;
        if layer_text(scene, OutputLayer::Verse).trim().is_empty() {
            return Err(OutputError::DispatchFailed(
                "vMix scene has no verse text".to_string(),
            ));
        }
        Ok(())
    }

    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.update_title_fields(scene)
            .map_err(output_error_from_vmix)?;
        self.run_function(
            &format!("PreviewOverlayInput{}", self.config.overlay_channel),
            &[Param::input(&self.config.title_input)],
        )
        .map_err(output_error_from_vmix)
    }

    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.update_title_fields(scene)
            .map_err(output_error_from_vmix)?;
        self.run_function(
            &format!("OverlayInput{}In", self.config.overlay_channel),
            &[Param::input(&self.config.title_input)],
        )
        .map_err(output_error_from_vmix)
    }

    fn clear(&mut self) -> Result<(), OutputError> {
        self.run_function(
            &format!("OverlayInput{}Out", self.config.overlay_channel),
            &[],
        )
        .map_err(output_error_from_vmix)
    }
}

/// Result of a non-mutating vMix status check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmixConnectionStatus {
    pub endpoint: String,
    pub title_input: String,
    pub overlay_channel: u8,
    pub title_input_found: bool,
    pub version: Option<String>,
}

/// vMix adapter errors with operator-safe messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmixError {
    InvalidConfig(String),
    AddressBlocked(String),
    ConnectionFailed(String),
    HttpStatus { code: u16, detail: String },
    MalformedResponse(String),
}

impl std::fmt::Display for VmixError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message)
            | Self::AddressBlocked(message)
            | Self::ConnectionFailed(message)
            | Self::MalformedResponse(message) => formatter.write_str(message),
            Self::HttpStatus { code, detail } => write!(formatter, "{detail} (HTTP {code})"),
        }
    }
}

impl std::error::Error for VmixError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmixHttpResponse {
    pub status_code: u16,
    pub body: String,
}

#[derive(Clone, Copy)]
struct Param<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> Param<'a> {
    fn new(key: &'a str, value: &'a str) -> Self {
        Self { key, value }
    }

    fn input(value: &'a str) -> Self {
        Self::new("Input", value)
    }
}

fn output_error_from_vmix(error: VmixError) -> OutputError {
    match error {
        VmixError::InvalidConfig(message) | VmixError::AddressBlocked(message) => {
            OutputError::DispatchFailed(message)
        }
        VmixError::ConnectionFailed(message) => OutputError::NotConnected(message),
        VmixError::HttpStatus { detail, .. } | VmixError::MalformedResponse(detail) => {
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

fn api_path(params: &[(String, String)]) -> String {
    if params.is_empty() {
        return "/api/".to_string();
    }

    let query = params
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("/api/?{query}")
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(*byte));
            }
            _ => {
                encoded.push('%');
                encoded.push(hex_digit(byte >> 4));
                encoded.push(hex_digit(byte & 0x0f));
            }
        }
    }
    encoded
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'A' + value - 10),
    }
}

fn connect_checked(config: &VmixConfig, timeout: Duration) -> Result<TcpStream, VmixError> {
    let mut last_error = None;
    let addresses = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|error| {
            VmixError::ConnectionFailed(format!("could not resolve vMix endpoint: {error}"))
        })?;

    for address in addresses {
        if !address_allowed(config, address) {
            return Err(VmixError::AddressBlocked(format!(
                "Blocked: {} is a private LAN address. Enable 'Private LAN' in the vMix settings panel to allow connections to production network hosts.",
                address.ip()
            )));
        }

        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    Err(VmixError::ConnectionFailed(format!(
        "vMix is not reachable at {}:{}{}",
        config.host,
        config.port,
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    )))
}

fn address_allowed(config: &VmixConfig, address: SocketAddr) -> bool {
    let ip = address.ip();
    ip.is_loopback() || (config.allow_private_network && is_private_ip(ip))
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(address) => address.is_private() || address.is_link_local(),
        IpAddr::V6(address) => {
            address.is_loopback() || ((address.segments()[0] & 0xfe00) == 0xfc00)
        }
    }
}

fn request_over_stream(
    mut stream: TcpStream,
    host: &str,
    path: &str,
    timeout: Duration,
    auth_header: Option<&str>,
) -> Result<VmixHttpResponse, VmixError> {
    stream.set_read_timeout(Some(timeout)).map_err(|error| {
        VmixError::ConnectionFailed(format!("could not set vMix read timeout: {error}"))
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|error| {
        VmixError::ConnectionFailed(format!("could not set vMix write timeout: {error}"))
    })?;

    let auth_line = match auth_header {
        Some(value) if !value.is_empty() => format!("Authorization: Basic {value}\r\n"),
        _ => String::new(),
    };
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Aletheia-vMix/0.1\r\nConnection: close\r\nAccept: application/xml,text/plain,*/*\r\n{auth_line}\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(|error| {
        VmixError::ConnectionFailed(format!("could not send vMix request: {error}"))
    })?;

    let mut bytes = Vec::new();
    stream
        .take(1_048_576)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            VmixError::ConnectionFailed(format!("could not read vMix response: {error}"))
        })?;
    parse_http_response(&bytes)
}

fn parse_http_response(bytes: &[u8]) -> Result<VmixHttpResponse, VmixError> {
    let response = String::from_utf8_lossy(bytes);
    let mut split = response.splitn(2, "\r\n\r\n");
    let headers = split
        .next()
        .ok_or_else(|| VmixError::MalformedResponse("vMix response had no headers".to_string()))?;
    let body = split.next().unwrap_or_default().to_string();
    let status_line = headers.lines().next().ok_or_else(|| {
        VmixError::MalformedResponse("vMix response had no status line".to_string())
    })?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| {
            VmixError::MalformedResponse("vMix response had no status code".to_string())
        })?
        .parse::<u16>()
        .map_err(|_| {
            VmixError::MalformedResponse("vMix response status code was invalid".to_string())
        })?;

    Ok(VmixHttpResponse { status_code, body })
}

fn extract_xml_text(body: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = body.find(&start_tag)? + start_tag.len();
    let end = body[start..].find(&end_tag)? + start;
    Some(body[start..end].trim().to_string())
}

/// Builds the value for an `Authorization: Basic ...` header, or `None` if no
/// username is configured. vMix's "Enhanced security on Web/TCP API" toggle
/// requires Basic Auth on every off-loopback request.
fn build_basic_auth_header(username: Option<&str>, password: Option<&str>) -> Option<String> {
    let user = username?.trim();
    if user.is_empty() {
        return None;
    }
    let pass = password.unwrap_or("");
    Some(base64_encode(&format!("{user}:{pass}")))
}

fn base64_encode(input: &str) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Looks for the configured title input in vMix state XML. Matches both the
/// raw filename (`Lower Third 5.xaml`) and the human title (`<title>...</title>`),
/// case-insensitively, so operators don't have to type an exact filename.
fn title_input_present(body: &str, configured: &str) -> bool {
    let needle = configured.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    let haystack = body.to_ascii_lowercase();
    if haystack.contains(&needle) {
        return true;
    }
    // Allow operators to enter just the stem ("Lower Third 5") even when vMix
    // reports the full filename ("Lower Third 5.xaml" or ".gtzip").
    let stem = needle
        .trim_end_matches(".gtzip")
        .trim_end_matches(".xaml")
        .trim_end_matches(".gtxml");
    !stem.is_empty() && haystack.contains(stem)
}

fn integration_id_or_fallback(value: &str) -> IntegrationId {
    if let Ok(id) = IntegrationId::new(value.to_string()) {
        return id;
    }
    if let Ok(id) = IntegrationId::new("vmix") {
        return id;
    }
    unreachable!("static vMix integration id is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aletheia_core::ServiceSessionId;

    #[test]
    fn api_path_encodes_vmix_title_values() {
        let path = api_path(&[
            ("Function".to_string(), "SetText".to_string()),
            ("Input".to_string(), "Aletheia Scripture.gtzip".to_string()),
            ("SelectedName".to_string(), "Headline.Text".to_string()),
            (
                "Value".to_string(),
                "Yea, though I walk / through the valley".to_string(),
            ),
        ]);

        assert_eq!(
            path,
            "/api/?Function=SetText&Input=Aletheia%20Scripture.gtzip&SelectedName=Headline.Text&Value=Yea%2C%20though%20I%20walk%20%2F%20through%20the%20valley"
        );
    }

    #[test]
    fn rejects_public_remote_addresses_by_default() {
        let config = VmixConfig {
            host: "8.8.8.8".to_string(),
            ..VmixConfig::default()
        };
        assert!(!address_allowed(
            &config,
            SocketAddr::from(([8, 8, 8, 8], 8088))
        ));
    }

    #[test]
    fn allows_private_remote_only_when_enabled() {
        let mut config = VmixConfig {
            host: "192.168.1.50".to_string(),
            ..VmixConfig::default()
        };
        let address = SocketAddr::from(([192, 168, 1, 50], 8088));
        assert!(!address_allowed(&config, address));
        config.allow_private_network = true;
        assert!(address_allowed(&config, address));
    }

    #[test]
    fn basic_auth_header_skips_empty_username() {
        assert!(build_basic_auth_header(None, Some("pw")).is_none());
        assert!(build_basic_auth_header(Some(""), Some("pw")).is_none());
        assert_eq!(
            build_basic_auth_header(Some("admin"), Some("secret")).as_deref(),
            Some("YWRtaW46c2VjcmV0")
        );
        assert_eq!(
            build_basic_auth_header(Some("admin"), None).as_deref(),
            Some("YWRtaW46")
        );
    }

    #[test]
    fn title_input_match_is_case_and_extension_insensitive() {
        let xml = "<vmix><inputs><input title=\"Lower Third 5\">Lower Third 5.xaml</input></inputs></vmix>";
        assert!(title_input_present(xml, "Lower Third 5.xaml"));
        assert!(title_input_present(xml, "lower third 5"));
        assert!(title_input_present(xml, "Lower Third 5.gtzip"));
        assert!(!title_input_present(xml, "NotPresent"));
    }

    #[test]
    fn scene_layer_text_maps_to_vmix_fields() {
        let session_id = match ServiceSessionId::new("sunday-am") {
            Ok(id) => id,
            Err(error) => panic!("valid session id failed: {error}"),
        };
        let scene = OutputScene::scripture(
            "scene-romans-828",
            session_id,
            "Romans 8:28",
            "KJV",
            "And we know that all things work together for good.",
            "broadcast-lower",
        );

        assert_eq!(
            layer_text(&scene, OutputLayer::Verse),
            "And we know that all things work together for good."
        );
        assert_eq!(
            layer_text(&scene, OutputLayer::Reference),
            "Romans 8:28 KJV"
        );
    }
}
