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
import type { SttStatus } from "../services/desktopApi";
import { ActionButton, StatusPill } from "./Primitives";

export function CaptureControlPanel({
  sttStatus,
  audioDevices,
  selectedAudioDevice,
  captureRunning,
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
  notice: string | null;
  onSelectDevice: (device: string | undefined) => void;
  onStartCapture: () => void;
  onStopCapture: () => void;
  onReloadModel: () => void;
  onRefreshDevices: () => void;
}) {
  const modelLoaded = sttStatus?.modelLoaded ?? false;
  const modelFile = sttStatus?.modelFilename ?? null;
  const assetRoot = sttStatus?.assetRoot ?? null;
  const loadError = sttStatus?.loadError ?? null;

  return (
    <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-sm font-semibold text-ink">Capture control</p>
          <p className="mt-1 text-xs leading-5 text-muted">
            Pick a microphone, confirm the speech-recognition model is loaded, then start capture.
            Transcripts appear on the Live Transcript screen within a few seconds of speech.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <StatusPill
            tone={modelLoaded ? "healthy" : "offline"}
            label={modelLoaded ? "Model ready" : "Model missing"}
          />
          <StatusPill
            tone={captureRunning ? "live" : "neutral"}
            label={captureRunning ? "Capturing" : "Stopped"}
          />
        </div>
      </div>

      {/* ── Whisper model row ── */}
      <div className="mt-4 grid gap-3 lg:grid-cols-[1fr_auto] lg:items-center">
        <div className="rounded-[6px] border border-line bg-mist p-3">
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
              {modelLoaded ? (
                <p className="mt-1 text-sm font-semibold text-ink truncate" title={sttStatus?.modelPath ?? ""}>
                  {modelFile ?? "Loaded"}
                </p>
              ) : (
                <div className="mt-1 space-y-1">
                  <p className="text-sm font-semibold text-ink">No model loaded.</p>
                  <p className="text-xs leading-5 text-muted">
                    Download <code className="font-mono">ggml-base.en.bin</code> from{" "}
                    <span className="font-mono">huggingface.co/ggerganov/whisper.cpp</span>, save it
                    as one of <code className="font-mono">stt-whisper-en-small.bin</code> or{" "}
                    <code className="font-mono">stt-whisper-multilingual.bin</code> in:
                  </p>
                  {assetRoot ? (
                    <p
                      className="mt-1 break-all rounded-[4px] bg-paper px-2 py-1 font-mono text-[11px] text-muted"
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
            </div>
          </div>
        </div>
        <ActionButton tone="secondary" onClick={onReloadModel}>
          <RefreshCw className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
          Reload model
        </ActionButton>
      </div>

      {/* ── Microphone row ── */}
      <div className="mt-4 grid gap-3 lg:grid-cols-[1fr_auto] lg:items-center">
        <div className="rounded-[6px] border border-line bg-mist p-3">
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
              className="flex-1 min-w-[200px] rounded-[6px] border border-line bg-paper px-3 py-1.5 text-sm text-ink outline-none focus:border-accent"
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
