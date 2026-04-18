import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { Radio, Download, Copy, PowerOff, Languages, Server, Globe } from "lucide-react";
import type { ScriptureCandidate } from "../types";
import { useStreamOverlayStore } from "../store/useStreamOverlayStore";
import { useTranslationStore } from "../store/useTranslationStore";
import { ActionButton, SectionHeader, StatusPill, cn } from "./Primitives";
import {
  type StreamOverlayServerStatus,
  getStreamOverlayServerStatus,
  startStreamOverlayServer,
  stopStreamOverlayServer,
  updateStreamOverlayState,
  vaultDeleteSecret,
  vaultReadSecret,
  vaultStoreSecret
} from "../services/desktopApi";

const TRANSLATION_VAULT_LABEL = "aletheia.translation.httpApiKey";

function GlossaryEditor() {
  const glossary = useTranslationStore((s) => s.glossary);
  const addGlossaryEntry = useTranslationStore((s) => s.addGlossaryEntry);
  const removeGlossaryEntry = useTranslationStore((s) => s.removeGlossaryEntry);
  const sourceLanguage = useTranslationStore((s) => s.sourceLanguage);
  const targetLanguages = useTranslationStore((s) => s.targetLanguages);
  const [draftTerm, setDraftTerm] = useState("");
  const [draftReplacement, setDraftReplacement] = useState("");
  const [draftTarget, setDraftTarget] = useState(targetLanguages[0] ?? "es");

  const submit = () => {
    if (!draftTerm.trim() || !draftReplacement.trim()) return;
    addGlossaryEntry({
      source: sourceLanguage,
      target: draftTarget,
      term: draftTerm.trim(),
      replacement: draftReplacement.trim(),
    });
    setDraftTerm("");
    setDraftReplacement("");
  };

  return (
    <div className="mt-4 border-t border-white/5 pt-3">
      <div className="flex items-center justify-between">
        <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">
          Glossary overrides
        </p>
        <span className="text-[10px] text-muted">{glossary.length} entries</span>
      </div>
      <p className="mt-1 text-[11px] text-muted">
        Locked terms — proper nouns, theological vocabulary, transliterations — applied
        post-translation so the same word always renders the same way.
      </p>
      <div className="mt-2 grid gap-2 md:grid-cols-[2fr_2fr_1fr_auto]">
        <input
          value={draftTerm}
          onChange={(e) => setDraftTerm(e.target.value)}
          placeholder="Source term (e.g. Yahweh)"
          className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-xs text-ink focus:border-violet-400/60 focus:outline-none"
        />
        <input
          value={draftReplacement}
          onChange={(e) => setDraftReplacement(e.target.value)}
          placeholder="Replacement (e.g. Yahwé)"
          className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-xs text-ink focus:border-violet-400/60 focus:outline-none"
        />
        <select
          value={draftTarget}
          onChange={(e) => setDraftTarget(e.target.value)}
          className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-xs text-ink focus:border-violet-400/60 focus:outline-none"
          aria-label="Glossary target language"
        >
          {(targetLanguages.length > 0 ? targetLanguages : ["es", "fr", "de", "pt", "zh", "ha", "ig", "yo", "sw", "xh"]).map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={submit}
          className="rounded-[6px] border border-violet-400/40 bg-violet-500/15 px-3 py-1.5 text-xs font-semibold text-white hover:bg-violet-500/25"
          aria-label="Add glossary entry"
        >
          Add
        </button>
      </div>
      {glossary.length > 0 ? (
        <ul className="mt-2 max-h-[140px] space-y-1 overflow-y-auto">
          {glossary.map((g, i) => (
            <li
              key={`${g.term}-${g.target}-${i}`}
              className="flex items-center justify-between rounded-[6px] border border-white/8 bg-black/20 px-2 py-1 text-[11px]"
            >
              <span>
                <span className="font-mono text-violet-300/90">{g.target}</span>{" "}
                <span className="text-white/85">{g.term}</span>
                <span className="mx-1.5 text-muted">→</span>
                <span className="text-white">{g.replacement}</span>
              </span>
              <button
                type="button"
                onClick={() => removeGlossaryEntry(i)}
                className="text-muted hover:text-rose-300"
                aria-label={`Remove glossary entry for ${g.term}`}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function StreamOverlayPanel({ live }: { live: ScriptureCandidate }) {
  const {
    tickerText,
    armed,
    liveReference,
    setTickerText,
    setArmed,
    publishLive,
    clearLive,
    exportHtml
  } = useStreamOverlayStore();

  const downloadHtml = () => {
    const html = exportHtml();
    const blob = new Blob([html], { type: "text/html;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "aletheia-stream-overlay.html";
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  const copyHtml = async () => {
    try {
      await navigator.clipboard.writeText(exportHtml());
    } catch {
      /* ignore */
    }
  };

  const publish = () => {
    if (!armed) return;
    publishLive(live.reference);
  };

  const translation = useTranslationStore();
  const [translations, setTranslations] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    async function run() {
      if (!translation.enabledForStream || translation.targetLanguages.length === 0) {
        setTranslations({});
        return;
      }
      const next: Record<string, string> = {};
      for (const t of translation.targetLanguages) {
        const out = await translation.translate(live.text, t);
        next[t] = out;
      }
      if (!cancelled) setTranslations(next);
    }
    void run();
    return () => {
      cancelled = true;
    };
  }, [live.text, translation.adapter, translation.enabledForStream, translation.targetLanguages.join(","), translation]);

  // Local HTTP server status — mirror Rust-side state so the OBS/vMix
  // browser source can point at http://127.0.0.1:<port>/ for live updates.
  const [serverStatus, setServerStatus] = useState<StreamOverlayServerStatus>({
    running: false,
    port: null,
    url: null,
    startedAtMs: null
  });
  const [serverBusy, setServerBusy] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void getStreamOverlayServerStatus()
      .then((s) => {
        if (!cancelled) setServerStatus(s);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  // Push overlay state to Rust whenever the displayed content changes so the
  // live HTTP page serves a fresh snapshot.
  useEffect(() => {
    if (!serverStatus.running) return;
    void updateStreamOverlayState({
      tickerText,
      armed,
      liveReference: liveReference ?? null,
      liveText: liveReference ? live.text : null,
      translations
    }).catch(() => undefined);
  }, [serverStatus.running, tickerText, armed, liveReference, live.text, translations]);

  const toggleServer = async () => {
    setServerBusy(true);
    setServerError(null);
    try {
      const next = serverStatus.running
        ? await stopStreamOverlayServer()
        : await startStreamOverlayServer();
      setServerStatus(next);
      if (next.running) {
        // Prime the server with the current state immediately.
        await updateStreamOverlayState({
          tickerText,
          armed,
          liveReference: liveReference ?? null,
          liveText: liveReference ? live.text : null,
          translations
        }).catch(() => undefined);
      }
    } catch (err) {
      setServerError(err instanceof Error ? err.message : String(err));
    } finally {
      setServerBusy(false);
    }
  };

  const copyServerUrl = async () => {
    if (!serverStatus.url) return;
    try {
      await navigator.clipboard.writeText(serverStatus.url);
    } catch {
      /* ignore */
    }
  };

  const toggleTarget = (code: string) => {
    const has = translation.targetLanguages.includes(code);
    translation.setTargetLanguages(has ? translation.targetLanguages.filter((c) => c !== code) : [...translation.targetLanguages, code]);
  };

  return (
    <section className="space-y-5">
      <SectionHeader
        eyebrow="Stream Overlay"
        title="Separate-lane overlay for the stream browser source"
        detail="Arm independently from the in-room projection. The HTML is self-contained — drop it into an OBS or vMix browser source."
        action={
          <div className="flex items-center gap-2">
            <StatusPill
              tone={armed ? (liveReference ? "live" : "armed") : "neutral"}
              label={armed ? (liveReference ? "Live on stream" : "Armed") : "Hold"}
              detail={liveReference ?? undefined}
            />
          </div>
        }
      />

      <div className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
        <div
          className={cn(
            "relative overflow-hidden rounded-[6px] border bg-[#0b0d12]",
            armed ? "border-rose-500/40 shadow-[0_0_0_1px_rgba(244,63,94,0.2),0_20px_60px_-24px_rgba(244,63,94,0.45)]" : "border-white/10"
          )}
        >
          <div className="flex items-center justify-between border-b border-white/10 px-3 py-2 text-[10px] text-white/45">
            <span>Stream overlay preview</span>
            <span className="font-mono tracking-widest">{armed ? (liveReference ? "PROGRAM" : "ARMED") : "PREVIEW"}</span>
          </div>
          <div className="relative h-[260px] bg-[radial-gradient(circle_at_30%_30%,#1a1033_0%,#06070c_70%)]">
            {liveReference ? (
              <motion.div
                key={liveReference}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.25 }}
                className="absolute bottom-14 left-[4%] rounded-[6px] bg-gradient-to-r from-violet-600/85 to-indigo-600/85 px-4 py-2 text-lg font-semibold text-white shadow-[0_8px_28px_-8px_rgba(124,58,237,0.6)]"
              >
                {liveReference}
              </motion.div>
            ) : null}
            <div className="absolute inset-x-0 bottom-0 overflow-hidden bg-black/70 py-2 text-sm text-white/85">
              <motion.div
                animate={{ x: ["100%", "-100%"] }}
                transition={{ duration: 28, repeat: Infinity, ease: "linear" }}
                className="whitespace-nowrap pl-4"
              >
                {tickerText || " "}
              </motion.div>
            </div>
          </div>
        </div>

        <div className="space-y-4 rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
          <div>
            <label className="text-xs font-semibold uppercase tracking-[0.12em] text-muted" htmlFor="ticker">
              Ticker text
            </label>
            <textarea
              id="ticker"
              value={tickerText}
              onChange={(e) => setTickerText(e.target.value)}
              rows={3}
              className="mt-2 w-full rounded-[6px] border border-white/10 bg-black/30 px-3 py-2 text-sm text-ink placeholder:text-muted focus:border-violet-400/60 focus:outline-none focus:ring-1 focus:ring-violet-400/40"
              placeholder="Welcome — announcements cycle here"
            />
          </div>

          <div className="grid grid-cols-2 gap-2">
            <ActionButton tone="secondary" onClick={() => setArmed(!armed)}>
              <PowerOff className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              {armed ? "Disarm" : "Arm overlay"}
            </ActionButton>
            <ActionButton onClick={publish} disabled={!armed}>
              <Radio className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              Publish live
            </ActionButton>
          </div>

          {liveReference ? (
            <ActionButton tone="danger" onClick={clearLive}>
              Clear overlay reference
            </ActionButton>
          ) : null}

          <div className="border-t border-white/5 pt-3">
            <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">Browser source</p>
            <div className="mt-2 grid grid-cols-2 gap-2">
              <ActionButton tone="secondary" onClick={downloadHtml}>
                <Download className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                Export HTML
              </ActionButton>
              <ActionButton tone="secondary" onClick={copyHtml}>
                <Copy className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                Copy HTML
              </ActionButton>
            </div>
            <p className="mt-2 text-[11px] leading-relaxed text-muted">
              Add the exported file as a local browser source in OBS/vMix. It updates when you re-export after edits.
            </p>
          </div>

          <div className="border-t border-white/5 pt-3">
            <div className="flex items-center justify-between gap-2">
              <p className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
                <Server className="h-3.5 w-3.5" aria-hidden="true" />
                Live overlay server
              </p>
              <StatusPill
                tone={serverStatus.running ? "live" : "neutral"}
                label={serverStatus.running ? "Online" : "Offline"}
                detail={serverStatus.port ? `:${serverStatus.port}` : undefined}
              />
            </div>
            <div className="mt-2 grid grid-cols-2 gap-2">
              <ActionButton
                tone={serverStatus.running ? "danger" : "secondary"}
                onClick={toggleServer}
                disabled={serverBusy}
                aria-label={serverStatus.running ? "Stop local overlay server" : "Start local overlay server"}
              >
                <Server className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                {serverStatus.running ? "Stop server" : "Start server"}
              </ActionButton>
              <ActionButton
                tone="secondary"
                onClick={copyServerUrl}
                disabled={!serverStatus.url}
                aria-label="Copy overlay URL to clipboard"
              >
                <Globe className="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
                Copy URL
              </ActionButton>
            </div>
            {serverStatus.url ? (
              <p className="mt-2 break-all rounded-[6px] border border-white/10 bg-black/30 px-2 py-1 font-mono text-[11px] text-violet-200/90">
                {serverStatus.url}
              </p>
            ) : (
              <p className="mt-2 text-[11px] leading-relaxed text-muted">
                Start the local server and point your OBS/vMix browser source at the printed URL for auto-refreshing live updates.
              </p>
            )}
            {serverError ? (
              <p className="mt-1 text-[11px] text-rose-400">{serverError}</p>
            ) : null}
          </div>
        </div>
      </div>

      {/* Translation lane */}
      <div className="rounded-[6px] border border-white/8 bg-white/[0.02] p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-ink">
            <Languages className="h-4 w-4 text-violet-400/80" aria-hidden="true" />
            Translation lane
          </div>
          <div className="flex items-center gap-2">
            <label className="flex items-center gap-2 text-[11px] text-muted">
              <span>Adapter</span>
              <select
                value={translation.adapter}
                onChange={(e) => translation.setAdapter(e.target.value as typeof translation.adapter)}
                className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1 text-xs text-ink focus:border-violet-400/60 focus:outline-none"
              >
                <option value="none">None</option>
                <option value="echo">Echo (local test)</option>
                <option value="http">HTTP endpoint</option>
              </select>
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-muted">
              <input
                type="checkbox"
                checked={translation.enabledForStream}
                onChange={(e) => translation.setEnabledForStream(e.target.checked)}
                className="h-3 w-3 accent-violet-500"
              />
              <span>Enabled on stream</span>
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-muted">
              <input
                type="checkbox"
                checked={translation.autoRouteFromDetection}
                onChange={(e) => translation.setAutoRouteFromDetection(e.target.checked)}
                className="h-3 w-3 accent-violet-500"
              />
              <span>Auto-route from detection</span>
            </label>
          </div>
        </div>

        {translation.adapter === "http" ? (
          <div className="mt-3 space-y-2">
            <div className="grid gap-2 md:grid-cols-[2fr_1fr]">
              <input
                value={translation.httpEndpoint}
                onChange={(e) => translation.setHttpEndpoint(e.target.value)}
                placeholder="https://your-worker.example.com/translate"
                className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-xs font-mono text-ink focus:border-violet-400/60 focus:outline-none"
                aria-label="Translation HTTP endpoint URL"
              />
              <input
                type="password"
                value={translation.httpApiKey}
                onChange={(e) => translation.setHttpApiKey(e.target.value)}
                placeholder="Bearer key (optional)"
                className="rounded-[6px] border border-white/10 bg-black/30 px-2 py-1.5 text-xs font-mono text-ink focus:border-violet-400/60 focus:outline-none"
                aria-label="Translation HTTP API key"
              />
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={async () => {
                  if (!translation.httpApiKey) return;
                  await vaultStoreSecret(TRANSLATION_VAULT_LABEL, translation.httpApiKey);
                }}
                className="rounded-[6px] border border-emerald-400/30 bg-emerald-500/10 px-2 py-1 text-[11px] font-medium text-emerald-200 hover:bg-emerald-500/20"
                title="Store the API key in the OS keyring (keychain / credential manager)"
              >
                Save key to OS vault
              </button>
              <button
                type="button"
                onClick={async () => {
                  const secret = await vaultReadSecret(TRANSLATION_VAULT_LABEL);
                  if (secret) translation.setHttpApiKey(secret);
                }}
                className="rounded-[6px] border border-white/10 bg-white/5 px-2 py-1 text-[11px] font-medium text-white/80 hover:bg-white/10"
                title="Load the API key from the OS keyring"
              >
                Load from vault
              </button>
              <button
                type="button"
                onClick={async () => {
                  await vaultDeleteSecret(TRANSLATION_VAULT_LABEL);
                  translation.setHttpApiKey("");
                }}
                className="rounded-[6px] border border-red-400/30 bg-red-500/10 px-2 py-1 text-[11px] font-medium text-red-200 hover:bg-red-500/20"
                title="Delete the API key from the OS keyring"
              >
                Forget vault key
              </button>
              <span className="text-[10px] text-muted">Keys persisted to OS vault never touch localStorage or KV mirror.</span>
            </div>
          </div>
        ) : null}

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">Targets</span>
          {["es", "fr", "de", "pt", "zh", "ha", "ig", "yo", "sw", "xh"].map((code) => {
            const active = translation.targetLanguages.includes(code);
            return (
              <button
                key={code}
                type="button"
                onClick={() => toggleTarget(code)}
                className={cn(
                  "rounded-[6px] border px-2 py-0.5 text-[11px] font-mono uppercase tracking-wider transition",
                  active
                    ? "border-violet-400/60 bg-violet-500/20 text-white"
                    : "border-white/10 bg-white/5 text-white/60 hover:border-white/20 hover:bg-white/10"
                )}
              >
                {code}
              </button>
            );
          })}
        </div>

        {translation.targetLanguages.length > 0 ? (
          <div className="mt-3 grid gap-2 md:grid-cols-2">
            {translation.targetLanguages.map((code) => (
              <div key={code} className="rounded-[6px] border border-white/8 bg-black/20 p-2.5 text-xs">
                <p className="font-mono text-[10px] uppercase tracking-widest text-violet-300/80">{code}</p>
                <p className="mt-1 text-ink/90">{translations[code] ?? <span className="italic text-muted">Translating…</span>}</p>
              </div>
            ))}
          </div>
        ) : (
          <p className="mt-3 text-[11px] text-muted">
            Pick target languages above. HTTP adapter expects POST JSON {"{ text, source, target }"} → {"{ text }"}.
          </p>
        )}

        <GlossaryEditor />
      </div>
    </section>
  );
}
