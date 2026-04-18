# Aletheia Rust Service Boundaries

This is the first executable backend slice for Aletheia. It compiles in this WSL environment without system package installs by using a user-local Rust toolchain, a user-local Zig linker, and bundled SQLite.

## Workspace Map

- `crates/aletheia-core`: shared IDs, security scopes, health reports, audit actions, and append-only domain events.
- `crates/aletheia-vad`: hybrid VAD skeleton with adaptive RMS fallback and future Silero model input.
- `crates/aletheia-audio-ingest`: audio ingest lifecycle, frame processing, degraded/recovery states, and event emission.
- `crates/aletheia-detection`: transcript, evidence, confidence bucket, detection tier, and detector trait contracts.
- `crates/aletheia-output`: output scene model, NDI-style layer contract, adapter status, and output adapter trait.
- `crates/aletheia-store`: local-first SQLite schema, FTS5 scripture search, append-only event storage, sync outbox, integration config, and audit log foundation.
- `apps/aletheia-audio-service`: deterministic bootstrap harness that simulates a pulpit mic frame, detects speech, runs scripture detection, and builds an output scene.

## Event Flow

1. `AudioIngestService::start` emits `AudioCaptureStarting` and `AudioCaptureStarted`.
2. `AudioIngestService::process_pcm_frame` measures RMS, peak, zero-crossing rate, and duration.
3. `HybridVad::analyze` returns `VadDecision` using Silero confidence when available or adaptive RMS fallback.
4. Audio ingest emits `AudioFrameMeasured` and `VadDecisionMade`.
5. `ScriptureDetector::detect` turns transcript segments into `ScriptureCandidate` values with evidence and confidence buckets.
6. `OutputScene::scripture` creates three layers: Verse, Reference, and ContextCard.
7. Future adapters implement `OutputAdapter` for HDMI, NDI, OBS, vMix, ProPresenter, EasyWorship, OSC, and Companion.

## Security Boundary

- The frontend must never call device, filesystem, shell, network, or credential APIs directly.
- Rust services emit events and accept typed commands only.
- `RedactedSecret` refuses debug printing.
- `CapabilityScope` models future Tauri window/plugin permissions.
- Live output remains a Rust-side policy decision; auto-live is disabled by default in `ConfidencePolicy`.

## Local-First Notes

- This slice has no cloud dependency.
- The VAD fallback works without model files.
- The detector has a deterministic keyword reference path before semantic reranking.
- SQLite stores local scripture translations, aliases, transcript segments, candidate approvals, presentation scenes, integration settings, domain events, audit rows, and sync outbox records.
- FTS5 powers offline phrase search for the manual fallback workflow.
- The sync outbox is append-only from local services first; optional cloud sync can replay it later without becoming a runtime dependency.

## Verification

Use:

```bash
./scripts/test-rust.sh
```

This runs the canonical native GNU-target tests through the user-local Zig linker. It covers the Rust services plus bundled SQLite and FTS5.

You can call the native runner directly with:

```bash
./scripts/test-rust-native.sh
```

The musl helper remains available for future release packaging experiments, but it is intentionally not the default while bundled SQLite is enabled.
