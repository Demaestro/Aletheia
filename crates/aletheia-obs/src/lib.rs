//! OBS WebSocket v5 output adapter for Aletheia worship production.
//!
//! Implements the OBS WebSocket protocol (version 5.x, as shipped with OBS 28+)
//! over a raw `TcpStream`. No async runtime or third-party WebSocket library is
//! required — RFC 6455 framing and the OBS authentication handshake are implemented
//! from first principles.
//!
//! ## Protocol summary (OBS WebSocket 5.x)
//!
//! 1. TCP connect to `host:port` (default `127.0.0.1:4455`).
//! 2. Send HTTP/1.1 WebSocket upgrade request.
//! 3. Receive `101 Switching Protocols`.
//! 4. Receive JSON `Hello` frame (op 0) with authentication challenge.
//! 5. Respond with JSON `Identify` frame (op 1); include auth string if password set.
//! 6. Receive `Identified` frame (op 2) confirming the negotiated RPC version.
//! 7. Send `Request` frames (op 6) and receive `RequestResponse` frames (op 7).
//! 8. Close with a WebSocket close frame (op 8).
//!
//! ## Authentication
//!
//! ```text
//! secret     = base64(sha256(password ++ salt))
//! auth       = base64(sha256(secret ++ challenge))
//! ```
//!
//! ## Aletheia output model
//!
//! - `send_preview`: Writes the verse text to the configured GDI+ or Text (FreeType 2)
//!   source, then makes it visible in the *Preview* program slot via
//!   `SetCurrentPreviewScene`.
//! - `send_live`:    Same text update, then `SetCurrentProgramScene` to take it live.
//! - `clear`:        Hides the scripture source via `SetSceneItemEnabled(visible=false)`.
//!
//! ## Security model
//!
//! Loopback-first; identical to vMix. Private-LAN hosts require an explicit opt-in.

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use base64::Engine as _;
use sha2::{Digest, Sha256};

use aletheia_core::IntegrationId;
use aletheia_output::{
    OutputAdapter, OutputAdapterStatus, OutputCapability, OutputError, OutputHealth, OutputKind,
    OutputLayer, OutputScene,
};

// ---------------------------------------------------------------------------
// Public defaults
// ---------------------------------------------------------------------------

pub const DEFAULT_OBS_HOST: &str = "127.0.0.1";
pub const DEFAULT_OBS_PORT: u16 = 4455;
pub const DEFAULT_OBS_SCENE: &str = "Scripture Lower Third";
pub const DEFAULT_OBS_SOURCE: &str = "Aletheia Scripture";
pub const DEFAULT_TIMEOUT_MS: u64 = 2_000;
/// OBS WebSocket protocol version this adapter targets.
pub const OBS_RPC_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// OBS WebSocket adapter configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObsConfig {
    pub integration_id: String,
    /// OBS WebSocket host (default `127.0.0.1`).
    pub host: String,
    /// OBS WebSocket port (default `4455`).
    pub port: u16,
    /// Optional password. Leave empty if OBS has no authentication configured.
    pub password: String,
    /// OBS scene that contains the scripture text source.
    pub scene_name: String,
    /// Name of the text (GDI+ or FreeType 2) source inside `scene_name`.
    pub source_name: String,
    /// Connect + read timeout.
    pub timeout_ms: u64,
    /// Allow connections to private-LAN OBS hosts.
    pub allow_private_network: bool,
}

impl Default for ObsConfig {
    fn default() -> Self {
        Self {
            integration_id: "obs-main".to_string(),
            host: DEFAULT_OBS_HOST.to_string(),
            port: DEFAULT_OBS_PORT,
            password: String::new(),
            scene_name: DEFAULT_OBS_SCENE.to_string(),
            source_name: DEFAULT_OBS_SOURCE.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            allow_private_network: false,
        }
    }
}

impl ObsConfig {
    /// Validates static configuration without opening a connection.
    pub fn validate(&self) -> Result<(), ObsError> {
        if self.port == 0 {
            return Err(ObsError::InvalidConfig(
                "OBS WebSocket port must be between 1 and 65535".to_string(),
            ));
        }
        if self.scene_name.trim().is_empty() {
            return Err(ObsError::InvalidConfig(
                "OBS scene name is required".to_string(),
            ));
        }
        if self.source_name.trim().is_empty() {
            return Err(ObsError::InvalidConfig(
                "OBS scripture source name is required".to_string(),
            ));
        }
        Ok(())
    }

