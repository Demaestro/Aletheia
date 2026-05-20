import { Activity, AudioLines, BrainCircuit, Clock3, Languages, Radar, type LucideIcon } from "lucide-react";
import { integrations as defaultIntegrations, serviceStats, transcriptSegments } from "../data/production";
import type { AiDetectionResult, Integration, ScriptureCandidate, Tone, TranscriptSegment } from "../types";
import { ActionButton, CandidateRow, Metric, SectionHeader, StatusPill } from "./Primitives";
import { CaptureControlPanel } from "./CaptureControlPanel";
import type { SttStatus } from "../services/desktopApi";

const fallbackAiDetection: AiDetectionResult = {
  mode: "local-first",
  decisionPolicy: "AI suggestions can enter preview, but live output remains manual.",
  processedSegments: 0,
  candidates: [],
  adapters: [
    {
      id: "local-ai",
      name: "Local AI assist",
      mode: "local",
      state: "degraded",
      detail: "Waiting for transcript analysis.",
      latencyMs: 0
    }
  ],
  languages: [],
  supportedLanguages: [],
  accuracyTarget: {
    targetPrecision: 95,
    targetRecall: 90,
    autoPreviewThreshold: 95,
    validatedPrecision: 0,
    validatedRecall: 0,
    validationSampleCount: 0,
    liveRequiresOperator: true,
    strategy: []
  },
  checkedAtMs: Date.now()
};

