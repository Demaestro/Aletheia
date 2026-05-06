import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, useDeferredValue } from "react";
import { ErrorBoundary } from "react-error-boundary";
import { LandingPage } from "./components/LandingPage";
import { LiveTranscriptView } from "./components/LiveTranscriptView";
import { usePollingLoop } from "./hooks/usePollingLoop";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useAlwaysOnCommandCapture } from "./hooks/useAlwaysOnCommandCapture";
import { useBrowserSpeechCommandLane } from "./hooks/useBrowserSpeechCommandLane";
import { tNavMeta } from "./utils/tNav";
import { classifyScriptureCommand, parseExplicitScriptureCommand } from "./utils/scriptureCommand";
import { VuMeter } from "./components/VuMeter";
import { PreviewLiveOutput } from "./components/PreviewLiveOutput";
import { QueueApprovalPanel } from "./components/QueueApprovalPanel";
import { WorkspaceShell } from "./components/WorkspaceShell";
import { CaptureControlPanel } from "./components/CaptureControlPanel";
// Hot screens — eager-loaded so the operator never waits for a chunk fetch
// when navigating to the dashboard, bible reader, or default landing.
import { OperatorDashboard } from "./components/OperatorDashboard";
import { BibleReaderPanel } from "./components/BibleReaderPanel";
import { ManualSearchFallback } from "./components/ManualSearchFallback";
import { HealthStatusPanel } from "./components/HealthStatusPanel";
import { IntegrationsSettings } from "./components/IntegrationsSettings";

// Cold screens — defer until the operator navigates to them.
const OnboardingFlow = lazy(() =>
  import("./components/OnboardingFlow").then((m) => ({ default: m.OnboardingFlow }))
);
const ThemeDesigner = lazy(() =>
  import("./components/ThemeDesigner").then((m) => ({ default: m.ThemeDesigner }))
);
const HardwareChecklistPanel = lazy(() =>
  import("./components/HardwareChecklistPanel").then((m) => ({ default: m.HardwareChecklistPanel }))
);
const StreamOverlayPanel = lazy(() =>
  import("./components/StreamOverlayPanel").then((m) => ({ default: m.StreamOverlayPanel }))
);
const SongLibraryPanel = lazy(() =>
  import("./components/SongLibraryPanel").then((m) => ({ default: m.SongLibraryPanel }))
);
const FleetSyncPanel = lazy(() =>
  import("./components/FleetSyncPanel").then((m) => ({ default: m.FleetSyncPanel }))
);
const ClipEdlPanel = lazy(() =>
  import("./components/ClipEdlPanel").then((m) => ({ default: m.ClipEdlPanel }))
);
import { manualSearchResults, screenOrder, themes } from "./data/production";
import { useDesktopStore } from "./store/useDesktopStore";
import { useThemeStore, hydrateThemeFromKv } from "./store/useThemeStore";
import { useHardwareStore } from "./store/useHardwareStore";
import { useTranslationStore, hydrateTranslationFromKv } from "./store/useTranslationStore";
import { hydrateServicePlanFromKv } from "./store/useServicePlanStore";
import { hydrateSongLibraryFromKv } from "./store/useSongLibraryStore";
import { hydrateStreamOverlayFromKv } from "./store/useStreamOverlayStore";
import { migrateLocalStorageToKv } from "./store/persistence";
import { useTauriEvents } from "./hooks/useTauriEvents";
import {
  analyzeTranscript,
  clearCompanionOutput,
  clearEasyWorshipOutput,
  clearObsOutput,
  clearOscOutput,
  clearProPresenterOutput,
  clearVmixOverlay,
  enablePluginManifest,
  getCompanionStatus,
  getOperatorName,
  getProPresenterStatus,
  listTrustedPlugins,
  recordCalibrationSample,
  logCcliUsage,
  listCcliUsage,
  signFleetBundle,
  verifyFleetBundle,
  startStreamOverlayServer,
  stopStreamOverlayServer,
  updateStreamOverlayState,
  getStreamOverlayServerStatus,
  getBibleChapter,
  revokeTrustedPlugin,
  setOperatorName,
  showMainWindow,
  listAudioDevices,
  getSttStatus,
  getSttLatencyProfile,
  reloadSttModel,
  isTauriRuntime,
  generateEasyWorshipSmbSetupGuide,
  getVmixTitleSetupGuide,
  exportBoothPack,
  exportSupportBundle,
  exportOfflineAssetPack,
  getDesktopServiceState,
  getEasyWorshipStatus,
  getObsStatus,
  getOscStatus,
  getProductionReadiness,
  getRecentIntegrationEvents,
  getVmixConfig,
  getVmixStatus,
  diagnoseBackend,
  installOfflineAsset,
  installOfflineAssetFromPath,
  recordDeviceAcceptance,
  runProductionReleaseGate,
  armAndSendLiveCandidate,
  sendCompanionLive,
  sendCompanionPreview,
  sendEasyWorshipLive,
  sendEasyWorshipPreview,
  sendObsLive,
  sendObsPreview,
  sendOscLive,
  sendOscPreview,
  sendOscTestPing,
  sendProPresenterLive,
  sendProPresenterPreview,
  updateCompanionConfig,
  updateEasyWorshipConfig,
  updateObsConfig,
  updateOscConfig,
  updateProPresenterConfig,
  verifyPluginManifest,
  renderPreviewScene,
  runLocalRehearsal,
  runPreServiceCheck,
  searchScripture,
  sendLiveCandidate,
  sendVmixLive,
  sendVmixPreview,
  setDestinationsArmed,
  setDataMiser,
  updateVmixConfig,
  fetchVerseOnDemand,
  approveCandidate as approveCandidateCmd,
  rejectCandidate as rejectCandidateCmd,
  previewCandidate as previewCandidateCmd,
  takeCandidateLive as takeCandidateLiveCmd,
  clearAllOutputs as clearAllOutputsCmd,
  type SttLatencyProfile,
  type BackendDiagnostics,
  type ProductionReleaseGate,
} from "./services/desktopApi";
import type {
  BoothPackExport,
  CompanionConfig,
  EasyWorshipConfig,
  LocalRehearsalReport,
  ManualSearchResult,
  ObsConfig,
  OfflinePackExport,
  OscConfig,
  PluginVerificationResult,
  ProPresenterConfig,
  ScreenKey,
  ScriptureCandidate,
  SupportBundleExport,
  TranscriptSegment,
  TrustedPlugin,
  VmixConfig,
} from "./types";

import { useTranslation } from "react-i18next";

type ParsedReference = {
  book: string;
  chapter: number;
  verse: number;
};

function parseCandidateReference(reference?: string | null): ParsedReference | null {
  if (!reference) return null;
  const match = reference.trim().match(/^(.+?)\s+(\d+):(\d+)/);
  if (!match) return null;
  const chapter = Number.parseInt(match[2], 10);
  const verse = Number.parseInt(match[3], 10);
  if (!match[1] || !Number.isFinite(chapter) || !Number.isFinite(verse)) return null;
  return { book: match[1], chapter, verse };
}

function resolveContextualVerseCommand(
  command: string,
  currentCandidate?: ScriptureCandidate | null,
): string | null {
  const current = parseCandidateReference(currentCandidate?.reference);
  if (!current) return null;
  const normalized = command
    .toLowerCase()
    .replace(/[.,:;]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!normalized) return null;

  if (
    /\b(next|continue)\b/.test(normalized) ||
    /\bkeep going\b/.test(normalized) ||
    /\bnext verse\b/.test(normalized)
  ) {
    return `${current.book} ${current.chapter}:${current.verse + 1}`;
  }

  if (
    /\b(previous verse|take it back|go back)\b/.test(normalized) ||
    /\bprevious\b/.test(normalized)
  ) {
    return `${current.book} ${current.chapter}:${Math.max(1, current.verse - 1)}`;
  }

  const goToVerse = normalized.match(/\b(?:go to\s+)?verse\s+(\d{1,3})\b/);
  if (goToVerse) {
    return `${current.book} ${current.chapter}:${Number.parseInt(goToVerse[1], 10)}`;
  }

  return null;
}

