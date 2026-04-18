# 1. Visual Thesis

Aletheia should feel like quiet broadcast control software: matte surfaces, sharp labels, obvious status, and just enough accent color to guide the next safe operator action.

# 2. Content Plan

- Landing page: establish trust, offline readiness, and production compatibility in one desktop-first composition.
- Operator workspace: surface the current service state, active transcript, queue, preview, live output, and health without dashboard clutter.
- Workflow views: isolate one production job per screen: listen, approve, present, theme, integrate, diagnose, onboard, and recover.
- System architecture: define a local-first Tauri platform where Rust services own capture, detection, storage, integrations, security, sync, and update reliability.

# 3. Interaction Thesis

- Hero entrance: one restrained fade/slide sequence for the landing composition.
- Sticky production rail: the active output, service health, and go-live controls remain available while operators move through dense workflows.
- Hover/reveal transition: interactive rows reveal secondary actions only on focus or hover, keeping the surface calm while preserving speed.

# 4. Design System

- Type: Inter for product UI and Atkinson Hyperlegible or system sans fallback for scripture presentation. JetBrains Mono only for logs, ports, IPs, and timecode. Maximum two active typefaces in production builds.
- Color: neutral white, graphite text, mist gray surfaces, line gray borders, and one accent green `#157A5C`. Semantic amber/red are reserved for health and safety only.
- Spacing: 4 px base grid, 16 px panel padding, 24 px section rhythm, 32 px primary workspace gaps, 48 px page rhythm.
- Layout: desktop-first at 1440 x 900. Left navigation 248 px, primary workspace fluid, right production rail 360 px. Mobile stacks into a read-only supervision view with critical actions grouped first.
- Shape: 6 px radius for buttons and functional panels. Cards are used only for click-to-select themes, integration tiles, queued verses, and onboarding steps.
- Borders: 1 px neutral dividers. Shadows only for overlays and live output depth.
- Copy: short operator labels. No marketing language inside the app. Every label answers state, source, confidence, or action.
- Accessibility: all primary actions reachable by keyboard, visible focus rings, no color-only status, large scripture preview text, reduced-motion support, and contrast target of WCAG AA or better.

# Required Screen Specifications

## Landing Page

- Purpose: explain the product promise and route teams to demo, onboarding, or operator mode.
- Layout structure: full-bleed first viewport with product name, short promise, compatibility strip, and a live-looking production surface as the dominant visual. Follow with three one-job sections: local-first reliability, integrations, and service workflow.
- Hierarchy: product name first, promise second, status-led proof points third, action row fourth.
- Key components: top navigation, hero workflow board, compatibility list, offline readiness band, comparison-free product principles, call to install.
- Copy examples: "Aletheia", "Detect scripture, approve with confidence, present without internet.", "Offline library ready", "EasyWorship, OBS, NDI, HDMI, OSC".
- Interaction notes: primary CTA opens onboarding; secondary CTA opens operator dashboard in demo mode.
- Accessibility notes: no text over low-contrast imagery, skip link, keyboard-visible CTA states, meaningful labels for integration badges.
- Motion usage: one hero entrance only.
- Responsive behavior: desktop composition remains one poster; tablet collapses proof points below hero; mobile becomes a vertical product intro with demo preview first.

## Operator Dashboard

- Purpose: give AV teams a single scan of current service readiness and the next decision.
- Layout structure: left nav, central "Now" workspace, right sticky production rail. Central area has current transcript, top suggestion, queue summary, and active source map.
- Hierarchy: service mode and live state first, active candidate second, queue and integration health third.
- Key components: service header, source status, AI confidence meter, "Approve to Preview", "Send Live", queue count, offline indicator.
- Copy examples: "Sunday second service", "Listening to pulpit mic", "Isaiah 40:31 detected from English NIV", "Manual confirmation recommended".
- Interaction notes: Enter approves focused candidate; Space toggles preview; Cmd/Ctrl+L sends preview live when armed.
- Accessibility notes: confidence shown with text and percent; live state announced with `aria-live`; destructive output changes require explicit focus.
- Motion usage: sticky right rail; row hover reveals "pin source" and "ignore speaker" actions.
- Responsive behavior: mobile is monitor-only unless admin enables remote approval.

