/**
 * useKeyboardShortcuts.ts
 *
 * Extracted from App.tsx — manages all global keyboard shortcuts so the
 * root component does not need to own the keydown listener inline.
 *
 * Shortcuts:
 *   Ctrl/Cmd+K  → navigate to Manual Search screen
 *   Ctrl/Cmd+L  → send preview live (double-press guard: must be pressed
 *                 twice within 800ms to prevent accidental fire)
 *   Alt+→       → navigate to next screen (wraps)
 *   Escape×2    → clear live output (double-press within 600ms)
 */

import { useEffect, useRef } from "react";
import type { ScreenKey, ScriptureCandidate } from "../types";
import { screenOrder } from "../data/production";

interface Deps {
  activeScreen: ScreenKey;
  previewCandidate: ScriptureCandidate | null;
  destinationsArmed: boolean;
  onNavigate: (screen: ScreenKey) => void;
  onSendLive: (candidate?: ScriptureCandidate) => void;
  onClearLive: () => void;
}

/** Elapsed-ms guard: returns true if two presses are within windowMs. */
function useDoublePress(windowMs: number) {
  const lastRef = useRef<number>(0);
  return () => {
    const now = Date.now();
    if (now - lastRef.current <= windowMs) {
      lastRef.current = 0;
      return true;
    }
    lastRef.current = now;
    return false;
  };
}

export function useKeyboardShortcuts(deps: Deps): void {
  const depsRef = useRef(deps);
  useEffect(() => { depsRef.current = deps; }, [deps]);

  // Isolated double-press checkers — stable across renders.
  const checkCtrlLDouble = useDoublePress(800);
  const checkEscDouble   = useDoublePress(600);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Do not intercept typing in inputs/textareas
      const tag = document.activeElement?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
        if (event.key === "Escape") (document.activeElement as HTMLElement).blur();
        return;
      }

      const { activeScreen, previewCandidate, onNavigate, onSendLive, onClearLive } =
        depsRef.current;
      const mod = event.ctrlKey || event.metaKey;

      // Ctrl/Cmd+K → Manual search
      if (mod && event.key.toLowerCase() === "k") {
        event.preventDefault();
        onNavigate("search");
        return;
      }

      // Ctrl/Cmd+L → Send live (requires double-press within 800ms as accidental-fire guard)
      if (mod && event.key.toLowerCase() === "l") {
        event.preventDefault();
        if (checkCtrlLDouble()) {
          if (previewCandidate) onSendLive(previewCandidate);
        }
        // First press is a "prime" — user sees nothing until second press.
        return;
      }

      // Alt+→ → next screen
      if (event.altKey && event.key === "ArrowRight") {
        event.preventDefault();
        const idx = screenOrder.indexOf(activeScreen);
        onNavigate(screenOrder[(idx + 1) % screenOrder.length]);
        return;
      }

      // Escape×2 → clear live
      if (event.key === "Escape") {
        if (checkEscDouble()) {
          onClearLive();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  // Stable deps: the effect registers once; live values are read via depsRef.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
