import { invoke } from "@tauri-apps/api/core";
import {
  healthItems,
  integrations,
  manualSearchResults,
  scriptureCandidates,
  transcriptSegments
} from "../data/production";

// ---------------------------------------------------------------------------
// Timeout-guarded invoke — prevents the UI from hanging indefinitely if the
// Rust backend is unresponsive.  Every Tauri command goes through this wrapper.
// ---------------------------------------------------------------------------

const DEFAULT_TIMEOUT_MS = 5_000;
const SLOW_OP_TIMEOUT_MS = 15_000;

function invokeWithTimeout<T>(cmd: string, args?: Record<string, unknown>, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  return Promise.race([
    invoke<T>(cmd, args),
    new Promise<never>((_, reject) => {
      controller.signal.addEventListener("abort", () =>
        reject(new Error(`Command "${cmd}" timed out after ${timeoutMs}ms`))
      );
    })
  ]).finally(() => clearTimeout(timer));
}
import type {
  AdapterDispatchResult,
  CalibrationReport,
  DesktopRuntimeStatus,
  AiDetectionResult,
  BoothPackExport,
  EasyWorshipConfig,
  HealthItem,
  Integration,
  IntegrationEvent,
  LocalRehearsalReport,
  ManualSearchResult,
  ObsConfig,
  OfflinePackExport,
  OscConfig,
  ProPresenterConfig,
  CompanionConfig,
  ProductionReadinessReport,
  OfflineAssetManifest,
  PluginVerificationResult,
  ScriptureCandidate,
  ServiceProfile,
  SupportBundleExport,
  TranscriptSegment,
  TrustedPlugin,
  VmixConfig,
  VmixDispatchResult,
  VmixStatus
} from "../types";

type DesktopSession = {
  id: string;
  name: string;
  startedAt: string;
  databasePath: string;
  mode: "tauri" | "browser-fallback";
  dataMiserEnabled: boolean;
  offlineModeEnabled: boolean;
  destinationsArmed: boolean;
  auditCount: number;
  lastEventSequence: number;
  checkedAtMs: number;
};

export type OutputSceneLayer = {
  layer: "verse" | "reference" | "contextCard";
  text: string;
  visible: boolean;
};

export type OutputScene = {
  id: string;
  reference: string;
  translation: string;
  themeId: string;
  layers: OutputSceneLayer[];
};

export type LiveOutputResult = {
  scene: OutputScene;
  auditCount: number;
};

export type DesktopServiceState = {
  session: DesktopRuntimeStatus;
  transcript: TranscriptSegment[];
  candidates: ScriptureCandidate[];
  integrations: Integration[];
  health: HealthItem[];
  preview: ScriptureCandidate;
  live: ScriptureCandidate;
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauriRuntime() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

/**
 * Reveal the main window.  The window starts invisible (visible:false in
 * tauri.conf.json) so that WebView2's initialisation phase never causes the
 * Win32 "(Not Responding)" freeze.  Call this as early as possible after
 * React mounts — the window will appear only once the JS engine is live.
 */
export async function showMainWindow(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    await invoke("show_main_window");
  } catch {
    // Non-fatal: worst case the window stays hidden; user can click taskbar.
  }
}

/**
 * Begin live audio capture + Whisper transcription. The Rust backend will emit
 * `aletheia://transcript-segment` and `aletheia://candidates-updated` events as
 * speech is processed. Subscribe via onTranscriptSegment / onCandidatesUpdated.
 *
 * Resolves with the path of the loaded STT model on success. Throws if no
 * offline model is installed or the microphone cannot be opened.
 */
export async function startAudioCapture(languageHint?: string, deviceName?: string): Promise<string> {
  if (!isTauriRuntime()) {
    throw new Error("Live capture requires the desktop runtime.");
  }
  return invokeWithTimeout<string>("start_audio_capture", { languageHint, deviceName }, SLOW_OP_TIMEOUT_MS);
}

export type BibleTranslationStatus = {
  id: string;
  name: string;
  versesLoaded: number;
  fullCanon: boolean;
};

export type BibleImportResult = {
  translationId: string;
  versesInserted: number;
};

/** Lists all known translations and how many verses are currently loaded. */
export async function listBibleTranslations(): Promise<BibleTranslationStatus[]> {
  if (!isTauriRuntime()) return [];
  try {
    return await invokeWithTimeout<BibleTranslationStatus[]>("list_bible_translations");
  } catch (err) {
    console.warn("list_bible_translations failed", err);
    return [];
  }
}

/**
 * Imports a full Bible JSON file (thiagobodruk schema) for a translation. Use
 * this for licensed translations (NKJV, NIV, NLT, MSG) that cannot be bundled.
 * `jsonPath` must be an absolute filesystem path; the operator picks the file
 * via the system file picker.
 */
export async function importBibleTranslation(
  translationId: string,
  translationName: string,
  license: string,
  jsonPath: string
): Promise<BibleImportResult> {
  if (!isTauriRuntime()) {
    throw new Error("Bible import requires the desktop runtime.");
  }
  // Bulk insert of ~31 000 verses can take 30+ seconds on slow disks. Use a
  // 2-minute ceiling so we don't false-fail mid-import.
  return invokeWithTimeout<BibleImportResult>(
    "import_bible_translation",
    { translationId, translationName, license, jsonPath },
    120_000
  );
}

/**
 * Returns the names of all audio input devices visible to the OS.
 * Use this to populate an audio device picker dropdown.
 */
export async function listAudioDevices(): Promise<string[]> {
  if (!isTauriRuntime()) return ["Default Microphone (Browser Fallback)"];
  try {
    return await invokeWithTimeout<string[]>("list_audio_devices");
  } catch {
    return [];
  }
}

/**
 * Returns a step-by-step SMB share setup guide for EasyWorship cross-machine
 * integration. The guide is tailored to the supplied watch directory path.
 * Issue 8 fix: removes the need for manual SMB setup knowledge.
 */
export function generateEasyWorshipSmbSetupGuide(watchDir: string): string {
  const shareName = "AletheiaFeed";
  const localPath = watchDir || "C:\\Aletheia\\EasyWorship";
  return [
    "# EasyWorship Cross-Machine Share Setup",
    "",
    "## Step 1 — On the Aletheia (this) computer",
    `1. Create the folder: ${localPath}`,
    `2. Right-click the folder → Properties → Sharing → Advanced Sharing.`,
    `3. Tick 'Share this folder', set Share Name to: ${shareName}`,
    "4. Click Permissions → Add → Everyone → Full Control.",
    "5. Click OK on all dialogs.",
    "",
    "## Step 2 — On the EasyWorship computer",
    "1. Press Win+R, type: \\\\<ALETHEIA_IP>\\" + shareName,
    "   (Replace <ALETHEIA_IP> with this machine's IP, e.g. 192.168.1.50)",
    "2. Right-click the share → Map network drive → Choose a drive letter (e.g. Z:).",
    "3. Tick 'Reconnect at sign-in'. Click Finish.",
    "",
    "## Step 3 — In Aletheia",
    `1. Open Integrations → EasyWorship.`,
    `2. Set Watch Folder to the UNC path: \\\\<ALETHEIA_IP>\\${shareName}`,
    "   OR use the mapped drive letter: Z:\\\\",
    "3. Click Save Configuration.",
    "4. Click Check EasyWorship — status should show 'ready'.",
    "",
    "## Step 4 — In EasyWorship",
    "1. Go to Schedule → Add Item → Media File.",
    "2. Browse to the mapped drive and select NowPlaying.txt.",
    "3. EasyWorship will display the verse whenever Aletheia writes to it.",
    "",
    "## Troubleshooting",
    "- If Check EasyWorship shows 'offline', verify Windows Firewall allows File & Printer Sharing.",
    "- Both machines must be on the same LAN or have a VPN bridge.",
    "- Disable password-protected sharing in Network & Sharing Center if prompted for credentials.",
  ].join("\n");
}

