import type { CSSProperties } from "react";
import { useEffect, useRef } from "react";
import type { AudioLevel } from "../services/desktopApi";

interface Props {
  active: boolean;
  deviceLabel?: string;
  backendLevel?: AudioLevel | null;
  compact?: boolean;
}

export function VuMeter({ active, deviceLabel, backendLevel, compact = false }: Props) {
  const meterRef = useRef<HTMLDivElement | null>(null);
  const valueRef = useRef<HTMLSpanElement | null>(null);

  const writeMeter = (pct: number, peakPct: number, isClipping: boolean, label: string) => {
    const safePct = Number.isFinite(pct) ? Math.max(0, Math.min(100, pct)) : 0;
    const safePeak = Number.isFinite(peakPct) ? Math.max(0, Math.min(100, peakPct)) : 0;
    const root = meterRef.current;
    if (root) {
      root.style.setProperty("--vu-scale", String(active ? safePct / 100 : 0));
      root.style.setProperty("--vu-peak", `${active ? safePeak : 0}%`);
      root.style.setProperty("--vu-clip-opacity", active && safePeak > 0 ? "1" : "0");
      root.style.setProperty(
        "--vu-color",
        isClipping ? "#ff4444" : safePct > 70 ? "#ffbb33" : "#22dd88",
      );
      root.style.setProperty("--vu-text-color", isClipping ? "#ff4444" : "#8fa");
    }
    if (valueRef.current) {
      valueRef.current.textContent = label;
    }
  };

  useEffect(() => {
    const backendFresh = backendLevel ? Date.now() - backendLevel.checkedAtMs < 4_000 : false;
    if (!active) {
      writeMeter(0, 0, false, "Idle");
      return;
    }
    if (!backendFresh) {
      writeMeter(0, 0, false, "Waiting");
      return;
    }

    const displayLevel = backendLevel?.level ?? 0;
    const displayPeak = backendLevel?.peakLevel ?? 0;
    const displayClipping = displayLevel >= 95;
    writeMeter(
      displayLevel,
      displayPeak,
      displayClipping,
      displayClipping ? "CLIP" : backendLevel?.speechDetected ? "Signal" : `${Math.round(displayLevel)}%`,
    );
  }, [active, backendLevel]);

  return (
    <div
      ref={meterRef}
      style={{
        "--vu-scale": "0",
        "--vu-peak": "0%",
        "--vu-color": "#22dd88",
        "--vu-text-color": "#8fa",
        "--vu-clip-opacity": "0",
        display: "flex",
        alignItems: "center",
        gap: "10px",
        padding: compact ? "8px 10px" : "6px 18px",
        background: compact ? "rgba(255,255,255,0.04)" : "rgba(0,0,0,0.28)",
        border: compact ? "1px solid rgba(255,255,255,0.08)" : undefined,
        borderRadius: compact ? 6 : undefined,
        fontSize: "12px",
        color: "#8fa",
        userSelect: "none",
      } as CSSProperties}
      aria-label="Microphone level indicator"
    >
      <span style={{ whiteSpace: "nowrap", minWidth: compact ? 48 : 80 }}>
        Mic · {deviceLabel ?? "Default"}
      </span>

      <div
        style={{
          position: "relative",
          flex: 1,
          height: 8,
          borderRadius: 4,
          background: "rgba(255,255,255,0.1)",
          overflow: "visible",
          maxWidth: compact ? 180 : 360,
        }}
      >
        <div
          style={{
            position: "absolute",
            left: 0,
            top: 0,
            height: "100%",
            width: "100%",
            background: "var(--vu-color)",
            borderRadius: 4,
            transform: "scaleX(var(--vu-scale))",
            transformOrigin: "left center",
            transition: "transform 80ms linear, background 120ms",
          }}
        />
        <div
          style={{
            position: "absolute",
            top: -2,
            left: "var(--vu-peak)",
            width: 2,
            height: 12,
            background: "var(--vu-color)",
            borderRadius: 1,
            transition: "left 80ms",
            opacity: "var(--vu-clip-opacity)",
          }}
        />
      </div>

      <span
        ref={valueRef}
        style={{
          minWidth: compact ? 44 : 54,
          textAlign: "right",
          color: "var(--vu-text-color)",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {active ? "Waiting" : "Idle"}
      </span>
    </div>
  );
}
