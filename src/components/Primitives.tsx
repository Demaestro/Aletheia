import { motion } from "framer-motion";
import type { ReactNode } from "react";
import type { ScriptureCandidate, Tone } from "../types";

export const cn = (...classes: Array<string | false | null | undefined>) =>
  classes.filter(Boolean).join(" ");

const toneClass: Record<Tone, string> = {
  healthy: "border-emerald-500/30 bg-emerald-500/10 text-emerald-400 shadow-neon shadow-emerald-500/20",
  degraded: "border-amber-500/30 bg-amber-500/10 text-amber-400 shadow-neon shadow-amber-500/20",
  offline: "border-red-500/30 bg-red-500/10 text-red-400 shadow-neon shadow-red-500/20",
  live: "border-red-500/40 bg-red-500/20 text-red-400 shadow-neon shadow-red-500/30",
  armed: "border-violet-500/30 bg-violet-500/10 text-violet-400 shadow-neon shadow-violet-500/20",
  neutral: "border-white/10 bg-white/5 text-white/60"
};

export const fadeUp = {
  hidden: { opacity: 0, y: 14 },
  visible: { opacity: 1, y: 0 }
};

export function StatusPill({
  tone = "neutral",
  label,
  detail
}: {
  tone?: Tone;
  label: string;
  detail?: string;
}) {
  return (
    <motion.span
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.18 }}
      className={cn(
        "inline-flex items-center gap-2 rounded-[6px] border px-2.5 py-1 text-xs font-semibold backdrop-blur-sm",
        toneClass[tone]
      )}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden="true" />
      <span>{label}</span>
      {detail ? <span className="font-normal opacity-75">{detail}</span> : null}
    </motion.span>
  );
}

export function SectionHeader({
  eyebrow,
  title,
  detail,
  action
}: {
  eyebrow: string;
  title: string;
  detail?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-4 border-b border-white/5 pb-5 md:flex-row md:items-end md:justify-between">
      <div>
        <p className="text-xs font-semibold uppercase tracking-[0.12em] text-violet-400/80">{eyebrow}</p>
        <h2 className="mt-2 text-2xl font-semibold tracking-tight text-ink">{title}</h2>
        {detail ? <p className="mt-2 max-w-3xl text-sm leading-6 text-muted">{detail}</p> : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}

export function ActionButton({
  children,
  onClick,
  tone = "primary",
  disabled = false,
  className
}: {
  children: ReactNode;
  onClick?: () => void;
  tone?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  className?: string;
}) {
  return (
    <motion.button
      type="button"
      whileTap={disabled ? undefined : { scale: 0.97 }}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "group relative inline-flex min-h-10 items-center justify-center overflow-hidden rounded-[6px] px-4 text-sm font-semibold transition-all duration-150 ease-out focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-violet-500 disabled:cursor-not-allowed disabled:opacity-45 active:scale-[0.98]",
        tone === "primary" &&
          "bg-gradient-to-r from-violet-600 to-indigo-600 text-white ring-1 ring-white/10 shadow-[0_4px_20px_-4px_rgba(124,58,237,0.5)] hover:from-violet-500 hover:to-indigo-500 hover:shadow-[0_8px_28px_-4px_rgba(124,58,237,0.65)] active:shadow-[0_2px_10px_-2px_rgba(124,58,237,0.4)]",
        tone === "secondary" &&
          "border border-white/10 bg-white/5 text-white/80 backdrop-blur-md hover:border-violet-400/30 hover:bg-white/[0.09] hover:text-white hover:shadow-[0_2px_12px_-2px_rgba(124,58,237,0.25)]",
        tone === "danger" &&
          "border border-red-500/30 bg-red-500/10 text-red-400 hover:bg-red-500/20 hover:shadow-[0_4px_18px_-4px_rgba(239,68,68,0.5)]",
        className
      )}
    >
      {tone === "primary" ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-white/40 to-transparent"
        />
      ) : null}
      <span className="relative z-10 inline-flex items-center">{children}</span>
    </motion.button>
  );
}