/**
 * Returns a step-by-step guide for creating the vMix GT title input file
 * named 'Aletheia Scripture.gtzip' so the API SetText calls work.
 * Issue 5 fix: explains what to do when vMix says 'title input not found'.
 */
export function getVmixTitleSetupGuide(verseField = "Headline.Text", referenceField = "Description.Text"): string {
  return [
    "# vMix GT Title Setup Guide",
    "",
    "Aletheia sends text to a vMix title input. If Check vMix says 'API reachable but",
    "title input not found', follow these steps:",
    "",
    "## Option A — Use a Built-In GT Title (Quickest)",
    "1. In vMix, click Add Input → Title / XAML.",
    "2. Choose any Lower Third template (e.g. 'Lower Third 1').",
    "3. Click Add and rename the input to exactly: Aletheia Scripture",
    "   (Right-click the input thumbnail → Edit Title → change the top field to",
    "    'Aletheia Scripture').",
    `4. Note the exact field names — in Aletheia → Integrations → vMix, set:`,
    `   - Verse Field: ${verseField}`,
    `   - Reference Field: ${referenceField}`,
    "5. Click Check vMix — should now show 'connected'.",
    "",
    "## Option B — Create a Custom XAML Title",
    "1. In vMix click Add Input → Title/XAML → Browse for an existing .gtzip.",
    "2. OR use GT Designer (bundled with vMix) to create a 2-field lower third:",
    `   - Field 1 name: ${verseField.split(".")[0]}  (verse text, large font)`,
    `   - Field 2 name: ${referenceField.split(".")[0]}  (reference + translation, smaller)`,
    "3. Save as 'Aletheia Scripture.gtzip' and load it into vMix.",
    "",
    "## Setting vMix to Allow Remote API",
    "1. vMix → Settings → Web Controller → tick 'Enable'.",
    "2. Note the port (default 8088).",
    "3. In Aletheia → Integrations → vMix, set Host to the vMix machine IP.",
    "4. If vMix is on the same LAN, leave 'Allow Private Network' OFF for safety.",
    "   Only enable it if vMix is on a private subnet (192.168.x.x / 10.x.x.x).",
  ].join("\n");
}


/** Stop live capture, drop the cpal stream, and release the Whisper model. */
export async function stopAudioCapture(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invokeWithTimeout<void>("stop_audio_capture");
}

// ---------------------------------------------------------------------------
// On-demand verse fetch — API.Bible fallback for references missing from the
// local seeded DB. Uses the free public KJV bible (no API key required).
// ---------------------------------------------------------------------------

type ApiBiblePassageResponse = {
  data?: {
    content?: string;
    reference?: string;
  };
};

/**
 * Converts a human reference (e.g. "Habakkuk 3:17") to an API.Bible passage
 * ID format (e.g. "HAB.3.17") for the KJV public bible.
 *
 * Only handles the common single-verse pattern; returns null for ranges.
 */
function referenceToApiBibleId(reference: string): string | null {
  const bookMap: Record<string, string> = {
    genesis: "GEN", exodus: "EXO", leviticus: "LEV", numbers: "NUM",
    deuteronomy: "DEU", joshua: "JOS", judges: "JDG", ruth: "RUT",
    "1 samuel": "1SA", "2 samuel": "2SA", "1 kings": "1KI", "2 kings": "2KI",
    "1 chronicles": "1CH", "2 chronicles": "2CH", ezra: "EZR", nehemiah: "NEH",
    esther: "EST", job: "JOB", psalms: "PSA", psalm: "PSA", proverbs: "PRO",
    ecclesiastes: "ECC", "song of solomon": "SNG", "song of songs": "SNG",
    isaiah: "ISA", jeremiah: "JER", lamentations: "LAM", ezekiel: "EZK",
    daniel: "DAN", hosea: "HOS", joel: "JOL", amos: "AMO", obadiah: "OBA",
    jonah: "JON", micah: "MIC", nahum: "NAM", habakkuk: "HAB", zephaniah: "ZEP",
    haggai: "HAG", zechariah: "ZEC", malachi: "MAL",
    matthew: "MAT", mark: "MRK", luke: "LUK", john: "JHN", acts: "ACT",
    romans: "ROM", "1 corinthians": "1CO", "2 corinthians": "2CO",
    galatians: "GAL", ephesians: "EPH", philippians: "PHP", colossians: "COL",
    "1 thessalonians": "1TH", "2 thessalonians": "2TH", "1 timothy": "1TI",
    "2 timothy": "2TI", titus: "TIT", philemon: "PHM", hebrews: "HEB",
    james: "JAS", "1 peter": "1PE", "2 peter": "2PE", "1 john": "1JN",
    "2 john": "2JN", "3 john": "3JN", jude: "JUD", revelation: "REV",
  };

  const m = reference.trim().match(/^(.+?)\s+(\d+):(\d+)$/i);
  if (!m) return null;
  const [, bookRaw, chapter, verse] = m;
  const bookKey = bookRaw.toLowerCase();
  const bookCode = bookMap[bookKey];
  if (!bookCode) return null;
  return `${bookCode}.${chapter}.${verse}`;
}

/**
 * Fetch a verse from the free public API.Bible KJV endpoint.
 * Returns null on any error so callers can display a graceful fallback.
 *
 * Does NOT require a paid API key — the public KJV bible ID is:
 *   de4e12af7f28f599-01
 */
export async function fetchVerseFromApiBible(
  reference: string
): Promise<{ text: string; reference: string } | null> {
  const passageId = referenceToApiBibleId(reference);
  if (!passageId) return null;

  const BIBLE_ID = "de4e12af7f28f599-01"; // American King James Version (public)
  const url = `https://api.bible/v1/bibles/${BIBLE_ID}/passages/${passageId}?content-type=text&include-notes=false&include-titles=false&include-chapter-numbers=false&include-verse-numbers=false&include-verse-spans=false`;

  try {
    const resp = await fetch(url, {
      headers: { "api-key": "no-key-required-for-public" },
      signal: AbortSignal.timeout(5000),
    });
    if (!resp.ok) return null;
    const json = (await resp.json()) as ApiBiblePassageResponse;
    const raw = json?.data?.content ?? "";
    // Strip HTML tags the API may return
    const text = raw.replace(/<[^>]+>/g, "").trim();
    if (!text) return null;
    return { text, reference: json?.data?.reference ?? reference };
  } catch {
    return null;
  }
}

/**
 * Try local Rust search first; if the result is empty or a placeholder,
 * fall back to API.Bible for the specific reference.
 */
export async function fetchVerseOnDemand(
  reference: string
): Promise<{ text: string; reference: string; source: "local" | "api.bible" } | null> {
  // 1. Try local DB
  try {
    const results = await invokeWithTimeout<Array<{ reference: string; text: string }>>(
      "search_scripture",
      { query: reference },
      5000
    );
    const hit = results?.find(
      (r) => r.reference?.toLowerCase().trim() === reference.toLowerCase().trim()
    );
    if (hit?.text && hit.text.length > 4) {
      return { text: hit.text, reference: hit.reference, source: "local" };
    }
  } catch {
    // local lookup failed — continue to API.Bible
  }

  // 2. API.Bible online fallback
  const online = await fetchVerseFromApiBible(reference);
  if (online) return { ...online, source: "api.bible" };

  return null;
}


export async function getDesktopServiceState(): Promise<DesktopServiceState> {
  if (!isTauriRuntime()) return fallbackServiceState();

  try {
    const state = await invokeWithTimeout<RawDesktopServiceState>("get_service_state");
    return normalizeServiceState(state);
  } catch (error) {
    console.warn("Aletheia desktop state unavailable, using browser fallback", error);
    return fallbackServiceState();
  }
}

export async function searchScripture(query: string): Promise<ManualSearchResult[]> {
  if (!isTauriRuntime()) return fallbackSearch(query);

  try {
    return await invokeWithTimeout<ManualSearchResult[]>("search_scripture", { query });
  } catch (error) {
    console.warn("Aletheia local scripture search failed, using browser fallback", error);
    return fallbackSearch(query);
  }
}

