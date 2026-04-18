# AI Accuracy, Language Routing, and Structural Hardening

Aletheia should behave like production software in a live booth: fast, explainable, recoverable, and conservative when confidence is weak. The product can use AI, but the operator must never depend on a network model to keep scriptures moving.

## Implemented Essentials

- Deterministic scripture alias routing now covers English, Yoruba, Igbo, Hausa, Twi, Swahili, Xhosa, Spanish, and French.
- Language detection runs before scripture matching so STT adapters, alias sets, and confidence policy can be tuned per language.
- The default confidence policy requires 95 percent before auto-preview. Live output still requires operator action.
- Local rehearsal includes a multilingual routing check and an accuracy fixture gate.
- Offline asset manifests distinguish installed scripture/alias packs from pending offline STT packs.

## Structural Hardening Still Required

- Run every service as a crash-only boundary with supervised restart, health probes, bounded queues, and explicit backpressure.
- Keep the event log append-only, replayable, and versioned. Every preview, live, clear, integration call, and AI recommendation should be recoverable after power loss.
- Add SQLite WAL mode, migration checksums, startup integrity checks, and rotating local backups.
- Add adapter contract tests for vMix, OBS, EasyWorship, ProPresenter, HDMI, NDI, OSC, HTTP, and Companion-style automation.
- Add fault injection for unplugged audio devices, unavailable displays, lost NDI discovery, corrupt model files, and slow cloud providers.
- Gate every plugin through signed manifests, explicit capability scopes, path sandboxing, and denied-by-default network access.
- Ship feature flags for risky integrations so a single adapter regression can be disabled without disabling scripture search.

## Language Support Model

The language layer has three levels:

- Alias-ready: local book names and reference phrases are packaged and work offline.
- STT-ready: an offline speech model or acoustic hint pack is installed and checksum-verified.
- Cloud-ready: optional cloud STT or reranking is allowed by the operator and network policy.

Hausa, Twi, Swahili, Xhosa, Spanish, and French are alias-ready in the app. Their offline STT packs are marked pending because the real model files must be licensed, downloaded, checksum-verified, and tested on the target Windows booth machine.

## Path To 95 Percent Accuracy

Do not treat 95 percent as a marketing number. Treat it as a release gate measured against real church audio.

- Build a labeled rehearsal dataset with at least 10 hours of Nigerian church audio, noisy pulpit audio, interpreters, singing bleed, crowd response, and code-switching.
- Split the dataset by language, service style, microphone quality, and church size so the score cannot hide weak languages.
- Measure precision, recall, time-to-detection, false live-risk events, and manual correction recovery.
- Use exact reference parsing first, then quote matching, then localized aliases, then service-plan bias, then thematic/coreference AI.
- Calibrate thresholds per language and per adapter. A 95 percent English threshold should not be blindly reused for Twi or Xhosa.
- Auto-preview only when measured precision is at or above 95 percent for that route. Keep live manual unless a church explicitly changes policy.
- Feed operator approvals and rejections back into local calibration after every rehearsal.
- Keep cloud reranking optional. If latency, bandwidth, or privacy policy fails, the local route remains the source of truth.

## Efficiency Strategy

- Run VAD before STT so silence and music do not waste inference cycles.
- Use a small local STT model for live drafts and a larger model only during rehearsal review or offline correction.
- Cache language routing, scripture aliases, and recent service-plan candidates in memory.
- Use bounded worker queues and drop stale transcript segments before blocking preview/live operations.
- Keep all integration calls async and time-boxed. A slow vMix or OBS call should degrade its adapter, not the scripture detector.
- Persist confidence evidence with each candidate so support bundles can explain why a verse was suggested without exposing raw secrets.
