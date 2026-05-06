// ---------------------------------------------------------------------------
// CaptureControlPanel
//
// First-thing-the-operator-sees panel for diagnosing the live capture
// pipeline. Surfaces:
//   - Whether the Whisper STT model is loaded (+ filename, asset folder)
//   - The microphone device picker (always visible — not gated on >1 device)
//   - Start / Stop capture buttons with a clear running indicator
//   - A "Reload model" button so the operator can drop a .bin file into the
//     assets folder and pick it up without restarting the app
//   - The most recent system notice (success or error) verbatim
//
// This component does no I/O itself; it receives state and callbacks from
// App.tsx, where the actual Tauri invocations live.
// ---------------------------------------------------------------------------

import { Mic, MicOff, RefreshCw, Square, Play, AlertTriangle, CheckCircle2 } from "lucide-react";
import type { SttLatencyProfile, SttStatus } from "../services/desktopApi";
import type { CaptureState } from "../hooks/useAlwaysOnCommandCapture";
import { ActionButton, StatusPill } from "./Primitives";

export function CaptureControlPanel({
  sttStatus,
  audioDevices,
  selectedAudioDevice,
  captureRunning,
  captureStarting = false,
  captureState,
  listenerStatus,
  latencyProfile,
  notice,
  onSelectDevice,
  onStartCapture,
  onStopCapture,
  onReloadModel,
  onRefreshDevices,
}: {
  sttStatus: SttStatus | null;
  audioDevices: string[];
  selectedAudioDevice: string | undefined;
  captureRunning: boolean;
  captureStarting?: boolean;
  captureState: CaptureState;
  listenerStatus: string;
  latencyProfile?: SttLatencyProfile | null;
  notice: string | null;
  onSelectDevice: (device: string | undefined) => void;
  onStartCapture: () => void;
  onStopCapture: () => void;
  onReloadModel: () => void;
  onRefreshDevices: () => void;
}) {
  const modelLoaded = sttStatus?.modelLoaded ?? false;
  const modelFile = sttStatus?.modelFilename ?? null;
  const modelQuality = sttStatus?.modelQuality ?? null;
  const modelSizeMb = sttStatus?.modelSizeMb ?? null;
  const modelWarning = sttStatus?.modelWarning ?? null;
  const assetRoot = sttStatus?.assetRoot ?? null;
  const loadError = sttStatus?.loadError ?? null;
  const modelInstalled = Boolean(modelFile);
  const latencyTone =
    latencyProfile?.state === "ready"
      ? "healthy"
      : latencyProfile?.state === "degraded"
      ? "degraded"
      : latencyProfile?.state === "blocked"
      ? "offline"
      : "neutral";
  const fmtMs = (value: number | null | undefined) => value == null ? "—" : `${value} ms`;

  return (
    <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-sm font-semibold text-ink">Capture control</p>
          <p className="mt-1 text-xs leading-5 text-muted">
            Confirm microphone and model health without crowding the transcript stream.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <StatusPill
            tone={modelLoaded ? "healthy" : modelInstalled ? "neutral" : "offline"}
            label={modelLoaded ? "Model ready" : modelInstalled ? "Model installed" : "Model missing"}
          />
          <StatusPill
            tone={
              captureRunning
                ? "live"
                : captureStarting || captureState === "micUnavailable" || captureState === "missingModel" || captureState === "recognitionDegraded"
                ? "degraded"
                : "neutral"
            }
            label={
              captureRunning
                ? "Capturing"
                : captureStarting
                ? "Starting"
                : captureState === "micUnavailable"
                ? "Mic unavailable"
                : captureState === "missingModel"
                ? "Model missing"
                : captureState === "recognitionDegraded"
                ? "Recognition weak"
                : "Stopped"
            }
          />
        </div>
      </div>

      <div className="mt-4 rounded-[6px] border border-line bg-mist p-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">
              STT latency profile
            </p>
            <p className="mt-1 text-xs leading-5 text-muted">
              Target is under {latencyProfile?.targetMs ?? 2000} ms from speech chunk to transcript.
            </p>
          </div>
          <StatusPill tone={latencyTone} label={latencyProfile?.state ?? "pending"} />
        </div>
        <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
          {[
            ["Latest", fmtMs(latencyProfile?.latestMs)],
            ["Average", fmtMs(latencyProfile?.averageMs)],
            ["P50", fmtMs(latencyProfile?.p50Ms)],
            ["P95", fmtMs(latencyProfile?.p95Ms)],
          ].map(([label, value]) => (
            <div key={label} className="rounded-[6px] border border-line bg-paper px-3 py-2">
              <p className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">{label}</p>
              <p className="mt-1 font-mono text-sm font-semibold text-ink">{value}</p>
            </div>
          ))}
        </div>
        <p className="mt-2 text-xs leading-5 text-muted">
          {latencyProfile?.detail ?? "No samples yet. Start capture and speak a scripture command."}
        </p>
      </div>

      <details className="mt-4 overflow-hidden rounded-[6px] border border-line bg-mist">
        <summary className="cursor-pointer px-3 py-2.5 text-xs font-semibold uppercase tracking-[0.12em] text-muted transition hover:text-ink">
          Model and input settings
        </summary>
        <div className="space-y-3 border-t border-line p-3">
          <div className="grid gap-3 lg:grid-cols-[1fr_auto] lg:items-center">
            <div className="rounded-[6px] border border-line bg-paper p-3">
              <div className="flex items-start gap-2">
                {modelLoaded ? (
                  <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-500" aria-hidden="true" />
                ) : (
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" aria-hidden="true" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">
                    Whisper speech model
                  </p>
                  {modelLoaded || modelInstalled ? (
                    <div className="mt-1 min-w-0">
                      <p className="truncate text-sm font-semibold text-ink" title={sttStatus?.modelPath ?? ""}>
                        {modelFile ?? "Installed"}
                      </p>
                      <p className="mt-1 text-xs text-muted">
                        {[
                          modelLoaded ? "Loaded in memory" : "Loads on capture start",
                          modelQuality,
                          modelSizeMb ? `${modelSizeMb} MiB` : null,
                        ].filter(Boolean).join(" · ")}
                      </p>
                    </div>
                  ) : (
                    <div className="mt-1 space-y-1">
                      <p className="text-sm font-semibold text-ink">No model loaded.</p>
                      <p className="text-xs leading-5 text-muted">
                        Download <code className="font-mono">ggml-small.en.bin</code> from{" "}
                        <span className="font-mono">huggingface.co/ggerganov/whisper.cpp</span>, save it
                        as one of <code className="font-mono">stt-whisper-en-small.bin</code> or{" "}
                        <code className="font-mono">stt-whisper-multilingual.bin</code> in:
                      </p>
                      {assetRoot ? (
                        <p
                          className="mt-1 break-all rounded-[4px] bg-mist px-2 py-1 font-mono text-[11px] text-muted"
                          title={assetRoot}
                        >
                          {assetRoot}
                        </p>
                      ) : null}
                    </div>
                  )}
                  {loadError ? (
                    <p className="mt-2 whitespace-pre-line rounded-[4px] border border-red-500/30 bg-red-500/5 px-2 py-1 text-xs text-red-500">
                      {loadError}
                    </p>
                  ) : null}
                  {modelWarning ? (
                    <p className="mt-2 whitespace-pre-line rounded-[4px] border border-amber-500/30 bg-amber-500/5 px-2 py-1 text-xs text-amber-600">
                      {modelWarning}
                    </p>
                  ) : null}
                </div>
              </div>
            </div>
            <ActionButton tone="secondary" onClick={onReloadModel}>
              <RefreshCw className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
              Reload model
            </ActionButton>
          </div>

          <div className="rounded-[6px] border border-line bg-paper p-3">
            <div className="flex items-center gap-2">
              <Mic className="h-4 w-4 shrink-0 text-muted" aria-hidden="true" />
              <label htmlFor="capture-device-picker" className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">
                Microphone input
              </label>
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <select
                id="capture-device-picker"
                value={selectedAudioDevice ?? ""}
                onChange={(e) => onSelectDevice(e.target.value || undefined)}
                className="min-w-[200px] flex-1 rounded-[6px] border border-line bg-paper px-3 py-1.5 text-sm text-ink outline-none focus:border-accent"
                disabled={captureRunning}
              >
                <option value="">OS Default</option>
                {audioDevices.map((d) => (
                  <option key={d} value={d}>{d}</option>
                ))}
              </select>
              <ActionButton tone="secondary" onClick={onRefreshDevices}>
                <RefreshCw className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
                Rescan
              </ActionButton>
            </div>
            {audioDevices.length === 0 ? (
              <p className="mt-2 text-xs text-amber-500">
                No input devices detected. Check Windows Settings → Privacy → Microphone.
              </p>
            ) : (
              <p className="mt-2 text-xs text-muted">
                {audioDevices.length} input device{audioDevices.length === 1 ? "" : "s"} available.
                Stop capture before changing device.
              </p>
            )}
          </div>
        </div>
      </details>

      <div className="mt-4 grid gap-3 lg:grid-cols-[1fr_auto] lg:items-center">
        <div className="rounded-[6px] border border-line bg-mist p-3">
          <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">Capture state</p>
          <p className="mt-1 text-xs leading-5 text-muted">
            {listenerStatus}
          </p>
        </div>
        <div className="flex flex-col gap-2">
          {captureRunning ? (
            <ActionButton tone="danger" onClick={onStopCapture}>
              <Square className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
              Stop capture
            </ActionButton>
          ) : (
            <ActionButton onClick={onStartCapture}>
              <Play className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
              Start capture
            </ActionButton>
          )}
          {captureRunning ? (
            <p className="text-center text-[10px] text-muted">
              <MicOff className="-mt-0.5 mr-1 inline h-3 w-3" aria-hidden="true" />
              Stop before changing inputs
            </p>
          ) : null}
        </div>
      </div>

      {/* ── Notice / error line ── */}
      {notice ? (
        <div className="mt-4 rounded-[6px] border border-line bg-mist p-3">
          <p className="whitespace-pre-line text-xs leading-5 text-muted">{notice}</p>
        </div>
      ) : null}
    </div>
  );
}