export async function renderPreviewScene(candidate: ScriptureCandidate): Promise<OutputScene> {
  if (!isTauriRuntime()) return fallbackScene(candidate);
  return invokeWithTimeout<OutputScene>("render_preview", { candidate });
}

export async function setDestinationsArmed(armed: boolean): Promise<DesktopRuntimeStatus> {
  if (!isTauriRuntime()) {
    return { ...fallbackRuntimeStatus(), destinationsArmed: armed, checkedAtMs: Date.now() };
  }
  // Rust returns void; re-fetch the full state after the mutation.
  await invokeWithTimeout<void>("set_destinations_armed", { armed });
  const updated = await invokeWithTimeout<RawDesktopServiceState>("get_service_state");
  return normalizeSession(updated.session);
}

export async function setDataMiser(enabled: boolean): Promise<DesktopRuntimeStatus> {
  if (!isTauriRuntime()) {
    return { ...fallbackRuntimeStatus(), dataMiserEnabled: enabled, checkedAtMs: Date.now() };
  }
  // Rust returns void; re-fetch the full state after the mutation.
  await invokeWithTimeout<void>("set_data_miser", { enabled });
  const updated = await invokeWithTimeout<RawDesktopServiceState>("get_service_state");
  return normalizeSession(updated.session);
}

export async function sendLiveCandidate(candidate: ScriptureCandidate, destinationsArmed: boolean): Promise<LiveOutputResult> {
  if (!destinationsArmed) throw new Error("Live output is blocked until destinations are armed.");

  if (!isTauriRuntime()) {
    return {
      scene: fallbackScene(candidate),
      auditCount: fallbackRuntimeStatus().auditCount + 1
    };
  }

  return invokeWithTimeout<LiveOutputResult>("send_live", { candidate, destinationsArmed });
}

export async function runPreServiceCheck(): Promise<HealthItem[]> {
  if (!isTauriRuntime()) return healthItems;

  try {
    return await invokeWithTimeout<HealthItem[]>("run_pre_service_check");
  } catch (error) {
    console.warn("Aletheia health check failed, using browser fallback", error);
    return healthItems;
  }
}

export async function analyzeTranscript(): Promise<AiDetectionResult> {
  if (!isTauriRuntime()) return fallbackAiDetection();

  try {
    return await invokeWithTimeout<AiDetectionResult>("analyze_transcript", undefined, SLOW_OP_TIMEOUT_MS);
  } catch (error) {
    console.warn("Aletheia AI assist failed, using browser fallback", error);
    return fallbackAiDetection();
  }
}

export async function getVmixStatus(): Promise<VmixStatus> {
  if (!isTauriRuntime()) return fallbackVmixStatus();

  try {
    return await invokeWithTimeout<VmixStatus>("get_vmix_status");
  } catch (error) {
    console.warn("Aletheia vMix status failed, using browser fallback", error);
    return fallbackVmixStatus();
  }
}

export async function getVmixConfig(): Promise<VmixConfig> {
  if (!isTauriRuntime()) return fallbackVmixConfig();

  try {
    return await invokeWithTimeout<VmixConfig>("get_vmix_config");
  } catch (error) {
    console.warn("Aletheia vMix config failed, using browser fallback", error);
    return fallbackVmixConfig();
  }
}

export async function updateVmixConfig(config: VmixConfig): Promise<VmixStatus> {
  if (!isTauriRuntime()) {
    return {
      ...config,
      state: "offline",
      detail: "Browser fallback cannot persist vMix settings. Open the Tauri desktop app for local configuration.",
      endpoint: `http://${config.host}:${config.port}/api/`,
      checkedAtMs: Date.now()
    };
  }

  return invokeWithTimeout<VmixStatus>("update_vmix_config", { config });
}

export async function getRecentIntegrationEvents(): Promise<IntegrationEvent[]> {
  if (!isTauriRuntime()) return [];

  try {
    return await invokeWithTimeout<IntegrationEvent[]>("get_recent_integration_events");
  } catch (error) {
    console.warn("Aletheia integration events failed", error);
    return [];
  }
}

export async function getProductionReadiness(): Promise<ProductionReadinessReport> {
  if (!isTauriRuntime()) return fallbackProductionReadiness();

  try {
    return await invokeWithTimeout<ProductionReadinessReport>("get_production_readiness");
  } catch (error) {
    console.warn("Aletheia production readiness unavailable, using browser fallback", error);
    return fallbackProductionReadiness();
  }
}

export async function installOfflineAsset(assetId: string): Promise<ProductionReadinessReport> {
  if (!isTauriRuntime()) {
    return applyOfflineAssetInstall(fallbackProductionReadiness(), assetId);
  }

  try {
    // Rust param is `asset_id` (snake_case) — Tauri maps camelCase `assetId` → `asset_id`
    await invokeWithTimeout<void>("install_offline_asset", { assetId });
    return await getProductionReadiness();
  } catch (error) {
    console.warn("Aletheia offline asset install failed", error);
    return getProductionReadiness();
  }
}

export async function installOfflineAssetFromPath(
  assetId: string,
  filePath: string,
  expectedChecksum: string
): Promise<ProductionReadinessReport> {
  if (!isTauriRuntime()) {
    return applyOfflineAssetInstall(fallbackProductionReadiness(), assetId);
  }

  try {
    // Rust params: file_path, asset_id, expected_checksum (Tauri camelCase→snake_case)
    await invokeWithTimeout<void>("install_offline_asset_from_path", {
      filePath,
      assetId,
      expectedChecksum
    });
    return await getProductionReadiness();
  } catch (error) {
    console.warn("Aletheia offline asset install from path failed", error);
    return getProductionReadiness();
  }
}

export async function recordDeviceAcceptance(
  deviceId: string,
  stepLabel: string,
  passed: boolean,
  note?: string,
  evidencePath?: string
): Promise<ProductionReadinessReport> {
  if (!isTauriRuntime()) return fallbackProductionReadiness();

  try {
    await invokeWithTimeout<void>("record_device_acceptance", {
      deviceId,
      stepLabel,
      passed,
      note,
      evidencePath
    });
    return await getProductionReadiness();
  } catch (error) {
    console.warn("Aletheia device acceptance recording failed", error);
    return getProductionReadiness();
  }
}

export async function verifyPluginManifest(
  manifestPath: string,
  trustedKeyIds: string[]
): Promise<PluginVerificationResult> {
  if (!isTauriRuntime()) {
    return {
      state: "rejected",
      detail: "Plugin verification requires the Tauri desktop runtime."
    };
  }

  try {
    // Rust reads the file from disk; frontend passes the path + trusted key IDs.
    return await invokeWithTimeout<PluginVerificationResult>("verify_plugin_manifest", {
      manifestPath,
      trustedKeyIds
    });
  } catch (error) {
    console.warn("Aletheia plugin manifest verification failed", error);
    return {
      state: "rejected",
      detail: error instanceof Error ? error.message : "Plugin verification failed."
    };
  }
}

export async function enablePluginManifest(
  manifestPath: string,
  trustedKeyIds: string[]
): Promise<PluginVerificationResult> {
  if (!isTauriRuntime()) {
    return {
      state: "rejected",
      detail: "Plugin enablement requires the Tauri desktop runtime."
    };
  }

  try {
    // Rust verifies + persists the plugin; returns verified/rejected state.
    return await invokeWithTimeout<PluginVerificationResult>("enable_plugin_manifest", {
      manifestPath,
      trustedKeyIds
    });
  } catch (error) {
    console.warn("Aletheia plugin enablement failed", error);
    return {
      state: "rejected",
      detail: error instanceof Error ? error.message : "Plugin enablement failed."
    };
  }
}

