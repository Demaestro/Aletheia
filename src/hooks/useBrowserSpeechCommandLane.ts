import { useEffect, useRef } from "react";
import type { TranscriptSegment } from "../types";
import { classifyScriptureCommand } from "../utils/scriptureCommand";

type SpeechRecognitionAlternative = {
  transcript: string;
  confidence?: number;
};

type SpeechRecognitionResult = {
  readonly isFinal: boolean;
  readonly length: number;
  item(index: number): SpeechRecognitionAlternative;
  [index: number]: SpeechRecognitionAlternative;
};

type SpeechRecognitionResultList = {
  readonly length: number;
  item(index: number): SpeechRecognitionResult;
  [index: number]: SpeechRecognitionResult;
};

type SpeechRecognitionEventLike = {
  readonly resultIndex: number;
  readonly results: SpeechRecognitionResultList;
};

type SpeechRecognitionErrorEventLike = {
  readonly error?: string;
};

type BrowserSpeechRecognition = {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  maxAlternatives: number;
  start: () => void;
  stop: () => void;
  abort: () => void;
  onresult: ((event: SpeechRecognitionEventLike) => void) | null;
  onerror: ((event: SpeechRecognitionErrorEventLike) => void) | null;
  onend: (() => void) | null;
};

declare global {
  interface Window {
    SpeechRecognition?: new () => BrowserSpeechRecognition;
    webkitSpeechRecognition?: new () => BrowserSpeechRecognition;
  }
}

type BrowserSpeechCommandLaneOptions = {
  enabled: boolean;
  onCommand: (command: string, source: "interim" | "final") => void;
  onTranscript: (segment: TranscriptSegment) => void;
  onStatus?: (status: string) => void;
};

const COMMAND_DEBOUNCE_MS = 650;

export function shouldDispatchInterimCommand(text: string): boolean {
  const trimmed = text.trim();
  if (trimmed.length < 4) return false;
  const kind = classifyScriptureCommand(trimmed);
  return kind === "explicitReference" || kind === "contextualFollowUp";
}

function makeSegment(text: string): TranscriptSegment {
  const now = Date.now();
  return {
    id: `browser-speech-${now}`,
    time: new Date(now).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }),
    speaker: "Live mic",
    language: "English",
    text,
    confidence: 86,
    latencyMs: 250,
  };
}

export function useBrowserSpeechCommandLane({
  enabled,
  onCommand,
  onTranscript,
  onStatus,
}: BrowserSpeechCommandLaneOptions) {
  const recognitionRef = useRef<BrowserSpeechRecognition | null>(null);
  const restartTimerRef = useRef<number | null>(null);
  const lastCommandRef = useRef<{ text: string; at: number }>({ text: "", at: 0 });
  const stoppedRef = useRef(false);

  useEffect(() => {
    if (!enabled || typeof window === "undefined") return undefined;
    const SpeechRecognitionCtor = window.SpeechRecognition ?? window.webkitSpeechRecognition;
    if (!SpeechRecognitionCtor) {
      onStatus?.("Browser speech command lane unavailable.");
      return undefined;
    }

    stoppedRef.current = false;
    const recognition = new SpeechRecognitionCtor();
    recognitionRef.current = recognition;
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = "en-NG";
    recognition.maxAlternatives = 1;

    const dispatchCommand = (text: string, source: "interim" | "final") => {
      const normalized = text.trim().replace(/\s+/g, " ");
      if (!normalized) return;
      const now = Date.now();
      if (
        lastCommandRef.current.text.toLowerCase() === normalized.toLowerCase() &&
        now - lastCommandRef.current.at < COMMAND_DEBOUNCE_MS
      ) {
        return;
      }
      lastCommandRef.current = { text: normalized, at: now };
      onCommand(normalized, source);
    };

    recognition.onresult = (event) => {
      let interim = "";
      let finalText = "";
      for (let index = event.resultIndex; index < event.results.length; index += 1) {
        const result = event.results[index];
        const transcript = result[0]?.transcript?.trim() ?? "";
        if (!transcript) continue;
        if (result.isFinal) {
          finalText += `${transcript} `;
        } else {
          interim += `${transcript} `;
        }
      }

      const finalTrimmed = finalText.trim();
      if (finalTrimmed) {
        onTranscript(makeSegment(finalTrimmed));
        dispatchCommand(finalTrimmed, "final");
        return;
      }

      const interimTrimmed = interim.trim();
      if (shouldDispatchInterimCommand(interimTrimmed)) {
        dispatchCommand(interimTrimmed, "interim");
      }
    };

    recognition.onerror = (event) => {
      if (event.error && !["no-speech", "aborted"].includes(event.error)) {
        onStatus?.(`Speech command lane: ${event.error}.`);
      }
    };

    recognition.onend = () => {
      if (stoppedRef.current) return;
      restartTimerRef.current = window.setTimeout(() => {
        try {
          recognition.start();
        } catch {
          onStatus?.("Speech command lane waiting to restart.");
        }
      }, 450);
    };

    try {
      recognition.start();
      onStatus?.("Fast speech command lane active.");
    } catch {
      onStatus?.("Fast speech command lane waiting for microphone permission.");
    }

    return () => {
      stoppedRef.current = true;
      if (restartTimerRef.current !== null) {
        window.clearTimeout(restartTimerRef.current);
      }
      recognition.onresult = null;
      recognition.onerror = null;
      recognition.onend = null;
      try {
        recognition.stop();
      } catch {
        recognition.abort();
      }
      recognitionRef.current = null;
    };
  }, [enabled, onCommand, onStatus, onTranscript]);
}
