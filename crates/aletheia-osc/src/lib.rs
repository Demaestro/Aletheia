//! OSC 1.0 UDP output adapter for Aletheia worship production.
//!
//! Sends scripture candidates to any OSC-capable presentation system including
//! EasyWorship 7.4+, ProPresenter 7 (OSC bridge), Resolume, and StreamDeck/Companion.
//!
//! ## Packet layout
//!
//! An OSC packet consists of:
//! 1. Address pattern: null-terminated ASCII string padded to a 4-byte boundary.
//! 2. Type tag string: `,` followed by argument type characters, null-terminated and padded.
//! 3. Arguments in the order their types appear in the type tag.
//!
//! ## Security model
//!
//! Loopback-first, identical to the vMix adapter. Private LAN addresses are allowed
//! when the operator opts in. Public addresses are rejected.

use std::net::{IpAddr, SocketAddr, UdpSocket};

use aletheia_core::IntegrationId;
use aletheia_output::{
    OutputAdapter, OutputAdapterStatus, OutputCapability, OutputError, OutputHealth, OutputKind,
    OutputLayer, OutputScene,
};

/// Default OSC host for loopback dispatch.
pub const DEFAULT_OSC_HOST: &str = "127.0.0.1";
/// Default EasyWorship 7 OSC port.
pub const DEFAULT_OSC_PORT: u16 = 7000;
/// Aletheia OSC namespace root.
pub const OSC_NAMESPACE: &str = "/aletheia";
/// Maximum UDP payload that fits in one Ethernet frame.
const MAX_PACKET_BYTES: usize = 1472;

/// OSC adapter configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OscConfig {
    pub integration_id: String,
    /// Destination hostname or IP address.
    pub host: String,
    /// Destination UDP port.
    pub port: u16,
    /// OSC address namespace prefix (e.g. `/aletheia`).
    pub namespace: String,
    /// Allow destinations outside loopback. Must be explicitly set for LAN hosts.
    pub allow_private_network: bool,
}

impl Default for OscConfig {
    fn default() -> Self {
        Self {
            integration_id: "osc-main".to_string(),
            host: DEFAULT_OSC_HOST.to_string(),
            port: DEFAULT_OSC_PORT,
            namespace: OSC_NAMESPACE.to_string(),
            allow_private_network: false,
        }
    }
}

impl OscConfig {
    /// Validates static configuration without opening a socket.
    pub fn validate(&self) -> Result<(), OscError> {
        if self.port == 0 {
            return Err(OscError::InvalidConfig(
                "OSC port must be between 1 and 65535".to_string(),
            ));
        }
        if self.namespace.is_empty() || !self.namespace.starts_with('/') {
            return Err(OscError::InvalidConfig(
                "OSC namespace must be a non-empty string starting with '/'".to_string(),
            ));
        }
        Ok(())
    }

    /// Returns `host:port` for display.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// OSC UDP adapter. Sends typed OSC packets over a bound loopback socket.
pub struct OscAdapter {
    config: OscConfig,
}

impl OscAdapter {
    /// Creates an adapter from explicit configuration.
    pub fn new(config: OscConfig) -> Self {
        Self { config }
    }

    /// Returns immutable configuration for diagnostics.
    pub fn config(&self) -> &OscConfig {
        &self.config
    }

    /// Sends an OSC ping (the health-check address `/aletheia/ping` with no arguments).
    /// Returns `Ok(())` if the packet was dispatched without a socket error; because UDP is
    /// connectionless there is no guarantee the remote end received it.
    pub fn ping(&self) -> Result<(), OscError> {
        let address = format!("{}/ping", self.config.namespace);
        self.dispatch_packet(build_packet(&address, &[], &[])?)
    }

    /// Sends `/aletheia/preview reference translation text`.
    fn send_preview_osc(&self, scene: &OutputScene) -> Result<(), OscError> {
        let address = format!("{}/preview", self.config.namespace);
        let reference = layer_text(scene, OutputLayer::Reference);
        let translation = &scene.translation;
        let verse = layer_text(scene, OutputLayer::Verse);
        let args = [
            OscArg::String(reference.to_string()),
            OscArg::String(translation.clone()),
            OscArg::String(verse.to_string()),
        ];
        self.dispatch_packet(build_packet(&address, &args, &[])?)
    }

