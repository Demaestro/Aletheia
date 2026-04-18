# Plugin Signing Policy

Aletheia plugins must be treated as production software, not convenience scripts.

## Required Manifest Controls

- Manifest payload is canonical JSON.
- Signature algorithm is Ed25519.
- Plugin key id must be trusted by the local release policy.
- Entrypoint must be relative to the signed plugin package.
- Capabilities must be explicit and unique.
- Wildcard network hosts are rejected.
- A plugin crash degrades only that destination.

## Capability Model

Start with narrow capabilities:

- `read-service-state`
- `read-transcript`
- `write-preview`
- `write-live-output`
- `manage-secrets`
- `bind-local-automation-endpoint`

Production builds should require admin approval for `write-live-output`, `manage-secrets`, and non-loopback network targets.

## Release Key Handling

Private signing keys never ship with the app. They belong in the release environment. The desktop app should ship only trusted public key ids and public verification keys.

The current Rust verifier lives in `crates/aletheia-ops` and verifies Ed25519 signatures over canonical manifest payloads.
