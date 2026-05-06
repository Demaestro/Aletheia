import { save as openSaveDialog } from "@tauri-apps/plugin-dialog";
import { Download, FileText, Pause, Play, Search, Square, X } from "lucide-react";
import React, { useEffect, useMemo, useRef, useState } from "react";
import { scriptureCandidates, transcriptSegments } from "../data/production";
import { isTauriRuntime, saveTranscriptExport } from "../services/desktopApi";
import type { ScriptureCandidate, TranscriptSegment } from "../types";
import { ActionButton, ConfidenceBar, SectionHeader, StatusPill } from "./Primitives";

/**
 * TranscriptRow — chat-bubble style.
 *
 * Layout:
 *   ┌──────────────────────────────────────────────────┐
 *   │  Pastor Daniel  [en]       00:18:13      455 ms  │
 *   │  And we know that all things work together for   │
 *   │  good to them that love God, who are the called… │
 *   └──────────────────────────────────────────────────┘
 *
 * The text is never constrained to a narrow column — it fills the
 * entire row width so long sentences flow naturally.
 */
const TranscriptRow = React.memo(({ segment, onSelectSegment }: {
  segment: TranscriptSegment;
  onSelectSegment?: (segment: TranscriptSegment) => void;
}) => {
  return (
    <button
      type="button"
      onClick={() => onSelectSegment?.(segment)}
      title="Click to seed manual search from this line"
      className="group w-full px-5 py-4 text-left transition-colors hover:bg-white/[0.04] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
    >
      {/* ── Meta row ── */}
      <div className="flex items-center gap-2 mb-1.5">
        <span className="text-sm font-semibold text-ink leading-none">{segment.speaker}</span>
        <span className="rounded bg-violet-500/15 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-violet-300 ring-1 ring-violet-500/20">
          {segment.language}
        </span>
        <span className="ml-auto font-mono text-[11px] text-muted">{segment.time}</span>
        <span className="font-mono text-[11px] text-muted">{segment.latencyMs} ms</span>
      </div>

      {/* ── Transcript text — always full width ── */}
      <p className="text-sm leading-relaxed text-graphite whitespace-pre-wrap break-words">
        {segment.text}
      </p>
    </button>
  );
});
TranscriptRow.displayName = "TranscriptRow";

type TranscriptRecording = {
  id: string;
  title: string;
  startedAt: Date;
  stoppedAt: Date | null;
  segments: TranscriptSegment[];
};

function defaultRecordingTitle() {
  const stamp = new Date().toISOString().slice(0, 16).replace("T", " ");
  return `Transcript ${stamp}`;
}

