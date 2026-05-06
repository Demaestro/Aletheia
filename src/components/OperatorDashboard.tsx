import { memo } from "react";
import { useTranslation } from "react-i18next";
import type { HealthItem, ScriptureCandidate } from "../types";
import type { AudioLevel } from "../services/desktopApi";
import type { CaptureState } from "../hooks/useAlwaysOnCommandCapture";
import { DashboardBiblePanel } from "./DashboardBiblePanel";
import { OutputCanvas, StatusPill } from "./Primitives";

export const OperatorDashboard = memo(function OperatorDashboard({
  candidate,
  activeBibleCandidate,
  previewCandidate,
  liveCandidate,
  suggestions = [],
  notice,
  onOpenCommand,
  onPreviewVerse,
  onPreviewSuggestion,
  onLiveSuggestion,
  listenerStatus,
  captureState = "idle",
  captureRunning,
  captureStarting,
  selectedAudioDevice,
  audioLevel,
  healthItems,
  onOpenCaptureDiagnostics,
  onSendLive,
}: {
  candidate: ScriptureCandidate;
  activeBibleCandidate?: ScriptureCandidate | null;
  previewCandidate: ScriptureCandidate;
  liveCandidate: ScriptureCandidate;
  suggestions?: ScriptureCandidate[];
  notice?: string;
  onOpenCommand?: (command: string, translationId?: string) => void;
  onPreviewVerse?: (reference: string, text: string, translation: string) => void;
  onPreviewSuggestion?: (candidate: ScriptureCandidate) => void;
  onLiveSuggestion?: (candidate: ScriptureCandidate) => void;
  listenerStatus?: string;
  captureState?: CaptureState;
  captureRunning?: boolean;
  captureStarting?: boolean;
  selectedAudioDevice?: string;
  audioLevel?: AudioLevel | null;
  healthItems?: HealthItem[];
  onOpenCaptureDiagnostics?: () => void;
  onSendLive?: () => void;
}) {
  const { t } = useTranslation();

  return (
    <section className="grid min-h-[calc(100vh-80px)] gap-4 xl:grid-cols-[minmax(0,1fr)_380px]">
      <div className="min-w-0">
        <DashboardBiblePanel
          activeCandidate={activeBibleCandidate ?? candidate}
          onOpenCommand={onOpenCommand}
          onPreviewVerse={onPreviewVerse}
          listenerStatus={listenerStatus ?? notice}
          captureState={captureState}
          captureRunning={captureRunning}
          captureStarting={captureStarting}
          selectedAudioDevice={selectedAudioDevice}
          audioLevel={audioLevel}
          healthItems={healthItems}
          onOpenCaptureDiagnostics={onOpenCaptureDiagnostics}
        />
      </div>

      <aside className="min-w-0 space-y-4">
        <div className="rounded-[8px] border border-line bg-paper p-3">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted">
                {t("dashboardBible.programMonitor", { defaultValue: "Program monitor" })}
              </p>
              <p className="mt-1 text-sm font-semibold text-ink">
                {t("dashboardBible.previewLive", { defaultValue: "Preview and live output." })}
              </p>
            </div>
            <StatusPill
              tone={liveCandidate?.text ? "live" : "neutral"}
              label={
                liveCandidate?.text
                  ? t("common.live", { defaultValue: "Live" })
                  : t("common.clear", { defaultValue: "Clear" })
              }
            />
          </div>
          <div className="space-y-3">
            <OutputCanvas label={t("common.preview", { defaultValue: "Preview" })} candidate={previewCandidate} state="preview" />
            <OutputCanvas label={t("common.live", { defaultValue: "Live" })} candidate={liveCandidate} state="live" />
            <button
              type="button"
              onClick={onSendLive}
              className="w-full rounded-[6px] border border-accent/45 bg-accent px-3 py-2.5 text-sm font-semibold text-white transition hover:bg-accent/90 focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
            >
              {t("output.sendPreviewLive", { defaultValue: "Send preview live" })}
            </button>
          </div>
        </div>
      </aside>
    </section>
  );
});
