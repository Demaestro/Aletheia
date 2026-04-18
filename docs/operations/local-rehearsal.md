# Local Rehearsal Runner

The local rehearsal runner is Aletheia's no-internet booth proof. It validates the parts of the system that can be checked without vendor hardware or cloud services.

## Checks

- SQLite scripture index resolves a known verse from the local database.
- Local AI detection emits a high-confidence scripture candidate from the bundled transcript.
- Preview rendering produces visible verse and reference layers.
- vMix configuration validates host, port, title input, fields, and overlay channel.
- Live safety confirms the current destination-arming state is explicit.
- Support redaction catches email, IP address, Windows path, and secret-like values.
- Local proof export writes rehearsal evidence to the app data directory.

## When To Run

Run this after:

- Installing or updating Aletheia.
- Changing the booth machine.
- Changing vMix settings.
- Adding offline scripture or STT assets.
- Before the first real hardware acceptance test.

## What It Does Not Replace

This runner cannot prove projector output, capture-card signal, NDI discovery, OBS WebSocket auth, EasyWorship import behavior, ProPresenter API permissions, or vMix hardware routing. Those remain device-level acceptance tests on the actual Windows booth machine.
