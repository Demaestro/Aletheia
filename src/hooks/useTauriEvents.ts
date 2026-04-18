import { useEffect } from "react";
import { useDesktopStore } from "../store/useDesktopStore";
import {
  onArmedChanged,
  onCandidatesUpdated,
  onLiveUpdated,
  onTranscriptSegment,
} from "../services/desktopApi";
import type { ScriptureCandidate } from "../types";

export function useTauriEvents(setCommandNotice: (msg: string) => void) {
  const setLiveCandidate = useDesktopStore(s => s.setLiveCandidate);
  const setDesktopStatus = useDesktopStore(s => s.setDesktopStatus);
  const setDestinationsArmed = useDesktopStore(s => s.setDestinationsArmed);
  const addTranscriptSegment = useDesktopStore(s => s.addTranscriptSegment);
  const mergeCandidates = useDesktopStore(s => s.mergeCandidates);
  const setSelectedCandidate = useDesktopStore(s => s.setSelectedCandidate);

  useEffect(() => {
    let unlistenLive: (() => void) | undefined;
    let unlistenArmed: (() => void) | undefined;
    let unlistenSegment: (() => void) | undefined;
    let unlistenCandidates: (() => void) | undefined;

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
      
      setCommandNotice(`Live: ${payload.reference} ${payload.translation}`);
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

    return () => {
      unlistenLive?.();
      unlistenArmed?.();
      unlistenSegment?.();
      unlistenCandidates?.();
    };
  }, [
    setLiveCandidate, 
    setDesktopStatus, 
    setDestinationsArmed, 
    addTranscriptSegment, 
    mergeCandidates, 
    setSelectedCandidate, 
    setCommandNotice
  ]);
}
