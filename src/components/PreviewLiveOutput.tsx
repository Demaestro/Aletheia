import { Circle, Monitor, RadioTower, Square, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { integrations as defaultIntegrations } from "../data/production";
import type { Integration, ScriptureCandidate } from "../types";
import { ActionButton, OutputCanvas, SectionHeader, StatusPill } from "./Primitives";

export function PreviewLiveOutput({
  preview,
  live,
  armed,
  integrations = defaultIntegrations,
  onSendLive,
  onToggleArmed,
  onClearLive,
  onBlackout,
  onStageDisplay,
  onLowerThird
}: {
  preview: ScriptureCandidate;
  live: ScriptureCandidate;
  armed: boolean;
  integrations?: Integration[];
  onSendLive: () => void;
  onToggleArmed: () => void;
  onClearLive?: () => void;
  onBlackout?: () => void;
  onStageDisplay?: () => void;
  onLowerThird?: () => void;
}) {
  const { t } = useTranslation();

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow={t("output.eyebrow", { defaultValue: "Presentation output" })}
        title={t("output.title", { defaultValue: "Preview is safe. Live is explicit." })}
        detail={t("output.detail", {
          defaultValue:
            "The output surface keeps preview and live separated so volunteers can rehearse without changing projectors or streams.",
        })}
        action={
          <StatusPill
            tone={armed ? "armed" : "neutral"}
            label={
              armed
                ? t("output.destinationsArmed", { defaultValue: "Destinations armed" })
                : t("output.safeHold", { defaultValue: "Safe hold" })
            }
          />
        }
      />

      <div className="grid gap-5 xl:grid-cols-2">
        <OutputCanvas label="Preview canvas" candidate={preview} state="preview" />
        <OutputCanvas label="Live canvas" candidate={live} state="live" />
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_360px]">
        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <p className="text-sm font-semibold text-ink">
            {t("output.controls", { defaultValue: "Output controls" })}
          </p>
          <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <ControlButton icon={Monitor} label={t("output.stageDisplay", { defaultValue: "Stage display" })} detail={t("output.mirrorPreview", { defaultValue: "Mirror preview" })} onClick={onStageDisplay} />
            <ControlButton icon={RadioTower} label={t("output.lowerThird", { defaultValue: "Lower third" })} detail={t("output.ndiUnavailable", { defaultValue: "NDI not available" })} disabled title="NDI output requires the NDI adapter add-on (not installed)" />
            <ControlButton icon={Square} label={t("output.clearLive", { defaultValue: "Clear live" })} detail={t("output.removeScripture", { defaultValue: "Remove scripture" })} danger onClick={onClearLive} />
            <ControlButton icon={Circle} label={t("output.blackOutput", { defaultValue: "Black output" })} detail={t("output.safetyBlackout", { defaultValue: "Safety blackout" })} danger onClick={onBlackout} />
          </div>
          <div className="mt-5 flex flex-wrap gap-2 border-t border-white/5 pt-5">
            <ActionButton tone="secondary" onClick={onToggleArmed}>
              {armed
                ? t("output.holdDestinations", { defaultValue: "Hold destinations" })
                : t("output.armDestinations", { defaultValue: "Arm destinations" })}
            </ActionButton>
            <ActionButton onClick={onSendLive}>
              {armed
                ? t("output.sendPreviewLive", { defaultValue: "Send preview live" })
                : t("output.armAndSend", { defaultValue: "Arm and send live" })}
            </ActionButton>
          </div>
        </div>

        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <p className="text-sm font-semibold text-ink">
            {t("output.destinationStatus", { defaultValue: "Destination status" })}
          </p>
          <div className="mt-4 space-y-3">
            {integrations.slice(0, 5).map((integration) => (
              <div key={integration.id} className="flex items-start justify-between gap-3 border-b border-white/5 pb-3 last:border-0 last:pb-0">
                <div>
                  <p className="text-sm font-semibold text-ink">{integration.name}</p>
                  <p className="mt-1 text-xs text-muted">{integration.capability}</p>
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

function ControlButton({
  icon: Icon,
  label,
  detail,
  danger,
  disabled,
  title,
  onClick
}: {
  icon: LucideIcon;
  label: string;
  detail: string;
  danger?: boolean;
  disabled?: boolean;
  title?: string;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className="rounded-[6px] border border-white/5 bg-white/5 p-4 text-left transition hover:border-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-40"
    >
      <Icon className={danger ? "h-5 w-5 text-danger" : "h-5 w-5 text-accent"} aria-hidden="true" />
      <p className="mt-3 text-sm font-semibold text-ink">{label}</p>
      <p className="mt-1 text-xs text-muted">{detail}</p>
    </button>
  );
}
