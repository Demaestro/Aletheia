# Aletheia Production Readiness Checklist

## Pre-Service Rehearsal

- Start Aletheia and confirm local SQLite opens without repair prompts.
- Run Offline Health and verify scripture library, STT pack, disk reserve, and audit log state.
- Search `jn 3 16`, `Psalm 23:4`, and a phrase query.
- Send a candidate to Preview, then Live, then Clear.
- Rehearse vMix: Check, Preview title, Take live, Clear.
- Confirm Data Miser behavior before enabling any cloud enhancement.

## Integration Acceptance

- vMix Web API reachable at the configured endpoint.
- vMix title input and text fields match Aletheia config.
- OBS and vMix failures appear as degraded destination state, not app failure.
- EasyWorship/ProPresenter export paths are writable and do not require admin elevation during service.
- HDMI/NDI output safe areas are verified on the actual projector and livestream canvas.

## Release Verification

Run:

```bash
npm run verify:production
```

This checks Rust format, Rust compile, Rust tests, TypeScript, production frontend build, npm audit, and RustSec audit. If network-dependent audits fail to fetch advisories, rerun before release on a reliable network.

## Power-Loss Drill

- Send a preview scene.
- Send a live scene.
- Kill the app process.
- Reopen Aletheia.
- Confirm local database loads, audit history remains readable, and live output requires explicit re-arming.

## Accessibility Drill

- Navigate all operator screens with keyboard only.
- Confirm focus rings are visible.
- Confirm status changes are announced through visible text, not color alone.
- Confirm dense tables remain readable at 125% and 150% display scaling.
