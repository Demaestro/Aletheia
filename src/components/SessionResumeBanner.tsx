/**
 * SessionResumeBanner
 *
 * Shown once at startup when check_session_resumption finds an interrupted
 * same-day session or a crash log from the previous process run.
 * The user can dismiss it OR re-open the session context.
 */
import React, { useEffect, useState } from "react";
import {
  checkSessionResumption,
  SessionResumptionInfo,
} from "../services/desktopApi";

interface Props {
  /** Called when operator clicks "Resume Session". */
  onResume: (info: SessionResumptionInfo) => void;
}

const SessionResumeBanner: React.FC<Props> = ({ onResume }) => {
  const [info, setInfo] = useState<SessionResumptionInfo | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    checkSessionResumption()
      .then((r) => {
        if (r.canResume) setInfo(r);
      })
      .catch(console.warn);
  }, []);

  if (!info || dismissed) return null;

  const startedAt = info.startedAtMs
    ? new Date(info.startedAtMs).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      })
    : "unknown";

  return (
    <div className="session-resume-banner" role="alert">
      <div className="srb-icon">
        {info.crashLogFound ? "⚠️" : "🔄"}
      </div>
      <div className="srb-body">
        <strong className="srb-title">
          {info.crashLogFound
            ? "Previous session ended unexpectedly"
            : "Resumable session detected"}
        </strong>
        <span className="srb-detail">
          {info.sessionName ?? "Service session"} started at {startedAt}
          {" — "}
          {info.segmentCount} transcript segment
          {info.segmentCount !== 1 ? "s" : ""},{" "}
          {info.candidateCount} scripture reference
          {info.candidateCount !== 1 ? "s" : ""} captured
        </span>
      </div>
      <div className="srb-actions">
        <button
          id="btn-session-resume"
          className="srb-btn srb-btn-primary"
          onClick={() => {
            onResume(info);
            setDismissed(true);
          }}
        >
          Resume
        </button>
        <button
          id="btn-session-dismiss"
          className="srb-btn srb-btn-ghost"
          onClick={() => setDismissed(true)}
        >
          Dismiss
        </button>
      </div>
    </div>
  );
};

export default SessionResumeBanner;