export function OperatorDashboard({
  candidate,
  transcript = transcriptSegments,
  integrations = defaultIntegrations,
  aiDetection = fallbackAiDetection,
  onPreview,
  onLive,
  onAnalyze,
  sttStatus,
  audioDevices,
  selectedAudioDevice,
  captureRunning,
  captureNotice,
  onSelectDevice,
  onStartCapture,
  onStopCapture,
  onReloadModel,
  onRefreshDevices,
}: {
  candidate: ScriptureCandidate;
  transcript?: TranscriptSegment[];
  integrations?: Integration[];
  aiDetection?: AiDetectionResult;
  onPreview: () => void;
  onLive: () => void;
  onAnalyze?: () => void;
  sttStatus?: SttStatus | null;
  audioDevices?: string[];
  selectedAudioDevice?: string | undefined;
  captureRunning?: boolean;
  captureNotice?: string | null;
  onSelectDevice?: (device: string | undefined) => void;
  onStartCapture?: () => void;
  onStopCapture?: () => void;
  onReloadModel?: () => void;
  onRefreshDevices?: () => void;
}) {
  const latestLatency = transcript[0]?.latencyMs ?? 0;
  const bestAiCandidate = aiDetection.candidates[0];

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Operator dashboard"
        title="Current service state"
        detail="The dashboard keeps the next safe action visible: listen, verify, preview, then send live."
        action={
          <div className="flex flex-wrap justify-end gap-2">
            <ActionButton tone="secondary" onClick={onAnalyze}>
              Run AI assist
            </ActionButton>
            <ActionButton onClick={onPreview}>Approve to preview</ActionButton>
          </div>
        }
      />

      {/* ── Capture control (visible up top so the operator never wonders why
          no transcript is appearing) ── */}
      {onStartCapture && onStopCapture && onReloadModel && onSelectDevice && onRefreshDevices ? (
        <CaptureControlPanel
          sttStatus={sttStatus ?? null}
          audioDevices={audioDevices ?? []}
          selectedAudioDevice={selectedAudioDevice}
          captureRunning={captureRunning ?? false}
          notice={captureNotice ?? null}
          onSelectDevice={onSelectDevice}
          onStartCapture={onStartCapture}
          onStopCapture={onStopCapture}
          onReloadModel={onReloadModel}
          onRefreshDevices={onRefreshDevices}
        />
      ) : null}

      {/* ── Service metrics ── */}
      <div className="grid gap-4 grid-cols-2 lg:grid-cols-4">
        {serviceStats.map((stat) => (
          <Metric key={stat.label} {...stat} />
        ))}
      </div>

      {/* ── Candidate + source map ── */}
      <div className="grid gap-5 xl:grid-cols-[1fr_360px]">
        <CandidateRow candidate={candidate} active onPreview={onPreview} onLive={onLive} />

        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <p className="text-sm font-semibold text-ink">Source map</p>
          <div className="mt-4 space-y-3">
            <StatusLine icon={AudioLines} label="Pulpit mic" detail="Active — see VU meter on Transcript screen" tone="healthy" />
            <StatusLine icon={Languages} label="Languages" detail="English, Hausa, Yoruba, Twi, Swahili, Xhosa, Spanish, French" tone="healthy" />
            <StatusLine icon={Clock3} label="STT latency" detail={`${latestLatency} ms last segment`} tone="healthy" />
            <StatusLine icon={Activity} label="Cloud enhance" detail="Skipped in low bandwidth mode" tone="degraded" />
          </div>
        </div>
      </div>

      {/* ── AI assist + decision ── */}
      <div className="grid gap-5 xl:grid-cols-[1fr_360px]">
        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-ink">AI scripture assist</p>
              <p className="mt-2 text-sm leading-6 text-muted">{aiDetection.decisionPolicy}</p>
            </div>
            <span className="flex-none"><StatusPill tone="neutral" label={aiDetection.mode} /></span>
          </div>
          <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {aiDetection.adapters.map((adapter) => (
              <div key={adapter.id} className="rounded-[6px] border border-line bg-mist p-3">
                <div className="flex items-center justify-between gap-3">
                  <p className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">{adapter.mode}</p>
                  <StatusPill tone={adapterTone(adapter.state)} label={adapter.state} />
                </div>
                <p className="mt-3 text-sm font-semibold text-ink">{adapter.name}</p>
                <p className="mt-2 text-xs leading-5 text-muted">{adapter.detail}</p>
                <p className="mt-3 font-mono text-xs text-graphite">{adapter.latencyMs} ms</p>
              </div>
            ))}
          </div>
        </div>

        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <p className="text-sm font-semibold text-ink">AI decision</p>
          <div className="mt-4 space-y-3">
            <StatusLine
              icon={BrainCircuit}
              label="Processed transcript"
              detail={`${aiDetection.processedSegments} segment${aiDetection.processedSegments === 1 ? "" : "s"} analyzed locally`}
              tone={aiDetection.processedSegments > 0 ? "healthy" : "degraded"}
            />
            <StatusLine
              icon={Radar}
              label="Best candidate"
              detail={bestAiCandidate ? `${bestAiCandidate.reference}, ${bestAiCandidate.confidence}% confidence` : "No scripture candidate yet"}
              tone={bestAiCandidate ? "healthy" : "degraded"}
            />
          </div>
        </div>
      </div>

      {/* ── Multilingual routing + detected languages ── */}
      <div className="grid gap-5 xl:grid-cols-[1fr_360px]">
        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-ink">Multilingual routing</p>
              <p className="mt-2 text-sm leading-6 text-muted">
                Language detection runs before scripture matching so STT, aliases, and confidence thresholds can be tuned per language.
              </p>
            </div>
            <span className="flex-none"><StatusPill tone="healthy" label={`${aiDetection.supportedLanguages.length} packs`} /></span>
          </div>
          <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {aiDetection.supportedLanguages.slice(0, 9).map((language) => (
              <div key={language.code} className="rounded-[6px] border border-line bg-mist p-3">
                <div className="flex items-center justify-between gap-3">
                  <p className="text-sm font-semibold text-ink">{language.name}</p>
                  <StatusPill tone={language.scriptureAliasesReady ? "healthy" : "degraded"} label={language.sttLocale} />
                </div>
                <p className="mt-2 text-xs leading-5 text-muted">
                  Aliases {language.scriptureAliasesReady ? "ready" : "pending"} · Offline STT {language.offlineSttReady ? "ready" : "queued"}
                </p>
              </div>
            ))}
          </div>
        </div>

        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <p className="text-sm font-semibold text-ink">Detected languages</p>
          <div className="mt-4 space-y-3">
            {aiDetection.languages.length ? (
              aiDetection.languages.slice(0, 5).map((language) => (
                <StatusLine
                  key={language.code}
                  icon={Languages}
                  label={language.name}
                  detail={`${language.confidence}% confidence · ${language.matchedTerms.slice(0, 3).join(", ") || "declared language"}`}
                  tone={language.confidence >= 70 ? "healthy" : "degraded"}
                />
              ))
            ) : (
              <p className="text-sm leading-6 text-muted">Run AI assist to see detected languages.</p>
            )}
          </div>
        </div>
      </div>

      {/* ── Accuracy target ── */}
      <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-sm font-semibold text-ink">Accuracy target</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Target {aiDetection.accuracyTarget.targetPrecision}% precision before auto-preview. Fixture validation is currently{" "}
              {aiDetection.accuracyTarget.validatedPrecision}% precision and {aiDetection.accuracyTarget.validatedRecall}% recall across{" "}
              {aiDetection.accuracyTarget.validationSampleCount} samples. Live remains manual.
            </p>
          </div>
          <span className="flex-none"><StatusPill tone="healthy" label={`${aiDetection.accuracyTarget.autoPreviewThreshold}% preview threshold`} /></span>
        </div>
        <div className="mt-4 grid gap-3 sm:grid-cols-2">
          {aiDetection.accuracyTarget.strategy.slice(0, 4).map((item) => (
            <p key={item} className="rounded-[6px] border border-line bg-mist px-3 py-2 text-xs leading-5 text-muted">
              {item}
            </p>
          ))}
        </div>
      </div>

      {/* ── Latest transcript + destination readiness ── */}
      <div className="grid gap-5 xl:grid-cols-2">
        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <p className="text-sm font-semibold text-ink">Latest transcript</p>
          <div className="mt-4 space-y-3">
            {transcript.slice(0, 3).map((segment) => (
              <button
                key={segment.id}
                type="button"
                className="w-full rounded-[6px] border border-transparent p-3 text-left transition hover:border-accent/40 hover:bg-mist focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
              >
                <span className="font-mono text-xs text-muted">{segment.time}</span>
                <span className="ml-3 text-xs font-semibold text-graphite">{segment.speaker}</span>
                <p className="mt-2 text-sm leading-6 text-graphite">{segment.text}</p>
              </button>
            ))}
          </div>
        </div>

        <div className="overflow-hidden rounded-[8px] border border-line bg-paper p-4">
          <p className="text-sm font-semibold text-ink">Destination readiness</p>
          <div className="mt-4 space-y-3">
            {integrations.slice(0, 4).map((integration) => (
              <div key={integration.id} className="flex items-center justify-between gap-3 border-b border-line pb-3 last:border-0 last:pb-0">
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-ink">{integration.name}</p>
                  <p className="mt-1 text-xs text-muted">{integration.detail}</p>
                </div>
                <StatusPill
                  tone={integration.state === "degraded" ? "degraded" : integration.state === "offline" ? "offline" : "healthy"}
                  label={integration.state}
                />
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

function StatusLine({
  icon: Icon,
  label,
  detail,
  tone
}: {
  icon: LucideIcon;
  label: string;
  detail: string;
  tone: "healthy" | "degraded";
}) {
  return (
    <div className="flex items-start gap-3">
      <Icon className="mt-0.5 h-4 w-4 shrink-0 text-muted" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-3">
          <p className="text-sm font-semibold text-ink">{label}</p>
          <StatusPill tone={tone} label={tone} />
        </div>
        <p className="mt-1 text-xs text-muted">{detail}</p>
      </div>
    </div>
  );
}

function adapterTone(state: AiDetectionResult["adapters"][number]["state"]): Tone {
  if (state === "ready" || state === "healthy") return "healthy";
  if (state === "offline") return "offline";
  return "degraded";
}
