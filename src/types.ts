import type { LucideIcon } from "lucide-react";

export type ScreenKey =
  | "landing"
  | "dashboard"
  | "transcript"
  | "queue"
  | "output"
  | "theme"
  | "integrations"
  | "health"
  | "onboarding"
  | "search"
  | "stream"
  | "songs"
  | "fleet"
  | "clips";

export type Tone = "healthy" | "degraded" | "offline" | "live" | "armed" | "neutral";

export type NavItem = {
  key: ScreenKey;
  label: string;
  eyebrow: string;
  icon: LucideIcon;
};

export type TranscriptSegment = {
  id: string;
  time: string;
  speaker: string;
  language: string;
  text: string;
  confidence: number;
  latencyMs: number;
};

export type ScriptureCandidate = {
  id: string;
  reference: string;
  translation: string;
  language: string;
  text: string;
  confidence: number;
  source: string;
  reason: string;
  status: "new" | "preview" | "approved" | "live" | "rejected";
};

export type Integration = {
  id: string;
  name: string;
  kind: string;
  state: "connected" | "degraded" | "offline" | "ready";
  detail: string;
  capability: string;
};

export type HealthItem = {
  label: string;
  state: Tone;
  detail: string;
  action: string;
};

export type ThemePreset = {
  id: string;
  name: string;
  mode: "Lower third" | "Full screen" | "Stage display";
  contrast: string;
  fontScale: string;
  languages: string[];
};
// ---------------------------------------------------------------------------
// Service plan (Planning Center / ProPresenter / Elvanto / .aletheia-plan.json)
// ---------------------------------------------------------------------------
export type ServicePlanItemKind =
  | "song"
  | "scripture"
  | "sermon"
  | "prayer"
  | "announcement"
  | "offering"
  | "other";

export type ServicePlanItem = {
  id: string;
  kind: ServicePlanItemKind;
  title: string;
  /** For scripture: "Romans 8:28". For songs: canonical reference e.g. CCLI#. */
  reference?: string;
  /** Estimated duration in seconds for the timeline widget. */
  durationSec: number;
  /** Optional note shown in the timeline tooltip. */
  note?: string;
};

export type ServicePlan = {
  id: string;
  name: string;
  /** ISO 8601 service start timestamp. */
  startsAt: string;
  /** Source provider for auditability. */
  source: "planning-center" | "propresenter" | "elvanto" | "file" | "manual";
  items: ServicePlanItem[];
  /** Epoch ms when the plan was imported. */
  importedAtMs: number;
};

// ---------------------------------------------------------------------------
// Stream overlay lane (separate from in-room projection)
// ---------------------------------------------------------------------------
export type StreamOverlayState = {
  /** Ticker text cycling along the bottom of the stream overlay. */
  tickerText: string;
  /** Whether the overlay is armed for the stream browser source. */
  armed: boolean;
  /** Currently published reference on the overlay. */
  liveReference: string | null;
  /** HTML file path last written for the OBS/vMix browser source. */
  browserSourcePath: string | null;
};

// ---------------------------------------------------------------------------
// Lyrics / song library
// ---------------------------------------------------------------------------
export type SongSection = {
  label: string; // e.g. "Verse 1", "Chorus", "Bridge"
  text: string;
};

export type Song = {
  id: string;
  title: string;
  /** CCLI song number for reporting; null if public domain / original. */
  ccliNumber: string | null;
  author: string;
  copyright: string;
  language: string;
  sections: SongSection[];
  /** Optional chord chart key, e.g. "G", "Ab". */
  songKey: string | null;
  /** Tempo in BPM, optional. */
  bpm: number | null;
  createdAtMs: number;
  updatedAtMs: number;
};

/** One row in the CCLI usage log — persisted and exported for quarterly reporting. */
export type CcliUsageEntry = {
  id: string;
  ccliNumber: string;
  songTitle: string;
  sentLiveAtMs: number;
  serviceSessionId: string;
  operator: string;
};

export type ManualSearchResult = {
  reference: string;
  translation: string;
  snippet: string;
  source: string;
  language?: string;
};

