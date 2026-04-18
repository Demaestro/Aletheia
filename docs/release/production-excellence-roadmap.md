# Production Excellence Roadmap

This list extends the remaining work already called out for vMix rehearsal, signed installers, secure storage, signed plugins, support bundles, packaged offline assets, and device acceptance.

## Additional Release Gates

- Release ownership: one named release captain, one security approver, and one worship-production approver.
- Migration discipline: every SQLite migration must have an upgrade test and a rollback or recovery note.
- Backup and restore: service database backup, restore, and corruption-repair flows must be rehearsed without internet.
- Accessibility certification: keyboard-only navigation, visible focus, status text beyond color, and 125-150% display scaling checks.
- Bible and model licensing: each packaged scripture translation, STT model, and language alias pack needs a distribution decision by region.
- Performance budgets: audio-to-candidate latency, preview render latency, memory ceiling, CPU ceiling, and startup recovery time.
- Incident response: support bundle export, redaction verification, crash-log retention, and emergency downgrade procedure.
- Operator permissions: safe defaults for volunteer mode, admin-only integration changes, and explicit live-output arming.

## Current Build Slice

- `aletheia-ops` owns production policy models and tests.
- The desktop shell exposes production readiness and redacted support bundle export.
- The Health screen now shows release gates, secret-vault status, plugin-signing status, offline assets, and device acceptance.

## Definition of Production Excellent

- A volunteer can recover from a failed integration in under 30 seconds.
- Live output never changes without an explicit operator action.
- The app runs a service offline with no loss of scripture search, preview, live output, or audit history.
- All support diagnostics are useful to engineering and safe to hand to a third party.
- Every external adapter can fail without taking down detection, manual search, or other outputs.