function safeFilename(value: string) {
  const cleaned = value
    .trim()
    .replace(/[<>:"/\\|?*\u0000-\u001F]/g, "-")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  return cleaned || "aletheia-transcript";
}

function formatDateTime(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(date);
}

function buildTranscriptMarkdown(recording: TranscriptRecording) {
  const stopped = recording.stoppedAt ?? new Date();
  const lines = [
    `# ${recording.title}`,
    "",
    `Started: ${formatDateTime(recording.startedAt)}`,
    `Stopped: ${formatDateTime(stopped)}`,
    `Segments: ${recording.segments.length}`,
    "",
    "## Transcript",
    "",
  ];

  for (const segment of recording.segments) {
    lines.push(
      `### ${segment.time} - ${segment.speaker} (${segment.language}, ${segment.latencyMs} ms)`,
      "",
      segment.text,
      ""
    );
  }

  return lines.join("\n");
}

function downloadTextFile(filename: string, content: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1_000);
}

export function LiveTranscriptView({
  transcript = transcriptSegments,
  candidates = scriptureCandidates,
  onPreview,
  onSelectSegment
}: {
  transcript?: TranscriptSegment[];
  candidates?: ScriptureCandidate[];
  onPreview: (candidate: ScriptureCandidate) => void;
  onSelectSegment?: (segment: TranscriptSegment) => void;
}) {
  const [paused, setPaused] = useState(false);
  const [recording, setRecording] = useState<TranscriptRecording | null>(null);
  const [lastRecording, setLastRecording] = useState<TranscriptRecording | null>(null);
  const [showSavePrompt, setShowSavePrompt] = useState(false);
  const [recordingTitle, setRecordingTitle] = useState(defaultRecordingTitle);
  const [clockTick, setClockTick] = useState(0);
  const [saveStatus, setSaveStatus] = useState<string | null>(null);
  const frozenRef = useRef<TranscriptSegment[]>([]);
  const recordedIdsRef = useRef<Set<string>>(new Set());
  const visibleTranscript = paused ? frozenRef.current : transcript;
  const recordingDuration = useMemo(() => {
    if (!recording) return "00:00";
    const elapsedMs = Date.now() - recording.startedAt.getTime();
    const minutes = Math.floor(elapsedMs / 60_000);
    const seconds = Math.floor((elapsedMs % 60_000) / 1_000);
    return `${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }, [clockTick, recording, transcript.length]);

  useEffect(() => {
    if (!recording) return;
    const timer = window.setInterval(() => setClockTick((value) => value + 1), 1_000);
    return () => window.clearInterval(timer);
  }, [recording]);

  useEffect(() => {
    if (!recording) return;
    const nextSegments = transcript.filter((segment) => !recordedIdsRef.current.has(segment.id));
    if (nextSegments.length === 0) return;

    for (const segment of nextSegments) {
      recordedIdsRef.current.add(segment.id);
    }
    setRecording((current) => {
      if (!current) return current;
      return {
        ...current,
        segments: [...current.segments, ...nextSegments.reverse()],
      };
    });
  }, [recording, transcript]);

  const startRecording = () => {
    const title = defaultRecordingTitle();
    recordedIdsRef.current = new Set(transcript.map((segment) => segment.id));
    setRecordingTitle(title);
    setLastRecording(null);
    setShowSavePrompt(false);
    setSaveStatus(null);
    setRecording({
      id: `transcript-${Date.now()}`,
      title,
      startedAt: new Date(),
      stoppedAt: null,
      segments: [],
    });
  };

  const stopRecording = () => {
    if (!recording) return;
    const stopped = { ...recording, title: recordingTitle, stoppedAt: new Date() };
    setRecording(null);
    setLastRecording(stopped);
    setShowSavePrompt(true);
  };

  const downloadRecording = async (format: "md" | "txt" | "json") => {
    if (!lastRecording) return;
    const filenameBase = safeFilename(lastRecording.title);
    const extension = format === "json" ? "json" : format;
    const mimeType = format === "json"
      ? "application/json;charset=utf-8"
      : format === "md"
        ? "text/markdown;charset=utf-8"
        : "text/plain;charset=utf-8";
    const content = format === "json"
      ? JSON.stringify(lastRecording, null, 2)
      : format === "md"
        ? buildTranscriptMarkdown(lastRecording)
        : lastRecording.segments
          .map((segment) => `[${segment.time}] ${segment.speaker}: ${segment.text}`)
          .join("\n\n");

    if (isTauriRuntime()) {
      try {
        const selectedPath = await openSaveDialog({
          title: "Save transcript",
          defaultPath: `${filenameBase}.${extension}`,
          filters: [
            {
              name: format === "json" ? "JSON transcript" : format === "md" ? "Markdown transcript" : "Text transcript",
              extensions: [extension],
            },
          ],
        });
        if (!selectedPath) return;
        const savedPath = await saveTranscriptExport(selectedPath, content);
        setSaveStatus(`Saved transcript to ${savedPath}`);
        return;
      } catch (error) {
        setSaveStatus(error instanceof Error ? error.message : "Desktop save failed. Using browser download instead.");
      }
    }

    if (format === "json") {
      downloadTextFile(
        `${filenameBase}.json`,
        content,
        mimeType
      );
      return;
    }

    if (format === "md") {
      downloadTextFile(`${filenameBase}.md`, content, mimeType);
      return;
    }

    downloadTextFile(`${filenameBase}.txt`, content, mimeType);
  };

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Live transcript"
        title="Speech stream and focused transcript capture"
        detail="Always-on listening can detect scripture all service. Recording only starts when you press Start transcript."
        action={
          <div className="flex flex-wrap gap-2">
            {recording ? (
              <ActionButton tone="secondary" onClick={stopRecording}>
                <Square className="mr-2 h-4 w-4" aria-hidden="true" />
                Stop transcript
              </ActionButton>
            ) : (
              <ActionButton onClick={startRecording}>
                <FileText className="mr-2 h-4 w-4" aria-hidden="true" />
                Start transcript
              </ActionButton>
            )}
            <ActionButton
              tone="secondary"
              onClick={() => { frozenRef.current = [...transcript]; setPaused(true); }}
              disabled={paused}
            >
              <Pause className="mr-2 h-4 w-4" aria-hidden="true" />
              Pause
            </ActionButton>
            <ActionButton onClick={() => setPaused(false)} disabled={!paused}>
              <Play className="mr-2 h-4 w-4" aria-hidden="true" />
              Resume
            </ActionButton>
          </div>
        }
      />

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="rounded-[8px] border border-white/8 bg-white/[0.035] p-4">
          <div className="flex flex-wrap items-center gap-3">
            <StatusPill
              tone={recording ? "live" : "neutral"}
              label={recording ? "Recording transcript" : "Transcript recording off"}
              detail={recording ? recordingDuration : "manual start"}
            />
            <input
              value={recordingTitle}
              onChange={(event) => setRecordingTitle(event.target.value)}
              disabled={!recording}
              aria-label="Transcript recording title"
              className="h-9 min-w-[220px] flex-1 rounded-[6px] border border-white/10 bg-black/20 px-3 text-sm text-ink outline-none transition placeholder:text-muted focus:border-accent disabled:cursor-not-allowed disabled:opacity-60"
              placeholder="Speaker or session name"
            />
          </div>
          <p className="mt-3 text-xs leading-5 text-muted">
            Use this for one speaker at a time: start when they begin, stop when they finish, then save the file. The always-on listener keeps working for scripture detection even when transcript recording is off.
          </p>
        </div>

        <div className="rounded-[8px] border border-white/8 bg-white/[0.035] p-4">
          <p className="text-sm font-semibold text-ink">Recording policy</p>
          <p className="mt-2 text-xs leading-5 text-muted">
            No export is created automatically. Only the segments captured between Start transcript and Stop transcript are downloadable.
          </p>
        </div>
      </div>

      {showSavePrompt && lastRecording ? (
        <div className="rounded-[8px] border border-emerald-500/30 bg-emerald-500/10 p-4">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <p className="text-sm font-semibold text-emerald-200">Transcript ready to save</p>
              <p className="mt-1 text-xs leading-5 text-emerald-100/80">
                {lastRecording.segments.length} segments captured from {formatDateTime(lastRecording.startedAt)}.
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <ActionButton tone="secondary" onClick={() => void downloadRecording("md")}>
                <Download className="mr-2 h-4 w-4" aria-hidden="true" />
                Save Markdown
              </ActionButton>
              <ActionButton tone="secondary" onClick={() => void downloadRecording("txt")}>
                Save Text
              </ActionButton>
              <ActionButton tone="secondary" onClick={() => void downloadRecording("json")}>
                Save JSON
              </ActionButton>
              <button
                type="button"
                onClick={() => setShowSavePrompt(false)}
                className="inline-flex h-10 w-10 items-center justify-center rounded-[6px] border border-white/10 text-muted transition hover:text-ink"
                aria-label="Dismiss save prompt"
              >
                <X className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>
          </div>
          {saveStatus ? (
            <p className="mt-3 text-xs leading-5 text-emerald-100/80">{saveStatus}</p>
          ) : null}
        </div>
      ) : null}

      {/* Two-column layout: transcript list | sidebar */}
      <div className="grid gap-5 xl:grid-cols-[1fr_320px]">

        {/* ── Transcript list ── */}
        <div className="overflow-hidden rounded-[8px] border border-white/8 bg-white/[0.035]">
          {/* Column header */}
          <div className="flex items-center justify-between border-b border-white/8 px-5 py-2.5 text-[11px] font-semibold uppercase tracking-widest text-muted">
            <span>Speaker · Transcript</span>
            <span>Time / Lag</span>
          </div>

          {/* Rows */}
          <div className="divide-y divide-white/[0.05]">
            {visibleTranscript.length === 0 ? (
              <p className="px-5 py-8 text-center text-sm text-muted">
                Waiting for speech input…
              </p>
            ) : (
              visibleTranscript.map((segment) => (
                <TranscriptRow
                  key={segment.id}
                  segment={segment}
                  onSelectSegment={onSelectSegment}
                />
              ))
            )}
          </div>
        </div>

        {/* ── Sidebar ── */}
        <aside className="space-y-4">
          <div className="rounded-[8px] border border-white/8 bg-white/[0.035] p-4">
            <p className="text-sm font-semibold text-ink">Source diagnostics</p>
            <div className="mt-3 space-y-2">
              <StatusPill tone="healthy" label="Offline STT" detail="active" />
              <StatusPill tone="healthy" label="Pulpit mic" detail="-14 dB" />
              <StatusPill tone="degraded" label="Cloud" detail="skipped" />
            </div>
            <p className="mt-4 text-xs leading-5 text-muted">
              Transcript recording is operator-controlled. Live detection remains active even when recording is off.
            </p>
          </div>

          <div className="rounded-[8px] border border-white/8 bg-white/[0.035] p-4">
            <p className="text-sm font-semibold text-ink">Detected references</p>
            <div className="mt-3 space-y-2">
              {candidates.slice(0, 3).map((candidate) => (
                <button
                  key={candidate.id}
                  type="button"
                  onClick={() => onPreview(candidate)}
                  className="w-full rounded-[6px] border border-white/8 p-3 text-left transition hover:border-accent/50 focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                >
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-semibold text-ink">{candidate.reference}</p>
                    <Search className="h-3.5 w-3.5 shrink-0 text-muted" aria-hidden="true" />
                  </div>
                  <p className="mt-1 text-xs text-muted">{candidate.reason}</p>
                  <div className="mt-2">
                    <ConfidenceBar value={candidate.confidence} />
                  </div>
                </button>
              ))}
            </div>
          </div>
        </aside>
      </div>
    </section>
  );
}