## Live Transcript View

- Purpose: monitor speech-to-text in real time while keeping scripture candidates visible.
- Layout structure: transcript stream in the center, candidate gutter to the right, source diagnostics at top.
- Hierarchy: current line first, detected verse references second, latency and adapter health third.
- Key components: transcript lines with timecode, source badges, language tags, confidence markers, pause/resume capture, correction controls.
- Copy examples: "00:18:42 Pastor: turn with me to Romans chapter eight", "Candidate: Romans 8:28", "STT: offline whisper, 410 ms lag".
- Interaction notes: click a transcript line to seed manual search; press J/K to move through lines.
- Accessibility notes: transcript supports large text mode, speaker labels are text, updates can be paused for screen readers.
- Motion usage: new lines use a subtle opacity entrance; no animated scrolling when reduced motion is on.
- Responsive behavior: transcript becomes single column with candidate drawer.

## Scripture Queue And Approval Panel

- Purpose: make AI-assisted detections safe by requiring transparent approval before presentation.
- Layout structure: queue list left, candidate detail right, approval footer fixed to panel bottom.
- Hierarchy: reference, source, confidence, translation, and action.
- Key components: candidate rows, duplicate merge, translation selector, passage range trim, approve, reject, preview, send live.
- Copy examples: "Psalm 23:1-3", "Detected from sermon audio", "87 percent confidence", "Trim to verse 1 only", "Reason: matched quotation and reference".
- Interaction notes: high-confidence candidates can auto-preview, never auto-live by default. Rejections feed local adaptation only after operator opt-in.
- Accessibility notes: each row has status text; shortcuts are visible in tooltips and help overlay.
- Motion usage: hover/reveal actions on rows.
- Responsive behavior: stacked candidate detail below list.

## Presentation Preview And Live Output

- Purpose: separate safe preview from actual broadcast/projector output.
- Layout structure: split preview/live canvases, output controls below, right rail for destination status.
- Hierarchy: live state first, preview candidate second, destinations third.
- Key components: preview canvas, live canvas, lower-third toggle, stage display toggle, output destination chips, black/freeze/clear.
- Copy examples: "Preview: John 3:16 NIV", "Live: Romans 8:28 Yoruba", "OBS connected on 127.0.0.1:4455", "HDMI 2 armed".
- Interaction notes: live output requires armed destination; clear live is fast but confirmed when streamed.
- Accessibility notes: output state includes text labels and destination names, not just red/green.
- Motion usage: live canvas crossfades only on approved send-live.
- Responsive behavior: canvases stack, live output stays first.

## Theme Designer

- Purpose: let churches build readable scripture styles without creating broken broadcast layouts.
- Layout structure: theme list left, editable controls center, output preview right.
- Hierarchy: selected theme first, typography and safe area second, background/output constraints third.
- Key components: theme cards, font scale, safe margins, language fallback, lower-third/fullscreen modes, brand lock.
- Copy examples: "Lower Third - Broadcast", "Safe for 1080p", "Yoruba fallback enabled", "Minimum contrast passed".
- Interaction notes: controls validate instantly; invalid contrast blocks save but allows draft.
- Accessibility notes: contrast checker, minimum text size warnings, keyboard-adjustable sliders.
- Motion usage: preview updates with a short layout transition.
- Responsive behavior: preview moves above controls on narrow screens.

## Integrations Settings

- Purpose: configure production destinations and automation without coupling core detection to vendor APIs.
- Layout structure: integration categories by output type: presentation, broadcast, automation, transport.
- Hierarchy: connection state first, capability second, setup details third.
- Key components: EasyWorship adapter, ProPresenter adapter, OBS websocket, vMix HTTP, NDI output, HDMI display, OSC endpoint, Stream Deck/Companion profile export.
- Copy examples: "EasyWorship: watch folder ready", "OBS: websocket authenticated", "NDI: Verse Output available", "Companion: 12 buttons exported".
- Interaction notes: test buttons run dry checks; adapters can be disabled without restarting.
- Accessibility notes: every setup field has helper text and validation; logs are copyable as text.
- Motion usage: hover reveal for diagnostics and reset.
- Responsive behavior: category groups stack; advanced fields collapse.