export type DesktopRuntimeStatus = {
  mode: "tauri" | "browser-fallback";
  serviceSession: string;
  databasePath: string;
  dataMiserEnabled: boolean;
  offlineModeEnabled: boolean;
  destinationsArmed: boolean;
  auditCount: number;
  lastEventSequence: number;
  checkedAtMs: number;
};


export type VmixConfig = {
  host: string;
  port: number;
  titleInput: string;
  verseField: string;
  referenceField: string;
  overlayChannel: number;
  allowPrivateNetwork: boolean;
  username?: string;
  password?: string;
};

export type VmixStatus = VmixConfig & {
  state: "connected" | "ready" | "degraded" | "offline";
  detail: string;
  endpoint: string;
  checkedAtMs: number;
  authEnabled?: boolean;
};

export type VmixDispatchResult = {
  state: "connected" | "ready" | "degraded" | "offline";
  detail: string;
  reference: string;
  auditCount: number;
};

export type IntegrationEvent = {
  timestampMs: number;
  integrationId: string;
  severity: "info" | "warn" | "error";
  action: string;
  detail: string;
};

export type SecretVaultStatus = {
  provider: string;
  state: string;
  detail: string;
  storedSecretCount: number;
  releaseRequired: boolean;
  policy: string[];
};

export type PluginPolicyStatus = {
  state: string;
  detail: string;
  trustedKeyCount: number;
  requiredControls: string[];
};

export type VerifiedPluginManifest = {
  id: string;
  name: string;
  version: string;
  digestSha256: string;
  keyId: string;
  capabilityCount: number;
};

export type PluginVerificationResult = {
  state: "verified" | "enabled" | "rejected";
  detail: string;
  manifest?: VerifiedPluginManifest;
};

export type OfflineAsset = {
  id: string;
  kind: string;
  label: string;
  language: string;
  license: string;
  state: string;
  sizeMb: number;
  checksumSha256: string;
  requiredForRelease: boolean;
};

export type OfflineAssetManifest = {
  state: string;
  installedCount: number;
  requiredCount: number;
  assets: OfflineAsset[];
};

export type OfflinePackExport = {
  path: string;
  manifestPath: string;
  checksumPath: string;
  assetCount: number;
  bytesWritten: number;
};

export type AcceptanceStep = {
  label: string;
  expected: string;
  required: boolean;
};

export type AcceptanceDevice = {
  id: string;
  name: string;
  category: string;
  state: string;
  requiredForRelease: boolean;
  steps: AcceptanceStep[];
};

export type HardwareChecklistItem = {
  id: string;
  label: string;
  detail: string;
  required: boolean;
};

export type ReleaseGateStatus = {
  label: string;
  state: string;
  detail: string;
};

export type SupportBundlePlan = {
  state: string;
  detail: string;
  includes: string[];
  excludes: string[];
};

export type RedactionSummary = {
  emails: number;
  ipAddresses: number;
  windowsPaths: number;
  unixPaths: number;
  secretLikeValues: number;
};

export type ProductionReadinessReport = {
  generatedAtMs: number;
  score: number;
  state: string;
  blockers: string[];
  secretVault: SecretVaultStatus;
  pluginPolicy: PluginPolicyStatus;
  supportBundle: SupportBundlePlan;
  offlineAssets: OfflineAssetManifest;
  acceptanceDevices: AcceptanceDevice[];
  releaseGates: ReleaseGateStatus[];
};

export type SupportBundleExport = {
  path: string;
  sizeBytes: number;
  redactionSummary: RedactionSummary;
  includedFiles: string[];
};

export type BoothPackExport = {
  path: string;
  generatedAtMs: number;
  files: string[];
};

export type LocalRehearsalStep = {
  label: string;
  state: string;
  detail: string;
  durationMs: number;
};

export type LocalRehearsalReport = {
  generatedAtMs: number;
  state: string;
  passed: number;
  total: number;
  proofPath: string;
  steps: LocalRehearsalStep[];
};

export type AiAdapterStatus = {
  id: string;
  name: string;
  mode: string;
  state: "ready" | "healthy" | "degraded" | "offline";
  detail: string;
  latencyMs: number;
};

