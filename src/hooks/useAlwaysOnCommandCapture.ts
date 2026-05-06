import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  onAudioLevel,
  onCaptureError,
  onCaptureHealth,
  onCaptureStarted,
  onCaptureStopped,
  startAudioCapture,
  stopAudioCapture,
  type AudioLevel,
  type CaptureHealth,
  type SttStatus,
} from "../services/desktopApi";

export type CaptureState =
  | "idle"
  | "starting"
  | "listening"
  | "framesReceived"
  | "lowSignal"
  | "signalDetected"
  | "speechDetected"
  | "recognitionDegraded"
  | "recognized"
  | "retrying"
  | "missingModel"
  | "micUnavailable"
  | "failed";

export type ListenerStatusTone = "neutral" | "live" | "degraded";

export type ListenerStatus = {
  text: string;
  tone: ListenerStatusTone;
};

const ACTIVE_CAPTURE_STATES: CaptureState[] = [
  "listening",
  "framesReceived",
  "lowSignal",
  "signalDetected",
  "speechDetected",
  "recognitionDegraded",
  "recognized",
];

type AlwaysOnCommandCaptureOptions = {
  selectedAudioDevice?: string;
  desktopReady: boolean;
  sttStatus: SttStatus | null;
};

const WATCHDOG_MS = 2_000;
const MAX_RETRIES = 4;

let globalCaptureStarted = false;
let globalCaptureStarting = false;
let globalManualStop = false;

function normalizeRequestedDevice(deviceName?: string): string | undefined {
  if (!deviceName) return undefined;
  const trimmed = deviceName.trim();
  if (!trimmed) return undefined;
  if (trimmed.toLowerCase().includes("browser fallback")) return undefined;
  return trimmed;
}

function classifyCaptureError(message: string): CaptureState {
  const lower = message.toLowerCase();
  if (
    lower.includes("no whisper") ||
    lower.includes("model") ||
    lower.includes("stt adapter") ||
    lower.includes("ggml")
  ) {
    return "missingModel";
  }
  if (
    lower.includes("microphone") ||
    lower.includes("input device") ||
    lower.includes("permission") ||
    lower.includes("no default input") ||
    lower.includes("no microphone signal")
  ) {
    return "micUnavailable";
  }
  return "failed";
}

function buildListenerStatus(
  state: CaptureState,
  detail?: string,
  deviceName?: string | null,
): ListenerStatus {
  if (detail) {
    return {
      text: detail,
      tone: state === "listening" ? "live" : state === "failed" || state === "missingModel" || state === "micUnavailable" ? "degraded" : "neutral",
    };
  }

  switch (state) {
    case "starting":
      return { text: "Starting always-on scripture command listener.", tone: "neutral" };
    case "retrying":
      return { text: "Retrying microphone connection.", tone: "degraded" };
    case "listening":
      return { text: `Listening on ${deviceName ?? "Default Microphone"}.`, tone: "live" };
    case "framesReceived":
      return { text: `Mic connected on ${deviceName ?? "Default Microphone"}.`, tone: "live" };
    case "lowSignal":
      return { text: "Input level low. Speak closer or raise the source gain.", tone: "neutral" };
    case "signalDetected":
    case "speechDetected":
      return { text: "Mic signal active. Waiting for recognized words.", tone: "live" };
    case "recognitionDegraded":
      return { text: "Speech heard, but recognition is weak. Improve mic clarity or reload the model.", tone: "degraded" };
    case "recognized":
      return { text: "Speech recognized.", tone: "live" };
    case "missingModel":
      return { text: "Model missing. Open Transcript to load a Whisper model.", tone: "degraded" };
    case "micUnavailable":
      return { text: "Mic permission blocked or no usable input detected.", tone: "degraded" };
    case "failed":
      return { text: "Listener failed to start.", tone: "degraded" };
    case "idle":
    default:
      return { text: "Listener on standby.", tone: "neutral" };
  }
}

export function captureStateLabel(state: CaptureState): string {
  switch (state) {
    case "starting":
      return "Starting";
    case "listening":
      return "Listening";
    case "framesReceived":
      return "Mic connected";
    case "lowSignal":
      return "Input low";
    case "signalDetected":
    case "speechDetected":
      return "Signal active";
    case "recognitionDegraded":
      return "Recognition weak";
    case "recognized":
      return "Recognized";
    case "retrying":
      return "Retrying";
    case "missingModel":
      return "Model missing";
    case "micUnavailable":
      return "Mic unavailable";
    case "failed":
      return "Failed";
    case "idle":
    default:
      return "Standby";
  }
}

