/**
 * DiagnosticsScreen.tsx — Operator-facing live system diagnostics.
 *
 * Polls `get_system_diagnostics` every 2 s and `get_model_catalogue` once on
 * mount. Designed for the sound-booth operator who needs at-a-glance
 * confidence that every pipeline stage is healthy before / during a service.
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  AudioLines,
  BookOpen,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  Clock,
  Cpu,
  Database,
  HardDrive,
  Mic,
  MicOff,
  Radio,
  ShieldCheck,
  ShieldX,
  Timer,
  TrendingUp,
  Wifi,
  WifiOff,
  XCircle,
  Zap,
} from "lucide-react";

// ── Tauri-side types ────────────────────────────────────────────────────────

interface LatencyStats {
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  maxMs: number;
  sampleCount: number;
  breachCount: number;
}

interface SystemDiagnostics {
  activeMic: string | null;
  captureRunning: boolean;
  audioQueueDepth: number;
  sttModelLoaded: boolean;
  sttModelFilename: string | null;
  sttModelQuality: string | null;
  sttModelSizeMb: number | null;
  sttModelChecksumOk: boolean | null;
  latency: LatencyStats;
  phrasePatternCount: number;
  lastDetectionReference: string | null;
  lastDetectionConfidence: number | null;
  lastDetectionBucket: string | null;
  detectionsThisSession: number;
  rejectedThisSession: number;
  suppressedThisSession: number;
  bibleDbOk: boolean;
  translationsLoaded: string[];
  vmixState: string;
  obsState: string;
  easyworshipState: string;
  propresenterState: string;
  companionState: string;
  oscState: string;
  checkedAtMs: number;
}

interface ModelCatalogueEntry {
  filename: string;
  quality: string;
  label: string;
  sizeMb: number;
  ramRequiredMb: number;
  expectedLatencyMs: number;
  sha256: string;
  downloadUrl: string;
  recommended: boolean;
  installed: boolean;
  checksumOk: boolean | null;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function stateColour(state: string): string {
  switch (state?.toLowerCase()) {
    case "online":
    case "connected":
    case "ready":
      return "#34d399"; // emerald-400
    case "degraded":
    case "warning":
      return "#fbbf24"; // amber-400
    case "offline":
    case "error":
    case "disconnected":
      return "#f87171"; // red-400
    default:
      return "#94a3b8"; // slate-400
  }
}

function stateIcon(state: string, size = 14) {
  const lower = state?.toLowerCase();
  if (lower === "online" || lower === "connected" || lower === "ready")
    return <CheckCircle2 size={size} />;
  if (lower === "degraded" || lower === "warning")
    return <Activity size={size} />;
  if (lower === "offline" || lower === "error" || lower === "disconnected")
    return <XCircle size={size} />;
  return <CircleDot size={size} />;
}

function msFormat(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function qualityColour(quality: string | null): string {
  switch (quality) {
    case "tiny":  return "#f87171";
    case "base":  return "#fb923c";
    case "small": return "#34d399";
    case "medium":return "#60a5fa";
    case "large": return "#a78bfa";
    default:      return "#94a3b8";
  }
}

// ── Sub-components ───────────────────────────────────────────────────────────

function SectionCard({ title, icon, children, accent }: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
  accent?: string;
}) {
  return (
    <div style={{
      background: "rgba(15,23,42,0.7)",
      border: `1px solid ${accent ?? "rgba(99,102,241,0.2)"}`,
      borderRadius: 12,
      padding: "16px 18px",
      backdropFilter: "blur(12px)",
    }}>
      <div style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        marginBottom: 14,
        color: accent ?? "#818cf8",
        fontSize: 13,
        fontWeight: 600,
        letterSpacing: "0.04em",
        textTransform: "uppercase",
      }}>
        {icon}
        {title}
      </div>
      {children}
    </div>
  );
}

function MetricRow({ label, value, sub, colour }: {
  label: string;
  value: React.ReactNode;
  sub?: string;
  colour?: string;
}) {
  return (
    <div style={{
      display: "flex",
      justifyContent: "space-between",
      alignItems: "baseline",
      padding: "5px 0",
      borderBottom: "1px solid rgba(255,255,255,0.05)",
    }}>
      <span style={{ color: "#94a3b8", fontSize: 12 }}>{label}</span>
      <span style={{ color: colour ?? "#e2e8f0", fontSize: 13, fontWeight: 600, textAlign: "right" }}>
        {value}
        {sub && <span style={{ color: "#64748b", fontSize: 11, marginLeft: 5 }}>{sub}</span>}
      </span>
    </div>
  );
}

function AdapterPill({ label, state }: { label: string; state: string }) {
  const colour = stateColour(state);
  return (
    <div style={{
      display: "flex",
      alignItems: "center",
      gap: 6,
      padding: "4px 10px",
      borderRadius: 8,
      background: `${colour}18`,
      border: `1px solid ${colour}40`,
    }}>
      <span style={{ color: colour, lineHeight: 1 }}>{stateIcon(state, 12)}</span>
      <span style={{ color: "#cbd5e1", fontSize: 12, fontWeight: 500 }}>{label}</span>
      <span style={{ color: colour, fontSize: 11, marginLeft: "auto" }}>{state}</span>
    </div>
  );
}

// Tiny latency bar chart (sparkbar)
function LatencyBar({ value, max, p95 }: { value: number; max: number; p95: number }) {
  const pct = max > 0 ? (value / max) * 100 : 0;
  const breach = value > 3000;
  const colour = breach ? "#f87171" : value > p95 ? "#fbbf24" : "#34d399";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <div style={{
        flex: 1,
        height: 6,
        borderRadius: 3,
        background: "rgba(255,255,255,0.07)",
        overflow: "hidden",
      }}>
        <div style={{
          width: `${pct}%`,
          height: "100%",
          background: colour,
          borderRadius: 3,
          transition: "width 0.3s ease",
        }} />
      </div>
      <span style={{ color: colour, fontSize: 11, minWidth: 42, textAlign: "right", fontFamily: "monospace" }}>
        {msFormat(value)}
      </span>
    </div>
  );
}

// ── Main component ───────────────────────────────────────────────────────────

export function DiagnosticsScreen() {
  const [diag, setDiag] = useState<SystemDiagnostics | null>(null);
  const [catalogue, setCatalogue] = useState<ModelCatalogueEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  async function fetchDiag() {
    try {
      const d = await invoke<SystemDiagnostics>("get_system_diagnostics");
      setDiag(d);
      setLastUpdated(new Date());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function fetchCatalogue() {
    try {
      const c = await invoke<ModelCatalogueEntry[]>("get_model_catalogue");
      setCatalogue(c);
    } catch {
      // non-fatal
    }
  }

  useEffect(() => {
    fetchDiag();
    fetchCatalogue();
    intervalRef.current = setInterval(fetchDiag, 2000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  // ── Render loading state ──────────────────────────────────────────────────
  if (!diag) {
    return (
      <div style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        color: "#64748b",
        fontSize: 14,
        gap: 10,
      }}>
        <Activity size={18} style={{ animation: "spin 1s linear infinite" }} />
        {error ? (
          <span style={{ color: "#f87171" }}>Diagnostics unavailable — {error}</span>
        ) : (
          "Loading diagnostics…"
        )}
      </div>
    );
  }

  const captureColour   = diag.captureRunning ? "#34d399" : "#f87171";
  const sttColour       = diag.sttModelLoaded ? "#34d399" : "#f87171";
  const dbColour        = diag.bibleDbOk      ? "#34d399" : "#f87171";
  const checksumColour  = diag.sttModelChecksumOk === true  ? "#34d399" :
                          diag.sttModelChecksumOk === false ? "#f87171" : "#94a3b8";

  const installedEntry = catalogue.find(e =>
    e.filename === diag.sttModelFilename
  );

  const adapters = [
    { label: "vMix",         state: diag.vmixState         },
    { label: "OBS",          state: diag.obsState          },
    { label: "EasyWorship",  state: diag.easyworshipState  },
    { label: "ProPresenter", state: diag.propresenterState },
    { label: "Companion",    state: diag.companionState     },
    { label: "OSC",          state: diag.oscState           },
  ];

  const connectedAdapters = adapters.filter(a =>
    ["online","connected","ready"].includes(a.state?.toLowerCase())
  ).length;

  return (
    <div style={{
      padding: "20px 24px",
      display: "flex",
      flexDirection: "column",
      gap: 16,
      height: "100%",
      overflowY: "auto",
      fontFamily: "'Inter', 'Segoe UI', sans-serif",
    }}>
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
        <div>
          <h2 style={{ margin: 0, color: "#e2e8f0", fontSize: 20, fontWeight: 700 }}>
            System Diagnostics
          </h2>
          <p style={{ margin: "3px 0 0", color: "#64748b", fontSize: 12 }}>
            Live pipeline health — refreshes every 2 s
          </p>
        </div>
        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 3 }}>
          <span style={{
            padding: "3px 10px",
            borderRadius: 20,
            background: diag.captureRunning ? "rgba(52,211,153,0.12)" : "rgba(248,113,113,0.12)",
            border: `1px solid ${captureColour}40`,
            color: captureColour,
            fontSize: 11,
            fontWeight: 600,
            display: "flex",
            alignItems: "center",
            gap: 5,
          }}>
            <span style={{
              width: 6, height: 6, borderRadius: "50%",
              background: captureColour,
              animation: diag.captureRunning ? "pulse 1.5s infinite" : "none",
            }} />
            {diag.captureRunning ? "CAPTURE ACTIVE" : "CAPTURE STOPPED"}
          </span>
          {lastUpdated && (
            <span style={{ color: "#475569", fontSize: 10 }}>
              Updated {lastUpdated.toLocaleTimeString()}
            </span>
          )}
        </div>
      </div>

      {/* ── Top row ─────────────────────────────────────────────────────── */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>

        {/* Audio */}
        <SectionCard title="Audio Capture" icon={<Mic size={14} />} accent="#818cf8">
          <MetricRow
            label="Microphone"
            value={diag.activeMic ?? "Default system mic"}
            colour="#cbd5e1"
          />
          <MetricRow
            label="Status"
            value={diag.captureRunning ? "Listening" : "Stopped"}
            colour={captureColour}
          />
          <MetricRow
            label="Queue depth"
            value={diag.audioQueueDepth}
            sub="frames"
            colour={diag.audioQueueDepth > 8 ? "#fbbf24" : "#34d399"}
          />
        </SectionCard>

        {/* STT model */}
        <SectionCard title="STT Engine" icon={<Cpu size={14} />} accent={qualityColour(diag.sttModelQuality)}>
          <MetricRow
            label="Model loaded"
            value={diag.sttModelLoaded ? "Yes" : "No"}
            colour={sttColour}
          />
          <MetricRow
            label="File"
            value={diag.sttModelFilename ?? "—"}
            colour="#cbd5e1"
          />
          <MetricRow
            label="Tier"
            value={diag.sttModelQuality
              ? <span style={{
                  padding: "1px 8px",
                  borderRadius: 6,
                  background: `${qualityColour(diag.sttModelQuality)}22`,
                  border: `1px solid ${qualityColour(diag.sttModelQuality)}50`,
                  color: qualityColour(diag.sttModelQuality),
                  fontSize: 11,
                  fontWeight: 700,
                  textTransform: "uppercase" as const,
                }}>
                  {diag.sttModelQuality}
                </span>
              : "Unknown"}
          />
          <MetricRow
            label="Size"
            value={diag.sttModelSizeMb ? `${diag.sttModelSizeMb} MB` : "—"}
          />
          <MetricRow
            label="SHA-256 integrity"
            value={
              diag.sttModelChecksumOk === null
                ? "—"
                : diag.sttModelChecksumOk
                  ? <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
                      <ShieldCheck size={12} /> Verified
                    </span>
                  : <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
                      <ShieldX size={12} /> FAILED
                    </span>
            }
            colour={checksumColour}
          />
          {installedEntry?.recommended === false && diag.sttModelQuality !== null && (
            <div style={{
              marginTop: 8,
              padding: "5px 8px",
              borderRadius: 6,
              background: "rgba(251,191,36,0.08)",
              border: "1px solid rgba(251,191,36,0.25)",
              color: "#fbbf24",
              fontSize: 11,
            }}>
              ⚠ For live services, upgrade to ggml-small.en.bin or better
            </div>
          )}
        </SectionCard>

        {/* Bible DB */}
        <SectionCard title="Bible Database" icon={<Database size={14} />} accent={dbColour}>
          <MetricRow
            label="Status"
            value={diag.bibleDbOk ? "Online" : "Unavailable"}
            colour={dbColour}
          />
          <MetricRow
            label="Translations"
            value={diag.translationsLoaded.length
              ? diag.translationsLoaded.join(", ")
              : "None loaded"}
            colour={diag.translationsLoaded.length ? "#34d399" : "#f87171"}
          />
          <MetricRow
            label="Phrase patterns"
            value={diag.phrasePatternCount}
            sub="loaded"
          />
        </SectionCard>
      </div>

      {/* ── Latency row ──────────────────────────────────────────────────── */}
      <SectionCard title="Pipeline Latency" icon={<Timer size={14} />} accent="#60a5fa">
        {diag.latency.sampleCount === 0 ? (
          <p style={{ color: "#64748b", fontSize: 12, margin: 0, textAlign: "center", padding: "12px 0" }}>
            No latency samples yet — start the audio capture to begin profiling.
          </p>
        ) : (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr 1fr", gap: 16 }}>
            {[
              { label: "P50", value: diag.latency.p50Ms },
              { label: "P95", value: diag.latency.p95Ms },
              { label: "P99", value: diag.latency.p99Ms },
              { label: "Max", value: diag.latency.maxMs },
            ].map(({ label, value }) => (
              <div key={label}>
                <div style={{ color: "#475569", fontSize: 11, marginBottom: 4 }}>{label}</div>
                <LatencyBar
                  value={value}
                  max={diag.latency.maxMs}
                  p95={diag.latency.p95Ms}
                />
              </div>
            ))}
          </div>
        )}
        {diag.latency.sampleCount > 0 && (
          <div style={{
            display: "flex",
            gap: 20,
            marginTop: 10,
            paddingTop: 10,
            borderTop: "1px solid rgba(255,255,255,0.06)",
          }}>
            <span style={{ color: "#64748b", fontSize: 11 }}>
              <span style={{ color: "#94a3b8", fontWeight: 600 }}>{diag.latency.sampleCount}</span> samples
            </span>
            <span style={{ color: "#64748b", fontSize: 11 }}>
              <span style={{ color: diag.latency.breachCount > 0 ? "#f87171" : "#34d399", fontWeight: 600 }}>
                {diag.latency.breachCount}
              </span> breaches &gt;3 s
            </span>
            {diag.latency.p95Ms < 2000 && (
              <span style={{ color: "#34d399", fontSize: 11, display: "flex", alignItems: "center", gap: 4 }}>
                <Zap size={11} /> On target (&lt;2 s P95)
              </span>
            )}
          </div>
        )}
      </SectionCard>

      {/* ── Detection + Adapters ──────────────────────────────────────────── */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>

        {/* Detection engine */}
        <SectionCard title="Detection Engine" icon={<TrendingUp size={14} />} accent="#a78bfa">
          <MetricRow label="Detections (raw)"     value={diag.detectionsThisSession} />
          <MetricRow
            label="Suppressed (debounce 15 s)"
            value={diag.suppressedThisSession}
            colour={diag.suppressedThisSession > 0 ? "#fbbf24" : "#94a3b8"}
          />
          <MetricRow
            label="Operator rejections"
            value={diag.rejectedThisSession}
            colour={diag.rejectedThisSession > 0 ? "#fb923c" : "#94a3b8"}
          />
          {diag.lastDetectionReference && (
            <>
              <div style={{ height: 1, background: "rgba(255,255,255,0.05)", margin: "8px 0" }} />
              <MetricRow
                label="Last reference"
                value={diag.lastDetectionReference}
                colour="#818cf8"
              />
              {diag.lastDetectionConfidence !== null && (
                <MetricRow
                  label="Confidence"
                  value={`${Math.round(diag.lastDetectionConfidence * 100)}%`}
                  colour={
                    diag.lastDetectionConfidence >= 0.85 ? "#34d399" :
                    diag.lastDetectionConfidence >= 0.68 ? "#fbbf24" : "#f87171"
                  }
                />
              )}
              {diag.lastDetectionBucket && (
                <MetricRow label="Bucket" value={diag.lastDetectionBucket} colour="#94a3b8" />
              )}
            </>
          )}
        </SectionCard>

        {/* Output adapters */}
        <SectionCard
          title={`Output Adapters (${connectedAdapters}/${adapters.length} online)`}
          icon={<Radio size={14} />}
          accent={connectedAdapters === adapters.length ? "#34d399" : connectedAdapters > 0 ? "#fbbf24" : "#f87171"}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {adapters.map(({ label, state }) => (
              <AdapterPill key={label} label={label} state={state} />
            ))}
          </div>
        </SectionCard>
      </div>

      {/* ── Model catalogue ───────────────────────────────────────────────── */}
      {catalogue.length > 0 && (
        <SectionCard title="Whisper Model Catalogue" icon={<HardDrive size={14} />} accent="#64748b">
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {catalogue.map(e => (
              <div key={e.filename} style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "7px 10px",
                borderRadius: 8,
                background: e.installed
                  ? (e.checksumOk === false ? "rgba(248,113,113,0.06)" : "rgba(52,211,153,0.06)")
                  : "rgba(255,255,255,0.02)",
                border: `1px solid ${
                  e.installed
                    ? (e.checksumOk === false ? "rgba(248,113,113,0.25)" : "rgba(52,211,153,0.2)")
                    : "rgba(255,255,255,0.06)"
                }`,
              }}>
                {/* tier badge */}
                <span style={{
                  padding: "2px 7px",
                  borderRadius: 5,
                  background: `${qualityColour(e.quality)}22`,
                  color: qualityColour(e.quality),
                  fontSize: 10,
                  fontWeight: 700,
                  textTransform: "uppercase" as const,
                  minWidth: 46,
                  textAlign: "center",
                }}>
                  {e.quality}
                </span>

                {/* label */}
                <span style={{ flex: 1, color: "#cbd5e1", fontSize: 12 }}>
                  {e.label}
                </span>

                {/* specs */}
                <span style={{ color: "#475569", fontSize: 11 }}>
                  {e.sizeMb} MB · ~{msFormat(e.expectedLatencyMs)} P95
                </span>

                {/* status */}
                {e.installed ? (
                  e.checksumOk === false
                    ? <span style={{ color: "#f87171", fontSize: 11, display: "flex", alignItems: "center", gap: 3 }}>
                        <ShieldX size={11} /> Tampered
                      </span>
                    : <span style={{ color: "#34d399", fontSize: 11, display: "flex", alignItems: "center", gap: 3 }}>
                        <ShieldCheck size={11} /> Verified
                      </span>
                ) : (
                  <span style={{ color: "#475569", fontSize: 11 }}>Not installed</span>
                )}

                {e.recommended && !e.installed && (
                  <span style={{
                    padding: "1px 6px",
                    borderRadius: 4,
                    background: "rgba(99,102,241,0.15)",
                    color: "#818cf8",
                    fontSize: 10,
                    fontWeight: 600,
                  }}>
                    RECOMMENDED
                  </span>
                )}
              </div>
            ))}
          </div>
        </SectionCard>
      )}

      {/* ── Keyframe styles (injected once) ──────────────────────────────── */}
      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.3; }
        }
        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
      `}</style>
    </div>
  );
}

export default DiagnosticsScreen;
