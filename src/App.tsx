import { lazy, Suspense, useEffect, useMemo, useRef, useState, useDeferredValue } from "react";
import { LandingPage } from "./components/LandingPage";
import { LiveTranscriptView } from "./components/LiveTranscriptView";
import { VuMeter } from "./components/VuMeter";
import { PreviewLiveOutput } from "./components/PreviewLiveOutput";
import { QueueApprovalPanel } from "./components/QueueApprovalPanel";
import { WorkspaceShell } from "./components/WorkspaceShell";

// Heavy/rarely-hot screens — defer until the operator navigates to them.
const HealthStatusPanel = lazy(() =>
  import("./components/HealthStatusPanel").then((m) => ({ default: m.HealthStatusPanel }))
);
const IntegrationsSettings = lazy(() =>
  import("./components/IntegrationsSettings").then((m) => ({ default: m.IntegrationsSettings }))
);
const ManualSearchFallback = lazy(() =>
  import("./components/ManualSearchFallback").then((m) => ({ default: m.ManualSearchFallback }))
);
const OnboardingFlow = lazy(() =>
  import("./components/OnboardingFlow").then((m) => ({ default: m.OnboardingFlow }))
);
const OperatorDashboard = lazy(() =>
  import("./components/OperatorDashboard").then((m) => ({ default: m.OperatorDashboard }))
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
  revokeTrustedPlugin,
  setOperatorName,
  showMainWindow,
  startAudioCapture,
  listAudioDevices,
  generateEasyWorshipSmbSetupGuide,
  getVmixTitleSetupGuide,
  stopAudioCapture,
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
  installOfflineAsset,
  installOfflineAssetFromPath,
  recordDeviceAcceptance,
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
  updateVmixConfig,
  fetchVerseOnDemand,
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
import { AnimatePresence, motion } from "framer-motion";

export default function App() {
  const [activeScreen, setActiveScreen] = useState<ScreenKey>("dashboard");
  const [searchQuery, setSearchQuery] = useState("jn 3 16");
  const [searchResults, setSearchResults] = useState<ManualSearchResult[]>(manualSearchResults);
  const savedThemeId = typeof localStorage !== "undefined" ? localStorage.getItem("aletheia:activeThemeId") : null;
  const initialTheme = savedThemeId ? (themes.find(t => t.id === savedThemeId) ?? themes[0]) : themes[0];
  const [selectedTheme, setSelectedTheme] = useState(initialTheme);
  const [commandNotice, setCommandNotice] = useState("Loading local-first desktop services.");

  // Modals / Specific exports
  const [pluginVerification, setPluginVerification] = useState<PluginVerificationResult | null>(null);
  const [supportBundleExport, setSupportBundleExport] = useState<SupportBundleExport | null>(null);
  const [offlinePackExport, setOfflinePackExport] = useState<OfflinePackExport | null>(null);
  const [boothPackExport, setBoothPackExport] = useState<BoothPackExport | null>(null);
  const [rehearsalReport, setRehearsalReport] = useState<LocalRehearsalReport | null>(null);

  const { t } = useTranslation();
  const activeMeta = { 
    title: t(`navigation.${activeScreen}.title` as any, { defaultValue: activeScreen }), 
    kicker: t(`navigation.${activeScreen}.kicker` as any) 
  };
  const currentIndex = useMemo(() => screenOrder.indexOf(activeScreen), [activeScreen]);

  // Zustand State hooks
  const desktop = useDesktopStore();
  const hw = useHardwareStore();
  const translation = useTranslationStore();

  // Auto-route detected languages into the translation target list whenever
  // a fresh AI-detection result arrives (no-op if the operator left the
  // "auto-route" toggle off).
  useEffect(() => {
    const codes = desktop.aiDetection?.languages?.map((l) => l.code) ?? [];
    if (codes.length === 0) return;
    translation.applyDetectedLanguages(codes);
  }, [desktop.aiDetection?.checkedAtMs, translation]);

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
      ]);
    })();
  }, []);
  
  // Attach realtime backend listeners
  useTauriEvents(setCommandNotice);

  // Trusted plugins state
  const [trustedPlugins, setTrustedPlugins] = useState<TrustedPlugin[]>([]);
  const [audioDevices, setAudioDevices] = useState<string[]>([]);
  const [selectedAudioDevice, setSelectedAudioDevice] = useState<string | undefined>(undefined);
  const [captureRunning, setCaptureRunning] = useState(false);
  const [onDemandVerse, setOnDemandVerse] = useState<Record<string, { text: string; source: string }>>({});
  const [fetchingVerse, setFetchingVerse] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    // Reveal the window as early as possible — Tauri starts with visible:false
    // to avoid the Win32 "(Not Responding)" freeze during WebView2 init.
    void showMainWindow();

    getDesktopServiceState()
      .then((state) => {
        if (cancelled) return;
        desktop.setCandidates(state.candidates);
        desktop.setTranscript(state.transcript);
        hw.setIntegrations(state.integrations);
        hw.setHealthItems(state.health);
        desktop.setPreviewCandidate(state.preview);
        desktop.setLiveCandidate(state.live);
        desktop.setSelectedCandidate(state.preview);
        desktop.setDestinationsArmed(state.session.destinationsArmed);
        desktop.setDesktopStatus(state.session);
        setCommandNotice(
          state.session.mode === "tauri"
            ? "Desktop core online. Local SQLite, audit, and command policy are active."
            : "Browser fallback active. Desktop commands will engage inside Tauri."
        );
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setCommandNotice(error instanceof Error ? error.message : "Desktop services are unavailable.");
      });

    getVmixConfig().then((config) => {
      if (!cancelled) {
        hw.setVmixStatus({ 
          state: "offline", 
          checkedAtMs: Date.now(), 
          detail: "", 
          endpoint: "", 
          ...config 
        });
      }
    });

    getVmixStatus().then((status) => {
      if (!cancelled) hw.setVmixStatus(status);
    });

    getRecentIntegrationEvents().then((events) => {
      if (!cancelled) hw.setIntegrationEvents(events);
    });

    getProductionReadiness().then((report) => {
      if (!cancelled) hw.setProductionReadiness(report);
    });

    analyzeTranscript().then((result) => {
      if (cancelled) return;
      desktop.setAiDetection(result);
      desktop.mergeCandidates(result.candidates);
      if (result.candidates[0]) {
        desktop.setSelectedCandidate(result.candidates[0]);
        desktop.setPreviewCandidate({ ...result.candidates[0], status: "preview" });
      }
    });

    getObsStatus().then((status) => {
      if (!cancelled) hw.setObsStatus(status);
    });

    getOscStatus().then((status) => {
      if (!cancelled) hw.setOscStatus(status);
    });

    getEasyWorshipStatus().then((status) => {
      if (!cancelled) hw.setEasyWorshipStatus(status);
    });

    getProPresenterStatus().then((status) => {
      if (!cancelled) hw.setProPresenterStatus(status);
    });

    getCompanionStatus().then((status) => {
      if (!cancelled) hw.setCompanionStatus(status);
    });

    getOperatorName().then((name) => {
      if (!cancelled) hw.setOperatorName(name);
    });

    listTrustedPlugins().then((plugins) => {
      if (!cancelled) setTrustedPlugins(plugins);
    });

    listAudioDevices().then((devices) => {
      if (!cancelled) setAudioDevices(devices);
    });

    return () => {
      cancelled = true;
    };
  }, []);

  const captureStartedRef = useRef(false);
  useEffect(() => {
    if (activeScreen !== "transcript" || captureStartedRef.current) return;
    captureStartedRef.current = true;
    // Pass undefined so Whisper auto-detects language; a future service profile
    // picker can supply an explicit hint via a state variable here.
    void startAudioCapture(undefined, selectedAudioDevice)
      .then((modelPath) => {
        setCaptureRunning(true);
        setCommandNotice(`Live capture armed (${modelPath.split(/[\\/]/).pop() ?? "model"}).`);
      })
      .catch((error: unknown) => {
        captureStartedRef.current = false;
        setCaptureRunning(false);
        setCommandNotice(error instanceof Error ? error.message : "Could not start live capture.");
      });
  }, [activeScreen]);

  useEffect(() => {
    return () => {
      if (captureStartedRef.current) {
        void stopAudioCapture().catch(() => undefined);
        setCaptureRunning(false);
      }
    };
  }, []);

  useEffect(() => {
    let vmixWasConnected = false;
    const interval = window.setInterval(() => {
      void analyzeTranscript()
        .then((result) => {
          desktop.setAiDetection(result);
          desktop.mergeCandidates(result.candidates);
        })
        .catch(() => undefined);
      void getDesktopServiceState()
        .then((state) => {
          desktop.setTranscript(state.transcript);
        })
        .catch(() => undefined);

      // vMix auto-reconnect: if vMix was connected but is now offline,
      // silently re-check every 30 s so mid-service crashes self-heal.
      const isConnected = hw.vmixStatus?.state === "connected" || hw.vmixStatus?.state === "ready";
      if (vmixWasConnected && !isConnected) {
        void getVmixStatus()
          .then((status) => {
            hw.setVmixStatus(status);
            if (status.state === "connected" || status.state === "ready") {
              setCommandNotice("vMix reconnected automatically.");
            }
          })
          .catch(() => undefined);
      }
      vmixWasConnected = isConnected;
    }, 8000);
    return () => window.clearInterval(interval);
  }, []);

  const deferredSearchQuery = useDeferredValue(searchQuery);

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

  useEffect(() => {
    let cancelled = false;
    searchScripture(deferredSearchQuery).then((results) => {
      if (!cancelled) setSearchResults(results);
    });

    return () => {
      cancelled = true;
    };
  }, [deferredSearchQuery]);

  const lastEscRef = useRef<number>(0);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Guard: do not intercept keyboard commands if the operator is typing in a search box or text field
      const activeTag = document.activeElement?.tagName;
      if (activeTag === "INPUT" || activeTag === "TEXTAREA") {
        if (event.key === "Escape") {
          (document.activeElement as HTMLElement).blur();
        }
        return;
      }

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setActiveScreen("search");
      }

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "l") {
        event.preventDefault();
        if (desktop.previewCandidate) sendLive(desktop.previewCandidate);
      }

      if (event.altKey && event.key === "ArrowRight") {
        event.preventDefault();
        setActiveScreen(screenOrder[(currentIndex + 1) % screenOrder.length]);
      }

      if (event.key === "Escape") {
        const now = Date.now();
        if (now - lastEscRef.current < 600) {
          clearLive();
          lastEscRef.current = 0;
        } else {
          lastEscRef.current = now;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [currentIndex, desktop.destinationsArmed, desktop.previewCandidate]);

  const preview = (candidate: ScriptureCandidate) => {
    const nextPreview: ScriptureCandidate = { ...candidate, status: "preview" };
    desktop.setSelectedCandidate(candidate);
    desktop.setPreviewCandidate(nextPreview);
    setCommandNotice(`Preview prepared for ${candidate.reference}.`);

    void renderPreviewScene(nextPreview).catch((error: unknown) => {
      setCommandNotice(error instanceof Error ? error.message : "Preview render failed.");
    });
  };

  const sendLive = (candidate = desktop.previewCandidate) => {
    if (!candidate) return;
    if (!desktop.destinationsArmed) {
      setCommandNotice("Live output blocked. Arm destinations before sending.");
      return;
    }

    void sendLiveCandidate(candidate, desktop.destinationsArmed)
      .then((result) => {
        const previewState: ScriptureCandidate = { ...candidate, status: "preview" };
        const liveState: ScriptureCandidate = { ...candidate, status: "live" };
        desktop.setSelectedCandidate(liveState);
        desktop.setPreviewCandidate(previewState);
        desktop.setLiveCandidate(liveState);
        setActiveScreen("output");
        if (desktop.desktopStatus) {
           desktop.setDesktopStatus({...desktop.desktopStatus, auditCount: result.auditCount, lastEventSequence: result.auditCount, checkedAtMs: Date.now()});
        }
        setCommandNotice(`Live output sent: ${result.scene.reference} ${result.scene.translation}.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Live output failed.");
      });
  };

  const toggleArmed = () => {
    const next = !desktop.destinationsArmed;
    desktop.setDestinationsArmed(next);
    setCommandNotice(next ? "Destinations armed for explicit live output." : "Safe hold enabled. Live output is blocked.");

    void setDestinationsArmed(next)
      .then((status) => desktop.setDesktopStatus(status))
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Could not update destination arming.");
      });
  };

  const runHealthCheck = () => {
    void runPreServiceCheck()
      .then((items) => {
        hw.setHealthItems(items);
        if (desktop.desktopStatus) desktop.setDesktopStatus({ ...desktop.desktopStatus, checkedAtMs: Date.now() });
        setCommandNotice("Pre-service check completed against the local core.");
        return getProductionReadiness();
      })
      .then((report) => {
        hw.setProductionReadiness(report);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Pre-service check failed.");
      });
  };

  const runAiAssist = () => {
    void analyzeTranscript()
      .then((result) => {
        desktop.setAiDetection(result);
        desktop.mergeCandidates(result.candidates);
        if (result.candidates[0]) {
          desktop.setSelectedCandidate(result.candidates[0]);
          desktop.setPreviewCandidate({ ...result.candidates[0], status: "preview" });
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
      .then(hw.setProductionReadiness)
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
        hw.setProductionReadiness(report);
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
        hw.setProductionReadiness(report);
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
        hw.setProductionReadiness(report);
        setCommandNotice(`Recorded ${deviceId} / ${stepLabel}: ${passed ? "pass" : "fail"}.`);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "Device acceptance recording failed.");
      });
  };

  const refreshIntegrationEvents = () => {
    void getRecentIntegrationEvents().then(hw.setIntegrationEvents);
  };

  const saveVmixConfig = (config: VmixConfig) => {
    void updateVmixConfig(config)
      .then((status) => {
        hw.setVmixStatus(status);
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
    if (!desktop.liveCandidate) return;
    const cleared: ScriptureCandidate = { ...desktop.liveCandidate, status: "new", text: "", reference: "—", reason: "Live output cleared by operator." };
    desktop.setLiveCandidate(cleared);
    setCommandNotice("Live output cleared.");
  };

  const blackout = () => {
    if (!desktop.liveCandidate) return;
    const black: ScriptureCandidate = { ...desktop.liveCandidate, status: "new", text: "", reference: "—", reason: "Safety blackout applied by operator." };
    desktop.setLiveCandidate(black);
    desktop.setPreviewCandidate(desktop.previewCandidate ? { ...desktop.previewCandidate, status: "new" } : null);
    setCommandNotice("Safety blackout applied. Both preview and live are cleared.");
  };

  const stageDisplay = () => {
    if (!desktop.previewCandidate) return;
    const mirrored: ScriptureCandidate = { ...desktop.previewCandidate, status: "live" };
    desktop.setLiveCandidate(mirrored);
    setCommandNotice(`Stage display: mirrored preview (${desktop.previewCandidate.reference}) to live.`);
  };

  // NDI lower-third is not yet available — button is disabled in PreviewLiveOutput.
  const lowerThird = () => undefined;

  const calibrateCandidate = (
    candidate: ScriptureCandidate,
    outcome: "confirmed" | "corrected" | "rejected"
  ) => {
    const transcriptText = desktop.transcript.map((s) => s.text).join(" ").slice(0, 4000);
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
    desktop.removeCandidate(candidate.id);
    setCommandNotice(`Rejected ${candidate.reference}. Removed from queue.`);
  };

  const selectTranscriptSegment = (segment: TranscriptSegment) => {
    setSearchQuery(segment.text.slice(0, 60));
    setActiveScreen("search");
    setCommandNotice(`Seeded search from transcript: "${segment.text.slice(0, 40)}…"`);
  };

  // VMIX Handlers
  const checkVmix = () => {
    void getVmixStatus()
      .then((status) => {
        hw.setVmixStatus(status);
        setCommandNotice(status.detail);
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix status check failed.");
      });
  };

  const sendPreviewToVmix = () => {
    if (!desktop.previewCandidate) return;
    void sendVmixPreview(desktop.previewCandidate)
      .then((result) => {
        hw.setVmixStatus({ ...hw.vmixStatus!, state: result.state, detail: result.detail, checkedAtMs: Date.now() });
        setCommandNotice(result.detail);
        refreshIntegrationEvents();
      })
      .catch((error: unknown) => {
        setCommandNotice(error instanceof Error ? error.message : "vMix preview failed.");
      });
  };

  const sendLiveToVmix = () => {
    if (!desktop.previewCandidate) return;
    void sendVmixLive(desktop.previewCandidate, desktop.destinationsArmed)
      .then((result) => {
        desktop.setLiveCandidate({ ...desktop.previewCandidate!, status: "live" });
        hw.setVmixStatus({ ...hw.vmixStatus!, state: result.state, detail: result.detail, checkedAtMs: Date.now() });
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
        hw.setVmixStatus({ ...hw.vmixStatus!, state: result.state, detail: result.detail, checkedAtMs: Date.now() });
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
      .then((result) => { hw.setObsStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS configuration save failed."));
  };
  const checkObs = () => {
    void getObsStatus().then((result) => { hw.setObsStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS status check failed."));
  };
  const sendPreviewToObs = () => {
    if (!desktop.previewCandidate) return;
    void sendObsPreview(desktop.previewCandidate).then((result) => { hw.setObsStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS preview failed."));
  };
  const sendLiveToObs = () => {
    if (!desktop.previewCandidate) return;
    void sendObsLive(desktop.previewCandidate, desktop.destinationsArmed).then((result) => { hw.setObsStatus(result); desktop.setLiveCandidate({ ...desktop.previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS live output failed."));
  };
  const clearObs = () => {
    void clearObsOutput().then((result) => { hw.setObsStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OBS clear failed."));
  };

  // ProPresenter handlers
  const saveProPresenterConfig = (config: ProPresenterConfig) => {
    void updateProPresenterConfig(config)
      .then((result) => { hw.setProPresenterStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter configuration save failed."));
  };
  const checkProPresenter = () => {
    void getProPresenterStatus().then((result) => { hw.setProPresenterStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter status check failed."));
  };
  const sendPreviewToProPresenter = () => {
    if (!desktop.previewCandidate) return;
    void sendProPresenterPreview(desktop.previewCandidate).then((result) => { hw.setProPresenterStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter preview failed."));
  };
  const sendLiveToProPresenter = () => {
    if (!desktop.previewCandidate) return;
    void sendProPresenterLive(desktop.previewCandidate, desktop.destinationsArmed).then((result) => { hw.setProPresenterStatus(result); desktop.setLiveCandidate({ ...desktop.previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter live output failed."));
  };
  const clearProPresenter = () => {
    void clearProPresenterOutput().then((result) => { hw.setProPresenterStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "ProPresenter clear failed."));
  };

  // Companion handlers
  const saveCompanionConfig = (config: CompanionConfig) => {
    void updateCompanionConfig(config).then((result) => { hw.setCompanionStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion configuration save failed."));
  };
  const checkCompanion = () => {
    void getCompanionStatus().then((result) => { hw.setCompanionStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion status check failed."));
  };
  const sendPreviewToCompanion = () => {
    if (!desktop.previewCandidate) return;
    void sendCompanionPreview(desktop.previewCandidate).then((result) => { hw.setCompanionStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion preview failed."));
  };
  const sendLiveToCompanion = () => {
    if (!desktop.previewCandidate) return;
    void sendCompanionLive(desktop.previewCandidate, desktop.destinationsArmed).then((result) => { hw.setCompanionStatus(result); desktop.setLiveCandidate({ ...desktop.previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion live output failed."));
  };
  const clearCompanion = () => {
    void clearCompanionOutput().then((result) => { hw.setCompanionStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "Companion clear failed."));
  };

  // Operator identity
  const saveOperator = (name: string) => {
    void setOperatorName(name)
      .then(() => { hw.setOperatorName(name); setCommandNotice(`Operator name saved: ${name}`); })
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
    void updateOscConfig(config).then((result) => { hw.setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC configuration save failed."));
  };
  const checkOsc = () => {
    void getOscStatus().then((result) => { hw.setOscStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC status check failed."));
  };
  const sendPreviewToOsc = () => {
    if (!desktop.previewCandidate) return;
    void sendOscPreview(desktop.previewCandidate).then((result) => { hw.setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC preview failed."));
  };
  const sendLiveToOsc = () => {
    if (!desktop.previewCandidate) return;
    void sendOscLive(desktop.previewCandidate, desktop.destinationsArmed).then((result) => { hw.setOscStatus(result); desktop.setLiveCandidate({ ...desktop.previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC live output failed."));
  };
  const clearOsc = () => {
    void clearOscOutput().then((result) => { hw.setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC clear failed."));
  };
  const sendOscPing = () => {
    void sendOscTestPing().then((result) => { hw.setOscStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "OSC ping failed."));
  };

  // EasyWorship handlers
  const saveEasyWorshipConfig = (config: EasyWorshipConfig) => {
    void updateEasyWorshipConfig(config).then((result) => { hw.setEasyWorshipStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship configuration save failed."));
  };
  const checkEasyWorship = () => {
    void getEasyWorshipStatus().then((result) => { hw.setEasyWorshipStatus(result); setCommandNotice(result.detail); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship status check failed."));
  };
  const sendPreviewToEasyWorship = () => {
    if (!desktop.previewCandidate) return;
    void sendEasyWorshipPreview(desktop.previewCandidate).then((result) => { hw.setEasyWorshipStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship preview failed."));
  };
  const sendLiveToEasyWorship = () => {
    if (!desktop.previewCandidate) return;
    void sendEasyWorshipLive(desktop.previewCandidate, desktop.destinationsArmed).then((result) => { hw.setEasyWorshipStatus(result); desktop.setLiveCandidate({ ...desktop.previewCandidate!, status: "live" }); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship live output failed."));
  };
  const clearEasyWorship = () => {
    void clearEasyWorshipOutput().then((result) => { hw.setEasyWorshipStatus(result); setCommandNotice(result.detail); refreshIntegrationEvents(); })
      .catch((error: unknown) => setCommandNotice(error instanceof Error ? error.message : "EasyWorship clear failed."));
  };

  if (activeScreen === "landing") {
    return <LandingPage onOpen={setActiveScreen} />;
  }

  return (
    <WorkspaceShell
      active={activeScreen}
      onNavigate={setActiveScreen}
      title={activeMeta.title}
      preview={desktop.previewCandidate!}
      live={desktop.liveCandidate!}
      armed={desktop.destinationsArmed}
      desktopStatus={desktop.desktopStatus || undefined}
      onToggleArmed={toggleArmed}
      onSendLive={() => sendLive()}
      onManualSearch={() => setActiveScreen("search")}
    >
      {activeScreen === "dashboard" && desktop.selectedCandidate ? (
        <Suspense fallback={<LazyFallback />}>
        <OperatorDashboard
          candidate={desktop.selectedCandidate}
          transcript={desktop.transcript}
          integrations={hw.integrations}
          aiDetection={desktop.aiDetection || undefined}
          onPreview={() => preview(desktop.selectedCandidate!)}
          onLive={() => sendLive(desktop.selectedCandidate!)}
          onAnalyze={runAiAssist}
        />
        </Suspense>
      ) : null}
      {activeScreen === "transcript" ? (
        <>
          <VuMeter active={captureRunning} deviceLabel={selectedAudioDevice} />
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
                  captureStartedRef.current = false;
                  setCaptureRunning(false);
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
            transcript={desktop.transcript}
            candidates={desktop.candidates}
            onPreview={preview}
            onSelectSegment={selectTranscriptSegment}
          />
        </>
      ) : null}
      {activeScreen === "queue" && desktop.selectedCandidate ? (
        <QueueApprovalPanel
          activeCandidate={desktop.selectedCandidate}
          candidates={desktop.candidates}
          onPreview={preview}
          onLive={sendLive}
          onReject={rejectCandidate}
          onMerge={(candidate) => desktop.mergeCandidates([candidate])}
          onCalibrate={calibrateCandidate}
        />
      ) : null}
      {activeScreen === "output" && desktop.previewCandidate && desktop.liveCandidate ? (
        <PreviewLiveOutput
          preview={desktop.previewCandidate}
          live={desktop.liveCandidate}
          armed={desktop.destinationsArmed}
          integrations={hw.integrations}
          onSendLive={() => sendLive()}
          onToggleArmed={toggleArmed}
          onClearLive={clearLive}
          onBlackout={blackout}
          onStageDisplay={stageDisplay}
          onLowerThird={lowerThird}
        />
      ) : null}
      {activeScreen === "theme" && desktop.previewCandidate ? (
        <Suspense fallback={<LazyFallback />}>
        <ThemeDesigner
          selectedTheme={selectedTheme}
          onSelectTheme={setSelectedTheme}
          preview={desktop.previewCandidate}
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
      {activeScreen === "integrations" && hw.vmixStatus ? (
        <Suspense fallback={<LazyFallback />}>
        <IntegrationsSettings
          integrations={hw.integrations}
          vmixStatus={hw.vmixStatus}
          integrationEvents={hw.integrationEvents}
          obsStatus={hw.obsStatus || undefined}
          proPresenterStatus={hw.proPresenterStatus || undefined}
          companionStatus={hw.companionStatus || undefined}
          oscStatus={hw.oscStatus || undefined}
          easyWorshipStatus={hw.easyWorshipStatus || undefined}
          operatorName={hw.operatorName}
          trustedPlugins={trustedPlugins}
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
        />
        </Suspense>
      ) : null}
      {activeScreen === "health" && hw.productionReadiness ? (
        <Suspense fallback={<LazyFallback />}>
        <div className="space-y-7">
        <HealthStatusPanel
          items={hw.healthItems}
          readiness={hw.productionReadiness}
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
      {activeScreen === "stream" && desktop.liveCandidate ? (
        <Suspense fallback={<LazyFallback />}>
          <StreamOverlayPanel live={desktop.liveCandidate} />
        </Suspense>
      ) : null}
      {activeScreen === "songs" ? (
        <Suspense fallback={<LazyFallback />}>
          <SongLibraryPanel
            serviceSessionId={desktop.desktopStatus?.serviceSession ?? "browser-session"}
            operator={hw.operatorName || "operator"}
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
              // Route through the full live pipeline so OBS/ProPresenter/vMix/Companion/OSC/EasyWorship
              // all receive the section, the audit chain records it, and the production rail updates.
              desktop.setSelectedCandidate(synthetic);
              desktop.setPreviewCandidate({ ...synthetic, status: "preview" });
              sendLive(synthetic);
              // Also log the CCLI usage in the Rust audit chain (durable past localStorage).
              if (song.ccliNumber) {
                void logCcliUsage(
                  song.ccliNumber,
                  song.title,
                  desktop.desktopStatus?.serviceSession ?? "browser-session",
                  hw.operatorName || "operator"
                ).catch(() => undefined);
              }
            }}
          />
        </Suspense>
      ) : null}
      {activeScreen === "fleet" ? (
        <Suspense fallback={<LazyFallback />}>
          <FleetSyncPanel deviceLabel={desktop.desktopStatus?.serviceSession ?? "Browser device"} />
        </Suspense>
      ) : null}
      {activeScreen === "clips" && desktop.liveCandidate ? (
        <Suspense fallback={<LazyFallback />}>
          <ClipEdlPanel
            live={desktop.liveCandidate}
            serviceStartedAtMs={desktop.desktopStatus?.checkedAtMs ?? Date.now()}
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
      <NoticeToast notice={commandNotice} />
    </WorkspaceShell>
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
    <AnimatePresence>
      {visible ? (
        <motion.div
          role="status"
          aria-live="polite"
          initial={{ opacity: 0, y: 10, scale: 0.95 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: 6, scale: 0.97 }}
          transition={{ duration: 0.18, ease: "easeOut" }}
          className={`pointer-events-none fixed bottom-6 right-6 z-50 max-w-sm rounded-[8px] border px-4 py-3 text-sm backdrop-blur-xl ${
            isError
              ? "border-rose-500/40 bg-rose-950/90 text-rose-100 shadow-[0_10px_40px_-10px_rgba(239,68,68,0.4)]"
              : "border-violet-500/30 bg-slate-900/95 text-white shadow-[0_10px_40px_-10px_rgba(124,58,237,0.4)]"
          }`}
        >
          {notice}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
