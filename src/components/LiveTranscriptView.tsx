import { Pause, Play, Search } from "lucide-react";
import React, { useRef, useState } from "react";
import { scriptureCandidates, transcriptSegments } from "../data/production";
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
  const frozenRef = useRef<TranscriptSegment[]>([]);
  const visibleTranscript = paused ? frozenRef.current : transcript;

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Live transcript"
        title="Speech stream and scripture evidence"
        detail="Pause to freeze the live feed, then click any line to seed a manual scripture search."
        action={
          <div className="flex gap-2">
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
              Transcript logging is off. Segments are held locally for detection and discarded after the service unless saved.
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
