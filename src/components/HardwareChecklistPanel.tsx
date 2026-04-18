import { useState } from "react";
import { hardwareChecklist } from "../data/production";
import type { HardwareChecklistItem } from "../types";
import { ActionButton, StatusPill } from "./Primitives";

type ChecklistState = Record<string, boolean>;

export function HardwareChecklistPanel({ items = hardwareChecklist }: { items?: HardwareChecklistItem[] }) {
  const [checked, setChecked] = useState<ChecklistState>(() =>
    items.reduce((acc, item) => {
      acc[item.id] = false;
      return acc;
    }, {} as ChecklistState)
  );

  const requiredCount = items.filter((item) => item.required).length;
  const requiredChecked = items.filter((item) => item.required && checked[item.id]).length;
  const allRequiredReady = requiredCount > 0 && requiredCount === requiredChecked;

  return (
    <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">Hardware checklist</p>
          <p className="mt-2 text-sm leading-6 text-muted">
            Confirm the physical chain is rehearsed before arming Live. Required devices must be checked.
          </p>
        </div>
        <StatusPill tone={allRequiredReady ? "healthy" : "degraded"} label={allRequiredReady ? "ready" : "pending"} />
      </div>

      <div className="mt-4 space-y-3">
        {items.map((item) => (
          <label
            key={item.id}
            className="flex items-start justify-between gap-3 rounded-[6px] border border-white/5 bg-paper px-3 py-2"
          >
            <span className="flex items-start gap-3">
              <input
                type="checkbox"
                checked={Boolean(checked[item.id])}
                onChange={(event) => setChecked((current) => ({ ...current, [item.id]: event.target.checked }))}
                className="mt-1 h-4 w-4 accent-accent"
              />
              <span>
                <span className="block text-sm font-semibold text-ink">{item.label}</span>
                <span className="mt-1 block text-xs leading-5 text-muted">{item.detail}</span>
              </span>
            </span>
            <StatusPill tone={item.required ? "healthy" : "neutral"} label={item.required ? "required" : "optional"} />
          </label>
        ))}
      </div>

      <div className="mt-4 flex items-center justify-between gap-3 text-xs text-muted">
        <span>
          Required complete: {requiredChecked}/{requiredCount}
        </span>
        <ActionButton
          tone="secondary"
          onClick={() =>
            setChecked((current) => {
              const next: ChecklistState = { ...current };
              items.forEach((item) => {
                if (item.required) next[item.id] = true;
              });
              return next;
            })
          }
        >
          Mark required as ready
        </ActionButton>
      </div>
    </div>
  );
}