    pub fn endpoint(&self) -> String {
        format!("ws://{}:{}", self.host, self.port)
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// OBS WebSocket v5 adapter. Each command opens a fresh TCP connection, performs
/// the full auth handshake, sends the request, then closes. This avoids persistent
/// connection management and is appropriate for the low command rate of a worship
/// service.
pub struct ObsAdapter {
    config: ObsConfig,
}

impl ObsAdapter {
    pub fn new(config: ObsConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ObsConfig {
        &self.config
    }

    /// Connects, authenticates, and returns a ready session.
    fn open_session(&self) -> Result<ObsSession, ObsError> {
        self.config.validate()?;
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let stream = connect_checked(&self.config, timeout)?;
        let mut session = ObsSession::new(stream, timeout);
        session.handshake(&self.config.password)?;
        Ok(session)
    }

    /// Sends a `GetVersion` request to confirm the connection is alive.
    pub fn check_status(&self) -> Result<ObsVersionInfo, ObsError> {
        let mut session = self.open_session()?;
        let response = session.request("GetVersion", serde_json::Value::Null)?;
        let obs_version = response["obsVersion"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let ws_version = response["obsWebSocketVersion"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(ObsVersionInfo {
            obs_version,
            ws_version,
        })
    }

    /// Sends the scripture text to the configured GDI+ source.
    fn set_text_source(&self, session: &mut ObsSession, text: &str) -> Result<(), ObsError> {
        let data = serde_json::json!({
            "inputName": self.config.source_name,
            "inputSettings": { "text": text }
        });
        session.request("SetInputSettings", data)?;
        Ok(())
    }

    /// Returns the scene item ID for `source_name` inside `scene_name`.
    fn get_scene_item_id(&self, session: &mut ObsSession) -> Result<i64, ObsError> {
        let data = serde_json::json!({
            "sceneName": self.config.scene_name,
            "sourceName": self.config.source_name
        });
        let response = session.request("GetSceneItemId", data)?;
        response["sceneItemId"]
            .as_i64()
            .ok_or_else(|| ObsError::Protocol("GetSceneItemId: sceneItemId missing".to_string()))
    }

    /// Shows or hides the scripture scene item.
    fn set_scene_item_enabled(
        &self,
        session: &mut ObsSession,
        item_id: i64,
        enabled: bool,
    ) -> Result<(), ObsError> {
        let data = serde_json::json!({
            "sceneName": self.config.scene_name,
            "sceneItemId": item_id,
            "sceneItemEnabled": enabled
        });
        session.request("SetSceneItemEnabled", data)?;
        Ok(())
    }
}

impl Default for ObsAdapter {
    fn default() -> Self {
        Self::new(ObsConfig::default())
    }
}

impl OutputAdapter for ObsAdapter {
    fn status(&self) -> OutputAdapterStatus {
        let health = match self.check_status() {
            Ok(info) => OutputHealth::Degraded(format!(
                "OBS {} / ws {} reachable at {}",
                info.obs_version,
                info.ws_version,
                self.config.endpoint()
            )),
            Err(ObsError::InvalidConfig(m)) => OutputHealth::Offline(m),
            Err(e) => OutputHealth::Offline(e.to_string()),
        };
        // Downgrade Degraded → Connected when OBS is actually reachable.
        let health = if matches!(health, OutputHealth::Degraded(_)) {
            OutputHealth::Connected
        } else {
            health
        };

        OutputAdapterStatus {
            id: integration_id_or_fallback(&self.config.integration_id),
            display_name: "OBS".to_string(),
            kind: OutputKind::Obs,
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
        self.check_status().map(|_| ()).map_err(obs_to_output_error)
    }

    fn send_preview(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        let verse = layer_text(scene, OutputLayer::Verse);
        let reference = layer_text(scene, OutputLayer::Reference);
        let text = format_overlay_text(verse, reference);

        let mut session = self.open_session().map_err(obs_to_output_error)?;
        self.set_text_source(&mut session, &text)
            .map_err(obs_to_output_error)?;
        let item_id = self
            .get_scene_item_id(&mut session)
            .map_err(obs_to_output_error)?;
        self.set_scene_item_enabled(&mut session, item_id, true)
            .map_err(obs_to_output_error)?;

        // Show in OBS preview slot without changing the program.
        let data = serde_json::json!({ "sceneName": self.config.scene_name });
        session
            .request("SetCurrentPreviewScene", data)
            .map_err(obs_to_output_error)?;

        Ok(())
    }

    fn send_live(&mut self, scene: &OutputScene) -> Result<(), OutputError> {
        let verse = layer_text(scene, OutputLayer::Verse);
        let reference = layer_text(scene, OutputLayer::Reference);
        let text = format_overlay_text(verse, reference);

        let mut session = self.open_session().map_err(obs_to_output_error)?;
        self.set_text_source(&mut session, &text)
            .map_err(obs_to_output_error)?;
        let item_id = self
            .get_scene_item_id(&mut session)
            .map_err(obs_to_output_error)?;
        self.set_scene_item_enabled(&mut session, item_id, true)
            .map_err(obs_to_output_error)?;

        // Take to program.
        let data = serde_json::json!({ "sceneName": self.config.scene_name });
        session
            .request("SetCurrentProgramScene", data)
            .map_err(obs_to_output_error)?;

        Ok(())
    }

    fn clear(&mut self) -> Result<(), OutputError> {
        let mut session = self.open_session().map_err(obs_to_output_error)?;
        let item_id = self
            .get_scene_item_id(&mut session)
            .map_err(obs_to_output_error)?;
        self.set_scene_item_enabled(&mut session, item_id, false)
            .map_err(obs_to_output_error)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// OBS WebSocket session (single-use)
// ---------------------------------------------------------------------------

/// A single-use authenticated OBS WebSocket session.
struct ObsSession {
    stream: TcpStream,
    request_id: u32,
    timeout: Duration,
}

impl ObsSession {
    fn new(stream: TcpStream, timeout: Duration) -> Self {
        Self {
            stream,
            request_id: 0,
            timeout,
        }
    }

    /// Executes the full WebSocket + OBS auth handshake.
    fn handshake(&mut self, password: &str) -> Result<(), ObsError> {
        ws_upgrade(&mut self.stream, &self.timeout)?;

        // Receive Hello (op 0).
        let hello: serde_json::Value = self.recv_json()?;
        if hello.get("op").and_then(|v| v.as_u64()) != Some(0) {
            return Err(ObsError::Protocol(format!(
                "expected Hello (op 0), got: {hello}"
            )));
        }

        // Build Identify payload (op 1).
        let mut identify_data = serde_json::json!({ "rpcVersion": OBS_RPC_VERSION });
        if let Some(auth_data) = hello["d"]["authentication"].as_object() {
            let challenge = auth_data
                .get("challenge")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ObsError::Protocol("Hello missing authentication.challenge".to_string())
                })?;
            let salt = auth_data
                .get("salt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ObsError::Protocol("Hello missing authentication.salt".to_string())
                })?;
            let auth_string = compute_auth(password, salt, challenge);
            identify_data["authentication"] = serde_json::Value::String(auth_string);
        }

        self.send_json(&serde_json::json!({ "op": 1, "d": identify_data }))?;

        // Receive Identified (op 2).
        let identified: serde_json::Value = self.recv_json()?;
        if identified.get("op").and_then(|v| v.as_u64()) != Some(2) {
            return Err(ObsError::Authentication(format!(
                "OBS rejected Identify; check password. Response: {identified}"
            )));
        }

        Ok(())
    }

    /// Sends an OBS `Request` (op 6) and returns the `responseData` from the
    /// corresponding `RequestResponse` (op 7).
    fn request(
        &mut self,
        request_type: &str,
        request_data: serde_json::Value,
    ) -> Result<serde_json::Value, ObsError> {
        self.request_id += 1;
        let rid = self.request_id.to_string();

        let mut payload = serde_json::json!({
            "op": 6,
            "d": {
                "requestType": request_type,
                "requestId": rid
            }
        });
        if !request_data.is_null() {
            payload["d"]["requestData"] = request_data;
        }

        self.send_json(&payload)?;

        // Read frames until we get our response (in practice the next frame).
        for _ in 0..8 {
            let frame: serde_json::Value = self.recv_json()?;
            if frame.get("op").and_then(|v| v.as_u64()) == Some(7) {
                let d = &frame["d"];
                if d["requestId"].as_str() == Some(&rid) {
                    let status = &d["requestStatus"];
                    let result = status["result"].as_bool().unwrap_or(false);
                    if !result {
                        let code = status["code"].as_u64().unwrap_or(0);
                        let comment = status["comment"].as_str().unwrap_or("").to_string();
                        return Err(ObsError::RequestFailed {
                            request_type: request_type.to_string(),
                            code,
                            comment,
                        });
                    }
                    return Ok(d["responseData"].clone());
                }
            }
        }

        Err(ObsError::Protocol(format!(
            "no response received for {request_type} (request id {rid})"
        )))
    }

    fn send_json(&mut self, value: &serde_json::Value) -> Result<(), ObsError> {
        let json = serde_json::to_string(value)
            .map_err(|e| ObsError::Protocol(format!("JSON serialization failed: {e}")))?;
        let frame = ws_encode_text(json.as_bytes());
        self.stream
            .write_all(&frame)
            .map_err(|e| ObsError::Connection(format!("OBS WebSocket write failed: {e}")))?;
        Ok(())
    }

    fn recv_json(&mut self) -> Result<serde_json::Value, ObsError> {
        let payload = ws_read_frame(&mut self.stream, self.timeout)?;
        serde_json::from_slice(&payload)
            .map_err(|e| ObsError::Protocol(format!("OBS JSON parse error: {e}")))
    }
}

// ---------------------------------------------------------------------------
// RFC 6455 WebSocket helpers
// ---------------------------------------------------------------------------

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Sends the HTTP/1.1 WebSocket upgrade request and validates the 101 response.
fn ws_upgrade(stream: &mut TcpStream, timeout: &Duration) -> Result<(), ObsError> {
    // Generate a 16-byte nonce per RFC 6455 §4.1.
    let nonce = ws_nonce();
    let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(nonce);

    // Extract host from the stream's peer address for the Host header.
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "localhost".to_string());

    let request = format!(
        "GET / HTTP/1.1\r\n\
         Host: {peer}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {nonce_b64}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         User-Agent: Aletheia-OBS/0.1\r\n\
         \r\n"
    );

    stream
        .set_write_timeout(Some(*timeout))
        .map_err(|e| ObsError::Connection(format!("OBS write timeout: {e}")))?;
    stream
        .set_read_timeout(Some(*timeout))
        .map_err(|e| ObsError::Connection(format!("OBS read timeout: {e}")))?;

    stream
        .write_all(request.as_bytes())
        .map_err(|e| ObsError::Connection(format!("OBS upgrade request failed: {e}")))?;

    // Read response headers (up to 4 KiB).
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| ObsError::Connection(format!("OBS upgrade response read failed: {e}")))?;
    let response = String::from_utf8_lossy(&buf[..n]);

    if !response.starts_with("HTTP/1.1 101") {
        return Err(ObsError::Connection(format!(
            "OBS WebSocket upgrade rejected: {}",
            response.lines().next().unwrap_or("empty response")
        )));
    }

    // Validate Sec-WebSocket-Accept.
    let expected_accept = ws_accept_key(&nonce_b64);
    let accept_header = response
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("sec-websocket-accept:"))
        .and_then(|l| l.split_once(':').map(|x| x.1))
        .map(|v| v.trim().to_string())
        .unwrap_or_default();

    if accept_header != expected_accept {
        return Err(ObsError::Connection(format!(
            "OBS WebSocket accept key mismatch (got '{accept_header}', expected '{expected_accept}')"
        )));
    }

    Ok(())
}

/// Computes the expected `Sec-WebSocket-Accept` header value.
fn ws_accept_key(nonce_b64: &str) -> String {
    // WebSocket uses SHA-1, not SHA-256, for the accept key.
    // We must implement SHA-1 or use a minimal approach here.
    // Since sha1 is not a dep, we use a lightweight implementation.
    let concatenated = format!("{nonce_b64}{WS_GUID}");
    // Fallback: SHA-1 via the 20-byte digest computed manually.
    base64::engine::general_purpose::STANDARD.encode(sha1_digest(concatenated.as_bytes()))
}

/// Encodes a text WebSocket frame with client masking (RFC 6455 §5.3).
fn ws_encode_text(payload: &[u8]) -> Vec<u8> {
    let mask = masking_key();
    let masked: Vec<u8> = payload
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ mask[i % 4])
        .collect();

    let mut frame = Vec::with_capacity(10 + masked.len());
    // FIN=1, opcode=1 (text).
    frame.push(0x81);

    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | len as u8); // MASK=1
    } else if len < 65536 {
        frame.push(0x80 | 126);
        frame.push((len >> 8) as u8);
        frame.push(len as u8);
    } else {
        frame.push(0x80 | 127);
        for i in (0..8).rev() {
            frame.push(((len >> (i * 8)) & 0xFF) as u8);
        }
    }

    frame.extend_from_slice(&mask);
    frame.extend_from_slice(&masked);
    frame
}

