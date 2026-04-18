import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "framer-motion";
import { Music, Plus, Trash2, Radio, Download, FileText, ShieldCheck, Upload, Music2, ArrowUp, ArrowDown } from "lucide-react";
import type { Song, SongSection } from "../types";
import { useSongLibraryStore } from "../store/useSongLibraryStore";
import { ActionButton, SectionHeader, StatusPill, cn } from "./Primitives";
import {
  type CcliUsageEntry,
  exportCcliUsageCsv,
  listCcliUsage
} from "../services/desktopApi";
import { parseAny, toSongRecord, transposeChordProText, shiftChord } from "../lib/songImport";

export function SongLibraryPanel({
  serviceSessionId,
  operator,
  onSendSectionLive
}: {
  serviceSessionId: string;
  operator: string;
  onSendSectionLive?: (song: Song, section: SongSection) => void;
}) {
  const {
    songs,
    usage,
    activeSongId,
    activeSectionIdx,
    selectSong,
    selectSection,
    upsertSong,
    deleteSong,
    createSong,
    logSectionLive,
    clearUsageLog,
    exportUsageCsv
  } = useSongLibraryStore();

  const [filter, setFilter] = useState("");
  const importFileRef = useRef<HTMLInputElement>(null);
  const [importNotice, setImportNotice] = useState<string | null>(null);

  // Authoritative CCLI log from the Rust audit chain. We still keep the local
  // store for optimistic UI, but this is what gets exported for reporting.
  const [auditUsage, setAuditUsage] = useState<CcliUsageEntry[] | null>(null);
  const [auditError, setAuditError] = useState<string | null>(null);

  const refreshAuditUsage = async () => {
    try {
      const rows = await listCcliUsage(500);
      setAuditUsage(rows);
      setAuditError(null);
    } catch (err) {
      setAuditError(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => {
    void refreshAuditUsage();
  }, []);

  // Re-pull from Rust each time a local send happens — cheap, ensures the
  // audit-backed table stays in sync with the optimistic store.
  useEffect(() => {
    if (usage.length === 0) return;
    void refreshAuditUsage();
  }, [usage.length]);

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return songs;
    return songs.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        s.author.toLowerCase().includes(q) ||
        (s.ccliNumber ?? "").includes(q)
    );
  }, [songs, filter]);

  const active = songs.find((s) => s.id === activeSongId) ?? null;
  const activeSection = active?.sections[activeSectionIdx] ?? null;

  const handleSendLive = () => {
    if (!active || !activeSection) return;
    logSectionLive(active.id, activeSection, serviceSessionId, operator);
    onSendSectionLive?.(active, activeSection);
  };

  const updateField = <K extends keyof Song>(key: K, value: Song[K]) => {
    if (!active) return;
    upsertSong({ ...active, [key]: value });
  };

  const updateSection = (idx: number, patch: Partial<SongSection>) => {
    if (!active) return;
    const sections = active.sections.map((s, i) => (i === idx ? { ...s, ...patch } : s));
    upsertSong({ ...active, sections });
  };

  const addSection = () => {
    if (!active) return;
    const n = active.sections.length + 1;
    upsertSong({ ...active, sections: [...active.sections, { label: `Verse ${n}`, text: "" }] });
  };

  // ChordPro / OpenLyrics file import.
  const onImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const parsed = parseAny(text);
      const song = toSongRecord(parsed);
      upsertSong(song);
      selectSong(song.id);
      setImportNotice(
        `Imported "${song.title}" — ${song.sections.length} section${song.sections.length === 1 ? "" : "s"}.`
      );
    } catch (err) {
      setImportNotice(err instanceof Error ? `Import failed: ${err.message}` : "Import failed.");
    } finally {
      e.target.value = "";
    }
  };

  // Transpose: shift the active song key by ±N semitones, rewriting any inline
  // [CHORD] markers across all sections.
  const transposeBy = (semitones: number) => {
    if (!active || !active.songKey) return;
    const SHARP_SCALE = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    const FLAT_SCALE = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];
    const fromIdx = SHARP_SCALE.findIndex((n) => n === active.songKey) >= 0
      ? SHARP_SCALE.indexOf(active.songKey)
      : FLAT_SCALE.indexOf(active.songKey);
    if (fromIdx < 0) return;
    const targetIdx = (fromIdx + ((semitones % 12) + 12)) % 12;
    const flats = /^(F|Bb|Eb|Ab|Db|Gb)/.test(active.songKey);
    const newKey = (flats ? FLAT_SCALE : SHARP_SCALE)[targetIdx];
    const newSections = active.sections.map((sec) => ({
      ...sec,
      // Re-wrap any "Cm7"-style chord-only line into [Cm7] so transpose works,
      // then transpose, then leave brackets in place — the projected text
      // already strips brackets when ChordPro-imported.
      text: transposeChordProText(sec.text, active.songKey ?? "C", newKey),
    }));
    upsertSong({ ...active, songKey: newKey, sections: newSections });
    setImportNotice(`Transposed to ${newKey}.`);
  };

  const removeSection = (idx: number) => {
    if (!active || active.sections.length <= 1) return;
    const sections = active.sections.filter((_, i) => i !== idx);
    upsertSong({ ...active, sections });
    if (activeSectionIdx >= sections.length) selectSection(Math.max(0, sections.length - 1));
  };

  const exportCsv = async () => {
    // Prefer the Rust-side CSV — it's derived from the tamper-evident audit log.
    let csv: string;
    try {
      csv = await exportCcliUsageCsv();
    } catch {
      csv = exportUsageCsv();
    }
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `aletheia-ccli-usage-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  return (
    <section className="space-y-5">
      <SectionHeader
        eyebrow="Songs"
        title="Lyrics library with CCLI usage log"
        detail="Curate sections once, send them live by verse/chorus/bridge. Every live send of a CCLI-numbered song is recorded for quarterly reporting."
        action={
          <div className="flex items-center gap-2">
            <StatusPill tone="neutral" label={`${songs.length} songs`} detail={`${usage.length} usage rows`} />
            <input
              ref={importFileRef}
              type="file"
              accept=".chordpro,.cho,.crd,.txt,.xml,.openlyrics,application/xml,text/xml,text/plain"
              className="hidden"
              onChange={(e) => void onImportFile(e)}
            />
            <ActionButton
              tone="secondary"
              onClick={() => importFileRef.current?.click()}
              aria-label="Import ChordPro or OpenLyrics file"
            >
              <Upload className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              Import
            </ActionButton>
            <ActionButton tone="secondary" onClick={() => void exportCsv()}>
              <Download className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              Export CCLI CSV
            </ActionButton>
          </div>
        }
      />

      <div className="grid gap-4 lg:grid-cols-[260px_1fr]">
        {/* Library list */}
        <div className="space-y-3 rounded-[6px] border border-white/8 bg-white/[0.02] p-3">
          <div className="flex items-center gap-2">
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Search title / author / CCLI"
              className="flex-1 rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-xs text-ink placeholder:text-muted focus:border-violet-400/60 focus:outline-none focus:ring-1 focus:ring-violet-400/40"
            />
            <button
              type="button"
              onClick={() => createSong()}
              title="New song"
              aria-label="Create new song"
              className="rounded-[6px] border border-white/10 bg-white/5 p-1.5 text-white/70 transition hover:border-violet-400/40 hover:bg-violet-500/10 hover:text-white"
            >
              <Plus className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </div>
          <div className="max-h-[520px] space-y-1 overflow-y-auto">
            {filtered.length === 0 ? (
              <p className="px-2 py-4 text-center text-xs text-muted">No songs match.</p>
            ) : (
              filtered.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  onClick={() => selectSong(s.id)}
                  className={cn(
                    "flex w-full flex-col items-start gap-0.5 rounded-[6px] border px-2.5 py-2 text-left transition",
                    s.id === activeSongId
                      ? "border-violet-400/50 bg-violet-500/10 text-white"
                      : "border-transparent bg-white/[0.02] text-white/75 hover:border-white/10 hover:bg-white/[0.05]"
                  )}
                >
                  <span className="flex w-full items-center gap-2">
                    <Music className="h-3 w-3 shrink-0 opacity-60" aria-hidden="true" />
                    <span className="flex-1 truncate text-sm font-medium">{s.title}</span>
                  </span>
                  <span className="ml-5 truncate text-[11px] text-muted">
                    {s.author || "—"} {s.ccliNumber ? `· CCLI ${s.ccliNumber}` : ""}
                  </span>
                </button>
              ))
            )}
          </div>
        </div>

        {/* Editor / sections */}
        <div className="space-y-4 rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
          {!active ? (
            <div className="flex h-[480px] items-center justify-center text-sm text-muted">
              Select or create a song to begin.
            </div>
          ) : (
            <>
              <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                <LabeledInput
                  label="Title"
                  value={active.title}
                  onChange={(v) => updateField("title", v)}
                />
                <LabeledInput
                  label="Author"
                  value={active.author}
                  onChange={(v) => updateField("author", v)}
                />
                <LabeledInput
                  label="CCLI #"
                  value={active.ccliNumber ?? ""}
                  onChange={(v) => updateField("ccliNumber", v || null)}
                />
                <LabeledInput
                  label="Copyright"
                  value={active.copyright}
                  onChange={(v) => updateField("copyright", v)}
                />
                <LabeledInput
                  label="Language"
                  value={active.language}
                  onChange={(v) => updateField("language", v)}
                />
                <LabeledInput
                  label="Key"
                  value={active.songKey ?? ""}
                  onChange={(v) => updateField("songKey", v || null)}
                />
                <LabeledInput
                  label="BPM"
                  value={active.bpm?.toString() ?? ""}
                  onChange={(v) => updateField("bpm", v ? Number(v) : null)}
                />
                <div className="flex items-end">
                  <ActionButton tone="danger" onClick={() => deleteSong(active.id)}>
                    <Trash2 className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                    Delete
                  </ActionButton>
                </div>
              </div>

              {importNotice ? (
                <p className="rounded-[6px] border border-emerald-400/30 bg-emerald-500/10 px-2 py-1 text-[11px] text-emerald-200">
                  {importNotice}
                </p>
              ) : null}

              {active.songKey ? (
                <div className="flex items-center gap-2 rounded-[6px] border border-white/8 bg-black/30 px-2.5 py-1.5">
                  <Music2 className="h-3.5 w-3.5 text-violet-400/80" aria-hidden="true" />
                  <span className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">Key</span>
                  <span className="font-mono text-sm font-semibold text-white">{active.songKey}</span>
                  <div className="ml-auto flex items-center gap-1">
                    <button
                      type="button"
                      onClick={() => transposeBy(-1)}
                      title={`Transpose down → ${shiftChord(active.songKey, -1)}`}
                      aria-label="Transpose down one semitone"
                      className="rounded-[6px] border border-white/10 bg-white/5 p-1 text-white/70 hover:border-violet-400/40 hover:text-white"
                    >
                      <ArrowDown className="h-3 w-3" aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      onClick={() => transposeBy(1)}
                      title={`Transpose up → ${shiftChord(active.songKey, 1)}`}
                      aria-label="Transpose up one semitone"
                      className="rounded-[6px] border border-white/10 bg-white/5 p-1 text-white/70 hover:border-violet-400/40 hover:text-white"
                    >
                      <ArrowUp className="h-3 w-3" aria-hidden="true" />
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="flex flex-wrap gap-1.5">
                {active.sections.map((sec, idx) => (
                  <button
                    key={`${active.id}-${idx}`}
                    type="button"
                    onClick={() => selectSection(idx)}
                    className={cn(
                      "rounded-[6px] border px-2.5 py-1 text-[11px] font-medium transition",
                      idx === activeSectionIdx
                        ? "border-violet-400/60 bg-violet-500/20 text-white"
                        : "border-white/10 bg-white/5 text-white/70 hover:border-white/20 hover:bg-white/10"
                    )}
                  >
                    {sec.label}
                  </button>
                ))}
                <button
                  type="button"
                  onClick={addSection}
                  className="rounded-[6px] border border-dashed border-white/15 bg-white/[0.02] px-2.5 py-1 text-[11px] text-white/60 hover:border-violet-400/40 hover:text-white"
                >
                  + Section
                </button>
              </div>

              {activeSection ? (
                <motion.div
                  key={`${active.id}-${activeSectionIdx}`}
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  className="space-y-3"
                >
                  <div className="flex items-center gap-2">
                    <LabeledInput
                      label="Label"
                      value={activeSection.label}
                      onChange={(v) => updateSection(activeSectionIdx, { label: v })}
                    />
                    {active.sections.length > 1 ? (
                      <button
                        type="button"
                        onClick={() => removeSection(activeSectionIdx)}
                        title="Remove section"
                        aria-label="Remove this section"
                        className="mt-5 rounded-[6px] border border-white/10 bg-white/5 p-1.5 text-white/60 hover:border-red-400/40 hover:bg-red-500/10 hover:text-red-300"
                      >
                        <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                      </button>
                    ) : null}
                  </div>
                  <textarea
                    value={activeSection.text}
                    onChange={(e) => updateSection(activeSectionIdx, { text: e.target.value })}
                    rows={8}
                    className="w-full rounded-[6px] border border-white/10 bg-black/30 px-3 py-2 font-mono text-sm leading-6 text-ink focus:border-violet-400/60 focus:outline-none focus:ring-1 focus:ring-violet-400/40"
                    placeholder="Lyric lines — one per line"
                  />
                  <div className="flex items-center justify-between">
                    <p className="text-[11px] text-muted">
                      {active.ccliNumber ? "Live send will be recorded to the CCLI usage log." : "No CCLI # — sends are not logged."}
                    </p>
                    <ActionButton onClick={handleSendLive}>
                      <Radio className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                      Send {activeSection.label} live
                    </ActionButton>
                  </div>
                </motion.div>
              ) : null}
            </>
          )}
        </div>
      </div>

      {/* CCLI usage table */}
      <div className="rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
        <div className="mb-3 flex items-center justify-between">
          <div className="flex items-center gap-2 text-sm font-semibold text-ink">
            <FileText className="h-4 w-4 text-violet-400/80" aria-hidden="true" />
            CCLI usage log
            {auditUsage ? (
              <span
                className="ml-1 inline-flex items-center gap-1 rounded-[6px] border border-emerald-400/30 bg-emerald-500/10 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-widest text-emerald-200"
                title="Backed by the chained-hash audit log"
              >
                <ShieldCheck className="h-2.5 w-2.5" aria-hidden="true" /> audit
              </span>
            ) : null}
          </div>
          <div className="flex items-center gap-2">
            <span className="text-[11px] text-muted">
              {(auditUsage ?? usage).length} rows
            </span>
            {usage.length > 0 ? (
              <ActionButton tone="danger" onClick={clearUsageLog}>
                Clear local
              </ActionButton>
            ) : null}
          </div>
        </div>
        {auditError ? (
          <p className="mb-2 rounded-[6px] border border-amber-400/30 bg-amber-500/10 px-2 py-1 text-[11px] text-amber-200">
            Audit log unavailable — showing local entries only. ({auditError})
          </p>
        ) : null}
        {(() => {
          const rows = auditUsage ?? usage;
          if (rows.length === 0) {
            return (
              <p className="py-6 text-center text-xs text-muted">
                No entries yet. Live sends of CCLI-numbered songs appear here.
              </p>
            );
          }
          return (
            <div className="max-h-[260px] overflow-auto">
              <table className="w-full text-left text-xs">
                <thead className="sticky top-0 bg-white/[0.04] text-[10px] uppercase tracking-wider text-muted">
                  <tr>
                    <th className="px-2 py-1.5">When</th>
                    <th className="px-2 py-1.5">Title</th>
                    <th className="px-2 py-1.5">CCLI</th>
                    <th className="px-2 py-1.5">Session</th>
                    <th className="px-2 py-1.5">Operator</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.slice(0, 200).map((u) => (
                    <tr key={u.id} className="border-t border-white/[0.04] text-ink/90">
                      <td className="px-2 py-1.5 font-mono text-[11px] text-muted">
                        {new Date(u.sentLiveAtMs).toLocaleString()}
                      </td>
                      <td className="px-2 py-1.5">{u.songTitle}</td>
                      <td className="px-2 py-1.5 font-mono">{u.ccliNumber}</td>
                      <td className="px-2 py-1.5 font-mono text-[11px] text-muted">{u.serviceSessionId}</td>
                      <td className="px-2 py-1.5">{u.operator}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        })()}
      </div>
    </section>
  );
}

function LabeledInput({
  label,
  value,
  onChange
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">{label}</span>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-sm text-ink focus:border-violet-400/60 focus:outline-none focus:ring-1 focus:ring-violet-400/40"
      />
    </label>
  );
}
