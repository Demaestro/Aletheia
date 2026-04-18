import { useRef } from "react";
import { motion } from "framer-motion";
import { CalendarClock, Upload, X, BookOpen, Music, Mic, HandHelping, Megaphone, Coins, MoreHorizontal } from "lucide-react";
import type { ServicePlanItem, ServicePlanItemKind } from "../types";
import { useServicePlanStore } from "../store/useServicePlanStore";
import { cn } from "./Primitives";

const kindIcon: Record<ServicePlanItemKind, typeof BookOpen> = {
  scripture: BookOpen,
  song: Music,
  sermon: Mic,
  prayer: HandHelping,
  announcement: Megaphone,
  offering: Coins,
  other: MoreHorizontal
};

const kindTone: Record<ServicePlanItemKind, string> = {
  scripture: "border-violet-400/40 bg-violet-500/10 text-violet-200",
  song: "border-emerald-400/40 bg-emerald-500/10 text-emerald-200",
  sermon: "border-amber-400/40 bg-amber-500/10 text-amber-200",
  prayer: "border-sky-400/40 bg-sky-500/10 text-sky-200",
  announcement: "border-fuchsia-400/40 bg-fuchsia-500/10 text-fuchsia-200",
  offering: "border-teal-400/40 bg-teal-500/10 text-teal-200",
  other: "border-white/15 bg-white/5 text-white/70"
};

function formatDuration(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return s === 0 ? `${m}m` : `${m}m${s.toString().padStart(2, "0")}s`;
}

export function ServicePlanTimeline() {
  const { plan, activeItemId, setActiveItem, loadFromJson, clear } = useServicePlanStore();
  const fileRef = useRef<HTMLInputElement>(null);

  const onPick = () => fileRef.current?.click();

  const onFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      loadFromJson(text);
    } catch (err) {
      console.error("Failed to load plan", err);
    } finally {
      e.target.value = "";
    }
  };

  if (!plan) {
    return (
      <div className="flex items-center gap-2 rounded-[6px] border border-dashed border-white/10 bg-white/[0.02] px-3 py-1.5 text-xs text-muted">
        <CalendarClock className="h-3.5 w-3.5 text-violet-400/80" aria-hidden="true" />
        <span>No service plan loaded</span>
        <button
          type="button"
          onClick={onPick}
          className="ml-1 inline-flex items-center gap-1 rounded-[4px] border border-white/10 bg-white/5 px-2 py-0.5 text-[11px] font-medium text-white/80 transition hover:border-violet-400/40 hover:bg-violet-500/10 hover:text-white"
        >
          <Upload className="h-3 w-3" aria-hidden="true" />
          Import .json
        </button>
        <input ref={fileRef} type="file" accept="application/json,.json" className="hidden" onChange={onFile} />
      </div>
    );
  }

  return (
    <div className="flex min-w-0 items-center gap-2">
      <div className="flex shrink-0 items-center gap-2 pr-2 text-xs">
        <CalendarClock className="h-3.5 w-3.5 text-violet-400/80" aria-hidden="true" />
        <span className="font-semibold text-ink">{plan.name}</span>
        <span className="text-muted">· {plan.items.length} items</span>
      </div>
      <div className="flex min-w-0 flex-1 gap-1.5 overflow-x-auto pb-1">
        {plan.items.map((item: ServicePlanItem) => {
          const Icon = kindIcon[item.kind] ?? MoreHorizontal;
          const active = item.id === activeItemId;
          return (
            <motion.button
              key={item.id}
              type="button"
              onClick={() => setActiveItem(item.id)}
              whileTap={{ scale: 0.97 }}
              title={item.note ?? `${item.title}${item.reference ? ` — ${item.reference}` : ""}`}
              className={cn(
                "group flex shrink-0 items-center gap-1.5 rounded-[6px] border px-2 py-1 text-[11px] font-medium transition",
                active
                  ? "border-violet-400/60 bg-violet-500/20 text-white shadow-[0_0_0_1px_rgba(124,58,237,0.35),0_6px_20px_-10px_rgba(124,58,237,0.55)]"
                  : kindTone[item.kind] + " hover:border-white/25 hover:bg-white/10"
              )}
            >
              <Icon className="h-3 w-3" aria-hidden="true" />
              <span className="max-w-[140px] truncate">{item.title}</span>
              <span className="font-mono text-[10px] opacity-70">{formatDuration(item.durationSec)}</span>
            </motion.button>
          );
        })}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={onPick}
          title="Replace plan"
          className="rounded-[4px] border border-white/10 bg-white/5 p-1 text-white/60 transition hover:border-violet-400/40 hover:bg-violet-500/10 hover:text-white"
        >
          <Upload className="h-3 w-3" aria-hidden="true" />
        </button>
        <button
          type="button"
          onClick={clear}
          title="Clear plan"
          className="rounded-[4px] border border-white/10 bg-white/5 p-1 text-white/60 transition hover:border-red-400/40 hover:bg-red-500/10 hover:text-red-300"
        >
          <X className="h-3 w-3" aria-hidden="true" />
        </button>
        <input ref={fileRef} type="file" accept="application/json,.json" className="hidden" onChange={onFile} />
      </div>
    </div>
  );
}