/// Reads exactly one WebSocket frame from the stream and returns the payload.
/// Handles fragmented frames (FIN=0) by reassembling them.
fn ws_read_frame(stream: &mut TcpStream, timeout: Duration) -> Result<Vec<u8>, ObsError> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| ObsError::Connection(format!("read timeout set failed: {e}")))?;

    let mut payload = Vec::new();

    loop {
        // Read first two header bytes.
        let mut header = [0u8; 2];
        stream
            .read_exact(&mut header)
            .map_err(|e| ObsError::Connection(format!("OBS WebSocket header read failed: {e}")))?;

        let fin = (header[0] & 0x80) != 0;
        let opcode = header[0] & 0x0F;
        let masked = (header[1] & 0x80) != 0;
        let len_byte = (header[1] & 0x7F) as usize;

        // Handle close frames.
        if opcode == 8 {
            return Err(ObsError::Connection(
                "OBS closed the WebSocket connection".to_string(),
            ));
        }

        let payload_len = if len_byte < 126 {
            len_byte
        } else if len_byte == 126 {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).map_err(|e| {
                ObsError::Connection(format!("OBS extended length read failed: {e}"))
            })?;
            u16::from_be_bytes(ext) as usize
        } else {
            let mut ext = [0u8; 8];
            stream.read_exact(&mut ext).map_err(|e| {
                ObsError::Connection(format!("OBS extended length read failed: {e}"))
            })?;
            u64::from_be_bytes(ext) as usize
        };

        let mask_bytes: Option<[u8; 4]> = if masked {
            let mut m = [0u8; 4];
            stream
                .read_exact(&mut m)
                .map_err(|e| ObsError::Connection(format!("OBS mask read failed: {e}")))?;
            Some(m)
        } else {
            None
        };

        let mut fragment = vec![0u8; payload_len];
        stream
            .read_exact(&mut fragment)
            .map_err(|e| ObsError::Connection(format!("OBS payload read failed: {e}")))?;

        if let Some(mask) = mask_bytes {
            for (i, byte) in fragment.iter_mut().enumerate() {
                *byte ^= mask[i % 4];
            }
        }

        payload.extend_from_slice(&fragment);

        if fin {
            return Ok(payload);
        }
    }
}

