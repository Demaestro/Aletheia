/**
 * AudioStreamContext.tsx
 *
 * Provides a single shared MediaStream from getUserMedia to all consumers.
 * The VuMeter previously opened its own second stream — on many Windows
 * USB audio drivers that forces the mic into sharing mode, adding latency
 * to the primary STT capture path.
 *
 * Usage:
 *   // In App.tsx root:
 *   <AudioStreamProvider active={captureRunning}>
 *     ...children containing VuMeter...
 *   </AudioStreamProvider>
 *
 *   // In VuMeter.tsx:
 *   const stream = useAudioStream();
 */

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";

interface AudioStreamContextValue {
  /** The shared MediaStream, or null if not yet acquired / permission denied. */
  stream: MediaStream | null;
  /** True while getUserMedia is pending. */
  acquiring: boolean;
  /** Error message if getUserMedia failed. */
  error: string | null;
}

const AudioStreamContext = createContext<AudioStreamContextValue>({
  stream: null,
  acquiring: false,
  error: null,
});

interface ProviderProps {
  /** When true the provider acquires the mic. When false it releases it. */
  active: boolean;
  /** Optional device label or deviceId to request. Falls back to default. */
  deviceLabel?: string;
  children: React.ReactNode;
}

export function AudioStreamProvider({ active, deviceLabel, children }: ProviderProps) {
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [acquiring, setAcquiring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamRef = useRef<MediaStream | null>(null);

  const release = useCallback(() => {
    streamRef.current?.getTracks().forEach((t) => t.stop());
    streamRef.current = null;
    setStream(null);
  }, []);

  useEffect(() => {
    if (!active) {
      release();
      return;
    }

    let cancelled = false;
    setAcquiring(true);
    setError(null);

    // Build constraints — try to match deviceLabel against enumerated devices.
    const acquireStream = async () => {
      let deviceId: string | undefined;

      if (deviceLabel) {
        const devices = await navigator.mediaDevices.enumerateDevices();
        const match = devices.find(
          (d) => d.kind === "audioinput" &&
                 (d.label === deviceLabel || d.deviceId === deviceLabel)
        );
        deviceId = match?.deviceId;
      }

      const constraints: MediaStreamConstraints = {
        audio: deviceId
          ? { deviceId: { exact: deviceId }, echoCancellation: false, noiseSuppression: false }
          : { echoCancellation: false, noiseSuppression: false },
        video: false,
      };

      const s = await navigator.mediaDevices.getUserMedia(constraints);
      if (cancelled) {
        s.getTracks().forEach((t) => t.stop());
        return;
      }
      streamRef.current = s;
      setStream(s);
    };

    acquireStream()
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Microphone access denied.");
        }
      })
      .finally(() => {
        if (!cancelled) setAcquiring(false);
      });

    return () => {
      cancelled = true;
      release();
    };
  }, [active, deviceLabel, release]);

  return (
    <AudioStreamContext.Provider value={{ stream, acquiring, error }}>
      {children}
    </AudioStreamContext.Provider>
  );
}

/** Hook — subscribe to the shared audio stream. */
export function useAudioStream(): AudioStreamContextValue {
  return useContext(AudioStreamContext);
}
