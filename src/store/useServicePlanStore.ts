import { create } from "zustand";
import type { ServicePlan, ServicePlanItem } from "../types";
import { clearDualPersisted, loadDualPersisted, loadLocalSync, saveDualPersisted } from "./persistence";

const STORAGE_KEY = "aletheia.servicePlan.v1";

type ServicePlanState = {
  plan: ServicePlan | null;
  activeItemId: string | null;
  setPlan: (plan: ServicePlan | null) => void;
  setActiveItem: (id: string | null) => void;
  advance: () => void;
  loadFromJson: (raw: string) => ServicePlan;
  clear: () => void;
};

function loadInitial(): ServicePlan | null {
  // Synchronous read from localStorage so the UI hydrates instantly.
  // The async Rust mirror is reconciled by `hydrateServicePlanFromKv()`
  // (called once on app boot below) which overrides this if Rust has a
  // newer record from a prior install.
  const raw = loadLocalSync<unknown>(STORAGE_KEY);
  if (!raw) return null;
  try {
    return validatePlan(raw);
  } catch {
    return null;
  }
}

function persist(plan: ServicePlan | null) {
  if (plan) saveDualPersisted(STORAGE_KEY, plan);
  else clearDualPersisted(STORAGE_KEY);
}

/** One-shot reconciler: pull from Rust KV and overwrite the store if found.
 *  Idempotent. Called from App.tsx during boot. */
export async function hydrateServicePlanFromKv(): Promise<void> {
  try {
    const raw = await loadDualPersisted<unknown>(STORAGE_KEY);
    if (!raw) return;
    const plan = validatePlan(raw);
    useServicePlanStore.setState({
      plan,
      activeItemId: plan.items[0]?.id ?? null,
    });
  } catch {
    /* keep whatever the synchronous load gave us */
  }
}

function validatePlan(raw: unknown): ServicePlan {
  if (!raw || typeof raw !== "object") throw new Error("Plan is not an object.");
  const obj = raw as Record<string, unknown>;
  const items = Array.isArray(obj.items) ? obj.items : [];
  const normalizedItems: ServicePlanItem[] = items.map((it, idx) => {
    const item = (it ?? {}) as Record<string, unknown>;
    return {
      id: typeof item.id === "string" ? item.id : `item-${idx}`,
      kind: (typeof item.kind === "string" ? item.kind : "other") as ServicePlanItem["kind"],
      title: typeof item.title === "string" ? item.title : `Item ${idx + 1}`,
      reference: typeof item.reference === "string" ? item.reference : undefined,
      durationSec: typeof item.durationSec === "number" && item.durationSec > 0 ? item.durationSec : 180,
      note: typeof item.note === "string" ? item.note : undefined
    };
  });
  return {
    id: typeof obj.id === "string" ? obj.id : `plan-${Date.now()}`,
    name: typeof obj.name === "string" ? obj.name : "Imported Service Plan",
    startsAt: typeof obj.startsAt === "string" ? obj.startsAt : new Date().toISOString(),
    source: (typeof obj.source === "string" ? obj.source : "file") as ServicePlan["source"],
    items: normalizedItems,
    importedAtMs: Date.now()
  };
}

export const useServicePlanStore = create<ServicePlanState>((set, get) => ({
  plan: loadInitial(),
  activeItemId: null,
  setPlan: (plan) => {
    persist(plan);
    set({ plan, activeItemId: plan?.items[0]?.id ?? null });
  },
  setActiveItem: (id) => set({ activeItemId: id }),
  advance: () => {
    const { plan, activeItemId } = get();
    if (!plan) return;
    const idx = plan.items.findIndex((it) => it.id === activeItemId);
    const next = plan.items[idx + 1] ?? plan.items[0];
    set({ activeItemId: next?.id ?? null });
  },
  loadFromJson: (raw) => {
    const parsed = JSON.parse(raw);
    const plan = validatePlan(parsed);
    persist(plan);
    set({ plan, activeItemId: plan.items[0]?.id ?? null });
    return plan;
  },
  clear: () => {
    persist(null);
    set({ plan: null, activeItemId: null });
  }
}));
