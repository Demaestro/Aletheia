/**
 * VuMeter — real-time microphone level indicator.
 *
 * Uses the Web Audio API (AnalyserNode) to show a live bar that confirms
 * audio is flowing to the STT pipeline without waiting 5+ seconds for the
 * first transcript segment. Only active while capture is running.
 */
import { useEffect, useRef, useState } from "react";

interface Props {
  /** Pass true while start_audio_capture has succeeded. */
  active: boolean;
  deviceLabel?: string;
}

export function VuMeter({ active, deviceLabel }: Props) {
  const [level, setLevel] = useState(0);          // 0-100
  const [peak, setPeak] = useState(0);             // 0-100, sticky peak hold
  const [clipping, setClipping] = useState(false);
  const animRef = useRef<number>(0);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const peakTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!active) {
      setLevel(0);
      setPeak(0);
      setClipping(false);
      return;
    }

    let cancelled = false;

    navigator.mediaDevices
      .getUserMedia({ audio: true, video: false })
      .then((stream) => {
        if (cancelled) { stream.getTracks().forEach((t) => t.stop()); return; }
        streamRef.current = stream;
        const ctx = new AudioContext();
        audioCtxRef.current = ctx;
        const source = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 256;
        analyser.smoothingTimeConstant = 0.6;
        source.connect(analyser);
        analyserRef.current = analyser;

        const data = new Uint8Array(analyser.frequencyBinCount);

        const tick = () => {
          if (cancelled) return;
          analyser.getByteFrequencyData(data);
          // RMS of frequency magnitudes → 0-100
          let sum = 0;
          for (let i = 0; i < data.length; i++) sum += data[i] * data[i];
          const rms = Math.sqrt(sum / data.length);
          const pct = Math.min(100, (rms / 255) * 100 * 2.5); // boost to fill bar
          setLevel(pct);
          setPeak((prev) => {
            if (pct >= prev) {
              if (peakTimerRef.current) clearTimeout(peakTimerRef.current);
              peakTimerRef.current = setTimeout(() => setPeak(0), 1500);
              return pct;
            }
            return prev;
          });
          setClipping(pct >= 95);
          animRef.current = requestAnimationFrame(tick);
        };
        animRef.current = requestAnimationFrame(tick);
      })
      .catch(() => {
        // Permission denied or no mic — meter stays at 0, no crash.
      });

    return () => {
      cancelled = true;
      cancelAnimationFrame(animRef.current);
      streamRef.current?.getTracks().forEach((t) => t.stop());
      audioCtxRef.current?.close().catch(() => undefined);
      analyserRef.current = null;
      streamRef.current = null;
      audioCtxRef.current = null;
    };
  }, [active]);

  if (!active) return null;

  const barColor = clipping
    ? "#ff4444"
    : level > 70
    ? "#ffbb33"
    : "#22dd88";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "10px",
        padding: "6px 18px",
        background: "rgba(0,0,0,0.28)",
        borderBottom: "1px solid rgba(255,255,255,0.06)",
        fontSize: "12px",
        color: "#8fa",
        userSelect: "none",
      }}
      aria-label="Microphone level indicator"
    >
      <span style={{ whiteSpace: "nowrap", minWidth: 80 }}>
        🎙 {deviceLabel ?? "Live Mic"}
      </span>

      {/* bar track */}
      <div
        style={{
          position: "relative",
          flex: 1,
          height: 8,
          borderRadius: 4,
          background: "rgba(255,255,255,0.1)",
          overflow: "visible",
          maxWidth: 360,
        }}
      >
        {/* fill */}
        <div
          style={{
            position: "absolute",
            left: 0,
            top: 0,
            height: "100%",
            width: `${level}%`,
            background: barColor,
            borderRadius: 4,
            transition: "width 60ms linear, background 120ms",
          }}
        />
        {/* peak marker */}
        <div
          style={{
            position: "absolute",
            top: -2,
            left: `${peak}%`,
            width: 2,
            height: 12,
            background: clipping ? "#ff4444" : "#fff",
            borderRadius: 1,
            transition: "left 80ms",
            opacity: peak > 0 ? 1 : 0,
          }}
        />
      </div>

      <span
        style={{
          minWidth: 38,
          textAlign: "right",
          color: clipping ? "#ff4444" : "#8fa",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {clipping ? "CLIP" : `${Math.round(level)}%`}
      </span>
    </div>
  );
}