## Offline Mode / Health Status Panel

- Purpose: show exactly what will keep working without internet.
- Layout structure: system health summary, local library coverage, adapter state, storage, update state, network quality.
- Hierarchy: blocking issues first, degraded capabilities second, healthy systems third.
- Key components: offline readiness score, Bible library coverage, STT model availability, sync backlog, disk space, update channel, last backup.
- Copy examples: "Ready offline for English, Yoruba, Igbo", "Cloud detection unavailable; local matcher active", "Sync backlog: 42 events", "Disk reserve: 18.4 GB".
- Interaction notes: "Run pre-service check" generates a local report; "Download language pack" queues background download.
- Accessibility notes: health statuses include clear severity labels and remediation copy.
- Motion usage: no decorative motion; progress bars update discretely.
- Responsive behavior: priority issues remain above fold.

## Onboarding Flow

- Purpose: get a volunteer from install to first safe output in under ten minutes.
- Layout structure: checklist steps with live verification: library, language, audio source, presentation destination, theme, rehearsal.
- Hierarchy: current step first, detected device state second, next action third.
- Key components: setup steps, device test, sample sermon phrase, output rehearsal, service profile save.
- Copy examples: "Choose your primary Bible translations", "Speak a reference into the mic", "Send a rehearsal verse to Preview only", "Save as Sunday AM".
- Interaction notes: all steps are skippable except library and output safety confirmation.
- Accessibility notes: clear error recovery, no time-limited steps, focus lands on next incomplete task.
- Motion usage: step transition only.
- Responsive behavior: works on laptop; mobile read-only setup summary.

## Manual Search And Fallback Workflow

- Purpose: provide a fast operator-controlled path when detection fails, audio is noisy, or the preacher changes direction.
- Layout structure: command-search field top, results center, translation/range controls right, output actions bottom.
- Hierarchy: search input first, exact reference results second, fuzzy phrase matches third.
- Key components: command palette, scripture lookup, phrase search, recent service references, range trim, translation switch, preview/live actions.
- Copy examples: "Search reference, phrase, or abbreviation", "jn 3 16", "the lord is my shepherd", "Use offline NIV", "Preview only".
- Interaction notes: search is local and instant; cloud semantic search can enhance but never block. Supports aliases like "Ps 23", "Yoh 3:16", and local language names.
- Accessibility notes: combobox pattern with active descendant, result count announced, keyboard-only flow.
- Motion usage: result rows reveal actions on focus/hover.
- Responsive behavior: command search stays primary; output controls remain sticky.

# 1. System Architecture

Aletheia is a local-first Tauri desktop application. React renders the operator interface in the WebView. Rust owns production-critical services: audio capture orchestration, STT adapters, scripture matching, local SQLite persistence, plugin runtime, integration transports, secure credential storage, logging, and update installation.

Core principle: AI assists decisions but does not own live output. The default path is detect -> explain -> approve -> preview -> send live. Auto-preview may be enabled per service profile; auto-live is disabled by default and requires an explicit signed policy.

Recommended process model:

- `ui-webview`: React, TypeScript, Tailwind, Framer Motion.
- `app-core`: Rust Tauri command layer, domain services, IPC event broker.
- `stt-workers`: sidecars or embedded Rust tasks for offline STT, hybrid streaming, and cloud adapters.
- `integration-workers`: isolated adapter tasks for EasyWorship, ProPresenter, OBS, vMix, NDI, OSC, HDMI, and Companion/Stream Deck exports.
- `sqlite-store`: local event-sourced operational database plus indexed scripture library.
- `optional-cloud`: sync, team profiles, hosted language packs, enhanced semantic matching, and remote backup.

Tauri notes: Tauri uses Rust plus an OS WebView and communicates through message passing, which matches the desired separation between operator UI and local system authority. Official plugins provide SQL access, shell/sidecar execution with explicit permissions, and updater support with signed artifacts.

# 2. Service Boundaries

