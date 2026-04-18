# Aletheia Production UI

Premium desktop-first worship production interface for a local-first scripture assistance platform.

The project contains:

- `docs/design-and-architecture.md`: full visual thesis, screen specifications, and production architecture.
- `docs/integrations/vmix.md`: vMix HTTP API setup, command mapping, and security policy.
- `docs/security/threat-model.md`: production threat model and required controls.
- `docs/security/plugin-signing.md`: signed plugin manifest policy and capability model.
- `docs/operations/support-bundles.md`: redacted support bundle policy.
- `docs/operations/booth-pack.md`: OBS, EasyWorship, ProPresenter, vMix, NDI, HDMI, OSC, and Companion booth export workflow.
- `docs/operations/local-rehearsal.md`: deterministic local rehearsal runner and proof policy.
- `docs/qa/production-readiness.md`: pre-service and release readiness checklist.
- `docs/qa/device-acceptance.md`: real-device acceptance plan for worship production integrations.
- `docs/release/production-excellence-roadmap.md`: release gates beyond first implementation.
- `src/App.tsx`: interactive screen router and operator state model.
- `src/components`: one component per required workflow screen plus shared production primitives.
- `src/data/production.ts`: real service content, integrations, transcript segments, health state, themes, and scripture candidates.

## Run Locally

```bash
npm install
npm run dev
```

The Vite dev server is configured for `127.0.0.1:5178`.

Run the production verification gate with:

```bash
npm run verify:production
```

## Production Direction

The UI is designed to sit inside a Tauri desktop shell. React owns operator presentation state. Rust should own local scripture search, SQLite persistence, STT adapters, plugin execution, integrations, health checks, secure storage, sync, and signed updates.

## Rust Service Layer

The first Rust slice is in the workspace root:

- `crates/aletheia-core`
- `crates/aletheia-vad`
- `crates/aletheia-audio-ingest`
- `crates/aletheia-detection`
- `crates/aletheia-output`
- `crates/aletheia-vmix`
- `apps/aletheia-audio-service`

Run the Rust tests in this WSL environment with:

```bash
./scripts/test-rust.sh
```

That command delegates to the native GNU target through the user-local Zig linker. This is the canonical path now that the local-first SQLite store uses bundled SQLite and FTS5.

You can call the native runner directly with:

```bash
./scripts/test-rust-native.sh
```

Run the deterministic audio-service harness with:

```bash
./scripts/run-audio-service.sh
```

The musl helper remains in `scripts/rust-musl-env.sh` for future packaging work, but it is not the default verification path while bundled SQLite is enabled.

## Booth Integration Export

Inside the desktop app, open **Integrations** and press **Export booth pack**. Aletheia writes a rehearsal-ready folder containing:

- OBS browser source HTML.
- EasyWorship scripture text handoff.
- ProPresenter cue JSON.
- vMix title setup notes.
- NDI, HDMI, OSC, and Companion-style wiring specs.

This pack is generated from the current preview scripture. It does not include secrets, and it keeps live output behind the destination arming policy.

## Local Rehearsal Runner

Inside **Health**, press **Run local rehearsal**. The app runs deterministic checks against:

- SQLite scripture lookup.
- Local AI scripture detection policy.
- Preview scene rendering.
- vMix command configuration.
- Live-output arming safety.
- Support-bundle redaction.
- Local proof export.

The runner writes a proof JSON file under the app data directory so a booth lead can keep rehearsal evidence without exposing credentials.

## Operator Principles

- AI can suggest and explain, but live output requires explicit operator control.
- Preview and live output stay separate.
- Manual search works offline and remains the fastest fallback.
- Integrations degrade independently so a failed vendor adapter does not stop scripture presentation.
