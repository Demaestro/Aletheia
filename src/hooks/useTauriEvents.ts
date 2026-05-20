import { useEffect, useRef } from "react";
import { useDesktopStore } from "../store/useDesktopStore";
import {
  onArmedChanged,
  onCandidatesUpdated,
  onLiveUpdated,
  onScriptureCandidate,
  onTranscriptSegment,
} from "../services/desktopApi";
import type { ScriptureCandidate } from "../types";
import type { LiveScriptureCandidateDto } from "../gen/LiveScriptureCandidateDto";

function liveDtoToCandidate(dto: LiveScriptureCandidateDto): ScriptureCandidate {
  // Map backend bucket/status to UI status union.
  const status: ScriptureCandidate["status"] =
    dto.status === "live" ? "live" :
    dto.status === "preview" ? "preview" :
    dto.status === "approved" ? "approved" :
    dto.status === "rejected" ? "rejected" : "new";
  return {
    id: dto.id,
    reference: dto.reference,
    translation: dto.translationId.toUpperCase(),
    language: dto.language || "en",
    text: dto.verseText,
    confidence: Math.round((dto.score ?? 0) * 100),
    source: "Live STT",
    reason: dto.reason,
    status,
  };
}

export function useTauriEvents(setCommandNotice: (msg: string) => void) {
  const setLiveCandidate = useDesktopStore(s => s.setLiveCandidate);
  const setDesktopStatus = useDesktopStore(s => s.setDesktopStatus);
  const setDestinationsArmed = useDesktopStore(s => s.setDestinationsArmed);
  const addTranscriptSegment = useDesktopStore(s => s.addTranscriptSegment);
  const mergeCandidates = useDesktopStore(s => s.mergeCandidates);
  const setSelectedCandidate = useDesktopStore(s => s.setSelectedCandidate);

  // Keep a stable ref so event handlers registered once always call the latest
  // setCommandNotice without needing to teardown and re-register listeners.
  const noticeRef = useRef(setCommandNotice);
  useEffect(() => { noticeRef.current = setCommandNotice; }, [setCommandNotice]);

  useEffect(() => {
    let unlistenLive: (() => void) | undefined;
    let unlistenArmed: (() => void) | undefined;
    let unlistenSegment: (() => void) | undefined;
    let unlistenCandidates: (() => void) | undefined;
    let unlistenLiveCandidate: (() => void) | undefined;

    onLiveUpdated((payload) => {
      const liveState: ScriptureCandidate = {
        id: payload.reference.toLowerCase().replace(/\s+/g, "-"),
        reference: payload.reference,
        translation: payload.translation,
        language: "English",
        text: "",
        confidence: 100,
        source: "Live output",
        reason: "Sent live by operator.",
        status: "live"
      };
      setLiveCandidate(liveState);
      
      const currentStatus = useDesktopStore.getState().desktopStatus;
      if (currentStatus) {
        setDesktopStatus({ ...currentStatus, auditCount: payload.auditCount, checkedAtMs: Date.now() });
      }
      
      noticeRef.current(`Live: ${payload.reference} ${payload.translation}`);
    }).then((fn) => { unlistenLive = fn; }).catch(() => undefined);

    onArmedChanged((payload) => {
      setDestinationsArmed(payload.destinationsArmed);
      
      const currentStatus = useDesktopStore.getState().desktopStatus;
      if (currentStatus) {
        setDesktopStatus({ ...currentStatus, destinationsArmed: payload.destinationsArmed, checkedAtMs: Date.now() });
      }
    }).then((fn) => { unlistenArmed = fn; }).catch(() => undefined);

    onTranscriptSegment((segment) => {
      addTranscriptSegment(segment);
    }).then((fn) => { unlistenSegment = fn; }).catch(() => undefined);

    onCandidatesUpdated((incoming) => {
      mergeCandidates(incoming);
      const state = useDesktopStore.getState();
      if (incoming[0] && !state.selectedCandidate) {
        setSelectedCandidate(incoming[0]);
      }
    }).then((fn) => { unlistenCandidates = fn; }).catch(() => undefined);

    onScriptureCandidate((dto) => {
      const candidate = liveDtoToCandidate(dto);
      mergeCandidates([candidate]);

      const state = useDesktopStore.getState();

      // Always surface the highest-confidence candidate as selected.
      if (!state.selectedCandidate || candidate.confidence > (state.selectedCandidate.confidence ?? 0)) {
        setSelectedCandidate(candidate);
      }

      // Smooth Operator: auto-promote to preview when confidence ≥ 75.
      if (candidate.confidence >= 75) {
        const currentPreview = state.previewCandidate;
        const isNewOrBetter =
          !currentPreview ||
          candidate.reference !== currentPreview.reference ||
          candidate.confidence > (currentPreview.confidence ?? 0);

        if (isNewOrBetter) {
          state.setPreviewCandidate({ ...candidate, status: "preview" });
        }
      }

      noticeRef.current(`Detected: ${candidate.reference} (${candidate.confidence}%)`);
    }).then((fn) => { unlistenLiveCandidate = fn; }).catch(() => undefined);

    return () => {
      unlistenLive?.();
      unlistenArmed?.();
      unlistenSegment?.();
      unlistenCandidates?.();
      unlistenLiveCandidate?.();
    };
  }, [
    setLiveCandidate, 
    setDesktopStatus, 
    setDestinationsArmed, 
    addTranscriptSegment, 
    mergeCandidates, 
    setSelectedCandidate
  ]);
}