    /// Sends `/aletheia/live reference translation text`.
    fn send_live_osc(&self, scene: &OutputScene) -> Result<(), OscError> {
        let address = format!("{}/live", self.config.namespace);
        let reference = layer_text(scene, OutputLayer::Reference);
        let translation = &scene.translation;
        let verse = layer_text(scene, OutputLayer::Verse);
        let args = [
            OscArg::String(reference.to_string()),
            OscArg::String(translation.clone()),
            OscArg::String(verse.to_string()),
        ];
        self.dispatch_packet(build_packet(&address, &args, &[])?)
    }

    /// Sends `/aletheia/clear` with no arguments.
    fn send_clear_osc(&self) -> Result<(), OscError> {
        let address = format!("{}/clear", self.config.namespace);
        self.dispatch_packet(build_packet(&address, &[], &[])?)
    }

    /// Opens a bound UDP socket and dispatches a single packet.
    fn dispatch_packet(&self, packet: Vec<u8>) -> Result<(), OscError> {
        self.config.validate()?;

        let dest: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|_| {
                OscError::InvalidConfig(format!(
                    "OSC endpoint '{}:{}' is not a valid socket address",
                    self.config.host, self.config.port
                ))
            })?;

        check_address_policy(&self.config, dest.ip())?;

        // Bind to an ephemeral loopback port for this packet.
        let bind_addr = if dest.ip().is_loopback() {
            "127.0.0.1:0"
        } else {
            "0.0.0.0:0"
        };

        let socket = UdpSocket::bind(bind_addr)
            .map_err(|e| OscError::SocketError(format!("could not bind OSC socket: {e}")))?;

        socket.send_to(&packet, dest).map_err(|e| {
            OscError::DispatchFailed(format!(
                "OSC packet send to {} failed: {e}",
                self.config.endpoint()
            ))
        })?;

        Ok(())
    }
}

impl Default for OscAdapter {
    fn default() -> Self {
        Self::new(OscConfig::default())
    }
}

impl OutputAdapter for OscAdapter {
    fn status(&self) -> OutputAdapterStatus {
        let health = match self.config.validate() {
            Err(e) => OutputHealth::Offline(e.to_string()),
            Ok(()) => match self.ping() {
                Ok(()) => OutputHealth::Connected,
                Err(e) => OutputHealth::Offline(e.to_string()),
            },
        };

        OutputAdapterStatus {
            id: integration_id_or_fallback(&self.config.integration_id),
            display_name: "OSC".to_string(),
            kind: OutputKind::Osc,
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
        self.config
            .validate()
            .map_err(|e| OutputError::DispatchFailed(e.to_string()))
    }

    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.send_preview_osc(scene).map_err(osc_to_output_error)
    }

    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        self.send_live_osc(scene).map_err(osc_to_output_error)
    }

    fn clear(&mut self) -> Result<(), OutputError> {
        self.send_clear_osc().map_err(osc_to_output_error)
    }
}

// ---------------------------------------------------------------------------
// OSC 1.0 packet encoding
// ---------------------------------------------------------------------------

/// Typed OSC argument.
pub enum OscArg {
    /// 32-bit big-endian integer.
    Int(i32),
    /// 32-bit big-endian IEEE 754 float.
    Float(f32),
    /// Null-terminated string, padded to a 4-byte boundary.
    String(String),
    /// Boolean true — encoded in the type tag only, no payload bytes.
    True,
    /// Boolean false — encoded in the type tag only, no payload bytes.
    False,
}

impl OscArg {
    fn type_char(&self) -> u8 {
        match self {
            Self::Int(_) => b'i',
            Self::Float(_) => b'f',
            Self::String(_) => b's',
            Self::True => b'T',
            Self::False => b'F',
        }
    }

