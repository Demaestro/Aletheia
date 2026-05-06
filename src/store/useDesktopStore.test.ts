/**
 * Unit tests for useDesktopStore.mergeCandidates()
 *
 * The merge algorithm is:
 *   - De-duplicates by `reference` string.
 *   - When the same reference appears in both existing and incoming,
 *     keeps the one with the higher confidence value.
 *   - Returns candidates sorted by descending confidence.
 */

import { describe, it, expect, beforeEach } from "vitest";
import type { ScriptureCandidate } from "../types";
import { useDesktopStore } from "./useDesktopStore";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeCandidate(overrides: Partial<ScriptureCandidate> & { reference: string; confidence: number }): ScriptureCandidate {
  return {
    id: overrides.reference.replace(/\s+/g, "-").toLowerCase(),
    translation: "KJV",
    language: "en",
    text: "For God so loved the world",
    source: "stt-detection",
    reason: "keyword match",
    status: "new",
    ...overrides
  };
}

// Reset store state before every test so tests don't bleed into each other.
beforeEach(() => {
  useDesktopStore.setState({
    candidates: [],
  });
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("mergeCandidates", () => {
  it("adds new candidates to an empty store", () => {
    const incoming = [
      makeCandidate({ reference: "John 3:16", confidence: 94 }),
      makeCandidate({ reference: "Romans 8:28", confidence: 87 }),
    ];

    useDesktopStore.getState().mergeCandidates(incoming);
    const { candidates } = useDesktopStore.getState();

    expect(candidates).toHaveLength(2);
    expect(candidates.map((c: ScriptureCandidate) => c.reference)).toEqual(["John 3:16", "Romans 8:28"]);
  });

  it("returns candidates sorted by confidence (descending)", () => {
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "Psalm 23:1", confidence: 60 }),
      makeCandidate({ reference: "Isaiah 40:31", confidence: 95 }),
      makeCandidate({ reference: "Jeremiah 29:11", confidence: 78 }),
    ]);

    const confidences = useDesktopStore.getState().candidates.map((c: ScriptureCandidate) => c.confidence);
    expect(confidences).toEqual([95, 78, 60]);
  });

  it("does not create duplicate references", () => {
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 80 }),
    ]);
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 90 }),
    ]);

    const { candidates } = useDesktopStore.getState();
    expect(candidates).toHaveLength(1);
    expect(candidates[0].reference).toBe("John 3:16");
  });

  it("keeps the higher-confidence candidate when the same reference is seen twice", () => {
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 70 }),
    ]);
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 92 }),
    ]);

    expect(useDesktopStore.getState().candidates[0].confidence).toBe(92);
  });

  it("does NOT replace an existing candidate when the incoming confidence is lower", () => {
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 90 }),
    ]);
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 55 }),
    ]);

    expect(useDesktopStore.getState().candidates[0].confidence).toBe(90);
  });

  it("treats candidates with equal confidence as an update (keeps incoming)", () => {
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 80, text: "old text" }),
    ]);
    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 80, text: "new text" }),
    ]);

    // confidence >=, so the incoming replaces the existing
    expect(useDesktopStore.getState().candidates[0].text).toBe("new text");
  });

  it("merges multiple new and duplicate references in one call", () => {
    useDesktopStore.setState({
      candidates: [
        makeCandidate({ reference: "John 3:16", confidence: 80 }),
        makeCandidate({ reference: "Romans 8:28", confidence: 75 }),
      ],
    });

    useDesktopStore.getState().mergeCandidates([
      makeCandidate({ reference: "John 3:16", confidence: 95 }),     // upgrade
      makeCandidate({ reference: "Psalm 23:1", confidence: 88 }),    // new
      makeCandidate({ reference: "Romans 8:28", confidence: 60 }),   // ignored (lower)
    ]);

    const { candidates } = useDesktopStore.getState();
    expect(candidates).toHaveLength(3);

    const refs = candidates.map((c: ScriptureCandidate) => c.reference);
    expect(refs).toContain("John 3:16");
    expect(refs).toContain("Romans 8:28");
    expect(refs).toContain("Psalm 23:1");

    const johnConf = candidates.find((c: ScriptureCandidate) => c.reference === "John 3:16")?.confidence;
    const romConf = candidates.find((c: ScriptureCandidate) => c.reference === "Romans 8:28")?.confidence;
    expect(johnConf).toBe(95);
    expect(romConf).toBe(75); // should not have been reduced
  });

  it("handles an empty incoming array without mutating the store", () => {
    useDesktopStore.setState({
      candidates: [makeCandidate({ reference: "John 3:16", confidence: 80 })],
    });

    useDesktopStore.getState().mergeCandidates([]);
    expect(useDesktopStore.getState().candidates).toHaveLength(1);
  });

  it("handles merging when the existing store is already at zero", () => {
    useDesktopStore.getState().mergeCandidates([]);
    expect(useDesktopStore.getState().candidates).toHaveLength(0);
  });
});