export async function runLocalRehearsal(): Promise<LocalRehearsalReport> {
  if (!isTauriRuntime()) return fallbackLocalRehearsal();

  try {
    return await invokeWithTimeout<LocalRehearsalReport>("run_local_rehearsal", undefined, SLOW_OP_TIMEOUT_MS);
  } catch (error) {
    console.warn("Aletheia local rehearsal failed, using browser fallback", error);
    return fallbackLocalRehearsal();
  }
}

export async function exportSupportBundle(includeTranscriptText = false): Promise<SupportBundleExport> {
  if (!isTauriRuntime()) {
    return {
      path: "Browser fallback: open the Tauri desktop app to export a support bundle.",
      sizeBytes: 0,
      redactionSummary: {
        emails: 0,
        ipAddresses: 0,
        windowsPaths: 0,
        unixPaths: 0,
        secretLikeValues: 0
      },
      includedFiles: []
    };
  }

  return invokeWithTimeout<SupportBundleExport>("export_support_bundle", { includeTranscriptText }, SLOW_OP_TIMEOUT_MS);
}

export async function exportOfflineAssetPack(targetDir: string): Promise<OfflinePackExport> {
  if (!isTauriRuntime()) {
    return {
      path: "Browser fallback: open the Tauri desktop app to export an offline asset pack.",
      manifestPath: "manifest.json",
      checksumPath: "checksums.json",
      assetCount: 0,
      bytesWritten: 0
    };
  }

  return invokeWithTimeout<OfflinePackExport>("export_offline_asset_pack", { targetDir }, SLOW_OP_TIMEOUT_MS);
}

export async function exportBoothPack(): Promise<BoothPackExport> {
  if (!isTauriRuntime()) {
    return {
      path: "Browser fallback: open the Tauri desktop app to write a booth compatibility pack.",
      generatedAtMs: Date.now(),
      files: [
        "obs/aletheia-browser-source.html",
        "easyworship/current-verse.txt",
        "vmix/setup.md"
      ]
    };
  }

  return invokeWithTimeout<BoothPackExport>("export_booth_pack");
}

export async function sendVmixPreview(candidate: ScriptureCandidate): Promise<VmixDispatchResult> {
  if (!isTauriRuntime()) {
    return {
      state: "offline",
      detail: "Browser fallback cannot send to vMix. Open the Tauri desktop app for local HTTP control.",
      reference: candidate.reference,
      auditCount: fallbackRuntimeStatus().auditCount
    };
  }

  return invokeWithTimeout<VmixDispatchResult>("send_vmix_preview", { candidate });
}

export async function sendVmixLive(candidate: ScriptureCandidate, destinationsArmed: boolean): Promise<VmixDispatchResult> {
  if (!destinationsArmed) throw new Error("vMix live output is blocked until destinations are armed.");

  if (!isTauriRuntime()) {
    return {
      state: "offline",
      detail: "Browser fallback cannot send to vMix. Open the Tauri desktop app for local HTTP control.",
      reference: candidate.reference,
      auditCount: fallbackRuntimeStatus().auditCount
    };
  }

  return invokeWithTimeout<VmixDispatchResult>("send_vmix_live", { candidate, destinationsArmed });
}

export async function clearVmixOverlay(): Promise<VmixDispatchResult> {
  if (!isTauriRuntime()) {
    return {
      state: "offline",
      detail: "Browser fallback cannot clear vMix. Open the Tauri desktop app for local HTTP control.",
      reference: "vMix overlay",
      auditCount: fallbackRuntimeStatus().auditCount
    };
  }

  return invokeWithTimeout<VmixDispatchResult>("clear_vmix_overlay");
}

// ---------------------------------------------------------------------------
// Service profile commands
// ---------------------------------------------------------------------------

export async function saveServiceProfile(
  id: string,
  name: string,
  languages: string[],
  outputPolicy: string
): Promise<ServiceProfile> {
  if (!isTauriRuntime()) {
    return fallbackServiceProfile(id, name, languages, outputPolicy);
  }
  // Rust expects a full ServiceProfileDto struct — wrap in `profile` key and supply all required fields.
  return invokeWithTimeout<ServiceProfile>("save_service_profile", {
    profile: {
      id,
      name,
      languages,
      outputPolicy,
      isActive: true,
      createdAtMs: Date.now(),
      updatedAtMs: Date.now(),
    }
  });
}

export async function listServiceProfiles(): Promise<ServiceProfile[]> {
  if (!isTauriRuntime()) return [];
  try {
    return await invokeWithTimeout<ServiceProfile[]>("list_service_profiles");
  } catch (error) {
    console.warn("Aletheia list service profiles failed", error);
    return [];
  }
}

export async function setActiveServiceProfile(id: string): Promise<ServiceProfile> {
  if (!isTauriRuntime()) {
    return fallbackServiceProfile(id, id, [], "manual-live");
  }
  return invokeWithTimeout<ServiceProfile>("set_active_service_profile", { id });
}

export async function deleteServiceProfile(id: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try {
    return await invokeWithTimeout<boolean>("delete_service_profile", { id });
  } catch (error) {
    console.warn("Aletheia delete service profile failed", error);
    return false;
  }
}

// ---------------------------------------------------------------------------
// OBS WebSocket v5 commands
// ---------------------------------------------------------------------------

export async function getObsStatus(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("obs");
  try {
    return await invokeWithTimeout<AdapterDispatchResult>("get_obs_status");
  } catch (error) {
    console.warn("Aletheia OBS status failed", error);
    return fallbackAdapterResult("obs");
  }
}

export async function updateObsConfig(config: ObsConfig): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("obs");
  return invokeWithTimeout<AdapterDispatchResult>("update_obs_config", { config });
}

export async function sendObsPreview(candidate: ScriptureCandidate): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("obs");
  return invokeWithTimeout<AdapterDispatchResult>("send_obs_preview", { candidate });
}

export async function sendObsLive(
  candidate: ScriptureCandidate,
  destinationsArmed: boolean
): Promise<AdapterDispatchResult> {
  if (!destinationsArmed) throw new Error("OBS live output is blocked until destinations are armed.");
  if (!isTauriRuntime()) return fallbackAdapterResult("obs");
  return invokeWithTimeout<AdapterDispatchResult>("send_obs_live", { candidate, destinationsArmed });
}

export async function clearObsOutput(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("obs");
  return invokeWithTimeout<AdapterDispatchResult>("clear_obs_output");
}

// ---------------------------------------------------------------------------
// ProPresenter 7+ REST commands
// ---------------------------------------------------------------------------

export async function getProPresenterStatus(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("propresenter");
  try {
    return await invokeWithTimeout<AdapterDispatchResult>("get_propresenter_status");
  } catch (error) {
    console.warn("Aletheia ProPresenter status failed", error);
    return fallbackAdapterResult("propresenter");
  }
}

export async function getProPresenterConfig(): Promise<ProPresenterConfig | null> {
  if (!isTauriRuntime()) return null;
  try {
    return await invokeWithTimeout<ProPresenterConfig>("get_propresenter_config");
  } catch (error) {
    console.warn("Aletheia ProPresenter config read failed", error);
    return null;
  }
}

export async function updateProPresenterConfig(
  config: ProPresenterConfig
): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("propresenter");
  return invokeWithTimeout<AdapterDispatchResult>("update_propresenter_config", { config });
}

export async function sendProPresenterPreview(
  candidate: ScriptureCandidate
): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("propresenter");
  return invokeWithTimeout<AdapterDispatchResult>("send_propresenter_preview", { candidate });
}

export async function sendProPresenterLive(
  candidate: ScriptureCandidate,
  destinationsArmed: boolean
): Promise<AdapterDispatchResult> {
  if (!destinationsArmed)
    throw new Error("ProPresenter live output is blocked until destinations are armed.");
  if (!isTauriRuntime()) return fallbackAdapterResult("propresenter");
  return invokeWithTimeout<AdapterDispatchResult>("send_propresenter_live", {
    candidate,
    destinationsArmed
  });
}

export async function clearProPresenterOutput(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("propresenter");
  return invokeWithTimeout<AdapterDispatchResult>("clear_propresenter_output");
}