export function useAlwaysOnCommandCapture({
  selectedAudioDevice,
  desktopReady,
  sttStatus,
}: AlwaysOnCommandCaptureOptions) {
  const [captureState, setCaptureState] = useState<CaptureState>("idle");
  const [listenerStatus, setListenerStatus] = useState<ListenerStatus>({
    text: "Loading local-first audio services.",
    tone: "neutral",
  });
  const [lastAudioLevel, setLastAudioLevel] = useState<AudioLevel | null>(null);
  const retryTimerRef = useRef<number | null>(null);
  const watchdogTimerRef = useRef<number | null>(null);
  const retryCountRef = useRef(0);
  const selectedDeviceRef = useRef(selectedAudioDevice);
  const didHandleInitialDeviceRef = useRef(false);
  const lastBackendSignalAtRef = useRef(0);
  const currentStateRef = useRef<CaptureState>("idle");

  useEffect(() => {
    selectedDeviceRef.current = selectedAudioDevice;
  }, [selectedAudioDevice]);

  useEffect(() => {
    currentStateRef.current = captureState;
  }, [captureState]);

  const clearRetry = useCallback(() => {
    if (retryTimerRef.current !== null) {
      window.clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, []);

  const clearWatchdog = useCallback(() => {
    if (watchdogTimerRef.current !== null) {
      window.clearTimeout(watchdogTimerRef.current);
      watchdogTimerRef.current = null;
    }
  }, []);

  const scheduleRetry = useCallback((reason: string, nextState: CaptureState = "retrying") => {
    clearRetry();
    clearWatchdog();
    globalCaptureStarted = false;
    globalCaptureStarting = false;

    if (retryCountRef.current >= MAX_RETRIES) {
      setCaptureState(nextState === "retrying" ? "failed" : nextState);
      setListenerStatus(buildListenerStatus(nextState === "retrying" ? "failed" : nextState, reason));
      return;
    }

    retryCountRef.current += 1;
    setCaptureState(nextState);
    setListenerStatus(buildListenerStatus(nextState, reason));
    retryTimerRef.current = window.setTimeout(() => {
      void start(false);
    }, Math.min(8_000, 1_000 * retryCountRef.current));
  }, [clearRetry, clearWatchdog]);

  const start = useCallback((manual = false) => {
    if (!desktopReady || globalCaptureStarting || globalCaptureStarted) return Promise.resolve();
    if (sttStatus && !sttStatus.modelLoaded && !sttStatus.modelPath && sttStatus.loadError) {
      setCaptureState("missingModel");
      setListenerStatus(buildListenerStatus("missingModel"));
      return Promise.resolve();
    }

    clearRetry();
    clearWatchdog();
    globalManualStop = false;
    globalCaptureStarting = true;
    setCaptureState(manual ? "starting" : retryCountRef.current > 0 ? "retrying" : "starting");
    setListenerStatus(
      buildListenerStatus(
        manual ? "starting" : retryCountRef.current > 0 ? "retrying" : "starting",
        retryCountRef.current > 0 ? "Retrying microphone connection." : "Starting always-on scripture command listener.",
      ),
    );

    return startAudioCapture("en", normalizeRequestedDevice(selectedDeviceRef.current), "command")
      .then(() => {
        lastBackendSignalAtRef.current = 0;
        clearWatchdog();
        watchdogTimerRef.current = window.setTimeout(() => {
          const age = Date.now() - lastBackendSignalAtRef.current;
          if (globalManualStop || !globalCaptureStarting || age <= WATCHDOG_MS) return;
          scheduleRetry("Capture started but no microphone signal reached Aletheia.", "micUnavailable");
        }, WATCHDOG_MS);
      })
      .catch((error: unknown) => {
        globalCaptureStarted = false;
        globalCaptureStarting = false;
        const message = error instanceof Error ? error.message : "Could not start always-on scripture command listener.";
        const nextState = classifyCaptureError(message);
        setCaptureState(nextState);
        setListenerStatus(buildListenerStatus(nextState, message));
        if (nextState === "failed") {
          scheduleRetry(message, "retrying");
        }
      });
  }, [clearRetry, clearWatchdog, desktopReady, scheduleRetry, sttStatus]);

  const stop = useCallback(() => {
    clearRetry();
    clearWatchdog();
    globalManualStop = true;
    globalCaptureStarted = false;
    globalCaptureStarting = false;
    setCaptureState("idle");
    setListenerStatus(buildListenerStatus("idle"));
    void stopAudioCapture().catch(() => undefined);
  }, [clearRetry, clearWatchdog]);

  useEffect(() => {
    let unlistenStarted: (() => void) | undefined;
    let unlistenHealth: (() => void) | undefined;
    let unlistenLevel: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;
    let unlistenStopped: (() => void) | undefined;

    const applyHealth = (health: CaptureHealth) => {
      const nextState = (health.state as CaptureState) in {
        idle: true,
        starting: true,
        listening: true,
        framesReceived: true,
        lowSignal: true,
        signalDetected: true,
        speechDetected: true,
        recognitionDegraded: true,
        recognized: true,
        retrying: true,
        missingModel: true,
        micUnavailable: true,
        failed: true,
      }
        ? (health.state as CaptureState)
        : classifyCaptureError(health.detail);
      setCaptureState(nextState);
      setListenerStatus(buildListenerStatus(nextState, health.detail, health.deviceName));
      if (nextState === "listening") {
        globalCaptureStarted = true;
        globalCaptureStarting = false;
      }
    };

    void onCaptureStarted((health) => {
      retryCountRef.current = 0;
      globalCaptureStarted = true;
      globalCaptureStarting = false;
      applyHealth(health);
    }).then((fn) => { unlistenStarted = fn; }).catch(() => undefined);

    void onCaptureHealth((health) => {
      applyHealth(health);
    }).then((fn) => { unlistenHealth = fn; }).catch(() => undefined);

    void onAudioLevel((level) => {
      lastBackendSignalAtRef.current = Date.now();
      setLastAudioLevel(level);
      if (globalCaptureStarting) {
        retryCountRef.current = 0;
        globalCaptureStarting = false;
        globalCaptureStarted = true;
        setCaptureState("framesReceived");
        setListenerStatus(buildListenerStatus("framesReceived", undefined, selectedDeviceRef.current ?? null));
        return;
      }
      if (
        level.speechDetected &&
        (currentStateRef.current === "lowSignal" || currentStateRef.current === "framesReceived")
      ) {
        setCaptureState("signalDetected");
        setListenerStatus(buildListenerStatus("signalDetected", undefined, selectedDeviceRef.current ?? null));
      }
    }).then((fn) => { unlistenLevel = fn; }).catch(() => undefined);

    void onCaptureError((detail) => {
      const nextState = classifyCaptureError(detail);
      globalCaptureStarted = false;
      globalCaptureStarting = false;
      setCaptureState(nextState);
      setListenerStatus(buildListenerStatus(nextState, detail));
      if (nextState === "failed") {
        scheduleRetry(detail, "retrying");
      }
    }).then((fn) => { unlistenError = fn; }).catch(() => undefined);

    void onCaptureStopped(() => {
      globalCaptureStarted = false;
      globalCaptureStarting = false;
      if (globalManualStop) {
        setCaptureState("idle");
        setListenerStatus(buildListenerStatus("idle"));
        return;
      }
      scheduleRetry("Always-on scripture command listener stopped unexpectedly.", "retrying");
    }).then((fn) => { unlistenStopped = fn; }).catch(() => undefined);

    return () => {
      unlistenStarted?.();
      unlistenHealth?.();
      unlistenLevel?.();
      unlistenError?.();
      unlistenStopped?.();
    };
  }, [scheduleRetry]);

  useEffect(() => {
    if (!desktopReady || sttStatus === null) return;
    if (sttStatus.modelLoaded || sttStatus.modelPath || !sttStatus.loadError) {
      void start(false);
      return;
    }
    setCaptureState("missingModel");
    setListenerStatus(buildListenerStatus("missingModel"));
  }, [desktopReady, start, sttStatus]);

  useEffect(() => {
    if (!didHandleInitialDeviceRef.current) {
      didHandleInitialDeviceRef.current = true;
      return;
    }
    if (!desktopReady || sttStatus === null) return;
    if (!globalCaptureStarted && !globalCaptureStarting) return;
    clearRetry();
    clearWatchdog();
    globalCaptureStarted = false;
    globalCaptureStarting = false;
    setCaptureState("retrying");
    setListenerStatus(buildListenerStatus("retrying", "Switching microphone input."));
    void stopAudioCapture().finally(() => {
      retryTimerRef.current = window.setTimeout(() => {
        void start(false);
      }, 300);
    });
  }, [clearRetry, clearWatchdog, desktopReady, selectedAudioDevice, start, sttStatus]);

  useEffect(() => {
    const stopOnWindowClose = () => {
      if (globalCaptureStarted) {
        void stopAudioCapture().catch(() => undefined);
      }
    };
    window.addEventListener("beforeunload", stopOnWindowClose);
    return () => window.removeEventListener("beforeunload", stopOnWindowClose);
  }, []);

  useEffect(() => () => {
    clearRetry();
    clearWatchdog();
  }, [clearRetry, clearWatchdog]);

  return useMemo(() => ({
    captureState,
    captureRunning: ACTIVE_CAPTURE_STATES.includes(captureState),
    captureStarting: captureState === "starting" || captureState === "retrying",
    listenerStatus,
    lastAudioLevel,
    startCapture: () => void start(true),
    stopCapture: stop,
  }), [captureState, listenerStatus, lastAudioLevel, start, stop]);
}