export type LanguageDetection = {
  code: string;
  name: string;
  confidence: number;
  matchedTerms: string[];
};

export type SupportedLanguage = {
  code: string;
  name: string;
  sttLocale: string;
  scriptureAliasesReady: boolean;
  offlineSttReady: boolean;
  cloudSttReady: boolean;
};

export type AccuracyTarget = {
  targetPrecision: number;
  targetRecall: number;
  autoPreviewThreshold: number;
  validatedPrecision: number;
  validatedRecall: number;
  validationSampleCount: number;
  liveRequiresOperator: boolean;
  strategy: string[];
};

export type AiDetectionResult = {
  mode: string;
  decisionPolicy: string;
  processedSegments: number;
  candidates: ScriptureCandidate[];
  adapters: AiAdapterStatus[];
  languages: LanguageDetection[];
  supportedLanguages: SupportedLanguage[];
  accuracyTarget: AccuracyTarget;
  checkedAtMs: number;
};

// ---------------------------------------------------------------------------
// Service profiles
// ---------------------------------------------------------------------------

export type ServiceProfile = {
  id: string;
  name: string;
  languages: string[];
  outputPolicy: "manual-live" | "auto-preview" | string;
  isActive: boolean;
  createdAtMs: number;
  updatedAtMs: number;
};

// ---------------------------------------------------------------------------
// Adapter dispatch results (OBS / OSC / EasyWorship)
// ---------------------------------------------------------------------------

export type AdapterDispatchResult = {
  adapter: string;
  state: "connected" | "ready" | "degraded" | "offline" | string;
  detail: string;
  reference: string;
  auditCount: number;
};

// ---------------------------------------------------------------------------
// OBS WebSocket v5 config
// ---------------------------------------------------------------------------

export type ObsConfig = {
  host: string;
  port: number;
  password: string;
  sceneName: string;
  sourceName: string;
  allowPrivateNetwork: boolean;
};

export type ObsStatus = ObsConfig & {
  state: "connected" | "ready" | "degraded" | "offline" | string;
  detail: string;
  checkedAtMs: number;
};

// ---------------------------------------------------------------------------
// ProPresenter 7+ REST config
// ---------------------------------------------------------------------------

export type ProPresenterConfig = {
  host: string;
  port: number;
  messageName: string;
  verseToken: string;
  referenceToken: string;
  allowPrivateNetwork: boolean;
};

// ---------------------------------------------------------------------------
// Bitfocus Companion HTTP control config
// ---------------------------------------------------------------------------

export type CompanionConfig = {
  host: string;
  port: number;
  page: number;
  row: number;
  column: number;
  verseVariable: string;
  referenceVariable: string;
  allowPrivateNetwork: boolean;
};

// ---------------------------------------------------------------------------
// OSC 1.0 UDP config
// ---------------------------------------------------------------------------

export type OscConfig = {
  host: string;
  port: number;
  namespace: string;
  allowPrivateNetwork: boolean;
};

export type OscStatus = OscConfig & {
  state: "connected" | "ready" | "degraded" | "offline" | string;
  detail: string;
  checkedAtMs: number;
};

// ---------------------------------------------------------------------------
// EasyWorship watch-folder config
// ---------------------------------------------------------------------------

export type EasyWorshipConfig = {
  watchDir: string;
};

export type EasyWorshipStatus = EasyWorshipConfig & {
  state: "connected" | "ready" | "degraded" | "offline" | string;
  detail: string;
  checkedAtMs: number;
};

// ---------------------------------------------------------------------------
// Trusted plugin registry (v6)
// ---------------------------------------------------------------------------

export type TrustedPlugin = {
  id: string;
  name: string;
  version: string;
  keyId: string;
  digest: string;
  capabilities: string[];
  enabled: boolean;
  trustedAtMs: number;
};

// ---------------------------------------------------------------------------
// Calibration dataset pipeline (v6)
// ---------------------------------------------------------------------------

export type CalibrationReport = {
  confirmed: number;
  corrected: number;
  rejected: number;
  total: number;
  /** Operator-confirmed precision 0–100, or null if fewer than 5 samples. */
  precision: number | null;
};
