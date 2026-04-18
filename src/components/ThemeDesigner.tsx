import { CheckCircle2, Contrast, Languages, Type, type LucideIcon } from "lucide-react";
import { themes } from "../data/production";
import type { ScriptureCandidate, ThemePreset } from "../types";
import { ActionButton, OutputCanvas, SectionHeader, StatusPill, cn } from "./Primitives";

export function ThemeDesigner({
  selectedTheme,
  onSelectTheme,
  preview,
  onPublish
}: {
  selectedTheme: ThemePreset;
  onSelectTheme: (theme: ThemePreset) => void;
  preview: ScriptureCandidate;
  onPublish?: (theme: ThemePreset) => void;
}) {
  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Theme designer"
        title="Readable scripture styles with guardrails"
        detail="Theme controls validate safe area, minimum text size, contrast, and multilingual fallback before a style can be published."
        action={<ActionButton onClick={() => onPublish?.(selectedTheme)}>Publish theme</ActionButton>}
      />

      <div className="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)_420px]">
        <div className="space-y-3">
          {themes.map((theme) => (
            <button
              key={theme.id}
              type="button"
              onClick={() => onSelectTheme(theme)}
              className={cn(
                "w-full rounded-[6px] border p-4 text-left transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent",
                selectedTheme.id === theme.id ? "border-accent bg-accent/10" : "border-white/5 bg-white/5 hover:border-accent/50"
              )}
            >
              <p className="text-sm font-semibold text-ink">{theme.name}</p>
              <p className="mt-1 text-xs text-muted">{theme.mode}</p>
              <div className="mt-3 flex flex-wrap gap-2">
                <StatusPill tone="healthy" label={theme.contrast} />
                <StatusPill tone="neutral" label={theme.fontScale} />
              </div>
            </button>
          ))}
        </div>

        <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
          <p className="text-sm font-semibold text-ink">Theme controls</p>
          <div className="mt-5 grid gap-4">
            <Control icon={Type} label="Scripture type scale" value={selectedTheme.fontScale} detail="Blocks values below 36 px for broadcast lower thirds." />
            <Control icon={Contrast} label="Contrast validation" value={selectedTheme.contrast} detail="Live output requires AA contrast or better." />
            <Control icon={Languages} label="Language fallback" value={selectedTheme.languages.join(", ")} detail="Fallback fonts checked before publish." />
            <Control icon={CheckCircle2} label="Safe area" value="92% width, 84% height" detail="Prevents cropped text on projectors and livestream frames." />
          </div>
          <div className="mt-6 border-t border-white/5 pt-5">
            <p className="text-sm font-semibold text-ink">Operator copy</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Theme changes affect Preview first. Live output keeps the previous published theme until the operator confirms.
            </p>
          </div>
        </div>

        <OutputCanvas label={selectedTheme.name} candidate={preview} state="preview" />
      </div>
    </section>
  );
}

function Control({
  icon: Icon,
  label,
  value,
  detail
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="rounded-[6px] border border-white/5 bg-mist p-4">
      <div className="flex items-start gap-3">
        <Icon className="mt-0.5 h-5 w-5 text-accent" aria-hidden="true" />
        <div>
          <p className="text-sm font-semibold text-ink">{label}</p>
          <p className="mt-1 text-sm text-graphite">{value}</p>
          <p className="mt-2 text-xs leading-5 text-muted">{detail}</p>
        </div>
      </div>
    </div>
  );
}
