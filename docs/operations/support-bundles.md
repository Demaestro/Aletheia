# Support Bundles

Support bundles are for engineering diagnosis after a rehearsal or service incident. They must be useful without exposing private church data.

## Default Contents

- Product version and runtime mode.
- Local health summary.
- Redacted integration configuration.
- Recent adapter delivery receipts.
- Device acceptance checklist.
- Production readiness report.

## Default Exclusions

- Provider API keys.
- Integration passwords.
- Local usernames and home paths.
- Raw transcript text.
- Full scripture database.

Transcript text is opt-in because sermons, names, counselling references, and prayer requests can appear in live audio.

## Redaction Rules

The current exporter redacts:

- Email addresses.
- IPv4 addresses.
- Windows paths.
- Linux, macOS, and WSL user paths.
- Secret-like key, token, password, and API key values.

The exporter writes JSON into the desktop app data directory under `support-bundles`.
