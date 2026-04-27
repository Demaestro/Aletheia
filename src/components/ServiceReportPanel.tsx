/**
 * ServiceReportPanel
 *
 * Read-only audit summary for the active or most-recent service session.
 * Driven by the `get_service_report` Tauri command (#7 requirement).
 *
 * Shown in the Health/Audit tab.  Key metrics: duration, segment count,
 * candidate count, operator verdicts (approve/reject/live/panic).
 * Full action log is scrollable below.
 */
import React, { useCallback, useEffect, useState } from "react";
import { getServiceReport, ServiceReport } from "../services/desktopApi";

const ACTION_LABEL: Record<string, string> = {
  approve:     "✅ Approved",
  reject:      "❌ Rejected",
  live:        "📺 Sent Live",
  panic_clear: "🚨 Panic Clear",
  preview:     "👁 Preview",
  dismiss:     "🚫 Dismissed",
};

const ServiceReportPanel: React.FC = () => {
  const [report, setReport] = useState<ServiceReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    getServiceReport()
      .then((r) => {
        setReport(r);
        setLoading(false);
      })
      .catch((e) => {
        setError(String(e));
        setLoading(false);
      });
  }, []);

  useEffect(() => { load(); }, [load]);

  if (loading) {
    return (
      <div className="srp-root">
        <div className="srp-loading">
          <span className="srp-spinner" /> Loading service report…
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="srp-root">
        <div className="srp-error">
          ⚠ {error}
          <button className="srp-retry" onClick={load}>Retry</button>
        </div>
      </div>
    );
  }

  if (!report) return null;

  const progressPct =
    report.candidateCount > 0
      ? Math.round((report.liveCount / report.candidateCount) * 100)
      : 0;

  return (
    <div className="srp-root">
      {/* ── header ──────────────────────────────────────── */}
      <div className="srp-header">
        <div className="srp-title-row">
          <h2 className="srp-title">📋 Service Report</h2>
          <button id="srp-refresh" className="srp-refresh-btn" onClick={load}>
            ↺
          </button>
        </div>
        <p className="srp-subtitle">
          {report.sessionName} · {report.durationMinutes} min ·{" "}
          <span className="srp-operator">@{report.operatorName}</span>
        </p>
      </div>

      {/* ── stat cards ──────────────────────────────────── */}
      <div className="srp-stats">
        <div className="srp-stat">
          <span className="srp-stat-value">{report.segmentCount}</span>
          <span className="srp-stat-label">Transcript segments</span>
        </div>
        <div className="srp-stat">
          <span className="srp-stat-value">{report.candidateCount}</span>
          <span className="srp-stat-label">Candidates detected</span>
        </div>
        <div className="srp-stat srp-stat-green">
          <span className="srp-stat-value">{report.liveCount}</span>
          <span className="srp-stat-label">Sent live</span>
        </div>
        <div className="srp-stat srp-stat-amber">
          <span className="srp-stat-value">{report.approvedCount}</span>
          <span className="srp-stat-label">Approved</span>
        </div>
        <div className="srp-stat srp-stat-red">
          <span className="srp-stat-value">{report.rejectedCount}</span>
          <span className="srp-stat-label">Rejected</span>
        </div>
        <div className="srp-stat">
          <span className="srp-stat-value">{report.panicClearCount}</span>
          <span className="srp-stat-label">Panic clears</span>
        </div>
      </div>

      {/* ── live rate bar ───────────────────────────────── */}
      {report.candidateCount > 0 && (
        <div className="srp-rate-row">
          <span className="srp-rate-label">Live rate</span>
          <div className="srp-rate-bar-track">
            <div
              className="srp-rate-bar-fill"
              style={{ width: `${progressPct}%` }}
            />
          </div>
          <span className="srp-rate-pct">{progressPct}%</span>
        </div>
      )}

      {/* ── action log ──────────────────────────────────── */}
      <div className="srp-log-header">Operator Action Log</div>
      {report.actions.length === 0 ? (
        <p className="srp-log-empty">No operator actions recorded this session.</p>
      ) : (
        <div className="srp-log-scroll">
          <table className="srp-log-table">
            <thead>
              <tr>
                <th>Time</th>
                <th>Action</th>
                <th>Reference / Detail</th>
                <th>Operator</th>
              </tr>
            </thead>
            <tbody>
              {report.actions.map((a, idx) => {
                let detail = "";
                try {
                  const parsed = JSON.parse(a.payloadJson);
                  detail = parsed.reference ?? parsed.reason ?? a.candidateId ?? "";
                } catch {
                  detail = a.candidateId ?? "";
                }
                return (
                  <tr key={idx} className={`srp-row srp-row-${a.actionType}`}>
                    <td className="srp-cell-time">{a.time}</td>
                    <td className="srp-cell-action">
                      {ACTION_LABEL[a.actionType] ?? a.actionType}
                    </td>
                    <td className="srp-cell-detail">{detail}</td>
                    <td className="srp-cell-actor">@{a.actor}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};

export default ServiceReportPanel;