// ---------------------------------------------------------------------------
// OBS authentication
// ---------------------------------------------------------------------------

/// Computes the OBS WebSocket authentication string:
/// `base64(sha256(base64(sha256(password + salt)) + challenge))`
fn compute_auth(password: &str, salt: &str, challenge: &str) -> String {
    let mut h1 = Sha256::new();
    h1.update(password.as_bytes());
    h1.update(salt.as_bytes());
    let secret = base64::engine::general_purpose::STANDARD.encode(h1.finalize());

    let mut h2 = Sha256::new();
    h2.update(secret.as_bytes());
    h2.update(challenge.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(h2.finalize())
}

// ---------------------------------------------------------------------------
// SHA-1 (needed for WebSocket Sec-WebSocket-Accept, RFC 6455 §1.3)
// ---------------------------------------------------------------------------

/// Minimal SHA-1 implementation. Only used for the WebSocket handshake key
/// validation. Not used for any security-critical purpose.
fn sha1_digest(input: &[u8]) -> [u8; 20] {
    // SHA-1 constants.
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    // Pre-processing: adding padding bits.
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut msg = input.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit block.
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, chunk) in block.chunks(4).enumerate().take(16) {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = h;
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999_u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1_u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC_u32),
                _ => (b ^ c ^ d, 0xCA62C1D6_u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut digest = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        digest[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// Generates a 16-byte WebSocket nonce using the system nanosecond clock.
/// This is not cryptographically strong but is sufficient for the WebSocket
/// nonce (which is only an obfuscation requirement per RFC 6455 §10.3).
fn ws_nonce() -> [u8; 16] {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    // XOR-spread across 16 bytes for uniform distribution.
    let mut nonce = [0u8; 16];
    for (i, byte) in nonce.iter_mut().enumerate() {
        *byte = ((ns >> (i % 4 * 8)) ^ (ns >> ((i / 4) * 8 + 4))) as u8;
    }
    nonce
}

/// Returns a 4-byte masking key derived from the nanosecond clock.
fn masking_key() -> [u8; 4] {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    [
        (ns >> 24) as u8,
        (ns >> 16) as u8,
        (ns >> 8) as u8,
        ns as u8,
    ]
}

// ---------------------------------------------------------------------------
// Network helpers
// ---------------------------------------------------------------------------

fn connect_checked(config: &ObsConfig, timeout: Duration) -> Result<TcpStream, ObsError> {
    let addresses = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|e| ObsError::Connection(format!("could not resolve OBS endpoint: {e}")))?;

    let mut last_error = None;
    for address in addresses {
        check_address_policy(config, address.ip())?;
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e.to_string()),
        }
    }

    Err(ObsError::Connection(format!(
        "OBS WebSocket is not reachable at {}:{}{}",
        config.host,
        config.port,
        last_error.map(|e| format!(": {e}")).unwrap_or_default()
    )))
}

