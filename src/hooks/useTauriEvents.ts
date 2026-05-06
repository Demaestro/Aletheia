import { useEffect, useRef } from "react";
import { useDesktopStore } from "../store/useDesktopStore";
import {
  onArmedChanged,
  onCandidatesUpdated,
  onLiveUpdated,
  previewCandidate as persistPreviewCandidate,
  renderPreviewScene,
  onScriptureCandidate,
  onTranscriptSegment,
} from "../services/desktopApi";
import type { ScriptureCandidate } from "../types";
import type { LiveScriptureCandidateDto } from "../gen/LiveScriptureCandidateDto";

function liveDtoToCandidate(dto: LiveScriptureCandidateDto): ScriptureCandidate {
  // Map backend decision vocab to the UI status union.
  // Backend emits one of: "open" | "preview" | "approval" | "ignored"
  // (see commands.rs::run_live_detection). Older paths may still emit
  // "live"/"approved"/"rejected" directly, so we accept both.
  const status: ScriptureCandidate["status"] =
    dto.status === "open"     ? "live" :
    dto.status === "live"     ? "live" :
    dto.status === "preview"  ? "preview" :
    dto.status === "approval" ? "new" :
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
  const setPreviewCandidate = useDesktopStore(s => s.setPreviewCandidate);

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
      // Honour the gated decision the backend already made:
      //   "live"    → already pushed to vMix in Auto/Rehearsal modes
      //   "preview" → ready for preview slot, operator confirms before live
      //   "new"     → needs explicit operator approval (low/ambiguous score)
      mergeCandidates([candidate]);
      const state = useDesktopStore.getState();
      const isAutoSent = candidate.status === "live";
      const shouldPreview = candidate.status === "preview";

      if (!state.selectedCandidate || shouldPreview || isAutoSent) {
        setSelectedCandidate(candidate);
      }
      if (shouldPreview) {
        setPreviewCandidate(candidate);
        void renderPreviewScene(candidate).catch(() => undefined);
        void persistPreviewCandidate(candidate.id).catch(() => undefined);
      }
      if (isAutoSent) {
        setLiveCandidate(candidate);
      }
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
    setSelectedCandidate,
    setPreviewCandidate
  ]);
}
