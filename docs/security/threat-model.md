# Aletheia Production Threat Model

## Assets

- Live worship output and clear/take-live authority.
- Local scripture database, service plan, transcript segments, and audit log.
- Integration configuration for vMix, OBS, EasyWorship, ProPresenter, NDI, OSC, and Companion.
- Optional cloud credentials, STT provider keys, and update signing material.
- Operator workstation availability during live service.

## Trust Boundaries

- React WebView to Rust/Tauri command boundary.
- Rust desktop shell to local SQLite database.
- Adapter boundary from Aletheia to vMix/OBS/ProPresenter/EasyWorship/NDI/OSC.
- Optional cloud enhancement boundary.
- Plugin install/update boundary.

## Attacker Capabilities

- Malicious network peer on a church LAN.
- Compromised plugin or unsigned adapter package.
- Accidental operator action during live service.
- Corrupt or stale local database after power loss.
- Dependency compromise or vulnerable webview/runtime package.

## Required Controls

- Live output requires explicit destination arming and operator action.
- Local HTTP integrations default to loopback. Private LAN targets require explicit admin enablement.
- Public IP integration targets are blocked unless a future signed plugin grants a scoped transport capability.
- Integration config stores non-secret data only. Secret values must use secure storage and appear in logs as redacted references.
- Every preview/live/clear/config-change action writes an audit event. Delivery receipts record adapter-level outcomes.
- Support bundles must redact provider keys, local usernames, absolute home paths, and transcript text unless the operator opts in.
- Updates must be signed, rollback-capable, and testable offline.

## Current Implementation

- Tauri command policy blocks live output when destinations are not armed.
- vMix control is loopback-first and blocks public remote addresses by default.
- vMix configuration persists in SQLite `integration_configs`; delivery receipts persist in `integration_events`.
- Local audit log chains live-output and configuration events with SHA-256 hashes.

## Open Security Work

- Add secure storage for future secrets.
- Add signed plugin manifests and plugin permission scopes.
- Add support bundle generation with automated redaction tests.
- Add signed installer/update pipeline with rollback validation.
- Add dependency policy gates for RustSec, npm audit, and Tauri/WebKit advisories.