    /// Returns the argument's wire bytes. Returns an empty slice for `True`/`False`
    /// since they are encoded only in the type tag.
    fn wire_bytes(&self) -> Vec<u8> {
        match self {
            Self::Int(v) => v.to_be_bytes().to_vec(),
            Self::Float(v) => v.to_be_bytes().to_vec(),
            Self::String(s) => osc_pad_string(s),
            Self::True | Self::False => Vec::new(),
        }
    }
}

/// Builds a complete OSC 1.0 packet.
///
/// `address`  — OSC address pattern, e.g. `/aletheia/preview`
/// `args`     — Typed arguments. Their types are assembled into the type tag automatically.
/// `_bundles` — Reserved for OSC bundle nesting; must be empty for v1.0 packets.
///
/// Returns an error if the address is not a valid OSC address pattern or if the
/// resulting packet would exceed `MAX_PACKET_BYTES`.
pub fn build_packet(address: &str, args: &[OscArg], _bundles: &[()]) -> Result<Vec<u8>, OscError> {
    if address.is_empty() || !address.starts_with('/') {
        return Err(OscError::InvalidConfig(format!(
            "OSC address '{address}' must start with '/'"
        )));
    }

    let mut packet = Vec::with_capacity(128);

    // Address pattern.
    packet.extend(osc_pad_string(address));

    // Type tag string: comma prefix followed by one char per argument.
    let mut tag = vec![b','];
    for arg in args {
        tag.push(arg.type_char());
    }
    packet.extend(osc_pad_bytes(&tag));

    // Argument wire bytes.
    for arg in args {
        packet.extend(arg.wire_bytes());
    }

    if packet.len() > MAX_PACKET_BYTES {
        return Err(OscError::PacketTooLarge {
            size: packet.len(),
            max: MAX_PACKET_BYTES,
        });
    }

    Ok(packet)
}

/// Appends a null terminator then pads to the next 4-byte boundary.
fn osc_pad_string(s: &str) -> Vec<u8> {
    osc_pad_bytes(s.as_bytes())
}

/// Appends a null terminator then pads to the next 4-byte boundary.
fn osc_pad_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.push(0); // null terminator
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

// ---------------------------------------------------------------------------
// Security helpers
// ---------------------------------------------------------------------------

fn check_address_policy(config: &OscConfig, ip: IpAddr) -> Result<(), OscError> {
    if ip.is_loopback() {
        return Ok(());
    }
    if config.allow_private_network && is_private_ip(ip) {
        return Ok(());
    }
    Err(OscError::AddressBlocked(format!(
        "blocked OSC endpoint {ip}. Use loopback by default, or explicitly allow a private production LAN address."
    )))
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || ((v6.segments()[0] & 0xfe00) == 0xfc00),
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// OSC adapter errors with operator-safe wording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OscError {
    InvalidConfig(String),
    AddressBlocked(String),
    SocketError(String),
    DispatchFailed(String),
    PacketTooLarge { size: usize, max: usize },
}

impl std::fmt::Display for OscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(m)
            | Self::AddressBlocked(m)
            | Self::SocketError(m)
            | Self::DispatchFailed(m) => f.write_str(m),
            Self::PacketTooLarge { size, max } => write!(
                f,
                "OSC packet is {size} bytes which exceeds the {max}-byte UDP limit; shorten verse text"
            ),
        }
    }
}

impl std::error::Error for OscError {}