- Presentation UI service: view state only, no direct filesystem, network, shell, or credential access.
- Command gateway: typed Tauri commands, validates permissions, records audit events.
- Capture service: owns input device selection, audio level, VAD, and source metadata.
- STT service: normalizes offline, hybrid, and cloud transcription into one transcript event stream.
- Detection service: parses references, phrases, aliases, multilingual book names, verse quotations, and context windows.
- Decision service: scores candidates and produces approval recommendations with reasons.
- Scripture library service: local translations, language packs, license metadata, indexes, and search.
- Queue service: candidate lifecycle, duplicate merge, manual overrides, approval and rejection history.
- Render service: transforms approved passages into preview/live scenes and exports rendered state.
- Integration service: adapter registry, connection lifecycle, command retries, and destination capabilities.
- Offline/sync service: local event log, outbound queue, conflict resolution, and bandwidth policy.
- Observability service: local structured logs, service health, metrics, and support bundle export.
- Update service: signed app updates, migration safety checks, language pack updates, plugin updates.

# 3. Module Responsibilities

- `ui/shell`: navigation, keyboard shortcuts, focus management, reduced motion.
- `ui/workflows`: dashboard, transcript, queue, output, theme, integrations, health, onboarding, search.
- `core/ipc`: Tauri command definitions, event subscriptions, typed error envelopes.
- `core/events`: append-only domain events and event broker.
- `core/audio`: device enumeration, capture sessions, noise gate, VAD.
- `core/stt`: adapter interface, model registry, latency tracking, failover.
- `core/detection`: reference parser, phrase matcher, multilingual aliases, confidence math.
- `core/scripture`: SQLite queries, FTS indexes, translation licensing, cached passages.
- `core/presentation`: theme compiler, safe-area validation, scene rendering.
- `core/integrations`: adapter SDK, permission scopes, retries, circuit breakers.
- `core/sync`: CRDT-friendly profile data, event upload, local conflict queue.
- `core/security`: keychain/Stronghold, token scope, plugin sandbox policy, signed adapter manifests.

# 4. Event Flow

1. Audio source emits frames with device ID, level, and timestamp.
2. STT adapter emits transcript segments with language, speaker guess, confidence, and latency.
3. Detection service receives transcript window, reference parser results, phrase matches, recent sermon context, and manual hints.
4. Decision service emits `ScriptureCandidateCreated` with confidence, reasons, source evidence, and recommended action.
5. Queue service merges duplicates and orders candidates by recency, confidence, and service policy.
6. Operator approves candidate to preview or sends manual search result to preview.
7. Render service compiles passage plus active theme into a preview scene.
8. Operator sends preview live to selected destinations.
9. Integration service dispatches destination-specific commands and records delivery receipts.
10. Observability service records timings, failures, retries, and operator actions.
11. Sync service uploads non-sensitive events when policy and bandwidth allow.

# 5. Data Models

```ts
type ServiceProfile = {
  id: string;
  name: string;
  defaultTranslations: string[];
  languages: string[];
  outputDestinations: string[];
  autoPreviewMinConfidence: number;
  autoLiveEnabled: false;
  bandwidthPolicy: "offline" | "low" | "normal";
};

type TranscriptSegment = {
  id: string;
  sessionId: string;
  startedAtMs: number;
  endedAtMs: number;
  speakerLabel?: string;
  language: string;
  text: string;
  confidence: number;
  sttAdapter: "offline" | "hybrid" | "cloud";
  latencyMs: number;
};

type ScriptureCandidate = {
  id: string;
  sessionId: string;
  reference: string;
  translation: string;
  language: string;
  confidence: number;
  reasons: string[];
  evidenceSegmentIds: string[];
  status: "new" | "previewed" | "approved" | "rejected" | "sent_live";
  source: "ai_detected" | "manual_search" | "imported_plan";
};

type IntegrationAdapter = {
  id: string;
  kind: "easyworship" | "propresenter" | "obs" | "vmix" | "ndi" | "hdmi" | "osc" | "companion";
  displayName: string;
  enabled: boolean;
  capabilities: string[];
  health: "connected" | "degraded" | "offline" | "misconfigured";
  lastCheckedAt: string;
};
```