// ---------------------------------------------------------------------------
// Bitfocus Companion HTTP control commands
// ---------------------------------------------------------------------------

export async function getCompanionStatus(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("companion");
  try {
    return await invokeWithTimeout<AdapterDispatchResult>("get_companion_status");
  } catch (error) {
    console.warn("Aletheia Companion status failed", error);
    return fallbackAdapterResult("companion");
  }
}

export async function getCompanionConfig(): Promise<CompanionConfig | null> {
  if (!isTauriRuntime()) return null;
  try {
    return await invokeWithTimeout<CompanionConfig>("get_companion_config");
  } catch (error) {
    console.warn("Aletheia Companion config read failed", error);
    return null;
  }
}

export async function updateCompanionConfig(
  config: CompanionConfig
): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("companion");
  return invokeWithTimeout<AdapterDispatchResult>("update_companion_config", { config });
}

export async function sendCompanionPreview(
  candidate: ScriptureCandidate
): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("companion");
  return invokeWithTimeout<AdapterDispatchResult>("send_companion_preview", { candidate });
}

export async function sendCompanionLive(
  candidate: ScriptureCandidate,
  destinationsArmed: boolean
): Promise<AdapterDispatchResult> {
  if (!destinationsArmed)
    throw new Error("Companion live output is blocked until destinations are armed.");
  if (!isTauriRuntime()) return fallbackAdapterResult("companion");
  return invokeWithTimeout<AdapterDispatchResult>("send_companion_live", { candidate, destinationsArmed });
}

export async function clearCompanionOutput(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("companion");
  return invokeWithTimeout<AdapterDispatchResult>("clear_companion_output");
}

// ---------------------------------------------------------------------------
// OSC 1.0 UDP commands
// ---------------------------------------------------------------------------

export async function getOscStatus(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  try {
    return await invokeWithTimeout<AdapterDispatchResult>("get_osc_status");
  } catch (error) {
    console.warn("Aletheia OSC status failed", error);
    return fallbackAdapterResult("osc");
  }
}

export async function updateOscConfig(config: OscConfig): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  return invokeWithTimeout<AdapterDispatchResult>("update_osc_config", { config });
}

export async function sendOscPreview(candidate: ScriptureCandidate): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  return invokeWithTimeout<AdapterDispatchResult>("send_osc_preview", { candidate });
}

export async function sendOscLive(
  candidate: ScriptureCandidate,
  destinationsArmed: boolean
): Promise<AdapterDispatchResult> {
  if (!destinationsArmed) throw new Error("OSC live output is blocked until destinations are armed.");
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  return invokeWithTimeout<AdapterDispatchResult>("send_osc_live", { candidate, destinationsArmed });
}

export async function clearOscOutput(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  return invokeWithTimeout<AdapterDispatchResult>("clear_osc_output");
}

/** Sends a diagnostic `/aletheia/ping` packet to the configured OSC host. */
export async function sendOscTestPing(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  return invokeWithTimeout<AdapterDispatchResult>("send_osc_test_ping");
}

// ---------------------------------------------------------------------------
// EasyWorship watch-folder commands
// ---------------------------------------------------------------------------

export async function getEasyWorshipStatus(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("easyworship");
  try {
    return await invokeWithTimeout<AdapterDispatchResult>("get_easyworship_status");
  } catch (error) {
    console.warn("Aletheia EasyWorship status failed", error);
    return fallbackAdapterResult("easyworship");
  }
}

export async function updateEasyWorshipConfig(config: EasyWorshipConfig): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("easyworship");
  return invokeWithTimeout<AdapterDispatchResult>("update_easyworship_config", { config });
}

export async function sendEasyWorshipPreview(candidate: ScriptureCandidate): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("easyworship");
  return invokeWithTimeout<AdapterDispatchResult>("send_easyworship_preview", { candidate });
}

export async function sendEasyWorshipLive(
  candidate: ScriptureCandidate,
  destinationsArmed: boolean
): Promise<AdapterDispatchResult> {
  if (!destinationsArmed) throw new Error("EasyWorship live output is blocked until destinations are armed.");
  if (!isTauriRuntime()) return fallbackAdapterResult("easyworship");
  return invokeWithTimeout<AdapterDispatchResult>("send_easyworship_live", { candidate, destinationsArmed });
}

export async function clearEasyWorshipOutput(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("easyworship");
  return invokeWithTimeout<AdapterDispatchResult>("clear_easyworship_output");
}

// ---------------------------------------------------------------------------
// Trusted plugin registry (v6)
// ---------------------------------------------------------------------------

/** Returns all plugins in the trust registry (enabled or revoked). */
export async function listTrustedPlugins(): Promise<TrustedPlugin[]> {
  if (!isTauriRuntime()) return [];
  return invokeWithTimeout<TrustedPlugin[]>("list_trusted_plugins");
}

/**
 * Permanently removes a plugin from the trust registry.
 * Returns `true` if the plugin was found and removed.
 */
export async function revokeTrustedPlugin(pluginId: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  return invokeWithTimeout<boolean>("revoke_trusted_plugin", { pluginId });
}

// ---------------------------------------------------------------------------
// Calibration dataset pipeline (v6)
// ---------------------------------------------------------------------------

/**
 * Records one operator-confirmed calibration sample.
 *
 * @param outcome  `"confirmed"` | `"corrected"` | `"rejected"`
 * @returns        The row id assigned by SQLite (used for cross-referencing).
 */
export async function recordCalibrationSample(
  language: string,
  transcriptText: string,
  expectedRef: string | null,
  outcome: "confirmed" | "corrected" | "rejected",
  detectedRef: string | null
): Promise<void> {
  if (!isTauriRuntime()) return;
  await invokeWithTimeout<void>("record_calibration_sample", {
    language,
    transcriptText,
    expectedRef,
    outcome,
    detectedRef,
  });
}

/** Returns aggregate accuracy statistics from stored calibration samples. */
export async function getCalibrationReport(): Promise<CalibrationReport> {
  if (!isTauriRuntime()) {
    return { confirmed: 0, corrected: 0, rejected: 0, total: 0, precision: null };
  }
  return invokeWithTimeout<CalibrationReport>("get_calibration_report");
}

// ---------------------------------------------------------------------------
// Tauri event subscriptions (live-updated, armed-changed)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tauri event payload types
// ---------------------------------------------------------------------------

/** Payload for `aletheia://live-updated` events emitted by the Rust backend. */
export type LiveUpdatedPayload = {
  reference: string;
  translation: string;
  status: string;
  auditCount: number;
};

/**
 * Payload for `aletheia://armed-changed` events emitted by the Rust backend.
 * The key `destinationsArmed` matches the serde_json literal in lib.rs.
 */
export type ArmedChangedPayload = {
  destinationsArmed: boolean;
};

// ---------------------------------------------------------------------------
// Tauri event subscriptions (live-updated, armed-changed)
// ---------------------------------------------------------------------------

/**
 * Subscribe to the `aletheia://live-updated` event emitted by the Rust backend
 * when a live send completes. Returns a Promise that resolves to an unlisten
 * function; call it in a useEffect cleanup to avoid memory leaks.
 */
