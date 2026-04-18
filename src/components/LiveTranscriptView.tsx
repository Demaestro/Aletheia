import { Pause, Play, Search } from "lucide-react";
import React, { useState } from "react";
import { scriptureCandidates, transcriptSegments } from "../data/production";
import type { ScriptureCandidate, TranscriptSegment } from "../types";
import { ActionButton, ConfidenceBar, SectionHeader, StatusPill } from "./Primitives";

const TranscriptRow = React.memo(({ segment, onSelectSegment }: { segment: TranscriptSegment; onSelectSegment?: (segment: TranscriptSegment) => void }) => {
  return (
    <button
      type="button"
      onClick={() => onSelectSegment?.(segment)}
      title="Click to search this line"
      className="group grid w-full grid-cols-[104px_160px_minmax(0,1fr)_120px] items-start gap-0 px-4 py-4 text-left transition hover:bg-mist focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
    >
      <span className="font-mono text-xs text-muted">{segment.time}</span>
      <span>
        <span className="block text-sm font-semibold text-ink">{segment.speaker}</span>
        <span className="mt-1 inline-block rounded bg-violet-500/15 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-violet-300 ring-1 ring-violet-500/20">
          {segment.language}
        </span>
      </span>
      <span className="pr-5 text-sm leading-6 text-graphite">{segment.text}</span>
      <span className="text-right font-mono text-xs text-muted">{segment.latencyMs} ms</span>
    </button>
  );
});

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
  const visibleTranscript = paused ? transcript.slice(0, transcript.length) : transcript;

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Live transcript"
        title="Speech stream and scripture evidence"
        detail="Operators can pause visual updates, inspect source evidence, and seed manual search from any transcript line."
        action={
          <div className="flex gap-2">
            <ActionButton tone="secondary" onClick={() => setPaused(true)} disabled={paused}>
              <Pause className="mr-2 h-4 w-4" aria-hidden="true" />
              Pause updates
            </ActionButton>
            <ActionButton onClick={() => setPaused(false)} disabled={!paused}>
              <Play className="mr-2 h-4 w-4" aria-hidden="true" />
              Resume
            </ActionButton>
          </div>
        }
      />

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <div className="rounded-[6px] border border-white/5 bg-white/5">
          <div className="grid grid-cols-[104px_160px_minmax(0,1fr)_120px] border-b border-white/5 px-4 py-3 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
            <span>Time</span>
            <span>Speaker</span>
            <span>Transcript</span>
            <span>Lag</span>
          </div>
          <div className="divide-y divide-line">
            {visibleTranscript.map((segment) => (
              <TranscriptRow key={segment.id} segment={segment} onSelectSegment={onSelectSegment} />
            ))}
          </div>
        </div>

        <aside className="space-y-4">
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-4">
            <p className="text-sm font-semibold text-ink">Source diagnostics</p>
            <div className="mt-4 space-y-3">
              <StatusPill tone="healthy" label="Offline STT" detail="active" />
              <StatusPill tone="healthy" label="Pulpit mic" detail="-14 dB" />
              <StatusPill tone="degraded" label="Cloud" detail="skipped" />
            </div>
            <p className="mt-4 text-sm leading-6 text-muted">
              Transcript logging is off. Segments are held locally for detection and discarded after the service unless saved.
            </p>
          </div>

          <div className="rounded-[6px] border border-white/5 bg-white/5 p-4">
            <p className="text-sm font-semibold text-ink">Detected references</p>
            <div className="mt-4 space-y-3">
              {candidates.slice(0, 3).map((candidate) => (
                <button
                  key={candidate.id}
                  type="button"
                  onClick={() => onPreview(candidate)}
                  className="w-full rounded-[6px] border border-white/5 p-3 text-left transition hover:border-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                >
                  <div className="flex items-center justify-between gap-3">
                    <p className="text-sm font-semibold text-ink">{candidate.reference}</p>
                    <Search className="h-4 w-4 text-muted" aria-hidden="true" />
                  </div>
                  <p className="mt-1 text-xs text-muted">{candidate.reason}</p>
                  <div className="mt-3">
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
