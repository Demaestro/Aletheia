import { Check, Merge, ThumbsDown, ThumbsUp, X } from "lucide-react";
import { scriptureCandidates } from "../data/production";
import type { ScriptureCandidate } from "../types";
import { ActionButton, CandidateRow, SectionHeader, StatusPill } from "./Primitives";

export function QueueApprovalPanel({
  activeCandidate,
  candidates = scriptureCandidates,
  onPreview,
  onLive,
  onReject,
  onMerge,
  onApprove,
  onCalibrate
}: {
  activeCandidate: ScriptureCandidate;
  candidates?: ScriptureCandidate[];
  onPreview: (candidate: ScriptureCandidate) => void;
  onLive: (candidate: ScriptureCandidate) => void;
  onReject?: (candidate: ScriptureCandidate) => void;
  onMerge?: (candidate: ScriptureCandidate) => void;
  onApprove?: (candidate: ScriptureCandidate) => void;
  onCalibrate?: (candidate: ScriptureCandidate, outcome: "confirmed" | "corrected" | "rejected") => void;
}) {
  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Queue and approval"
        title="Make every detection explain itself"
        detail="Candidates are ordered by confidence and service context. High confidence may enter preview, but live output remains operator controlled."
        action={<StatusPill tone="armed" label="Auto-preview" detail="85% minimum" />}
      />

      <div className="grid gap-5 xl:grid-cols-[minmax(420px,0.9fr)_minmax(0,1fr)]">
        <div className="space-y-3">
          {candidates.map((candidate) => (
            <CandidateRow
              key={candidate.id}
              candidate={candidate}
              active={candidate.id === activeCandidate.id}
              onPreview={() => onPreview(candidate)}
              onLive={() => onLive(candidate)}
            />
          ))}
        </div>

        <aside className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Candidate detail</p>
              <h3 className="mt-2 text-3xl font-semibold tracking-tight text-ink">{activeCandidate.reference}</h3>
              <p className="mt-2 text-sm text-muted">
                {activeCandidate.translation} · {activeCandidate.language} · {activeCandidate.source}
              </p>
            </div>
            <StatusPill tone="armed" label={`${activeCandidate.confidence}%`} />
          </div>

          <blockquote className="mt-6 border-l-2 border-accent pl-4 text-2xl font-semibold leading-tight text-ink">
            {activeCandidate.text}
          </blockquote>

          <div className="mt-6 rounded-[6px] border border-white/5 bg-mist p-4">
            <p className="text-sm font-semibold text-ink">Why this candidate?</p>
            <p className="mt-2 text-sm leading-6 text-muted">{activeCandidate.reason}</p>
          </div>

          <div className="mt-6 grid gap-3 sm:grid-cols-2">
            <Field label="Translation" value={activeCandidate.translation} />
            <Field label="Range" value={activeCandidate.reference.includes(":") ? "Single verse" : "Chapter"} />
            <Field label="Language" value={activeCandidate.language} />
            <Field label="Policy" value="Manual live" />
          </div>

          {onCalibrate ? (
            <div className="mt-6 rounded-[6px] border border-violet-500/20 bg-violet-500/5 p-4">
              <p className="text-xs font-semibold uppercase tracking-[0.12em] text-violet-300">Calibration feedback</p>
              <p className="mt-1 text-xs leading-5 text-muted">
                Help the detector learn. Samples are stored locally and never uploaded.
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <ActionButton tone="secondary" onClick={() => onCalibrate(activeCandidate, "confirmed")}>
                  <ThumbsUp className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
                  Confirm detection
                </ActionButton>
                <ActionButton tone="secondary" onClick={() => onCalibrate(activeCandidate, "corrected")}>
                  Mark wrong reference
                </ActionButton>
                <ActionButton tone="secondary" onClick={() => onCalibrate(activeCandidate, "rejected")}>
                  <ThumbsDown className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
                  Not scripture
                </ActionButton>
              </div>
            </div>
          ) : null}

          <div className="mt-7 flex flex-wrap gap-2 border-t border-white/5 pt-5">
            <ActionButton tone="secondary" onClick={() => onMerge?.(activeCandidate)}>
              <Merge className="mr-2 h-4 w-4" aria-hidden="true" />
              Merge duplicate
            </ActionButton>
            <ActionButton tone="danger" onClick={() => onReject?.(activeCandidate)}>
              <X className="mr-2 h-4 w-4" aria-hidden="true" />
              Reject
            </ActionButton>
            <ActionButton tone="secondary" onClick={() => onApprove?.(activeCandidate)}>
              <ThumbsUp className="mr-2 h-4 w-4" aria-hidden="true" />
              Approve
            </ActionButton>
            <ActionButton tone="secondary" onClick={() => onPreview(activeCandidate)}>
              Preview
            </ActionButton>
            <ActionButton onClick={() => onLive(activeCandidate)}>
              <Check className="mr-2 h-4 w-4" aria-hidden="true" />
              Send live
            </ActionButton>
          </div>
        </aside>
      </div>
    </section>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[6px] border border-white/5 bg-white/5 px-3 py-3">
      <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">{label}</p>
      <p className="mt-1 text-sm font-semibold text-ink">{value}</p>
    </div>
  );
}