export default function App() {
  const [activeScreen, setActiveScreen] = useState<ScreenKey>("dashboard");
  const [searchQuery, setSearchQuery] = useState("jn 3 16");
  const [searchResults, setSearchResults] = useState<ManualSearchResult[]>(manualSearchResults);
  const [dashboardSuggestions, setDashboardSuggestions] = useState<ScriptureCandidate[]>([]);
  const savedThemeId = typeof localStorage !== "undefined" ? localStorage.getItem("aletheia:activeThemeId") : null;
  const initialTheme = savedThemeId ? (themes.find(t => t.id === savedThemeId) ?? themes[0]) : themes[0];
  const [selectedTheme, setSelectedTheme] = useState(initialTheme);
  const [listenerStatus, setListenerStatus] = useState("Loading local-first audio services.");
  const [appNotice, setAppNotice] = useState("Loading local-first desktop services.");
  const commandNotice = appNotice;
  const setCommandNotice = setAppNotice;

  // Modals / Specific exports
  const [pluginVerification, setPluginVerification] = useState<PluginVerificationResult | null>(null);
  const [supportBundleExport, setSupportBundleExport] = useState<SupportBundleExport | null>(null);
  const [offlinePackExport, setOfflinePackExport] = useState<OfflinePackExport | null>(null);
  const [boothPackExport, setBoothPackExport] = useState<BoothPackExport | null>(null);
  const [rehearsalReport, setRehearsalReport] = useState<LocalRehearsalReport | null>(null);

  const { t } = useTranslation();
  const activeMeta = useMemo(() => tNavMeta(t, activeScreen), [t, activeScreen]);
  const currentIndex = useMemo(() => screenOrder.indexOf(activeScreen), [activeScreen]);

  // Zustand — granular selectors to prevent whole-tree re-renders on every store mutation.
  const candidates       = useDesktopStore(s => s.candidates);
  const transcript       = useDesktopStore(s => s.transcript);
  const aiDetection      = useDesktopStore(s => s.aiDetection);
  const previewCandidate = useDesktopStore(s => s.previewCandidate);
  const liveCandidate    = useDesktopStore(s => s.liveCandidate);
  const selectedCandidate = useDesktopStore(s => s.selectedCandidate);
  const destinationsArmed = useDesktopStore(s => s.destinationsArmed);
  const desktopStatus    = useDesktopStore(s => s.desktopStatus);
  const commandContextCandidateRef = useRef<ScriptureCandidate | null>(null);
  const setCandidates    = useDesktopStore(s => s.setCandidates);
  const setTranscript    = useDesktopStore(s => s.setTranscript);
  const setAiDetection   = useDesktopStore(s => s.setAiDetection);
  const mergeCandidates  = useDesktopStore(s => s.mergeCandidates);
  const setPreviewCandidate = useDesktopStore(s => s.setPreviewCandidate);
  const setLiveCandidate = useDesktopStore(s => s.setLiveCandidate);
  const setSelectedCandidateAction = useDesktopStore(s => s.setSelectedCandidate);
  const setDestinationsArmedAction = useDesktopStore(s => s.setDestinationsArmed);
  const setDesktopStatusAction = useDesktopStore(s => s.setDesktopStatus);
  const removeCandidate  = useDesktopStore(s => s.removeCandidate);
  const addTranscriptSegment = useDesktopStore(s => s.addTranscriptSegment);

  useEffect(() => {
    commandContextCandidateRef.current = previewCandidate ?? selectedCandidate ?? liveCandidate ?? null;
  }, [liveCandidate, previewCandidate, selectedCandidate]);

  const integrations       = useHardwareStore(s => s.integrations);
  const vmixStatus         = useHardwareStore(s => s.vmixStatus);
  const obsStatus          = useHardwareStore(s => s.obsStatus);
  const oscStatus          = useHardwareStore(s => s.oscStatus);
  const proPresenterStatus = useHardwareStore(s => s.proPresenterStatus);
  const companionStatus    = useHardwareStore(s => s.companionStatus);
  const easyWorshipStatus  = useHardwareStore(s => s.easyWorshipStatus);
  const healthItems        = useHardwareStore(s => s.healthItems);
  const productionReadiness = useHardwareStore(s => s.productionReadiness);
  const integrationEvents  = useHardwareStore(s => s.integrationEvents);
  const operatorName       = useHardwareStore(s => s.operatorName);
  const setVmixStatus      = useHardwareStore(s => s.setVmixStatus);
  const setObsStatus       = useHardwareStore(s => s.setObsStatus);
  const setOscStatus       = useHardwareStore(s => s.setOscStatus);
  const setProPresenterStatus = useHardwareStore(s => s.setProPresenterStatus);
  const setCompanionStatus = useHardwareStore(s => s.setCompanionStatus);
  const setEasyWorshipStatus = useHardwareStore(s => s.setEasyWorshipStatus);
  const setHealthItems     = useHardwareStore(s => s.setHealthItems);
  const setProductionReadiness = useHardwareStore(s => s.setProductionReadiness);
  const setIntegrationEvents = useHardwareStore(s => s.setIntegrationEvents);
  const setIntegrations    = useHardwareStore(s => s.setIntegrations);
  const setOperatorNameAction = useHardwareStore(s => s.setOperatorName);

  const translation = useTranslationStore();

  // Apply theme mode (light/dark/system) to document root. When the operator
  // selects "system", honour `prefers-color-scheme` and listen for OS-level
  // changes so the broadcast booth follows ambient lighting overrides.
  const themeMode = useThemeStore((s) => s.themeMode);
  useEffect(() => {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    if (themeMode !== "system") {
      root.setAttribute("data-theme", themeMode);
      return;
    }
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => root.setAttribute("data-theme", mql.matches ? "dark" : "light");
    apply();
    mql.addEventListener("change", apply);
    return () => mql.removeEventListener("change", apply);
  }, [themeMode]);

  // Auto-route detected languages into the translation target list whenever
  // a fresh AI-detection result arrives (no-op if the operator left the
  // "auto-route" toggle off).
  useEffect(() => {
    const codes = aiDetection?.languages?.map((l) => l.code) ?? [];
    if (codes.length === 0) return;
    translation.applyDetectedLanguages(codes);
  }, [aiDetection?.checkedAtMs, translation]);

  // Hydrate UI stores from the Rust KV once on boot. Order:
  //   1. Migrate any existing localStorage values up to Rust (one-time).
  //   2. Pull authoritative state down from Rust to overwrite synchronous
  //      hydration if Rust has fresher data (e.g. after a reinstall).
  // All of this is best-effort — failures keep us on localStorage.
  useEffect(() => {
    const PERSISTED_KEYS = [
      "aletheia.servicePlan.v1",
      "aletheia.songs.v1",
      "aletheia.ccliUsage.v1",
      "aletheia.streamOverlay.v1",
      "aletheia.translation.v1",
      "aletheia.clipMarkers.v1",
    ];
    void (async () => {
      await migrateLocalStorageToKv(PERSISTED_KEYS);
      await Promise.all([
        hydrateServicePlanFromKv(),
        hydrateSongLibraryFromKv(),
        hydrateStreamOverlayFromKv(),
        hydrateTranslationFromKv(),
        hydrateThemeFromKv(),
      ]);
    })();
  }, []);

  // Attach realtime backend listeners
  useTauriEvents(setAppNotice);
  // Background polling (AI analysis, transcript sync, vMix reconnect)
  usePollingLoop(setAppNotice);

  // Trusted plugins state
  const [trustedPlugins, setTrustedPlugins] = useState<TrustedPlugin[]>([]);
  const [audioDevices, setAudioDevices] = useState<string[]>([]);
  const [selectedAudioDevice, setSelectedAudioDevice] = useState<string | undefined>(undefined);
  const [sttStatus, setSttStatus] = useState<import("./services/desktopApi").SttStatus | null>(null);
  const [sttLatencyProfile, setSttLatencyProfile] = useState<SttLatencyProfile | null>(null);
  const [backendDiagnostics, setBackendDiagnostics] = useState<BackendDiagnostics | null>(null);
  const [releaseGate, setReleaseGate] = useState<ProductionReleaseGate | null>(null);
  const [onDemandVerse, setOnDemandVerse] = useState<Record<string, { text: string; source: string }>>({});
  const [fetchingVerse, setFetchingVerse] = useState<string | null>(null);
  const desktopReady = isTauriRuntime() || desktopStatus?.mode === "tauri";
  const {
    captureState,
    captureRunning,
    captureStarting,
    listenerStatus: captureListenerStatus,
    lastAudioLevel,
    startCapture: startDashboardCapture,
    stopCapture,
  } = useAlwaysOnCommandCapture({
    selectedAudioDevice,
    desktopReady,
    sttStatus,
  });

  useEffect(() => {
    setListenerStatus(captureListenerStatus.text);
  }, [captureListenerStatus.text]);

  useEffect(() => {
    let cancelled = false;

    // Reveal the window as early as possible — Tauri starts with visible:false
    // to avoid the Win32 "(Not Responding)" freeze during WebView2 init.
    void showMainWindow();

    // Boot fetch with 3-step backoff so a single slow response during the
    // bundled-bible write storm doesn't latch the UI into browser-fallback.
    // Each attempt uses the 15s SLOW_OP_TIMEOUT inside getDesktopServiceState.
    const fetchWithRetry = async (): Promise<void> => {
      // Tight retry: first attempt immediate, one quick follow-up. The 7s
      // worst-case latched the UI in browser-fallback for users on cold boot.
      const delays = [0, 500, 1500];
      for (const delay of delays) {
        if (cancelled) return;
        if (delay > 0) await new Promise((r) => setTimeout(r, delay));
        const state = await getDesktopServiceState();
        if (cancelled) return;
        setCandidates(state.candidates);
        setTranscript(state.transcript);
        setIntegrations(state.integrations);
        setHealthItems(state.health);
        setPreviewCandidate(state.preview);
        setLiveCandidate(state.live);
        setSelectedCandidateAction(state.preview);
        setDestinationsArmedAction(state.session.destinationsArmed);
        setDesktopStatusAction(state.session);
        if (state.session.mode === "tauri") {
          setCommandNotice("Desktop core online. Local SQLite, audit, and command policy are active.");
          return; // success — stop retrying
        }
        // Fallback path returned — only surface the message after the final attempt.
        if (delay === delays[delays.length - 1]) {
          setCommandNotice("Desktop core unavailable. Browser fallback is active.");
        }
      }
    };
    fetchWithRetry().catch((error: unknown) => {
      if (cancelled) return;
      setCommandNotice(error instanceof Error ? error.message : "Desktop services are unavailable.");
    });

    getVmixConfig().then((config) => {
      if (!cancelled) {
        setVmixStatus({
          state: "offline",
          checkedAtMs: Date.now(),
          detail: "",
          endpoint: "",
          ...config
        });
      }
    });

    getOperatorName().then((name) => {
      if (!cancelled) setOperatorNameAction(name);
    });

    listTrustedPlugins().then((plugins) => {
      if (!cancelled) setTrustedPlugins(plugins);
    });

    listAudioDevices().then((devices) => {
      if (cancelled) return;
      setAudioDevices(devices);
      setSelectedAudioDevice((current) => {
        if (current && devices.includes(current)) return current;
        return undefined;
      });
    });

    getSttStatus().then((status) => {
      if (cancelled) return;
      // Do not eagerly load Whisper here. `start_audio_capture` owns the
      // native adapter lifecycle so boot never performs two 400MB+ model loads
      // in parallel.
      setSttStatus(status);
    }).catch(() => undefined);

    getSttLatencyProfile().then((profile) => {
      if (!cancelled) setSttLatencyProfile(profile);
    }).catch(() => undefined);

    const slowStartupTimer = window.setTimeout(() => {
      void (async () => {
        try {
          const events = await getRecentIntegrationEvents();
          if (!cancelled) setIntegrationEvents(events);
        } catch {
          // Non-critical boot data.
        }

        try {
          const report = await getProductionReadiness();
          if (!cancelled) setProductionReadiness(report);
        } catch {
          // Non-critical boot data.
        }
      })();
    }, 2_500);

    return () => {
      cancelled = true;
      window.clearTimeout(slowStartupTimer);
    };
  }, []);

  useEffect(() => {
    if (!desktopReady) return;
    void listAudioDevices()
      .then((devices) => {
        setAudioDevices(devices);
        setSelectedAudioDevice((current) => {
          if (current && devices.includes(current)) return current;
          return undefined;
        });
      })
      .catch(() => undefined);
  }, [desktopReady]);

  useEffect(() => {
    if (!captureRunning) return;
    let cancelled = false;
    const refresh = () => {
      void getSttLatencyProfile()
        .then((profile) => {
          if (!cancelled) setSttLatencyProfile(profile);
        })
        .catch(() => undefined);
    };
    refresh();
    const timer = window.setInterval(refresh, 3_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [captureRunning, transcript.length]);

  const refreshAudioDevices = () => {
    void listAudioDevices()
      .then((devices) => {
        setAudioDevices(devices);
        setCommandNotice(`${devices.length} input device${devices.length === 1 ? "" : "s"} available.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Audio device scan failed.");
      });
  };

  const refreshSttModel = () => {
    void reloadSttModel()
      .then((status) => {
        setSttStatus(status);
        setCommandNotice(status.modelLoaded ? "STT model loaded." : status.loadError ?? "STT model not loaded.");
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "STT model reload failed.");
      });
  };

  // Stable noop refs — populated after handlers are defined below.
  const clearLiveRef = useRef<() => void>(() => undefined);

  // Keyboard shortcuts (Ctrl+K, Ctrl+L w/ double-press guard, Alt+→, Escape×2)
  useKeyboardShortcuts({
    activeScreen,
    previewCandidate: previewCandidate ?? null,
    destinationsArmed: destinationsArmed,
    onNavigate: setActiveScreen,
    onSendLive: (candidate) => sendLive(candidate),
    onClearLive: () => clearLiveRef.current(),
  });

  const deferredSearchQuery = useDeferredValue(searchQuery);
  const commandSearchSeqRef = useRef(0);
  const previewRenderTimerRef = useRef<number | null>(null);
  const lastPreviewRenderRef = useRef<{ key: string; at: number }>({ key: "", at: 0 });

  const schedulePreviewRender = useCallback((candidate: ScriptureCandidate) => {
    const key = `${candidate.reference}|${candidate.translation}|${candidate.text}`;
    const now = Date.now();
    if (lastPreviewRenderRef.current.key === key && now - lastPreviewRenderRef.current.at < 1_500) {
      return;
    }
    lastPreviewRenderRef.current = { key, at: now };
    if (previewRenderTimerRef.current !== null) {
      window.clearTimeout(previewRenderTimerRef.current);
    }
    previewRenderTimerRef.current = window.setTimeout(() => {
      void renderPreviewScene(candidate).catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Preview render failed.");
      });
    }, 120);
  }, []);

  useEffect(() => {
    return () => {
      if (previewRenderTimerRef.current !== null) {
        window.clearTimeout(previewRenderTimerRef.current);
      }
    };
  }, []);

  /** On-demand fetch for a specific reference not in the local DB. */
  const fetchVerseForRef = (ref: string) => {
    setFetchingVerse(ref);
    void fetchVerseOnDemand(ref)
      .then((result) => {
        if (result) {
          setOnDemandVerse((prev) => ({ ...prev, [ref]: { text: result.text, source: result.source } }));
          setCommandNotice(`Fetched "${ref}" from ${result.source}.`);
        } else {
          setCommandNotice(`Could not resolve "${ref}" — check spelling or add it manually.`);
        }
      })
      .catch(() => setCommandNotice(`Verse lookup failed for "${ref}".`))
      .finally(() => setFetchingVerse(null));
  };

  const candidateFromSearchHit = (
    hit: ManualSearchResult,
    command: string,
    index = 0,
  ): ScriptureCandidate => ({
    id: `manual-${hit.reference.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${index}`,
    reference: hit.reference,
    translation: hit.translation,
    language: hit.language ?? "English",
    text: hit.snippet,
    confidence: hit.source.toLowerCase().includes("spoken") || hit.source.toLowerCase().includes("exact")
      ? 99
      : Math.max(72, 88 - index * 6),
    source: hit.source,
    reason: `Opened from dashboard command: "${command}"`,
    status: "preview",
  });

  const openScriptureCommand = useCallback((
    command: string,
    translationId = "kjv",
    options?: { suppressChapterOnly?: boolean },
  ) => {
    const requestSeq = ++commandSearchSeqRef.current;

    const directReference = parseExplicitScriptureCommand(command);
    const contextualCommand = directReference
      ? null
      : resolveContextualVerseCommand(command, commandContextCandidateRef.current);
    const lookupCommand = contextualCommand ?? command;
    setCommandNotice(`Looking up: ${lookupCommand}`);

    const explicitReference = directReference ?? parseExplicitScriptureCommand(lookupCommand);
    if (explicitReference) {
      if (options?.suppressChapterOnly && !explicitReference.explicitVerse) {
        setCommandNotice(`Heard ${explicitReference.book} ${explicitReference.chapter}. Waiting for verse number.`);
        return;
      }
      void getBibleChapter(translationId, explicitReference.book, explicitReference.chapter)
        .then((chapterVerses) => {
          if (requestSeq !== commandSearchSeqRef.current) return;
          const hit = chapterVerses.find((item) => item.verse === explicitReference.verse);
          if (!hit) {
            setDashboardSuggestions([]);
            setCommandNotice(
              `${translationId.toUpperCase()} ${explicitReference.book} ${explicitReference.chapter}:${explicitReference.verse} was not found in the loaded Bible library.`,
            );
            return;
          }
          const candidate: ScriptureCandidate = {
            id: `voice-${hit.reference.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${hit.translation.toLowerCase()}`,
            reference: hit.reference,
            translation: hit.translation,
            language: "English",
            text: hit.text,
            confidence: 100,
            source: "Voice command",
            reason: `Opened from command: "${lookupCommand}"`,
            status: "preview",
          };
          setDashboardSuggestions([]);
          mergeCandidates([candidate]);
          setSelectedCandidateAction(candidate);
          setPreviewCandidate(candidate);
          setCommandNotice(`Opened ${hit.reference} ${hit.translation}.`);
          schedulePreviewRender(candidate);
        })
        .catch((error: unknown) => {
          if (requestSeq !== commandSearchSeqRef.current) return;
          setCommandNotice(error instanceof Error ? error.message : `Lookup failed for "${lookupCommand}".`);
        });
      return;
    }

    void searchScripture(lookupCommand, translationId)
      .then((results) => {
        if (requestSeq !== commandSearchSeqRef.current) return;
        const hit = results[0];
        if (!hit) {
          setDashboardSuggestions([]);
          setCommandNotice(`No scripture found for "${lookupCommand}". Try "Genesis 1:5" or "open Genesis chapter 1 verse 5".`);
          return;
        }
        const candidatesFromResults = results.map((result, index) =>
          candidateFromSearchHit(result, lookupCommand, index)
        );
        const candidate = candidatesFromResults[0];
        setDashboardSuggestions(candidatesFromResults.slice(1));
        mergeCandidates(candidatesFromResults);
        setSelectedCandidateAction(candidate);
        setPreviewCandidate(candidate);
        setCommandNotice(
          candidatesFromResults.length > 1
            ? `Opened ${hit.reference}. ${candidatesFromResults.length - 1} likely option${candidatesFromResults.length === 2 ? "" : "s"} available.`
            : `Opened ${hit.reference} ${hit.translation}. Ready to preview or send live.`
        );
        void recordCalibrationSample(
          candidate.language,
          lookupCommand,
          candidate.reference,
          "confirmed",
          candidate.reference,
        ).catch(() => undefined);
        schedulePreviewRender(candidate);
      })
      .catch((error: unknown) => {
        if (requestSeq !== commandSearchSeqRef.current) return;
        setCommandNotice(error instanceof Error ? error.message : `Lookup failed for "${lookupCommand}".`);
      });
  }, [
    mergeCandidates,
    schedulePreviewRender,
    setPreviewCandidate,
    setSelectedCandidateAction,
  ]);

  const handleBrowserSpeechCommand = useCallback(
    (command: string, source: "interim" | "final") => {
      const kind = classifyScriptureCommand(command);
      openScriptureCommand(command, "kjv", {
        suppressChapterOnly: source === "interim" || kind === "chapterReference",
      });
    },
    [openScriptureCommand],
  );

  const handleBrowserSpeechStatus = useCallback((status: string) => {
    if (status.toLowerCase().includes("unavailable")) return;
    setListenerStatus(status);
  }, []);

  useBrowserSpeechCommandLane({
    enabled: desktopReady,
    onCommand: handleBrowserSpeechCommand,
    onTranscript: addTranscriptSegment,
    onStatus: handleBrowserSpeechStatus,
  });

  useEffect(() => {
    // Min-length guard: avoid FTS queries on single characters
    if (deferredSearchQuery.trim().length < 2) return;
    let cancelled = false;
    searchScripture(deferredSearchQuery).then((results) => {
      if (!cancelled) setSearchResults(results);
    });

    return () => {
      cancelled = true;
    };
  }, [deferredSearchQuery]);


  const preview = useCallback((candidate: ScriptureCandidate) => {
    const nextPreview: ScriptureCandidate = { ...candidate, status: "preview" };
    setSelectedCandidateAction(candidate);
    setPreviewCandidate(nextPreview);
    setCommandNotice(`Preview prepared for ${candidate.reference}.`);

    schedulePreviewRender(nextPreview);
    // Persist the verdict against the live candidate row when this came from
    // the live STT pipeline (id pattern "<segment>#<reference>").
    if (candidate.id.includes("#")) {
      void previewCandidateCmd(candidate.id).catch(() => undefined);
    }
  }, [schedulePreviewRender, setPreviewCandidate, setSelectedCandidateAction]);

  const previewVerseFromDashboard = useCallback((reference: string, text: string, translation: string) => {
    const candidate: ScriptureCandidate = {
      id: `dashboard-${reference.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${translation.toLowerCase()}`,
      reference,
      translation,
      language: "English",
      text,
      confidence: 100,
      source: "Dashboard Bible",
      reason: "Opened manually from the dashboard Bible reader.",
      status: "preview",
    };
    setSelectedCandidateAction(candidate);
    setPreviewCandidate(candidate);
    setCommandNotice(`Preview prepared: ${reference} (${translation}).`);
    schedulePreviewRender(candidate);
  }, [schedulePreviewRender, setPreviewCandidate, setSelectedCandidateAction]);

  const sendLive = (candidate = previewCandidate) => {
    if (!candidate) return;

    const wasArmed = destinationsArmed;
    if (!wasArmed) {
      setDestinationsArmedAction(true);
    }

    if (candidate.id.includes("#")) {
      void takeCandidateLiveCmd(candidate.id).catch(() => undefined);
    }
    void (wasArmed ? sendLiveCandidate(candidate) : armAndSendLiveCandidate(candidate))
      .then((result) => {
        const previewState: ScriptureCandidate = { ...candidate, status: "preview" };
        const liveState: ScriptureCandidate = { ...candidate, status: "live" };
        setSelectedCandidateAction(liveState);
        setPreviewCandidate(previewState);
        setLiveCandidate(liveState);
        setActiveScreen("output");
        if (desktopStatus) {
           setDesktopStatusAction({...desktopStatus, destinationsArmed: true, auditCount: result.auditCount, lastEventSequence: result.auditCount, checkedAtMs: Date.now()});
        }
        setCommandNotice(`Live output sent: ${result.scene.reference} ${result.scene.translation}.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Live output failed.");
      });
  };

  const toggleArmed = () => {
    const next = !destinationsArmed;
    setDestinationsArmedAction(next);
    setCommandNotice(next ? "Destinations armed for explicit live output." : "Safe hold enabled. Live output is blocked.");

    void setDestinationsArmed(next)
      .then((status) => setDesktopStatusAction(status))
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Could not update destination arming.");
      });
  };

  const setCloudAiEnabled = (enabled: boolean) => {
    void setDataMiser(!enabled)
      .then((status) => {
        setDesktopStatusAction(status);
        setCommandNotice(
          enabled
            ? "Cloud AI enabled. Local detection remains primary."
            : "Cloud AI disabled. Aletheia is running local-only detection."
        );
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Could not update cloud AI mode.");
      });
  };

  const runHealthCheck = () => {
    void Promise.all([
      runPreServiceCheck(),
      diagnoseBackend(),
      runProductionReleaseGate(),
    ])
      .then(([items, diagnostics, gate]) => {
        setBackendDiagnostics(diagnostics);
        setReleaseGate(gate);
        return items;
      })
      .then((items) => {
        setHealthItems(items);
        if (desktopStatus) setDesktopStatusAction({ ...desktopStatus, checkedAtMs: Date.now() });
        setCommandNotice("Pre-service check completed against the local core.");
        return getProductionReadiness();
      })
      .then((report) => {
        setProductionReadiness(report);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Pre-service check failed.");
      });
  };

  const runAiAssist = () => {
    void analyzeTranscript()
      .then((result) => {
        setAiDetection(result);
        mergeCandidates(result.candidates);
        if (result.candidates[0]) {
          setSelectedCandidateAction(result.candidates[0]);
          setPreviewCandidate({ ...result.candidates[0], status: "preview" });
        }
        setCommandNotice(
          result.candidates[0]
            ? `AI assist found ${result.candidates[0].reference}. Preview is ready for operator approval.`
            : "AI assist completed. No scripture candidate crossed the approval threshold."
        );
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "AI assist failed.");
      });
  };

  const createSupportBundle = () => {
    void exportSupportBundle(false)
      .then((bundle) => {
        setSupportBundleExport(bundle);
        setCommandNotice(bundle.path);
        return getProductionReadiness();
      })
      .then(setProductionReadiness)
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Support bundle export failed.");
      });
  };

  const createOfflinePack = (targetDir: string) => {
    setCommandNotice("Preparing offline distribution pack...");
    void exportOfflineAssetPack(targetDir)
      .then((pack) => {
        setOfflinePackExport(pack);
        setCommandNotice(`Offline pack exported to ${pack.path}`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Offline pack export failed.");
      });
  };

  const createBoothPack = () => {
    void exportBoothPack()
      .then((pack) => {
        setBoothPackExport(pack);
        setCommandNotice(`Booth pack exported: ${pack.path}`);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Booth pack export failed.");
      });
  };

  const runRehearsal = () => {
    void runLocalRehearsal()
      .then((report) => {
        setRehearsalReport(report);
        setCommandNotice(`Local rehearsal completed: ${report.passed}/${report.total} checks passed.`);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Local rehearsal failed.");
      });
  };

  const handleInstallOfflineAsset = (assetId: string) => {
    setCommandNotice(`Installing offline asset ${assetId}...`);
    void installOfflineAsset(assetId)
      .then((report) => {
        setProductionReadiness(report);
        setCommandNotice(`Offline asset ${assetId} installed and verified.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Offline asset install failed.");
      });
  };

  const handleInstallOfflineAssetFromPath = (
    assetId: string,
    filePath: string,
    expectedChecksum: string
  ) => {
    setCommandNotice(`Verifying ${assetId}...`);
    void installOfflineAssetFromPath(assetId, filePath, expectedChecksum)
      .then((report) => {
        setProductionReadiness(report);
        setCommandNotice(`Offline asset ${assetId} installed from ${filePath}.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Offline asset install failed.");
      });
  };

  const handleRecordDeviceAcceptance = (
    deviceId: string,
    stepLabel: string,
    passed: boolean,
    note?: string,
    evidencePath?: string
  ) => {
    setCommandNotice(`Recording ${deviceId} - ${stepLabel}...`);
    void recordDeviceAcceptance(deviceId, stepLabel, passed, note, evidencePath)
      .then((report) => {
        setProductionReadiness(report);
        setCommandNotice(`Recorded ${deviceId} / ${stepLabel}: ${passed ? "pass" : "fail"}.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Device acceptance recording failed.");
      });
  };

  const refreshIntegrationEvents = () => {
    void getRecentIntegrationEvents().then(setIntegrationEvents);
  };

  const saveVmixConfig = (config: VmixConfig) => {
    void updateVmixConfig(config)
      .then((status) => {
        setVmixStatus(status);
        setCommandNotice(status.detail);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix configuration save failed.");
      });
  };

  const handleVerifyPluginManifest = (manifestPath: string, trustedKeyIds: string[]) => {
    setCommandNotice("Verifying plugin manifest...");
    void verifyPluginManifest(manifestPath, trustedKeyIds)
      .then((result) => {
        setPluginVerification(result);
        setCommandNotice(result.detail);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Plugin verification failed.");
      });
  };

  const handleEnablePluginManifest = (manifestPath: string, trustedKeyIds: string[]) => {
    setCommandNotice("Verifying and enabling plugin manifest...");
    void enablePluginManifest(manifestPath, trustedKeyIds)
      .then((result) => {
        setPluginVerification(result);
        setCommandNotice(result.detail);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Plugin enablement failed.");
      });
  };

  const clearLive = () => {
    if (!liveCandidate) return;
    const cleared: ScriptureCandidate = { ...liveCandidate, status: "new", text: "", reference: "—", reason: "Live output cleared by operator." };
    setLiveCandidate(cleared);
    setCommandNotice("Live output cleared.");
  };
  clearLiveRef.current = clearLive;

  const blackout = () => {
    if (!liveCandidate) return;
    const black: ScriptureCandidate = { ...liveCandidate, status: "new", text: "", reference: "—", reason: "Safety blackout applied by operator." };
    setLiveCandidate(black);
    setPreviewCandidate(previewCandidate ? { ...previewCandidate, status: "new" } : null);
    setCommandNotice("Safety blackout applied. Both preview and live are cleared.");
  };

  const stageDisplay = () => {
    if (!previewCandidate) return;
    const mirrored: ScriptureCandidate = { ...previewCandidate, status: "live" };
    setLiveCandidate(mirrored);
    setCommandNotice(`Stage display: mirrored preview (${previewCandidate.reference}) to live.`);
  };

  // NDI lower-third is not yet available — button is disabled in PreviewLiveOutput.
  const lowerThird = () => undefined;

  const calibrateCandidate = (
    candidate: ScriptureCandidate,
    outcome: "confirmed" | "corrected" | "rejected"
  ) => {
    const transcriptText = transcript.map((s) => s.text).join(" ").slice(0, 4000);
    void recordCalibrationSample(
      candidate.language ?? "en",
      transcriptText,
      candidate.reference,
      outcome,
      candidate.reference
    )
      .then(() => setCommandNotice(`Calibration recorded: ${outcome} — ${candidate.reference}.`))
      .catch((error: unknown) =>
        setCommandNotice(error instanceof Error ? error.message : "Calibration save failed.")
      );
  };

  const rejectCandidate = (candidate: ScriptureCandidate) => {
    removeCandidate(candidate.id);
    setCommandNotice(`Rejected ${candidate.reference}. Removed from queue.`);
    if (candidate.id.includes("#")) {
      void rejectCandidateCmd(candidate.id).catch(() => undefined);
    }
  };

  const panicClearOutputs = () => {
    void clearAllOutputsCmd("operator-ui")
      .then((targets) => {
        setLiveCandidate(null);
        setDestinationsArmedAction(false);
        setCommandNotice(
          targets.length > 0
            ? `Panic clear: cleared ${targets.join(", ")}.`
            : "Panic clear sent (no outputs reported).",
        );
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Panic clear failed.");
      });
  };

  const approveCandidate = (candidate: ScriptureCandidate) => {
    const next: ScriptureCandidate = { ...candidate, status: "approved" };
    setSelectedCandidateAction(next);
    setCommandNotice(`Approved ${candidate.reference}.`);
    if (candidate.id.includes("#")) {
      void approveCandidateCmd(candidate.id).catch(() => undefined);
    }
  };

  // Global panic-clear hotkey: Ctrl+Shift+. — chosen to avoid OS clashes and
  // be reachable one-handed in a panic. Always-on while the app is focused.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && (e.key === "." || e.code === "Period")) {
        e.preventDefault();
        panicClearOutputs();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // Also expose via window for tests/DevTools.
  if (typeof window !== "undefined") {
    (window as unknown as { aletheiaPanicClear?: () => void }).aletheiaPanicClear = panicClearOutputs;
  }

  const selectTranscriptSegment = (segment: TranscriptSegment) => {
    setSearchQuery(segment.text.slice(0, 60));
    setActiveScreen("search");
    setCommandNotice(`Seeded search from transcript: "${segment.text.slice(0, 40)}…"`);
  };

  const checkVmix = () => {
    void getVmixStatus()
      .then((status) => {
        setVmixStatus(status);
        setCommandNotice(status.detail);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix status check failed.");
      });
  };

  const sendPreviewToVmix = () => {
    if (!previewCandidate) return;
    void sendVmixPreview(previewCandidate)
      .then((result) => {
        setVmixStatus({ ...vmixStatus!, state: result.state, detail: result.detail, checkedAtMs: Date.now() });
        setCommandNotice(result.detail);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix preview failed.");
      });
  };

  const sendLiveToVmix = () => {
    if (!previewCandidate) return;
    void sendVmixLive(previewCandidate, destinationsArmed)
      .then((result) => {
        setLiveCandidate({ ...previewCandidate!, status: "live" });
        setVmixStatus({ ...vmixStatus!, state: result.state, detail: result.detail, checkedAtMs: Date.now() });
        setCommandNotice(result.detail);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix live output failed.");
      });
  };

  const clearVmix = () => {
    void clearVmixOverlay()
      .then((result) => {
        setVmixStatus({ ...vmixStatus!, state: result.state, detail: result.detail, checkedAtMs: Date.now() });
        setCommandNotice(result.detail);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix clear failed.");
      });
  };

  // OBS handlers
  const saveObsConfig = (config: ObsConfig) => {
    void updateObsConfig(config)
      .then((result) => { setObsStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS configuration save failed."));
  };
  const checkObs = () => {
    void getObsStatus().then((result) => { setObsStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS status check failed."));
  };
  const sendPreviewToObs = () => {
    if (!previewCandidate) return;
    void sendObsPreview(previewCandidate).then((result) => { setObsStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS preview failed."));
  };
  const sendLiveToObs = () => {
    if (!previewCandidate) return;
    void sendObsLive(previewCandidate, destinationsArmed).then((result) => { setObsStatus(result); setLiveCandidate({ ...previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS live output failed."));
  };
  const clearObs = () => {
    void clearObsOutput().then((result) => { setObsStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS clear failed."));
  };

  // ProPresenter handlers
  const saveProPresenterConfig = (config: ProPresenterConfig) => {
    void updateProPresenterConfig(config)
      .then((result) => { setProPresenterStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter configuration save failed."));
  };
  const checkProPresenter = () => {
    void getProPresenterStatus().then((result) => { setProPresenterStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter status check failed."));
  };
  const sendPreviewToProPresenter = () => {
    if (!previewCandidate) return;
    void sendProPresenterPreview(previewCandidate).then((result) => { setProPresenterStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter preview failed."));
  };
  const sendLiveToProPresenter = () => {
    if (!previewCandidate) return;
    void sendProPresenterLive(previewCandidate, destinationsArmed).then((result) => { setProPresenterStatus(result); setLiveCandidate({ ...previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter live output failed."));
  };
  const clearProPresenter = () => {
    void clearProPresenterOutput().then((result) => { setProPresenterStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter clear failed."));
  };

  // Companion handlers
  const saveCompanionConfig = (config: CompanionConfig) => {
    void updateCompanionConfig(config).then((result) => { setCompanionStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion configuration save failed."));
  };
  const checkCompanion = () => {
    void getCompanionStatus().then((result) => { setCompanionStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion status check failed."));
  };
  const sendPreviewToCompanion = () => {
    if (!previewCandidate) return;
    void sendCompanionPreview(previewCandidate).then((result) => { setCompanionStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion preview failed."));
  };
  const sendLiveToCompanion = () => {
    if (!previewCandidate) return;
    void sendCompanionLive(previewCandidate, destinationsArmed).then((result) => { setCompanionStatus(result); setLiveCandidate({ ...previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion live output failed."));
  };
  const clearCompanion = () => {
    void clearCompanionOutput().then((result) => { setCompanionStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion clear failed."));
  };

  // Operator identity
  const saveOperator = (name: string) => {
    void setOperatorName(name)
      .then(() => { setOperatorNameAction(name); setCommandNotice(`Operator name saved: ${name}`); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Could not save operator name."));
  };

  // Trusted plugin revoke
  const handleRevokePlugin = (pluginId: string) => {
    void revokeTrustedPlugin(pluginId)
      .then(() => {
        setTrustedPlugins((prev) => prev.filter((p) => p.id !== pluginId));
        setCommandNotice("Plugin trust revoked.");
      })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Could not revoke plugin."));
  };

  // OSC handlers
  const saveOscConfig = (config: OscConfig) => {
    void updateOscConfig(config).then((result) => { setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC configuration save failed."));
  };
  const checkOsc = () => {
    void getOscStatus().then((result) => { setOscStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC status check failed."));
  };
  const sendPreviewToOsc = () => {
    if (!previewCandidate) return;
    void sendOscPreview(previewCandidate).then((result) => { setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC preview failed."));
  };
  const sendLiveToOsc = () => {
    if (!previewCandidate) return;
    void sendOscLive(previewCandidate, destinationsArmed).then((result) => { setOscStatus(result); setLiveCandidate({ ...previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC live output failed."));
  };
  const clearOsc = () => {
    void clearOscOutput().then((result) => { setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC clear failed."));
  };
  const sendOscPing = () => {
    void sendOscTestPing().then((result) => { setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC ping failed."));
  };

  // EasyWorship handlers
  const saveEasyWorshipConfig = (config: EasyWorshipConfig) => {
    void updateEasyWorshipConfig(config).then((result) => { setEasyWorshipStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship configuration save failed."));
  };
  const checkEasyWorship = () => {
    void getEasyWorshipStatus().then((result) => { setEasyWorshipStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship status check failed."));
  };
  const sendPreviewToEasyWorship = () => {
    if (!previewCandidate) return;
    void sendEasyWorshipPreview(previewCandidate).then((result) => { setEasyWorshipStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship preview failed."));
  };
  const sendLiveToEasyWorship = () => {
    if (!previewCandidate) return;
    void sendEasyWorshipLive(previewCandidate, destinationsArmed).then((result) => { setEasyWorshipStatus(result); setLiveCandidate({ ...previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship live output failed."));
  };
  const clearEasyWorship = () => {
    void clearEasyWorshipOutput().then((result) => { setEasyWorshipStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship clear failed."));
  };

  if (activeScreen === "landing") {
    return <LandingPage onOpen={setActiveScreen} />;
  }

  return (
    <ErrorBoundary
      fallback={
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
                      height: "100vh", gap: 16, color: "#f87171", background: "#0d0d14", fontFamily: "sans-serif" }}>
          <span style={{ fontSize: 48 }}>&#x26A0;&#xFE0F;</span>
          <strong style={{ fontSize: 20 }}>Aletheia encountered an error</strong>
          <p style={{ color: "#94a3b8", maxWidth: 360, textAlign: "center" }}>
            An unexpected error occurred in the interface. Your session data is safe.
          </p>
          <button
            onClick={() => window.location.reload()}
            style={{ padding: "10px 24px", borderRadius: 8, background: "#4f46e5",
                     color: "#fff", border: "none", cursor: "pointer", fontSize: 14 }}
          >
            Reload Aletheia
          </button>
        </div>
      }
    >
    <WorkspaceShell
      active={activeScreen}
      onNavigate={setActiveScreen}
      title={activeMeta.title}
      preview={previewCandidate!}
      live={liveCandidate!}
      armed={destinationsArmed}
      desktopStatus={desktopStatus || undefined}
      onToggleArmed={toggleArmed}
      onSendLive={() => sendLive()}
      onManualSearch={() => setActiveScreen("search")}
    >
      {activeScreen === "dashboard" && selectedCandidate ? (
        <Suspense fallback={<LazyFallback />}>
        <OperatorDashboard
          candidate={selectedCandidate}
          activeBibleCandidate={previewCandidate ?? selectedCandidate}
          previewCandidate={previewCandidate ?? selectedCandidate}
          liveCandidate={liveCandidate ?? selectedCandidate}
          suggestions={dashboardSuggestions}
          notice={listenerStatus}
          onOpenCommand={openScriptureCommand}
          onPreviewVerse={previewVerseFromDashboard}
          onPreviewSuggestion={preview}
          onLiveSuggestion={sendLive}
          captureState={captureState}
          captureRunning={captureRunning}
          captureStarting={captureStarting}
          listenerStatus={listenerStatus}
          selectedAudioDevice={selectedAudioDevice}
          audioLevel={lastAudioLevel}
          healthItems={healthItems}
          onOpenCaptureDiagnostics={() => setActiveScreen("transcript")}
          onSendLive={() => sendLive(previewCandidate ?? selectedCandidate)}
        />
        </Suspense>
      ) : null}
      {activeScreen === "transcript" ? (
        <>
          <div className="mb-4">
            <CaptureControlPanel
              sttStatus={sttStatus}
              latencyProfile={sttLatencyProfile}
              audioDevices={audioDevices}
              selectedAudioDevice={selectedAudioDevice}
              captureRunning={captureRunning}
              captureStarting={captureStarting}
              captureState={captureState}
              listenerStatus={listenerStatus}
              notice={commandNotice}
              onSelectDevice={(device) => {
                setSelectedAudioDevice(device);
              }}
              onStartCapture={startDashboardCapture}
              onStopCapture={stopCapture}
              onReloadModel={refreshSttModel}
              onRefreshDevices={refreshAudioDevices}
            />
          </div>
          <VuMeter active={captureRunning || captureStarting} deviceLabel={selectedAudioDevice} backendLevel={lastAudioLevel} />
          {audioDevices.length > 1 && (
            <div style={{
              display: "flex", alignItems: "center", gap: "10px",
              padding: "8px 18px", background: "rgba(255,255,255,0.04)",
              borderBottom: "1px solid rgba(255,255,255,0.07)", fontSize: "13px"
            }}>
              <label htmlFor="audio-device-picker" style={{ color: "var(--text-muted, #8fa)", whiteSpace: "nowrap" }}>
                🎤 Input Device:
              </label>
              <select
                id="audio-device-picker"
                value={selectedAudioDevice ?? ""}
                onChange={(e) => {
                  const val = e.target.value || undefined;
                  setSelectedAudioDevice(val);
                }}
                style={{
                  background: "rgba(0,0,0,0.4)", color: "#e0ffe0", border: "1px solid rgba(255,255,255,0.15)",
                  borderRadius: "6px", padding: "4px 10px", flex: 1, maxWidth: "360px", cursor: "pointer"
                }}
              >
                <option value="">OS Default</option>
                {audioDevices.map((d) => (
                  <option key={d} value={d}>{d}</option>
                ))}
              </select>
            </div>
          )}
          <LiveTranscriptView
            transcript={transcript}
            candidates={candidates}
            onPreview={preview}
            onSelectSegment={selectTranscriptSegment}
          />
        </>
      ) : null}
      {activeScreen === "queue" && selectedCandidate ? (
        <QueueApprovalPanel
          activeCandidate={selectedCandidate}
          candidates={candidates}
          onPreview={preview}
          onLive={sendLive}
          onReject={rejectCandidate}
          onApprove={approveCandidate}
          onMerge={(candidate) => mergeCandidates([candidate])}
          onCalibrate={calibrateCandidate}
        />
      ) : null}
      {activeScreen === "output" && previewCandidate && liveCandidate ? (
        <PreviewLiveOutput
          preview={previewCandidate}
          live={liveCandidate}
          armed={destinationsArmed}
          integrations={integrations}
          onSendLive={() => sendLive()}
          onToggleArmed={toggleArmed}
          onClearLive={clearLive}
          onBlackout={blackout}
          onStageDisplay={stageDisplay}
          onLowerThird={lowerThird}
        />
      ) : null}
      {activeScreen === "theme" && previewCandidate ? (
        <Suspense fallback={<LazyFallback />}>
        <ThemeDesigner
          selectedTheme={selectedTheme}
          onSelectTheme={setSelectedTheme}
          preview={previewCandidate}
          onPublish={(theme) => {
            // Persist active theme across sessions via localStorage
            if (typeof localStorage !== "undefined") {
              localStorage.setItem("aletheia:activeThemeId", theme.id);
            }
            setSelectedTheme(theme);
            setCommandNotice(`Theme "${theme.name}" saved and active. Live output will use this theme on next send.`);
          }}
        />
        </Suspense>
      ) : null}
      {activeScreen === "integrations" && vmixStatus ? (
        <Suspense fallback={<LazyFallback />}>
        <IntegrationsSettings
          integrations={integrations}
          vmixStatus={vmixStatus}
          integrationEvents={integrationEvents}
          obsStatus={obsStatus || undefined}
          proPresenterStatus={proPresenterStatus || undefined}
          companionStatus={companionStatus || undefined}
          oscStatus={oscStatus || undefined}
          easyWorshipStatus={easyWorshipStatus || undefined}
          operatorName={operatorName}
          trustedPlugins={trustedPlugins}
          desktopStatus={desktopStatus}
          onSaveVmixConfig={saveVmixConfig}
          onCheckVmix={checkVmix}
          onSendVmixPreview={sendPreviewToVmix}
          onSendVmixLive={sendLiveToVmix}
          onClearVmix={clearVmix}
          onSaveObsConfig={saveObsConfig}
          onCheckObs={checkObs}
          onSendObsPreview={sendPreviewToObs}
          onSendObsLive={sendLiveToObs}
          onClearObs={clearObs}
          onSaveProPresenterConfig={saveProPresenterConfig}
          onCheckProPresenter={checkProPresenter}
          onSendProPresenterPreview={sendPreviewToProPresenter}
          onSendProPresenterLive={sendLiveToProPresenter}
          onClearProPresenter={clearProPresenter}
          onSaveCompanionConfig={saveCompanionConfig}
          onCheckCompanion={checkCompanion}
          onSendCompanionPreview={sendPreviewToCompanion}
          onSendCompanionLive={sendLiveToCompanion}
          onClearCompanion={clearCompanion}
          onSaveOscConfig={saveOscConfig}
          onCheckOsc={checkOsc}
          onSendOscPreview={sendPreviewToOsc}
          onSendOscLive={sendLiveToOsc}
          onSendOscPing={sendOscPing}
          onClearOsc={clearOsc}
          onSaveEasyWorshipConfig={saveEasyWorshipConfig}
          onCheckEasyWorship={checkEasyWorship}
          onSendEasyWorshipPreview={sendPreviewToEasyWorship}
          onSendEasyWorshipLive={sendLiveToEasyWorship}
          onClearEasyWorship={clearEasyWorship}
          onExportBoothPack={createBoothPack}
          boothPackExport={boothPackExport}
          onVerifyPluginManifest={handleVerifyPluginManifest}
          onEnablePluginManifest={handleEnablePluginManifest}
          pluginVerification={pluginVerification || undefined}
          onSaveOperatorName={saveOperator}
          onRevokePlugin={handleRevokePlugin}
          onSetCloudAiEnabled={setCloudAiEnabled}
        />
        </Suspense>
      ) : null}
      {activeScreen === "health" && productionReadiness ? (
        <Suspense fallback={<LazyFallback />}>
        <div className="space-y-7">
        <HealthStatusPanel
          items={healthItems}
          readiness={productionReadiness}
          backendDiagnostics={backendDiagnostics || undefined}
          releaseGate={releaseGate || undefined}
          supportBundleExport={supportBundleExport || undefined}
          rehearsalReport={rehearsalReport || undefined}
          offlinePackExport={offlinePackExport || undefined}
          onRunCheck={runHealthCheck}
          onRunLocalRehearsal={runRehearsal}
          onExportSupportBundle={createSupportBundle}
          onExportOfflinePack={createOfflinePack}
          onInstallOfflineAsset={handleInstallOfflineAsset}
          onInstallOfflineAssetFromPath={handleInstallOfflineAssetFromPath}
          onRecordDeviceAcceptance={handleRecordDeviceAcceptance}
        />
        <HardwareChecklistPanel />
        </div>
        </Suspense>
      ) : null}
      {activeScreen === "onboarding" ? (
        <Suspense fallback={<LazyFallback />}>
          <OnboardingFlow onContinue={() => setActiveScreen("health")} />
        </Suspense>
      ) : null}
      {activeScreen === "stream" && liveCandidate ? (
        <Suspense fallback={<LazyFallback />}>
          <StreamOverlayPanel live={liveCandidate} />
        </Suspense>
      ) : null}
      {activeScreen === "songs" ? (
        <Suspense fallback={<LazyFallback />}>
          <SongLibraryPanel
            serviceSessionId={desktopStatus?.serviceSession ?? "browser-session"}
            operator={operatorName || "operator"}
            onSendSectionLive={(song, section) => {
              const synthetic: ScriptureCandidate = {
                id: `song-${song.id}-${Date.now()}`,
                reference: `${song.title}${section.label ? ` — ${section.label}` : ""}`,
                translation: song.songKey ? `Key ${song.songKey}` : "Lyrics",
                language: song.language,
                text: section.text,
                confidence: 100,
                source: song.ccliNumber ? `CCLI ${song.ccliNumber}` : "Local library",
                reason: "Song section sent live by operator.",
                status: "live"
              };
              setSelectedCandidateAction(synthetic);
              setPreviewCandidate({ ...synthetic, status: "preview" });
              sendLive(synthetic);
              if (song.ccliNumber) {
                void logCcliUsage(
                  song.ccliNumber,
                  song.title,
                  desktopStatus?.serviceSession ?? "browser-session",
                  operatorName || "operator"
                ).catch(() => undefined);
              }
            }}
          />
        </Suspense>
      ) : null}
      {activeScreen === "fleet" ? (
        <Suspense fallback={<LazyFallback />}>
          <FleetSyncPanel deviceLabel={desktopStatus?.serviceSession ?? "Browser device"} />
        </Suspense>
      ) : null}
      {activeScreen === "clips" && liveCandidate ? (
        <Suspense fallback={<LazyFallback />}>
          <ClipEdlPanel
            live={liveCandidate}
            serviceStartedAtMs={desktopStatus?.checkedAtMs ?? Date.now()}
          />
        </Suspense>
      ) : null}
      {activeScreen === "search" ? (
        <Suspense fallback={<LazyFallback />}>
        <ManualSearchFallback
          query={searchQuery}
          results={searchResults}
          onQueryChange={setSearchQuery}
          onPreview={preview}
          onLive={sendLive}
        />
        </Suspense>
      ) : null}
      {activeScreen === "bible" ? (
        <Suspense fallback={<LazyFallback />}>
          <BibleReaderPanel
            onPreviewVerse={(reference, text, translation) => {
              const candidate = {
                id: `bible-${reference.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
                reference,
                translation,
                language: "English",
                text,
                confidence: 100,
                source: "Bible Reader",
                reason: `Opened manually from the Bible Reader.`,
                status: "preview" as const,
              };
              mergeCandidates([candidate]);
              setSelectedCandidateAction(candidate);
              setPreviewCandidate(candidate);
              setCommandNotice(`Preview prepared: ${reference} (${translation}).`);
              void renderPreviewScene(candidate).catch(() => undefined);
            }}
            onLiveVerse={(reference, text, translation) => {
              const candidate = {
                id: `bible-${reference.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
                reference,
                translation,
                language: "English",
                text,
                confidence: 100,
                source: "Bible Reader",
                reason: `Sent live from the Bible Reader.`,
                status: "live" as const,
              };
              mergeCandidates([candidate]);
              setSelectedCandidateAction(candidate);
              setPreviewCandidate({ ...candidate, status: "preview" });
              sendLive(candidate);
            }}
          />
        </Suspense>
      ) : null}
      <NoticeToast notice={commandNotice} />
    </WorkspaceShell>
    </ErrorBoundary>
  );
}

function LazyFallback() {
  return (
    <div className="flex h-full min-h-[240px] items-center justify-center text-sm text-muted">
      <span className="inline-flex items-center gap-2">
        <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-violet-400" />
        Loading screen…
      </span>
    </div>
  );
}

function NoticeToast({ notice }: { notice: string }) {
  const [visible, setVisible] = useState(false);
  const lastNoticeRef = useRef<string>("");

  useEffect(() => {
    if (!notice || notice === lastNoticeRef.current) return;
    lastNoticeRef.current = notice;
    setVisible(true);
    const timer = window.setTimeout(() => setVisible(false), 4000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const lower = notice.toLowerCase();
  const isError =
    lower.includes("fail") ||
    lower.includes("blocked") ||
    lower.includes("could not") ||
    lower.includes("unavailable");

  return (
    visible ? (
      <div
        role="status"
        aria-live="polite"
        className={`pointer-events-none fixed bottom-6 right-6 z-50 max-w-sm rounded-[8px] border px-4 py-3 text-sm backdrop-blur-xl transition-opacity duration-150 ${
          isError
            ? "border-rose-500/40 bg-rose-950/90 text-rose-100 shadow-[0_10px_40px_-10px_rgba(239,68,68,0.4)]"
            : "border-violet-500/30 bg-slate-900/95 text-white shadow-[0_10px_40px_-10px_rgba(124,58,237,0.4)]"
        }`}
      >
        {notice}
      </div>
    ) : null
  );
}
