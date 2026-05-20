/**
 * VuMeter — real-time microphone level indicator.
 *
 * Consumes the shared MediaStream from AudioStreamContext instead of
 * opening its own getUserMedia — prevents the double-stream issue that
 * forced some Windows USB drivers into sharing mode, adding STT latency.
 *
 * Uses getByteTimeDomainData (waveform RMS) for accurate speech-level
 * metering — frequency-domain RMS badly underreports quiet voices.
 */
import { useEffect, useRef, useState } from "react";
import { useAudioStream } from "../contexts/AudioStreamContext";

interface Props {
  /** Pass true while start_audio_capture has succeeded. */
  active: boolean;
  deviceLabel?: string;
}

export function VuMeter({ active, deviceLabel }: Props) {
  const [level, setLevel]       = useState(0);   // 0-100
  const [peak, setPeak]         = useState(0);   // 0-100, sticky hold
  const [clipping, setClipping] = useState(false);

  const animRef     = useRef<number>(0);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const peakTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Consume the shared stream — no second getUserMedia call.
  const { stream } = useAudioStream();

  useEffect(() => {
    if (!active || !stream) {
      setLevel(0);
      setPeak(0);
      setClipping(false);
      return;
    }

    let cancelled = false;

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
      // Time-domain waveform RMS: each byte is 0-255, centred at 128 (silence).
      analyser.getByteTimeDomainData(data);
      let sum = 0;
      for (let i = 0; i < data.length; i++) {
        const deviation = (data[i] - 128) / 128;
        sum += deviation * deviation;
      }
      const rms = Math.sqrt(sum / data.length);
      const pct = Math.min(100, rms * 100 * 3.2); // calibrated boost for speech
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

    return () => {
      cancelled = true;
      cancelAnimationFrame(animRef.current);
      if (peakTimerRef.current) clearTimeout(peakTimerRef.current);
      audioCtxRef.current?.close().catch(() => undefined);
      analyserRef.current = null;
      audioCtxRef.current = null;
    };
  }, [active, stream]);

  if (!active) return null;

  const barColor = clipping ? "#ff4444" : level > 70 ? "#ffbb33" : "#22dd88";

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