export async function onLiveUpdated(
  callback: (payload: LiveUpdatedPayload) => void
): Promise<() => void> {
  if (!isTauriRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<LiveUpdatedPayload>(
    "aletheia://live-updated",
    (event) => callback(event.payload)
  );
}

/**
 * Subscribe to the `aletheia://armed-changed` event emitted by the Rust backend
 * when destinations arming state changes. Returns a Promise that resolves to an
 * unlisten function.
 */
export async function onArmedChanged(
  callback: (payload: ArmedChangedPayload) => void
): Promise<() => void> {
  if (!isTauriRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<ArmedChangedPayload>(
    "aletheia://armed-changed",
    (event) => callback(event.payload)
  );
}

/**
 * Subscribe to `aletheia://transcript-segment` events emitted by the live STT
 * inference task. Each payload is a fully-formed TranscriptSegment ready to
 * render — the React caller should prepend it to the transcript state list and
 * cap the visible window to whatever the UI prefers.
 */
export async function onTranscriptSegment(
  callback: (payload: TranscriptSegment) => void
): Promise<() => void> {
  if (!isTauriRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<TranscriptSegment>(
    "aletheia://transcript-segment",
    (event) => callback(event.payload)
  );
}

/**
 * Subscribe to `aletheia://candidates-updated` events emitted by the live
 * scripture detector. Each payload is the latest ranked candidate set after a
 * new transcript segment has been processed.
 */
export async function onCandidatesUpdated(
  callback: (payload: ScriptureCandidate[]) => void
): Promise<() => void> {
  if (!isTauriRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<ScriptureCandidate[]>(
    "aletheia://candidates-updated",
    (event) => callback(event.payload)
  );
}

type RawDesktopServiceState = {
  session: DesktopSession;
  transcript: TranscriptSegment[];
  candidates: ScriptureCandidate[];
  integrations: Integration[];
  health: HealthItem[];
  preview: ScriptureCandidate;
  live: ScriptureCandidate;
};

function normalizeServiceState(state: RawDesktopServiceState): DesktopServiceState {
  return {
    ...state,
    session: normalizeSession(state.session)
  };
}

function normalizeSession(session: DesktopSession): DesktopRuntimeStatus {
  return {
    mode: session.mode,
    serviceSession: session.name || session.id,
    databasePath: session.databasePath,
    dataMiserEnabled: session.dataMiserEnabled,
    offlineModeEnabled: session.offlineModeEnabled,
    destinationsArmed: session.destinationsArmed,
    auditCount: session.auditCount,
    lastEventSequence: session.lastEventSequence,
    checkedAtMs: session.checkedAtMs
  };
}

function fallbackServiceState(): DesktopServiceState {
  return {
    session: fallbackRuntimeStatus(),
    transcript: transcriptSegments,
    candidates: scriptureCandidates,
    integrations,
    health: healthItems,
    preview: scriptureCandidates[0],
    live: scriptureCandidates[2]
  };
}

function fallbackRuntimeStatus(): DesktopRuntimeStatus {
  return {
    mode: "browser-fallback",
    serviceSession: "Browser demo service",
    databasePath: "Demo data in memory",
    dataMiserEnabled: true,
    offlineModeEnabled: true,
    destinationsArmed: true,
    auditCount: 0,
    lastEventSequence: 0,
    checkedAtMs: Date.now()
  };
}

function fallbackSearch(query: string): ManualSearchResult[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return manualSearchResults;

  return manualSearchResults.filter((result) =>
    `${result.reference} ${result.snippet} ${result.source}`.toLowerCase().includes(normalizedQuery)
  );
}

function fallbackScene(candidate: ScriptureCandidate): OutputScene {
  return {
    id: `scene-${candidate.id}`,
    reference: candidate.reference,
    translation: candidate.translation,
    themeId: "broadcast-lower",
    layers: [
      { layer: "verse", text: candidate.text, visible: true },
      { layer: "reference", text: `${candidate.reference} ${candidate.translation}`, visible: true },
      { layer: "contextCard", text: "Manual live required", visible: false }
    ]
  };
}


function fallbackVmixConfig(): VmixConfig {
  return {
    host: "127.0.0.1",
    port: 8088,
    titleInput: "Aletheia Scripture.gtzip",
    verseField: "Headline.Text",
    referenceField: "Description.Text",
    overlayChannel: 2,
    allowPrivateNetwork: false,
    username: "",
    password: ""
  };
}

function fallbackAiDetection(): AiDetectionResult {
  return {
    mode: "browser fallback local assist",
    decisionPolicy: "AI suggestions can enter preview, but live output remains manual.",
    processedSegments: transcriptSegments.length,
    candidates: [
      {
        id: "romans-8-28-ai",
        reference: "Romans 8:28",
        translation: "KJV",
        language: "English",
        text: "And we know that all things work together for good to them that love God.",
        confidence: 95,
        source: "Local AI assist",
        reason: "Matched spoken reference and quoted phrase in the latest transcript.",
        status: "preview"
      },
      {
        id: "isaiah-40-31-ai",
        reference: "Isaiah 40:31",
        translation: "KJV",
        language: "English",
        text: "They that wait upon the LORD shall renew their strength.",
        confidence: 95,
        source: "Local AI assist",
        reason: "Matched spoken reference in sermon context. Operator review required before live.",
        status: "new"
      }
    ],
    adapters: [
      {
        id: "vad-local",
        name: "Hybrid VAD",
        mode: "local",
        state: "ready",
        detail: "Speech gating is local and does not need internet.",
        latencyMs: 35
      },
      {
        id: "stt-local",
        name: "Offline Whisper adapter",
        mode: "local",
        state: "ready",
        detail: "Transcript segments feed scripture detection without cloud calls.",
        latencyMs: 460
      },
      {
        id: "cloud-enhance",
        name: "Cloud enhancement",
        mode: "optional",
        state: "offline",
        detail: "Data Miser keeps cloud reranking disabled until the operator allows it.",
        latencyMs: 0
      }
    ],
    languages: [
      { code: "en", name: "English", confidence: 92, matchedTerms: ["romans", "chapter", "verse"] },
      { code: "yo", name: "Yoruba", confidence: 86, matchedTerms: ["olorun", "awon"] },
      { code: "ha", name: "Hausa", confidence: 82, matchedTerms: ["romawa"] },
      { code: "tw", name: "Twi", confidence: 80, matchedTerms: ["romafo"] },
      { code: "sw", name: "Swahili", confidence: 82, matchedTerms: ["warumi"] },
      { code: "xh", name: "Xhosa", confidence: 78, matchedTerms: ["kwabaseroma"] },
      { code: "es", name: "Spanish", confidence: 82, matchedTerms: ["romanos"] },
      { code: "fr", name: "French", confidence: 82, matchedTerms: ["romains"] }
    ],
    supportedLanguages: defaultSupportedLanguages(),
    accuracyTarget: defaultAccuracyTarget(),
    checkedAtMs: Date.now()
  };
}

function fallbackVmixStatus(): VmixStatus {
  const config = fallbackVmixConfig();
  return {
    ...config,
    state: "offline",
    detail: "vMix control is available in the desktop app. Default target is loopback HTTP on port 8088.",
    endpoint: "http://127.0.0.1:8088/api/",
    checkedAtMs: Date.now()
  };
}

function defaultSupportedLanguages() {
  return [
    languagePack("en", "English", "en", true),
    languagePack("yo", "Yoruba", "yo", false),
    languagePack("ig", "Igbo", "ig", false),
    languagePack("ha", "Hausa", "ha", false),
    languagePack("tw", "Twi", "ak", false),
    languagePack("sw", "Swahili", "sw", false),
    languagePack("xh", "Xhosa", "xh", false),
    languagePack("es", "Spanish", "es", false),
    languagePack("fr", "French", "fr", false)
  ];
}

function languagePack(code: string, name: string, sttLocale: string, offlineSttReady: boolean) {
  return {
    code,
    name,
    sttLocale,
    scriptureAliasesReady: true,
    offlineSttReady,
    cloudSttReady: true
  };
}

function defaultAccuracyTarget() {
  return {
    targetPrecision: 95,
    targetRecall: 90,
    autoPreviewThreshold: 95,
    validatedPrecision: 100,
    validatedRecall: 100,
    validationSampleCount: 8,
    liveRequiresOperator: true,
    strategy: [
      "Route transcript segments through language detection before scripture matching.",
      "Fuse exact reference, quotation, alias, service plan, and operator feedback evidence.",
      "Calibrate per-language confidence thresholds with rehearsal data.",
      "Keep cloud reranking optional and never blocking in low-bandwidth mode."
    ]
  };
}

function defaultOfflineAssets() {
  return [
    offlineAsset("bible-kjv", "scripture", "King James Version", "English", "public-domain", "installed", 5, true),
    offlineAsset("bible-web", "scripture", "World English Bible", "English", "public-domain", "installed", 7, true),
    offlineAsset("aliases-yoruba", "scripture-aliases", "Yoruba book aliases", "Yoruba", "internal-index", "installed", 1, true),
    offlineAsset("aliases-igbo", "scripture-aliases", "Igbo book aliases", "Igbo", "internal-index", "installed", 1, true),
    offlineAsset("aliases-hausa", "scripture-aliases", "Hausa book aliases", "Hausa", "internal-index", "installed", 1, true),
    offlineAsset("aliases-twi", "scripture-aliases", "Twi book aliases", "Twi", "internal-index", "installed", 1, true),
    offlineAsset("aliases-swahili", "scripture-aliases", "Swahili book aliases", "Swahili", "internal-index", "installed", 1, true),
    offlineAsset("aliases-xhosa", "scripture-aliases", "Xhosa book aliases", "Xhosa", "internal-index", "installed", 1, true),
    offlineAsset("aliases-spanish", "scripture-aliases", "Spanish book aliases", "Spanish", "internal-index", "installed", 1, true),
    offlineAsset("aliases-french", "scripture-aliases", "French book aliases", "French", "internal-index", "installed", 1, true),
    offlineAsset("stt-whisper-en-small", "stt-model", "Offline English STT small", "English", "operator-provided-model", "installed", 466, true),
    offlineAsset("stt-yoruba-pack", "stt-model", "Offline Yoruba acoustic hints", "Yoruba", "operator-provided-model", "pending", 180, false),
    offlineAsset("stt-hausa-pack", "stt-model", "Offline Hausa acoustic hints", "Hausa", "operator-provided-model", "pending", 180, false),
    offlineAsset("stt-twi-pack", "stt-model", "Offline Twi acoustic hints", "Twi", "operator-provided-model", "pending", 180, false),
    offlineAsset("stt-swahili-pack", "stt-model", "Offline Swahili acoustic hints", "Swahili", "operator-provided-model", "pending", 180, false),
    offlineAsset("stt-xhosa-pack", "stt-model", "Offline Xhosa acoustic hints", "Xhosa", "operator-provided-model", "pending", 180, false),
    offlineAsset("stt-spanish-pack", "stt-model", "Offline Spanish acoustic hints", "Spanish", "operator-provided-model", "pending", 220, false),
    offlineAsset("stt-french-pack", "stt-model", "Offline French acoustic hints", "French", "operator-provided-model", "pending", 220, false)
  ];
}

function offlineAsset(
  id: string,
  kind: string,
  label: string,
  language: string,
  license: string,
  state: string,
  sizeMb: number,
  requiredForRelease: boolean
) {
  return {
    id,
    kind,
    label,
    language,
    license,
    state,
    sizeMb,
    checksumSha256: state === "installed" ? "packaged-at-build" : "pending-download",
    requiredForRelease
  };
}

function fallbackProductionReadiness(): ProductionReadinessReport {
  const offlineAssets = defaultOfflineAssets();
  const requiredOfflineAssets = offlineAssets.filter((asset) => asset.requiredForRelease);

  return {
    generatedAtMs: Date.now(),
    score: 78,
    state: "degraded",
    blockers: [
      "Production plugin signing key is not configured.",
      "Device-level acceptance rehearsals still need real hardware evidence."
    ],
    secretVault: {
      provider: "OS vault boundary",
      state: "ready",
      detail: "SQLite stores only secret references. Raw provider keys must live in the desktop vault.",
      storedSecretCount: 0,
      releaseRequired: true,
      policy: [
        "No raw secrets in SQLite, logs, support bundles, or plugin manifests.",
        "Credential reads are adapter-scoped and audited by reference id."
      ]
    },
    pluginPolicy: {
      state: "blocked",
      detail: "Add a production plugin signing key before enabling third-party plugins.",
      trustedKeyCount: 0,
      requiredControls: [
        "Ed25519 signature over canonical manifest payload.",
        "No wildcard network hosts.",
        "Explicit capability list and relative package entrypoint."
      ]
    },
    supportBundle: {
      state: "ready",
      detail: "Exports diagnostics as redacted JSON. Transcript text is excluded unless the operator opts in.",
      includes: ["Runtime mode", "Health summary", "Adapter receipts", "Device checklist"],
      excludes: ["Provider keys", "Unredacted local usernames", "Transcript text by default"]
    },
    offlineAssets: {
      state: "ready",
      installedCount: offlineAssets.filter((asset) => asset.state === "installed").length,
      requiredCount: requiredOfflineAssets.length,
      assets: offlineAssets
    },
    acceptanceDevices: [
      devicePlan("vmix", "vMix", "broadcast", true, ["Check API", "Preview title", "Take live", "Clear"]),
      devicePlan("obs", "OBS Studio", "broadcast", true, ["Connect", "Preview text", "Live text"]),
      devicePlan("easyworship", "EasyWorship", "presentation", true, ["Export", "Schedule handoff"]),
      devicePlan("propresenter", "ProPresenter", "presentation", false, ["API health", "Playlist cue"]),
      devicePlan("hdmi", "HDMI output", "display", true, ["Display detect", "Safe area"]),
      devicePlan("ndi", "NDI output", "video", false, ["Discovery", "Alpha"])
    ],
    releaseGates: [
      {
        label: "Build verification",
        state: "healthy",
        detail: "Run npm run verify:production before every signed build."
      },
      {
        label: "Signed installer",
        state: "degraded",
        detail: "Release key and updater endpoint must be configured outside source control."
      },
      {
        label: "Rollback drill",
        state: "degraded",
        detail: "Install, upgrade, rollback, and offline reinstall must be rehearsed on Windows."
      }
    ]
  };
}

function applyOfflineAssetInstall(
  readiness: ProductionReadinessReport,
  assetId: string
): ProductionReadinessReport {
  const updatedAssets = readiness.offlineAssets.assets.map((asset) =>
    asset.id === assetId
      ? { ...asset, state: "installed", checksumSha256: "operator-verified-install" }
      : asset
  );
  const installedCount = updatedAssets.filter((asset) => asset.state === "installed").length;
  const requiredCount = updatedAssets.filter((asset) => asset.requiredForRelease).length;
  const missingRequired = updatedAssets.some(
    (asset) => asset.requiredForRelease && asset.state !== "installed"
  );

  return {
    ...readiness,
    offlineAssets: {
      ...readiness.offlineAssets,
      state: missingRequired ? "blocked" : "ready",
      installedCount,
      requiredCount,
      assets: updatedAssets
    }
  };
}

function fallbackLocalRehearsal(): LocalRehearsalReport {
  return {
    generatedAtMs: Date.now(),
    state: "degraded",
    passed: 5,
    total: 7,
    proofPath: "Browser fallback: open the Tauri desktop app to write rehearsal proof.",
    steps: [
      {
        label: "SQLite scripture index",
        state: "healthy",
        detail: "Demo data can resolve Romans 8:28, but the desktop SQLite proof has not run.",
        durationMs: 1
      },
      {
        label: "AI detection policy",
        state: "healthy",
        detail: "Browser fallback detected scripture from bundled transcript content.",
        durationMs: 2
      },
      {
        label: "Preview scene render",
        state: "healthy",
        detail: "Preview and reference layers are visible in the UI.",
        durationMs: 1
      },
      {
        label: "vMix command configuration",
        state: "degraded",
        detail: "Open the desktop app to validate the local vMix adapter configuration.",
        durationMs: 0
      },
      {
        label: "Live safety gate",
        state: "healthy",
        detail: "Destination arming remains explicit.",
        durationMs: 0
      },
      {
        label: "Support redaction",
        state: "healthy",
        detail: "Redaction policy is available in the Rust operations layer.",
        durationMs: 0
      },
      {
        label: "Local proof export",
        state: "degraded",
        detail: "Browser fallback cannot write app-data rehearsal evidence.",
        durationMs: 0
      }
    ]
  };
}

function devicePlan(id: string, name: string, category: string, requiredForRelease: boolean, labels: string[]) {
  return {
    id,
    name,
    category,
    state: "not-run",
    requiredForRelease,
    steps: labels.map((label) => ({
      label,
      expected: "Pass during device rehearsal.",
      required: true
    }))
  };
}

function fallbackAdapterResult(adapter: string): AdapterDispatchResult {
  return {
    adapter,
    state: "offline",
    detail: `${adapter} control requires the Tauri desktop app.`,
    reference: "",
    auditCount: 0
  };
}

// ---------------------------------------------------------------------------
// Operator identity
// ---------------------------------------------------------------------------

/** Sets the operator name shown in audit logs. Persists across restarts. */
export async function setOperatorName(name: string): Promise<void> {
  if (!isTauriRuntime()) return;
  await invokeWithTimeout<void>("set_operator_name", { name });
}

/** Returns the current operator name used for audit attribution. */
export async function getOperatorName(): Promise<string> {
  if (!isTauriRuntime()) return "operator:local-booth";
  try {
    return await invokeWithTimeout<string>("get_operator_name");
  } catch {
    return "operator:local-booth";
  }
}

// ---------------------------------------------------------------------------
// Session persistence
// ---------------------------------------------------------------------------

/** Manually flush runtime state to disk. Called automatically on key actions. */
export async function saveSession(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invokeWithTimeout<void>("save_session");
}

// ---------------------------------------------------------------------------
// Test connection (per-adapter connectivity probe)
// ---------------------------------------------------------------------------

export async function testVmixConnection(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("vmix");
  return invokeWithTimeout<AdapterDispatchResult>("test_vmix_connection");
}

export async function testObsConnection(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("obs");
  return invokeWithTimeout<AdapterDispatchResult>("test_obs_connection");
}

export async function testProPresenterConnection(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("propresenter");
  return invokeWithTimeout<AdapterDispatchResult>("test_propresenter_connection");
}

export async function testCompanionConnection(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("companion");
  return invokeWithTimeout<AdapterDispatchResult>("test_companion_connection");
}

export async function testOscConnection(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("osc");
  return invokeWithTimeout<AdapterDispatchResult>("test_osc_connection");
}

export async function testEasyWorshipConnection(): Promise<AdapterDispatchResult> {
  if (!isTauriRuntime()) return fallbackAdapterResult("easyworship");
  return invokeWithTimeout<AdapterDispatchResult>("test_easyworship_connection");
}

// ---------------------------------------------------------------------------
// App-level KV (used by domain stores for cross-restart persistence)
// ---------------------------------------------------------------------------

export type KvKey = {
  key: string;
  updatedAtMs: number;
};

export async function kvGet(key: string): Promise<string | null> {
  try {
    const res = await invokeWithTimeout<string | null>("kv_get", { key });
    return res ?? null;
  } catch {
    return null;
  }
}

export async function kvSet(key: string, valueJson: string): Promise<KvKey | null> {
  try {
    return await invokeWithTimeout<KvKey>("kv_set", { key, valueJson });
  } catch {
    return null;
  }
}

export async function kvDelete(key: string): Promise<void> {
  try {
    await invokeWithTimeout<void>("kv_delete", { key });
  } catch {
    /* ignore */
  }
}

export async function kvListKeys(): Promise<KvKey[]> {
  try {
    return await invokeWithTimeout<KvKey[]>("kv_list_keys");
  } catch {
    return [];
  }
}

// ---------------------------------------------------------------------------
// Vault — secrets in OS keyring (translation API key, etc.)
// ---------------------------------------------------------------------------

export async function vaultStoreSecret(label: string, secret: string): Promise<boolean> {
  try {
    await invokeWithTimeout<void>("vault_store_secret", { label, secret });
    return true;
  } catch (err) {
    console.warn("vault_store_secret failed", err);
    return false;
  }
}

export async function vaultReadSecret(label: string): Promise<string | null> {
  try {
    return await invokeWithTimeout<string>("vault_read_secret", { label });
  } catch {
    return null;
  }
}

export async function vaultDeleteSecret(label: string): Promise<void> {
  try {
    await invokeWithTimeout<void>("vault_delete_secret", { label });
  } catch {
    /* ignore */
  }
}

// ---------------------------------------------------------------------------
// CCLI usage (audit-chain backed)
// ---------------------------------------------------------------------------

export type CcliUsageEntry = {
  id: string;
  ccliNumber: string;
  songTitle: string;
  sentLiveAtMs: number;
  serviceSessionId: string;
  operator: string;
};

export async function logCcliUsage(
  ccliNumber: string,
  songTitle: string,
  serviceSessionId: string,
  operator: string
): Promise<CcliUsageEntry> {
  return invokeWithTimeout<CcliUsageEntry>("log_ccli_usage", {
    ccliNumber,
    songTitle,
    serviceSessionId,
    operator,
  });
}

export async function listCcliUsage(limit?: number): Promise<CcliUsageEntry[]> {
  return invokeWithTimeout<CcliUsageEntry[]>("list_ccli_usage", { limit });
}

export async function exportCcliUsageCsv(): Promise<string> {
  return invokeWithTimeout<string>("export_ccli_usage_csv", undefined, SLOW_OP_TIMEOUT_MS);
}

// ---------------------------------------------------------------------------
// Fleet bundle signing (ed25519)
// ---------------------------------------------------------------------------

export type SignedFleetBundle = {
  signatureHex: string;
  publicKeyHex: string;
  payloadSha256: string;
  payloadJson: string;
  signedAtMs: number;
  signerLabel: string;
};

export type FleetVerifyResult = {
  valid: boolean;
  detail: string;
  publicKeyHex: string;
  payloadSha256: string;
};

export async function signFleetBundle(
  payloadJson: string,
  signerLabel: string
): Promise<SignedFleetBundle> {
  return invokeWithTimeout<SignedFleetBundle>("sign_fleet_bundle", {
    payloadJson,
    signerLabel,
  });
}

export async function verifyFleetBundle(
  bundle: SignedFleetBundle
): Promise<FleetVerifyResult> {
  return invokeWithTimeout<FleetVerifyResult>("verify_fleet_bundle", { bundle });
}

export async function getFleetPublicKey(): Promise<string> {
  return invokeWithTimeout<string>("get_fleet_public_key");
}

// ---------------------------------------------------------------------------
// Stream overlay local HTTP server
// ---------------------------------------------------------------------------

export type StreamOverlayState = {
  tickerText: string;
  armed: boolean;
  liveReference: string | null;
  liveText: string | null;
  translations: Record<string, string>;
};

export type StreamOverlayServerStatus = {
  running: boolean;
  port: number | null;
  url: string | null;
  startedAtMs: number | null;
};

export async function startStreamOverlayServer(
  port?: number
): Promise<StreamOverlayServerStatus> {
  return invokeWithTimeout<StreamOverlayServerStatus>("start_stream_overlay_server", {
    port,
  });
}

export async function stopStreamOverlayServer(): Promise<StreamOverlayServerStatus> {
  return invokeWithTimeout<StreamOverlayServerStatus>("stop_stream_overlay_server");
}

export async function updateStreamOverlayState(
  newState: StreamOverlayState
): Promise<StreamOverlayServerStatus> {
  return invokeWithTimeout<StreamOverlayServerStatus>("update_stream_overlay_state", {
    newState,
  });
}

export async function getStreamOverlayServerStatus(): Promise<StreamOverlayServerStatus> {
  return invokeWithTimeout<StreamOverlayServerStatus>("get_stream_overlay_server_status");
}

function fallbackServiceProfile(
  id: string,
  name: string,
  languages: string[],
  outputPolicy: string
): ServiceProfile {
  const now = Date.now();
  return {
    id,
    name,
    languages,
    outputPolicy,
    isActive: false,
    createdAtMs: now,
    updatedAtMs: now
  };
}