SQLite tables:

- `service_profiles`
- `service_sessions`
- `translations`
- `scripture_books`
- `scripture_verses`
- `transcript_segments`
- `scripture_candidates`
- `queue_events`
- `presentation_scenes`
- `integration_configs`
- `integration_events`
- `sync_outbox`
- `audit_log`
- `health_snapshots`
- `plugin_manifests`

Indexes:

- FTS5 on verse text per translation.
- Normalized book aliases by language.
- Candidate lookup by session, status, confidence, and created time.
- Integration event lookup by destination and severity.

# 6. Plugin / Integration Model

Opinionated model: integrations are plugins, but only local signed plugins get production output authority.

Adapter interface:

- `discover()`: find local apps, ports, displays, NDI routes, watch folders.
- `connect(config)`: authenticate or open transport.
- `capabilities()`: report preview/live/lower-third/clear/freeze support.
- `send(scene)`: deliver a rendered scene or command payload.
- `clear()`, `freeze()`, `blackout()`: optional safety controls.
- `health()`: return state, latency, last error, and remediation.
- `dryRun()`: validate without changing live output.

Integration specifics:

- EasyWorship: primary compatibility through watch folder, schedule import/export, image/text slide generation, and optional local API if available.
- ProPresenter: local network API adapter with strict host allowlist.
- OBS: websocket adapter, scene/source updates, browser source or text source modes.
- vMix: HTTP API adapter for title input and overlays.
- NDI: local output service for scripture scene feed.
- HDMI: dedicated borderless output window bound to selected display.
- OSC: cue messages for lighting, media servers, and automation.
- Companion/Stream Deck: export button profiles and expose local HTTP endpoints for safe commands.

# 7. Offline Architecture

- The app must boot, search scripture, approve, preview, and present without internet.
- SQLite stores scripture libraries, service profiles, themes, queue state, and event logs.
- Offline STT models are installed per language pack and versioned separately from the app.
- Detection has three layers: exact reference parser, phrase/FTS matcher, and optional semantic matcher. Only the first two are mandatory offline.
- Cloud calls are opportunistic and cancelable. They never block manual search, local detection, preview, live output, or integration controls.
- Low-bandwidth mode disables large sync payloads, defers model updates, compresses logs, and sends only metadata plus approved events.
- Pre-service check verifies scripture library, STT models, output destinations, disk reserve, and update safety.

# 8. Sync Model

- Local device is source of truth during a service.
- Sync is append-only for operational events and last-writer-wins only for non-critical preferences.
- Cloud sync queues:
  - profile changes
  - approved/rejected candidate feedback
  - anonymized detection metrics
  - theme backups
  - integration configuration backup without secrets
- Conflict policy:
  - service events never merge destructively
  - themes keep versions and allow restore
  - credentials do not sync unless explicitly stored through a secure team vault
  - translation license metadata syncs, verse text depends on license terms
- Bandwidth policy:
  - `offline`: no cloud
  - `low`: sync metadata only under 32 KB batches
  - `normal`: sync full non-sensitive event bundles and support reports

# 9. Performance Strategy

- UI target: first meaningful shell under 1.5 s on a mid-range Windows AV booth machine.
- Search target: local reference lookup under 50 ms, phrase search under 150 ms for installed translations.
- STT latency target: under 800 ms offline for short transcript segments, visible warning above 1500 ms.
- Output target: preview render under 100 ms, send-live dispatch under 250 ms excluding destination API latency.
- Use Rust for hot-path parsing, scoring, SQLite access, and integration IO.
- Use WebView only for operator state and rendering preview surfaces.
- Maintain in-memory caches for book aliases, active translations, theme compilation, and current session queue.
- Backpressure transcript processing. Drop duplicate low-confidence transcript fragments before detection.
- Virtualize long transcript and log lists.
- Prefer sidecar isolation for heavy STT so UI and integration controls remain responsive.

# 10. Security Model