fn check_address_policy(config: &ObsConfig, ip: IpAddr) -> Result<(), ObsError> {
    if ip.is_loopback() {
        return Ok(());
    }
    if config.allow_private_network && is_private_ip(ip) {
        return Ok(());
    }
    Err(ObsError::InvalidConfig(format!(
        "blocked OBS endpoint {ip}. Use loopback, or enable allow_private_network for a production LAN host."
    )))
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || ((v6.segments()[0] & 0xfe00) == 0xfc00),
    }
}

fn integration_id_or_fallback(value: &str) -> IntegrationId {
    IntegrationId::new(value.to_string())
        .or_else(|_| IntegrationId::new("obs".to_string()))
        .expect("static OBS integration id is valid")
}

fn layer_text(scene: &OutputScene, layer: OutputLayer) -> &str {
    scene
        .layers
        .iter()
        .find(|l| l.layer == layer && l.visible)
        .map(|l| l.text.as_str())
        .unwrap_or("")
}

fn format_overlay_text(verse: &str, reference: &str) -> String {
    if reference.is_empty() {
        verse.to_string()
    } else {
        format!("{verse}\n— {reference}")
    }
}

fn obs_to_output_error(e: ObsError) -> OutputError {
    match e {
        ObsError::InvalidConfig(m) => OutputError::DispatchFailed(m),
        ObsError::Connection(m) | ObsError::Authentication(m) => OutputError::NotConnected(m),
        ObsError::Protocol(m) => OutputError::DispatchFailed(m),
        ObsError::RequestFailed {
            request_type,
            code,
            comment,
        } => OutputError::DispatchFailed(format!(
            "OBS {request_type} failed (code {code}): {comment}"
        )),
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of a successful `GetVersion` request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObsVersionInfo {
    pub obs_version: String,
    pub ws_version: String,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// OBS adapter errors with operator-safe wording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObsError {
    InvalidConfig(String),
    Connection(String),
    Authentication(String),
    Protocol(String),
    RequestFailed {
        request_type: String,
        code: u64,
        comment: String,
    },
}

impl std::fmt::Display for ObsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(m)
            | Self::Connection(m)
            | Self::Authentication(m)
            | Self::Protocol(m) => f.write_str(m),
            Self::RequestFailed {
                request_type,
                code,
                comment,
            } => write!(f, "OBS {request_type} failed (code {code}): {comment}"),
        }
    }
}

