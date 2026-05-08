/**
 * usePollingLoop.ts
 *
 * Two-speed background loop:
 *
 *   FAST (1 s)  — Re-run AI transcript analysis so candidate cards are
 *                 always current. Auto-promotes the top candidate to
 *                 previewCandidate when confidence ≥ 75 ("Smooth Operator").
 *
 *   SLOW (8 s)  — Full desktop service state sync + vMix auto-reconnect.
 *
 * Both loops are guarded so concurrent ticks cannot stack when the Rust
 * backend is slow.  The analysis tick fires immediately on mount so the
 * operator gets results without waiting for the first 1-second interval.
 */

import { useEffect, useRef } from "react";
import { useDesktopStore } from "../store/useDesktopStore";
import { useHardwareStore } from "../store/useHardwareStore";
import {
  analyzeTranscript,
  getDesktopServiceState,
  getVmixStatus,
} from "../services/desktopApi";

const FAST_POLL_MS = 1_000;   // AI analysis — real-time detection
const SLOW_POLL_MS = 8_000;   // State sync + vMix reconnect

export function usePollingLoop(
  setCommandNotice: (msg: string) => void
): void {
  const noticeRef = useRef(setCommandNotice);
  useEffect(() => { noticeRef.current = setCommandNotice; }, [setCommandNotice]);

  useEffect(() => {
    let cancelled = false;
    let analysisInFlight = false;
    let syncInFlight = false;
    let vmixWasConnected = false;
    // Track last auto-previewed reference to avoid spamming notice toasts.
    let lastAutoPreviewRef = "";

    // ── FAST: AI transcript analysis ──────────────────────────────────────
    const analysisTick = async () => {
      if (cancelled || analysisInFlight) return;
      analysisInFlight = true;

      try {
        const result = await analyzeTranscript();
        if (cancelled) { analysisInFlight = false; return; }

        const store = useDesktopStore.getState();
        store.setAiDetection(result);
        store.mergeCandidates(result.candidates);

        // Smooth Operator: auto-promote high-confidence candidate to preview.
        // Threshold 75 — matches the architecture vision "prepare as next suggestion".
        const top = result.candidates[0];
        if (top && top.confidence >= 75) {
          const currentPreview = store.previewCandidate;
          const isNewOrBetter =
            !currentPreview ||
            top.reference !== currentPreview.reference ||
            top.confidence > currentPreview.confidence;

          if (isNewOrBetter) {
            store.setPreviewCandidate({ ...top, status: "preview" });
            store.setSelectedCandidate(top);
            if (top.reference !== lastAutoPreviewRef) {
              lastAutoPreviewRef = top.reference;
              noticeRef.current(
                `Preview ready: ${top.reference} — ${top.confidence}% confidence`
              );
            }
          }
        }
      } catch {
        // Non-fatal — keep polling
      }

      analysisInFlight = false;
    };

    // ── SLOW: state sync + vMix reconnect ─────────────────────────────────
    const syncTick = async () => {
      if (cancelled || syncInFlight) return;
      syncInFlight = true;

      try {
        const state = await getDesktopServiceState();
        if (!cancelled) {
          useDesktopStore.getState().setTranscript(state.transcript);
        }
      } catch {
        // Non-fatal
      }

      // vMix auto-reconnect
      const liveVmixState = useHardwareStore.getState().vmixStatus?.state;
      const isConnected = liveVmixState === "connected" || liveVmixState === "ready";
      if (vmixWasConnected && !isConnected && !cancelled) {
        try {
          const status = await getVmixStatus();
          if (!cancelled) {
            useHardwareStore.getState().setVmixStatus(status);
            if (status.state === "connected" || status.state === "ready") {
              noticeRef.current("vMix reconnected automatically.");
            }
          }
        } catch {
          // Silent — stays offline until next tick
        }
      }
      vmixWasConnected = isConnected;

      syncInFlight = false;
    };

    // Fire analysis immediately so the operator doesn't wait 1 s on startup.
    void analysisTick();

    const fastId = window.setInterval(() => { void analysisTick(); }, FAST_POLL_MS);
    const slowId = window.setInterval(() => { void syncTick(); }, SLOW_POLL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(fastId);
      window.clearInterval(slowId);
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