fn osc_to_output_error(e: OscError) -> OutputError {
    match e {
        OscError::InvalidConfig(m) | OscError::AddressBlocked(m) => OutputError::DispatchFailed(m),
        OscError::SocketError(m) | OscError::DispatchFailed(m) => OutputError::NotConnected(m),
        OscError::PacketTooLarge { .. } => OutputError::DispatchFailed(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

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
        .or_else(|_| IntegrationId::new("osc".to_string()))
        .expect("static OSC integration id is valid")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Packet encoding -------------------------------------------------------

    #[test]
    fn osc_pad_string_aligns_to_four_bytes() {
        // "hi" + null + 1 padding = 4 bytes
        assert_eq!(osc_pad_string("hi"), vec![b'h', b'i', 0, 0]);
        // "abc" + null = 4 bytes (already aligned)
        assert_eq!(osc_pad_string("abc"), vec![b'a', b'b', b'c', 0]);
        // "" + null + 3 padding = 4 bytes
        assert_eq!(osc_pad_string(""), vec![0, 0, 0, 0]);
        // 7-char string + null = 8 bytes (already aligned)
        assert_eq!(osc_pad_string("1234567").len(), 8);
    }

    #[test]
    fn build_packet_no_args_has_correct_layout() {
        let packet = build_packet("/test", &[], &[]).expect("valid address");
        // Address: "/test\0\0\0" = 8 bytes
        assert_eq!(&packet[..8], b"/test\0\0\0");
        // Type tag: ",\0\0\0" = 4 bytes
        assert_eq!(&packet[8..12], b",\0\0\0");
        assert_eq!(packet.len(), 12);
    }

    #[test]
    fn build_packet_string_args_encode_correctly() {
        let args = [
            OscArg::String("John 3:16".to_string()),
            OscArg::String("KJV".to_string()),
        ];
        let packet = build_packet("/aletheia/preview", &args, &[]).expect("valid");

        // Address "/aletheia/preview" = 17 bytes + null = 18, padded to 20
        assert_eq!(packet[..20], *b"/aletheia/preview\0\0\0");
        // Type tag: ",ss\0" = 4 bytes
        assert_eq!(&packet[20..24], b",ss\0");
        // "John 3:16" = 9 + null = 10, padded to 12
        assert_eq!(&packet[24..33], b"John 3:16");
        // Total size is divisible by 4
        assert_eq!(packet.len() % 4, 0);
    }

    #[test]
    fn build_packet_int_arg_is_big_endian() {
        let args = [OscArg::Int(256)];
        let packet = build_packet("/x", &args, &[]).expect("valid");
        // /x\0\0 = 4, ,i\0\0 = 4, then 4 bytes for the int
        let int_bytes = &packet[8..12];
        assert_eq!(int_bytes, [0, 0, 1, 0]); // 256 in big-endian
    }

    #[test]
    fn build_packet_bool_types_have_no_payload_bytes() {
        let args = [OscArg::True, OscArg::False];
        let packet = build_packet("/flag", &args, &[]).expect("valid");
        // /flag\0\0\0 = 8, ,TF\0 = 4 bytes — no argument payload at all
        assert_eq!(packet.len(), 12);
        assert_eq!(&packet[8..12], b",TF\0");
    }

    #[test]
    fn build_packet_rejects_bad_address() {
        assert!(build_packet("no-slash", &[], &[]).is_err());
        assert!(build_packet("", &[], &[]).is_err());
    }

    // ---- Config validation -------------------------------------------------------

    #[test]
    fn config_validates_port_zero() {
        let config = OscConfig {
            port: 0,
            ..OscConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_validates_namespace_must_start_with_slash() {
        let config = OscConfig {
            namespace: "aletheia".to_string(),
            ..OscConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn default_config_is_valid() {
        assert!(OscConfig::default().validate().is_ok());
    }

    // ---- Address policy -------------------------------------------------------

    #[test]
    fn loopback_always_allowed() {
        let config = OscConfig::default();
        assert!(check_address_policy(&config, "127.0.0.1".parse().unwrap()).is_ok());
    }

    #[test]
    fn private_lan_blocked_by_default() {
        let config = OscConfig::default();
        assert!(check_address_policy(&config, "192.168.1.100".parse().unwrap()).is_err());
    }

    #[test]
    fn private_lan_allowed_when_opted_in() {
        let config = OscConfig {
            allow_private_network: true,
            ..OscConfig::default()
        };
        assert!(check_address_policy(&config, "192.168.1.100".parse().unwrap()).is_ok());
    }

    #[test]
    fn public_ip_always_blocked() {
        let config = OscConfig {
            allow_private_network: true,
            ..OscConfig::default()
        };
        assert!(check_address_policy(&config, "8.8.8.8".parse().unwrap()).is_err());
    }
}