- Default deny for shell, filesystem, network, and plugin permissions.
- Tauri capability files are scoped by window. The main operator window gets only the commands it needs.
- Credentials stored in OS keychain or Stronghold-style encrypted vault, never in SQLite plain text.
- Plugins require signed manifests, declared permissions, version, checksum, vendor, and supported transports.
- Local HTTP control endpoints require loopback binding, random per-session token, and explicit operator enablement.
- OBS/vMix/ProPresenter passwords are redacted in logs and support bundles.
- Scripture libraries are license-tracked; export modes respect licensing flags.
- Audit log records manual live output, auto-preview policy changes, plugin install, credential update, and theme publish.
- Remote cloud enhancement is opt-in per organization and per service profile.

# 11. Observability / Logging Strategy

- Local structured logs in JSONL with rotation and support bundle export.
- Operator-facing health is translated into plain remedies: "Reconnect OBS websocket", "Download Yoruba STT pack", "Select HDMI display".
- Metrics:
  - STT latency by adapter
  - detection precision feedback
  - approval time
  - send-live delivery time
  - integration retries and failures
  - sync backlog size
  - offline readiness
- Privacy:
  - transcript logging defaults to off
  - support bundles redact secrets and can omit transcript text
  - cloud metrics aggregate candidate outcomes without sermon audio

# 12. Deployment And Auto-Update Strategy

- Ship as signed Tauri installers for Windows first, then macOS and Linux.
- Release channels: stable, preview, and church-internal beta.
- App updates are signed and validated by the Tauri updater.
- Language packs, STT models, translation indexes, and plugins have independent signed manifests and rollback support.
- Migrations run before opening a service session; failed migrations rollback and keep previous app build available.
- Update UX:
  - never force update during active service
  - allow "install after service"
  - show update size and whether it affects output, libraries, or plugins
  - Windows installer limitations are handled by warning before install

# 13. QA / Testing Strategy

- Unit tests:
  - reference parser aliases
  - multilingual book names
  - confidence scoring
  - theme contrast validation
  - SQLite migrations
- Integration tests:
  - OBS websocket mock
  - vMix HTTP mock
  - EasyWorship watch-folder export
  - OSC message emission
  - HDMI output window routing
- End-to-end tests:
  - noisy transcript -> candidate -> approve -> preview -> live
  - offline boot with no network
  - low-bandwidth sync queue
  - manual search fallback under time pressure
  - plugin disabled while service is live
- AV booth rehearsal tests:
  - 1080p and 4K output
  - duplicate displays
  - hot-unplug HDMI
  - audio device changes
  - OBS restart during service
- Accessibility tests:
  - keyboard-only operation
  - screen reader announcements for live state
  - large text mode
  - reduced motion
  - color contrast

# 14. Phased Roadmap From MVP To v1

## MVP: Safe Local Presentation

- Tauri shell, React operator UI, SQLite library, manual search, preview/live output, theme basics, HDMI output, service profiles, health panel.
- Offline English scripture library and reference parser.
- No cloud dependency.

## Alpha: AI-Assisted Detection

- Offline STT adapter, transcript stream, candidate queue, confidence reasoning, approval workflow, FTS phrase matching, operator feedback loop.
- OBS websocket and EasyWorship watch-folder adapters.

## Beta: Production Integrations

- ProPresenter, vMix, OSC, NDI, Companion export, multilingual aliases, Yoruba/Igbo language packs, pre-service check, support bundle export.
- Signed plugin manifest and adapter registry.

## Release Candidate: Reliability And Teams

- Optional sync, team profiles, signed auto-updates, rollback, plugin update channels, structured observability, migration hardening, license-aware scripture exports.

## v1: Church Production Platform

- Offline/hybrid/cloud STT modes, confidence policy engine, multiple output destinations, advanced theme designer, low-bandwidth sync, secure remote backup, full QA matrix, and documented integration SDK.

# Platform References

- Tauri architecture: https://v2.tauri.app/concept/architecture/
- Tauri SQL plugin and migrations: https://v2.tauri.app/plugin/sql/
- Tauri shell plugin and permissions: https://v2.tauri.app/plugin/shell/
- Tauri capabilities: https://v2.tauri.app/security/capabilities/
- Tauri updater: https://v2.tauri.app/plugin/updater/