impl std::error::Error for ObsError {}

// ---------------------------------------------------------------------------
// Tests (unit tests that do not require a live OBS instance)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- SHA-1 ---------------------------------------------------------------

    #[test]
    fn sha1_empty_string_matches_known_digest() {
        // SHA-1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        let digest = sha1_digest(b"");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[test]
    fn sha1_abc_matches_known_digest() {
        // SHA-1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
        let digest = sha1_digest(b"abc");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn sha1_longer_input() {
        // SHA-1("The quick brown fox jumps over the lazy dog")
        // = 2fd4e1c67a2d28fced849ee1bb76e7391b93eb12
        let digest = sha1_digest(b"The quick brown fox jumps over the lazy dog");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
    }

    // ---- WebSocket accept key ------------------------------------------------

    #[test]
    fn ws_accept_key_matches_rfc_example() {
        // RFC 6455 §1.3 example:
        // Key:    dGhlIHNhbXBsZSBub25jZQ==
        // Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
        let accept = ws_accept_key("dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    // ---- WebSocket frame encoding -------------------------------------------

    #[test]
    fn ws_encode_text_small_payload_has_correct_header() {
        let frame = ws_encode_text(b"hi");
        // Byte 0: FIN=1, opcode=1 → 0x81
        assert_eq!(frame[0], 0x81);
        // Byte 1: MASK=1, payload_len=2 → 0x82
        assert_eq!(frame[1], 0x82);
        // Total: 2 header + 4 mask + 2 payload = 8 bytes
        assert_eq!(frame.len(), 8);
    }

    #[test]
    fn ws_encode_text_payload_is_correctly_masked() {
        let payload = b"hello";
        let frame = ws_encode_text(payload);
        // frame[2..6] is the masking key; frame[6..11] is the masked payload.
        let mask = &frame[2..6];
        for (i, byte) in frame[6..].iter().enumerate() {
            assert_eq!(*byte ^ mask[i % 4], payload[i]);
        }
    }

    #[test]
    fn ws_encode_text_126_byte_payload_uses_extended_length() {
        let payload = vec![b'x'; 126];
        let frame = ws_encode_text(&payload);
        // Byte 1: MASK=1, len=126 → 0xFE
        assert_eq!(frame[1], 0xFE);
        // Bytes 2-3: 16-bit big-endian length = 126
        assert_eq!(u16::from_be_bytes([frame[2], frame[3]]), 126);
    }

    // ---- Authentication -----------------------------------------------------

    #[test]
    fn compute_auth_matches_obs_spec_example() {
        // From the OBS WebSocket 5.x specification README:
        // password  = "supersecretpassword"
        // salt      = "PZVbYpvAnZut2SS6JNJytDm9"
        // challenge = "ztTBnnuqrqaKDzRM3xcVdbYm"
        // expected  = "zZgWipvwSGrw748kHN4gNpBC1IaeiiWX3Hjkrm849Sc="
        let auth = compute_auth(
            "supersecretpassword",
            "PZVbYpvAnZut2SS6JNJytDm9",
            "ztTBnnuqrqaKDzRM3xcVdbYm",
        );
        assert_eq!(auth, "zZgWipvwSGrw748kHN4gNpBC1IaeiiWX3Hjkrm849Sc=");
    }

    // ---- Config validation --------------------------------------------------

    #[test]
    fn default_config_is_valid() {
        assert!(ObsConfig::default().validate().is_ok());
    }

    #[test]
    fn config_rejects_port_zero() {
        let config = ObsConfig {
            port: 0,
            ..ObsConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_rejects_empty_scene_name() {
        let config = ObsConfig {
            scene_name: "  ".to_string(),
            ..ObsConfig::default()
        };
        assert!(config.validate().is_err());
    }

    // ---- Address policy -----------------------------------------------------

    #[test]
    fn loopback_is_always_allowed() {
        let config = ObsConfig::default();
        assert!(check_address_policy(&config, "127.0.0.1".parse().unwrap()).is_ok());
    }

    #[test]
    fn lan_blocked_by_default() {
        let config = ObsConfig::default();
        assert!(check_address_policy(&config, "192.168.1.50".parse().unwrap()).is_err());
    }

    #[test]
    fn lan_allowed_when_opted_in() {
        let config = ObsConfig {
            allow_private_network: true,
            ..ObsConfig::default()
        };
        assert!(check_address_policy(&config, "192.168.1.50".parse().unwrap()).is_ok());
    }

    // ---- Overlay text formatting --------------------------------------------

    #[test]
    fn format_overlay_text_includes_em_dash_prefix() {
        let text = format_overlay_text("For God so loved...", "John 3:16 NIV");
        assert_eq!(text, "For God so loved...\n— John 3:16 NIV");
    }

    #[test]
    fn format_overlay_text_no_reference_is_verse_only() {
        let text = format_overlay_text("The Lord is my shepherd.", "");
        assert_eq!(text, "The Lord is my shepherd.");
    }
}
