//! Fleet bundle ed25519 signing.
//!
//! Signing keypair is stored in the OS keychain under service "Aletheia",
//! label "fleet-signing-key". The private key is a 32-byte seed; we store it
//! hex-encoded so it fits the keyring's string API. If no key exists yet,
//! `sign_fleet_bundle` generates one on first call and persists it — this is
//! exactly the "trust on first use" model operators expect for their own
//! device fleet.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::DesktopState;
use crate::audit::{record_audit_state, record_integration_event_state};
use crate::dto::{FleetVerifyResultDto, SignedFleetBundleDto};
use crate::vault;
use aletheia_core::{AuditAction, now_ms};

const FLEET_KEY_LABEL: &str = "fleet-signing-key";

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
    }
    s
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("hex string has odd length".into());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|e| format!("hex decode failed: {e}"))?;
        out.push(byte);
    }
    Ok(out)
}

fn load_or_create_signing_key() -> Result<SigningKey, String> {
    match vault::read_secret(FLEET_KEY_LABEL) {
        Ok(hex) => {
            let bytes = hex_decode(&hex)?;
            if bytes.len() != 32 {
                return Err("stored fleet key is not 32 bytes".into());
            }
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            Ok(SigningKey::from_bytes(&seed))
        }
        Err(_) => {
            // First-use: generate and persist.
            use rand::rngs::OsRng;
            let mut rng = OsRng;
            let key = SigningKey::generate(&mut rng);
            let seed_hex = hex_encode(&key.to_bytes());
            vault::store_secret(FLEET_KEY_LABEL, &seed_hex)?;
            Ok(key)
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

#[tauri::command]
pub fn sign_fleet_bundle(
    payload_json: String,
    signer_label: String,
    state: State<'_, DesktopState>,
) -> Result<SignedFleetBundleDto, String> {
    if payload_json.trim().is_empty() {
        return Err("payload is empty".into());
    }
    let key = load_or_create_signing_key()?;
    let signature: Signature = key.sign(payload_json.as_bytes());
    let verifying: VerifyingKey = key.verifying_key();

    let digest = sha256_hex(payload_json.as_bytes());
    let result = SignedFleetBundleDto {
        signature_hex: hex_encode(&signature.to_bytes()),
        public_key_hex: hex_encode(&verifying.to_bytes()),
        payload_sha256: digest.clone(),
        payload_json,
        signed_at_ms: now_ms(),
        signer_label: signer_label.clone(),
    };

    let operator = state.operator_name();
    record_audit_state(
        &state,
        AuditAction::PluginInstalled,
        &operator,
        &format!("fleet-bundle signed: sha256={} signer={}", digest, signer_label),
    )?;
    record_integration_event_state(
        &state,
        "fleet-sync",
        "info",
        "bundle.signed",
        &format!("Bundle signed ({} bytes).", result.payload_json.len()),
    )?;
    Ok(result)
}

#[tauri::command]
pub fn verify_fleet_bundle(
    bundle: SignedFleetBundleDto,
    state: State<'_, DesktopState>,
) -> Result<FleetVerifyResultDto, String> {
    let pk_bytes = hex_decode(&bundle.public_key_hex)?;
    if pk_bytes.len() != 32 {
        return Ok(FleetVerifyResultDto {
            valid: false,
            detail: "public key is not 32 bytes".into(),
            public_key_hex: bundle.public_key_hex,
            payload_sha256: bundle.payload_sha256,
        });
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let verifying = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(v) => v,
        Err(e) => {
            return Ok(FleetVerifyResultDto {
                valid: false,
                detail: format!("invalid public key: {e}"),
                public_key_hex: bundle.public_key_hex,
                payload_sha256: bundle.payload_sha256,
            });
        }
    };
    let sig_bytes = hex_decode(&bundle.signature_hex)?;
    if sig_bytes.len() != 64 {
        return Ok(FleetVerifyResultDto {
            valid: false,
            detail: "signature is not 64 bytes".into(),
            public_key_hex: bundle.public_key_hex,
            payload_sha256: bundle.payload_sha256,
        });
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);

    // Recompute the digest and cross-check the declared one.
    let digest = sha256_hex(bundle.payload_json.as_bytes());
    if digest != bundle.payload_sha256 {
        record_integration_event_state(
            &state,
            "fleet-sync",
            "warn",
            "bundle.digest-mismatch",
            "SHA-256 digest does not match declared value.",
        )?;
        return Ok(FleetVerifyResultDto {
            valid: false,
            detail: "payload sha256 does not match declared digest".into(),
            public_key_hex: bundle.public_key_hex,
            payload_sha256: digest,
        });
    }

    match verifying.verify(bundle.payload_json.as_bytes(), &sig) {
        Ok(_) => {
            record_integration_event_state(
                &state,
                "fleet-sync",
                "info",
                "bundle.verified",
                &format!("Bundle signature OK, sha256={}", digest),
            )?;
            Ok(FleetVerifyResultDto {
                valid: true,
                detail: "Signature valid.".into(),
                public_key_hex: bundle.public_key_hex,
                payload_sha256: digest,
            })
        }
        Err(e) => {
            record_integration_event_state(
                &state,
                "fleet-sync",
                "error",
                "bundle.signature-invalid",
                &format!("Signature rejected: {e}"),
            )?;
            Ok(FleetVerifyResultDto {
                valid: false,
                detail: format!("signature rejected: {e}"),
                public_key_hex: bundle.public_key_hex,
                payload_sha256: digest,
            })
        }
    }
}

#[tauri::command]
pub fn get_fleet_public_key() -> Result<String, String> {
    let key = load_or_create_signing_key()?;
    Ok(hex_encode(&key.verifying_key().to_bytes()))
}
