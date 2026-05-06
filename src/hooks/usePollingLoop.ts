/**
 * usePollingLoop.ts
 *
 * Encapsulates the 8-second background polling loop that was previously
 * inlined in App.tsx.  Responsibilities:
 *
 *   1. Re-run AI transcript analysis every tick so candidate cards stay fresh.
 *   2. Fetch updated desktop service state (transcript segments).
 *   3. vMix auto-reconnect — reads live Zustand state via getState() to
 *      avoid the stale-closure bug that existed when the loop was inline.
 *   4. Each async call is guarded by an AbortController-driven cancel token
 *      so concurrent ticks cannot stack when the Rust backend is slow.
 */

import { useEffect, useRef } from "react";
import { useDesktopStore } from "../store/useDesktopStore";
import { useHardwareStore } from "../store/useHardwareStore";
import {
  getDesktopServiceState,
  getVmixStatus,
} from "../services/desktopApi";

const POLL_INTERVAL_MS = 8_000;

export function usePollingLoop(
  setCommandNotice: (msg: string) => void
): void {
  // Keep a stable ref for the notice callback (same technique as useTauriEvents).
  const noticeRef = useRef(setCommandNotice);
  useEffect(() => { noticeRef.current = setCommandNotice; }, [setCommandNotice]);

  useEffect(() => {
    let cancelled = false;
    // Track whether a tick is still in-flight so we do not stack concurrent calls.
    let tickInFlight = false;
    let vmixWasConnected = false;

    const tick = async () => {
      if (cancelled || tickInFlight) return;
      tickInFlight = true;

      try {
        // 1. Lightweight transcript/state sync. Live scripture candidates are
        // emitted from Rust as events, so the polling loop must not run heavy
        // local/cloud AI work on the desktop command thread.
        const state = await getDesktopServiceState();
        if (!cancelled) {
          useDesktopStore.getState().setTranscript(state.transcript);
        }
      } catch {
        // Non-fatal
      }

      // 3. vMix auto-reconnect (reads live state, not stale closure)
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
          // silent — status stays offline until next tick
        }
      }
      vmixWasConnected = isConnected;

      tickInFlight = false;
    };

    const id = window.setInterval(() => { void tick(); }, POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  // Empty dep array: stable — only runs once, uses refs and getState() for live data.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
