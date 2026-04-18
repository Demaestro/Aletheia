import { CheckCircle2, Circle, MonitorCheck, Volume2 } from "lucide-react";
import { onboardingSteps } from "../data/production";
import { HardwareChecklistPanel } from "./HardwareChecklistPanel";
import { ActionButton, SectionHeader, StatusPill, cn } from "./Primitives";

export function OnboardingFlow({ onContinue }: { onContinue?: () => void }) {
  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Onboarding"
        title="From install to safe rehearsal in under ten minutes"
        detail="The setup flow verifies local libraries, audio, destinations, and output safety before a volunteer touches Live."
        action={<ActionButton onClick={onContinue}>Continue rehearsal</ActionButton>}
      />

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_380px]">
        <div className="rounded-[6px] border border-white/5 bg-white/5">
          {onboardingSteps.map((step, index) => {
            const done = step.status === "Complete";
            const active = step.status === "Next" || step.status === "Needs check";

            return (
              <div key={step.title} className="grid grid-cols-[72px_minmax(0,1fr)] border-b border-white/5 last:border-0">
                <div className="flex justify-center py-5">
                  <div
                    className={cn(
                      "grid h-10 w-10 place-items-center rounded-full border",
                      done ? "border-accent bg-accent text-white" : active ? "border-caution bg-caution/10 text-caution" : "border-white/5 bg-paper text-muted"
                    )}
                  >
                    {done ? <CheckCircle2 className="h-5 w-5" aria-hidden="true" /> : <Circle className="h-4 w-4" aria-hidden="true" />}
                  </div>
                </div>
                <div className="py-5 pr-5">
                  <div className="flex items-center justify-between gap-3">
                    <p className="text-base font-semibold text-ink">{index + 1}. {step.title}</p>
                    <StatusPill tone={done ? "healthy" : active ? "degraded" : "neutral"} label={step.status} />
                  </div>
                  <p className="mt-2 text-sm leading-6 text-muted">{step.detail}</p>
                </div>
              </div>
            );
          })}
        </div>

        <aside className="space-y-4">
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <Volume2 className="h-5 w-5 text-accent" aria-hidden="true" />
            <p className="mt-3 text-sm font-semibold text-ink">Audio test phrase</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Speak: "Please open Romans eight verse twenty eight." The system should detect a reference and send it to Preview only.
            </p>
          </div>
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <MonitorCheck className="h-5 w-5 text-accent" aria-hidden="true" />
            <p className="mt-3 text-sm font-semibold text-ink">Output safety check</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Confirm the HDMI display, OBS scene, and NDI output can be previewed, cleared, and held without changing Live.
            </p>
          </div>
          <div className="rounded-[6px] border border-white/5 bg-mist p-5">
            <p className="text-sm font-semibold text-ink">Save service profile</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Profile name: Sunday AM. Languages: English, Yoruba, Igbo, Hausa, Twi, Swahili, Xhosa, Spanish, French. Output: OBS, NDI, HDMI 2. Policy: manual live.
            </p>
          </div>
          <HardwareChecklistPanel />
        </aside>
      </div>
    </section>
  );
}
