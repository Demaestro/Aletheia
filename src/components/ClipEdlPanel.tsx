import { useEffect, useMemo, useRef, useState } from "react";
import { Film, Download, Plus, Trash2, Zap } from "lucide-react";
import type { ScriptureCandidate } from "../types";
import { ActionButton, SectionHeader, StatusPill, cn } from "./Primitives";
import { onLiveUpdated } from "../services/desktopApi";
import { loadDualPersisted, loadLocalSync, saveDualPersisted } from "../store/persistence";

type Marker = {
  id: string;
  atMs: number;
  label: string;
  reference: string;
  headSec: number;
  tailSec: number;
};

const STORAGE_KEY = "aletheia.clipMarkers.v1";

function loadMarkers(): Marker[] {
  const fromLocal = loadLocalSync<Marker[]>(STORAGE_KEY);
  return Array.isArray(fromLocal) ? fromLocal : [];
}

function persistMarkers(markers: Marker[]) {
  saveDualPersisted(STORAGE_KEY, markers);
}

function secToHms(sec: number): string {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  return [h, m, s].map((n) => n.toString().padStart(2, "0")).join(":");
}

export function ClipEdlPanel({
  live,
  serviceStartedAtMs
}: {
  live: ScriptureCandidate;
  serviceStartedAtMs: number;
}) {
  const [markers, setMarkers] = useState<Marker[]>(() => loadMarkers());

  // Reconcile with Rust KV after mount — cross-restart durability without
  // blocking initial render.
  useEffect(() => {
    void loadDualPersisted<Marker[]>(STORAGE_KEY).then((fromKv) => {
      if (Array.isArray(fromKv) && fromKv.length > 0) setMarkers(fromKv);
    });
  }, []);
  const [headSec, setHeadSec] = useState<number>(5);
  const [tailSec, setTailSec] = useState<number>(30);
  const [autoMode, setAutoMode] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem("aletheia.clipMarkers.auto") === "1";
  });
  const lastAutoReferenceRef = useRef<string>("");

  // Persist auto-mode preference.
  useEffect(() => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem("aletheia.clipMarkers.auto", autoMode ? "1" : "0");
  }, [autoMode]);

  // Auto-drop a marker whenever a new reference goes live (dedup consecutive identicals).
  useEffect(() => {
    if (!autoMode) return;
    let unlisten: (() => void) | undefined;
    void onLiveUpdated((payload) => {
      if (!payload?.reference) return;
      if (payload.reference === lastAutoReferenceRef.current) return;
      lastAutoReferenceRef.current = payload.reference;
      setMarkers((prev) => {
        const m: Marker = {
          id: `mk-auto-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
          atMs: Date.now(),
          label: payload.reference,
          reference: payload.reference,
          headSec,
          tailSec
        };
        const next = [m, ...prev];
        persistMarkers(next);
        return next;
      });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [autoMode, headSec, tailSec]);

  const addMarker = () => {
    const m: Marker = {
      id: `mk-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
      atMs: Date.now(),
      label: `${live.reference}`,
      reference: live.reference,
      headSec,
      tailSec
    };
    const next = [m, ...markers];
    persistMarkers(next);
    setMarkers(next);
  };

  const removeMarker = (id: string) => {
    const next = markers.filter((m) => m.id !== id);
    persistMarkers(next);
    setMarkers(next);
  };

  const clearAll = () => {
    persistMarkers([]);
    setMarkers([]);
  };

  const edl = useMemo(() => buildEdl(markers, serviceStartedAtMs), [markers, serviceStartedAtMs]);

  const downloadEdl = () => {
    const blob = new Blob([edl], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `aletheia-clips-${new Date().toISOString().slice(0, 10)}.edl`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  const downloadCsv = () => {
    const rows = ["label,reference,at_iso,head_seconds,tail_seconds,clip_in_service_hms"];
    for (const m of markers) {
      const offsetSec = Math.max(0, (m.atMs - serviceStartedAtMs) / 1000 - m.headSec);
      rows.push(
        [m.label, m.reference, new Date(m.atMs).toISOString(), m.headSec, m.tailSec, secToHms(offsetSec)]
          .map((v) => `"${String(v).replace(/"/g, '""')}"`)
          .join(",")
      );
    }
    const blob = new Blob([rows.join("\n")], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `aletheia-clips-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  return (
    <section className="space-y-5">
      <SectionHeader
        eyebrow="Clip EDL"
        title="Auto-clip markers for highlight exports"
        detail="Drop a marker whenever the live reference is noteworthy. Export to CMX3600 EDL for Resolve/Premiere or CSV for Descript/Opus — head & tail handles are configurable per marker."
        action={
          <div className="flex items-center gap-2">
            <StatusPill tone="neutral" label={`${markers.length} markers`} />
            <label className="inline-flex cursor-pointer items-center gap-2 rounded-[6px] border border-white/10 bg-white/5 px-2.5 py-1 text-[11px] font-medium text-white/80 transition hover:border-violet-400/40 hover:bg-violet-500/10">
              <Zap
                className={cn("h-3 w-3", autoMode ? "text-violet-300" : "text-white/40")}
                aria-hidden="true"
              />
              <input
                type="checkbox"
                checked={autoMode}
                onChange={(e) => setAutoMode(e.target.checked)}
                className="h-3 w-3 accent-violet-500"
                aria-label="Toggle auto-drop clip markers on live-updated events"
              />
              <span>Auto-drop on live</span>
            </label>
          </div>
        }
      />

      <div className="flex flex-wrap items-end gap-3 rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
        <div>
          <p className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">Current live</p>
          <p className="mt-1 font-mono text-sm text-ink">{live.reference}</p>
        </div>
        <NumericInput label="Head (sec)" value={headSec} onChange={setHeadSec} />
        <NumericInput label="Tail (sec)" value={tailSec} onChange={setTailSec} />
        <ActionButton onClick={addMarker}>
          <Plus className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
          Drop marker now
        </ActionButton>
        <div className="ml-auto flex gap-2">
          <ActionButton tone="secondary" onClick={downloadEdl} disabled={markers.length === 0}>
            <Download className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            Export .edl
          </ActionButton>
          <ActionButton tone="secondary" onClick={downloadCsv} disabled={markers.length === 0}>
            <Download className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            Export .csv
          </ActionButton>
          {markers.length > 0 ? (
            <ActionButton tone="danger" onClick={clearAll}>
              Clear all
            </ActionButton>
          ) : null}
        </div>
      </div>

      {markers.length === 0 ? (
        <div className="flex h-[160px] items-center justify-center rounded-[6px] border border-dashed border-white/10 bg-white/[0.015] text-sm text-muted">
          <span className="inline-flex items-center gap-2">
            <Film className="h-4 w-4 text-violet-400/80" aria-hidden="true" />
            No clip markers yet. Drop one when the live reference is highlight-worthy.
          </span>
        </div>
      ) : (
        <div className="overflow-auto rounded-[6px] border border-white/8 bg-white/[0.02]">
          <table className="w-full text-left text-xs">
            <thead className="bg-white/[0.04] text-[10px] uppercase tracking-wider text-muted">
              <tr>
                <th className="px-3 py-2">Service offset</th>
                <th className="px-3 py-2">Reference</th>
                <th className="px-3 py-2">Wall time</th>
                <th className="px-3 py-2">Head/Tail</th>
                <th className="px-3 py-2"></th>
              </tr>
            </thead>
            <tbody>
              {markers.map((m) => {
                const offsetSec = Math.max(0, (m.atMs - serviceStartedAtMs) / 1000);
                return (
                  <tr key={m.id} className={cn("border-t border-white/[0.04] text-ink/90")}>
                    <td className="px-3 py-2 font-mono">{secToHms(offsetSec)}</td>
                    <td className="px-3 py-2">{m.reference}</td>
                    <td className="px-3 py-2 font-mono text-[11px] text-muted">
                      {new Date(m.atMs).toLocaleTimeString()}
                    </td>
                    <td className="px-3 py-2 font-mono text-[11px]">
                      -{m.headSec}s / +{m.tailSec}s
                    </td>
                    <td className="px-3 py-2 text-right">
                      <button
                        type="button"
                        onClick={() => removeMarker(m.id)}
                        className="rounded-[4px] border border-white/10 bg-white/5 p-1 text-white/60 hover:border-red-400/40 hover:bg-red-500/10 hover:text-red-300"
                        title="Remove"
                        aria-label={`Remove clip marker for ${m.reference}`}
                      >
                        <Trash2 className="h-3 w-3" aria-hidden="true" />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function NumericInput({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">{label}</span>
      <input
        type="number"
        min={0}
        value={value}
        onChange={(e) => onChange(Math.max(0, Number(e.target.value) || 0))}
        className="w-24 rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-sm text-ink focus:border-violet-400/60 focus:outline-none focus:ring-1 focus:ring-violet-400/40"
      />
    </label>
  );
}

function buildEdl(markers: Marker[], serviceStartedAtMs: number): string {
  // Minimal CMX3600-style EDL. Most NLEs import this as cue/marker list.
  const lines: string[] = ["TITLE: ALETHEIA_CLIPS", "FCM: NON-DROP FRAME", ""];
  markers.forEach((m, i) => {
    const inSec = Math.max(0, (m.atMs - serviceStartedAtMs) / 1000 - m.headSec);
    const outSec = inSec + m.headSec + m.tailSec;
    const inTc = toTc(inSec);
    const outTc = toTc(outSec);
    lines.push(
      `${(i + 1).toString().padStart(3, "0")}  AX       V     C        ${inTc} ${outTc} ${inTc} ${outTc}`
    );
    lines.push(`* FROM CLIP NAME: ${m.reference.replace(/\s+/g, "_")}`);
    lines.push("");
  });
  return lines.join("\n");
}

function toTc(sec: number): string {
  const frames = Math.floor((sec - Math.floor(sec)) * 30);
  const s = Math.floor(sec) % 60;
  const m = Math.floor(sec / 60) % 60;
  const h = Math.floor(sec / 3600);
  return [h, m, s, frames].map((n) => n.toString().padStart(2, "0")).join(":");
}
