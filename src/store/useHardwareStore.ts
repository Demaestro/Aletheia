import { create } from "zustand";
import type { 
  VmixStatus, 
  ProductionReadinessReport, 
  HealthItem, 
  Integration,
  IntegrationEvent
} from "../types";
import { healthItems, integrations } from "../data/production";

export type AdapterDispatchResult = {
  adapter: string;
  state: "connected" | "ready" | "degraded" | "offline" | string;
  detail: string;
  reference: string;
  auditCount: number;
};

const initialVmixStatus: VmixStatus = {
  state: "offline",
  detail: "vMix status has not been checked yet.",
  host: "127.0.0.1",
  port: 8088,
  endpoint: "http://127.0.0.1:8088/api/",
  titleInput: "Aletheia Scripture.gtzip",
  verseField: "Headline.Text",
  referenceField: "Description.Text",
  overlayChannel: 2,
  allowPrivateNetwork: false,
  checkedAtMs: Date.now()
};

const initialProductionReadiness: ProductionReadinessReport = {
  generatedAtMs: Date.now(),
  score: 0,
  state: "degraded",
  blockers: ["Production readiness has not been checked yet."],
  secretVault: {
    provider: "OS vault boundary",
    state: "ready",
    detail: "Secret vault status has not been loaded yet.",
    storedSecretCount: 0,
    releaseRequired: true,
    policy: []
  },
  pluginPolicy: {
    state: "blocked",
    detail: "Plugin signing policy has not been loaded yet.",
    trustedKeyCount: 0,
    requiredControls: []
  },
  supportBundle: {
    state: "ready",
    detail: "Support bundle policy has not been loaded yet.",
    includes: [],
    excludes: []
  },
  offlineAssets: {
    state: "degraded",
    installedCount: 0,
    requiredCount: 0,
    assets: []
  },
  acceptanceDevices: [],
  releaseGates: []
};

interface HardwareState {
  healthItems: HealthItem[];
  integrations: Integration[];
  integrationEvents: IntegrationEvent[];
  productionReadiness: ProductionReadinessReport | null;

  vmixStatus: VmixStatus | null;
  obsStatus: AdapterDispatchResult | null;
  proPresenterStatus: AdapterDispatchResult | null;
  companionStatus: AdapterDispatchResult | null;
  oscStatus: AdapterDispatchResult | null;
  easyWorshipStatus: AdapterDispatchResult | null;

  operatorName: string;

  setHealthItems: (items: HealthItem[]) => void;
  setIntegrations: (integrations: Integration[]) => void;
  setIntegrationEvents: (events: IntegrationEvent[]) => void;
  setProductionReadiness: (report: ProductionReadinessReport) => void;

  setVmixStatus: (status: VmixStatus) => void;
  setObsStatus: (status: AdapterDispatchResult | null) => void;
  setProPresenterStatus: (status: AdapterDispatchResult | null) => void;
  setCompanionStatus: (status: AdapterDispatchResult | null) => void;
  setOscStatus: (status: AdapterDispatchResult | null) => void;
  setEasyWorshipStatus: (status: AdapterDispatchResult | null) => void;

  setOperatorName: (name: string) => void;
}

export const useHardwareStore = create<HardwareState>((set) => ({
  healthItems: healthItems,
  integrations: integrations,
  integrationEvents: [],
  productionReadiness: initialProductionReadiness,

  vmixStatus: initialVmixStatus,
  obsStatus: null,
  proPresenterStatus: null,
  companionStatus: null,
  oscStatus: null,
  easyWorshipStatus: null,

  operatorName: "operator:local-booth",

  setHealthItems: (healthItems) => set({ healthItems }),
  setIntegrations: (integrations) => set({ integrations }),
  setIntegrationEvents: (integrationEvents) => set({ integrationEvents }),
  setProductionReadiness: (productionReadiness) => set({ productionReadiness }),

  setVmixStatus: (vmixStatus) => set({ vmixStatus }),
  setObsStatus: (obsStatus) => set({ obsStatus }),
  setProPresenterStatus: (proPresenterStatus) => set({ proPresenterStatus }),
  setCompanionStatus: (companionStatus) => set({ companionStatus }),
  setOscStatus: (oscStatus) => set({ oscStatus }),
  setEasyWorshipStatus: (easyWorshipStatus) => set({ easyWorshipStatus }),

  setOperatorName: (operatorName) => set({ operatorName }),
}));