export function ConfidenceBar({ value }: { value: number }) {
  const tone =
    value >= 85
      ? "bg-gradient-to-r from-emerald-500 to-teal-400"
      : value >= 72
      ? "bg-gradient-to-r from-amber-500 to-orange-400"
      : "bg-gradient-to-r from-rose-500 to-red-400";

  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-xs text-muted">
        <span>Confidence</span>
        <span className="font-mono text-ink">{value}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-white/5 ring-1 ring-white/5" aria-hidden="true">
        <div
          className={cn("h-full rounded-full transition-[width] duration-500 ease-out", tone)}
          style={{ width: `${value}%` }}
        />
      </div>
    </div>
  );
}

export function CandidateRow({
  candidate,
  active,
  onPreview,
  onLive
}: {
  candidate: ScriptureCandidate;
  active?: boolean;
  onPreview?: () => void;
  onLive?: () => void;
}) {
  return (
    <motion.article
      layout
      whileHover={{ y: -2, transition: { duration: 0.15 } }}
      className={cn(
        "group rounded-[6px] border p-4 transition-all duration-200 ease-out",
        active
          ? "border-violet-500/40 bg-gradient-to-br from-violet-500/10 to-transparent shadow-[0_8px_30px_-8px_rgba(124,58,237,0.3)]"
          : "border-white/8 bg-white/[0.03] hover:border-violet-400/30 hover:bg-white/[0.05] hover:shadow-[0_4px_20px_-8px_rgba(124,58,237,0.2)]"
      )}
    >
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-base font-semibold text-ink">{candidate.reference}</p>
          <p className="mt-1 text-xs font-medium text-muted">
            {candidate.translation} · {candidate.language} · {candidate.source}
          </p>
        </div>
        <StatusPill
          tone={candidate.status === "live" ? "live" : candidate.status === "preview" ? "armed" : "neutral"}
          label={candidate.status.replace("_", " ")}
        />
      </div>
      <p className="mt-3 text-sm leading-6 text-graphite">{candidate.text}</p>
      <div className="mt-4">
        <ConfidenceBar value={candidate.confidence} />
      </div>
      <p className="mt-3 text-xs leading-5 text-muted">{candidate.reason}</p>
      <div className="mt-4 flex flex-wrap gap-2 opacity-100 transition md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100">
        <ActionButton tone="secondary" onClick={onPreview}>
          Preview
        </ActionButton>
        <ActionButton onClick={onLive}>Send live</ActionButton>
      </div>
    </motion.article>
  );
}

export function OutputCanvas({
  label,
  candidate,
  state
}: {
  label: string;
  candidate: ScriptureCandidate;
  state: "preview" | "live";
}) {
  return (
    <div
      className={cn(
        "overflow-hidden rounded-[6px] border border-white/10 bg-[#0c0e12]",
        state === "live" &&
          "shadow-[inset_0_0_0_1px_rgba(244,63,94,0.15),0_0_28px_-12px_rgba(244,63,94,0.5)]"
      )}
    >
      {label ? (
        <div className="flex items-center justify-between border-b border-white/10 px-3 py-2 text-[10px] text-white/50">
          <span>{label}</span>
          <span className="font-mono tracking-widest">{state === "live" ? "PROGRAM" : "PREVIEW"}</span>
        </div>
      ) : (
        <div className="flex items-center justify-end border-b border-white/8 px-3 py-1.5">
          <span className="font-mono text-[9px] tracking-widest text-white/30">
            {state === "live" ? "PROGRAM" : "PREVIEW"}
          </span>
        </div>
      )}
      <div className="flex min-h-[140px] flex-col justify-end bg-[#0c0e12] p-4 text-white">
        <motion.div key={candidate.id} initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.18 }}>
          <p className="text-[10px] font-semibold uppercase tracking-[0.18em] text-white/50">
            {candidate.reference} · {candidate.translation}
          </p>
          <p className="mt-2 text-base font-semibold leading-snug tracking-tight text-white/90">
            {candidate.text || <span className="text-white/20 italic">No content</span>}
          </p>
        </motion.div>
      </div>
    </div>
  );
}

export function Metric({
  label,
  value,
  detail
}: {
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="border-l border-white/5 pl-4">
      <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">{label}</p>
      <p className="mt-2 text-xl font-semibold text-ink">{value}</p>
      <p className="mt-1 text-sm text-muted">{detail}</p>
    </div>
  );
}
