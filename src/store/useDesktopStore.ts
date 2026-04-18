import { create } from "zustand";
import type { 
  ScriptureCandidate, 
  TranscriptSegment, 
  DesktopRuntimeStatus, 
  AiDetectionResult 
} from "../types";
import { scriptureCandidates, transcriptSegments } from "../data/production";

const initialRuntimeStatus: DesktopRuntimeStatus = {
  mode: "browser-fallback",
  serviceSession: "Loading local core",
  databasePath: "Checking desktop store",
  dataMiserEnabled: true,
  offlineModeEnabled: true,
  destinationsArmed: true,
  auditCount: 0,
  lastEventSequence: 0,
  checkedAtMs: Date.now()
};

const initialAiDetection: AiDetectionResult = {
  mode: "local-first",
  decisionPolicy: "AI assist has not run yet.",
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

interface DesktopState {
  candidates: ScriptureCandidate[];
  transcript: TranscriptSegment[];
  previewCandidate: ScriptureCandidate | null;
  liveCandidate: ScriptureCandidate | null;
  selectedCandidate: ScriptureCandidate | null;
  desktopStatus: DesktopRuntimeStatus | null;
  aiDetection: AiDetectionResult | null;
  destinationsArmed: boolean;

  // Actions
  setCandidates: (candidates: ScriptureCandidate[]) => void;
  mergeCandidates: (newCandidates: ScriptureCandidate[]) => void;
  removeCandidate: (id: string) => void;
  setTranscript: (transcript: TranscriptSegment[]) => void;
  addTranscriptSegment: (segment: TranscriptSegment) => void;
  setPreviewCandidate: (candidate: ScriptureCandidate | null) => void;
  setLiveCandidate: (candidate: ScriptureCandidate | null) => void;
  setSelectedCandidate: (candidate: ScriptureCandidate | null) => void;
  setDesktopStatus: (status: DesktopRuntimeStatus) => void;
  setAiDetection: (detection: AiDetectionResult) => void;
  setDestinationsArmed: (armed: boolean) => void;
}

export const useDesktopStore = create<DesktopState>((set, get) => ({
  candidates: scriptureCandidates,
  transcript: transcriptSegments,
  previewCandidate: scriptureCandidates[0] || null,
  liveCandidate: scriptureCandidates[2] || null,
  selectedCandidate: scriptureCandidates[0] || null,
  desktopStatus: initialRuntimeStatus,
  aiDetection: initialAiDetection,
  destinationsArmed: true,

  setCandidates: (candidates) => set({ candidates }),
  
  mergeCandidates: (newCandidates) => set((state) => {
    const byReference = new Map<string, ScriptureCandidate>();

    for (const candidate of state.candidates) {
      byReference.set(candidate.reference, candidate);
    }

    for (const candidate of newCandidates) {
      const existing = byReference.get(candidate.reference);
      if (!existing || candidate.confidence >= existing.confidence) {
        byReference.set(candidate.reference, candidate);
      }
    }

    const merged = [...byReference.values()].sort((left, right) => right.confidence - left.confidence);
    return { candidates: merged };
  }),

  removeCandidate: (id) => set((state) => ({
    candidates: state.candidates.filter(c => c.id !== id)
  })),

  setTranscript: (transcript) => set({ transcript }),

  addTranscriptSegment: (segment) => set((state) => {
    const next = [segment, ...state.transcript.filter((s) => s.id !== segment.id)];
    return { transcript: next.slice(0, 50) };
  }),

  setPreviewCandidate: (previewCandidate) => set({ previewCandidate }),
  setLiveCandidate: (liveCandidate) => set({ liveCandidate }),
  setSelectedCandidate: (selectedCandidate) => set({ selectedCandidate }),
  
  setDesktopStatus: (status) => set(s => ({ 
    desktopStatus: { ...(s.desktopStatus || {}), ...status } as DesktopRuntimeStatus,
    destinationsArmed: status.destinationsArmed !== undefined ? status.destinationsArmed : s.destinationsArmed
  })),

  setAiDetection: (aiDetection) => set({ aiDetection }),
  setDestinationsArmed: (destinationsArmed) => set({ destinationsArmed })
}));
