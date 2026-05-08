/**
 * ModelDownloadWizard
 *
 * In-app Whisper GGML model download flow (#9 requirement).
 * Polls `get_stt_download_progress` every second while a download is active.
 *
 * Model sizes and capabilities:
 *  - tiny    ~75 MB  — fastest, lowest accuracy
 *  - base    ~142 MB — good balance for English sermons
 *  - small   ~466 MB — recommended for Nigerian English (DEFAULT)
 *  - medium  ~1.5 GB — highest accuracy, multilingual
 */
import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  downloadSttModel,
  getSttDownloadProgress,
  ModelDownloadStatus,
  ModelSize,
  parseDownloadStatus,
  resetSttDownloadProgress,
} from "../services/desktopApi";

interface ModelOption {
  id: ModelSize;
  label: string;
  size: string;
  description: string;
  recommended?: boolean;
}

const MODEL_OPTIONS: ModelOption[] = [
  {
    id: "tiny",
    label: "Tiny",
    size: "~75 MB",
    description: "Fastest. Lower accuracy. Good for testing.",
  },
  {
    id: "base",
    label: "Base",
    size: "~142 MB",
    description: "Balanced speed and accuracy. Standard English.",
  },
  {
    id: "small",
    label: "Small",
    size: "~466 MB",
    description: "High accuracy. Handles Nigerian English accents well.",
    recommended: true,
  },
  {
    id: "medium",
    label: "Medium",
    size: "~1.5 GB",
    description: "Best accuracy. Multilingual. Requires more RAM.",
  },
];

const POLL_INTERVAL_MS = 1000;

const ModelDownloadWizard: React.FC = () => {
  const [selected, setSelected] = useState<ModelSize>("small");
  const [status, setStatus] = useState<ModelDownloadStatus>("idle");
  const [progress, setProgress] = useState(-1);
  const [statusMessage, setStatusMessage] = useState("");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  const startPolling = useCallback(() => {
    stopPolling();
    pollRef.current = setInterval(async () => {
      try {
        const pct = await getSttDownloadProgress();
        const s = parseDownloadStatus(pct);
        setProgress(pct);
        setStatus(s);
        if (s !== "downloading") stopPolling();
      } catch {
        // ignore transient errors
      }
    }, POLL_INTERVAL_MS);
  }, [stopPolling]);

  useEffect(() => {
    // On mount, sync current progress in case a download is already running.
    getSttDownloadProgress()
      .then((pct) => {
        const s = parseDownloadStatus(pct);
        setProgress(pct);
        setStatus(s);
        if (s === "downloading") startPolling();
      })
      .catch(console.warn);
    return () => stopPolling();
  }, [startPolling, stopPolling]);

  const handleDownload = async () => {
    if (status === "downloading") return;
    // Clear any prior error state.
    if (status === "error_network" || status === "error_size") {
      await resetSttDownloadProgress().catch(() => {});
    }
    setStatus("downloading");
    setProgress(0);
    setStatusMessage("");
    try {
      const msg = await downloadSttModel(selected);
      setStatusMessage(msg);
      startPolling();
    } catch (e) {
      setStatus("error_network");
      setStatusMessage(String(e));
    }
  };

  const handleRetry = async () => {
    await resetSttDownloadProgress().catch(() => {});
    setStatus("idle");
    setProgress(-1);
    setStatusMessage("");
  };

  const progressDisplay =
    status === "downloading" && progress >= 0 ? progress : 0;

  return (
    <div className="mdw-root">
      {/* ── title ────────────────────────────────────────── */}
      <div className="mdw-header">
        <span className="mdw-icon">🤖</span>
        <div>
          <h3 className="mdw-title">Speech Recognition Model</h3>
          <p className="mdw-subtitle">
            Download a Whisper GGML model from Hugging Face for offline
            transcription. Once installed, the model is auto-loaded
            immediately — no restart needed.
          </p>
        </div>
      </div>

      {/* ── model selector ───────────────────────────────── */}
      <div className="mdw-options">
        {MODEL_OPTIONS.map((opt) => (
          <label
            key={opt.id}
            className={`mdw-option ${selected === opt.id ? "mdw-option-selected" : ""}`}
            htmlFor={`mdw-opt-${opt.id}`}
          >
            <input
              id={`mdw-opt-${opt.id}`}
              type="radio"
              name="model-size"
              value={opt.id}
              checked={selected === opt.id}
              onChange={() => setSelected(opt.id)}
              disabled={status === "downloading"}
            />
            <div className="mdw-opt-body">
              <div className="mdw-opt-top">
                <span className="mdw-opt-label">{opt.label}</span>
                <span className="mdw-opt-size">{opt.size}</span>
                {opt.recommended && (
                  <span className="mdw-opt-badge">Recommended</span>
                )}
              </div>
              <span className="mdw-opt-desc">{opt.description}</span>
            </div>
          </label>
        ))}
      </div>

      {/* ── progress bar ─────────────────────────────────── */}
      {status === "downloading" && (
        <div className="mdw-progress-area">
          <div className="mdw-progress-track">
            <div
              className="mdw-progress-fill"
              style={{ width: `${progressDisplay}%` }}
            />
          </div>
          <span className="mdw-progress-label">
            {progressDisplay < 1 ? "Connecting…" : `${progressDisplay}% downloaded`}
          </span>
        </div>
      )}

      {/* ── status messages ──────────────────────────────── */}
      {status === "complete" && (
        <div className="mdw-success">
          ✅ Model downloaded and loaded. Speech recognition is active.
        </div>
      )}
      {status === "error_network" && (
        <div className="mdw-error">
          ⚠ Download failed (network or write error).{" "}
          {statusMessage && <em>{statusMessage}</em>}
          <button className="mdw-retry-btn" onClick={handleRetry}>
            Retry
          </button>
        </div>
      )}
      {status === "error_size" && (
        <div className="mdw-error">
          ⚠ Downloaded file was too small — likely a corrupt response.
          <button className="mdw-retry-btn" onClick={handleRetry}>
            Retry
          </button>
        </div>
      )}

      {/* ── action button ────────────────────────────────── */}
      {(status === "idle" || status === "error_network" || status === "error_size") && (
        <button
          id="mdw-download-btn"
          className="mdw-download-btn"
          onClick={handleDownload}
        >
          ⬇ Download {MODEL_OPTIONS.find((o) => o.id === selected)?.label} model
        </button>
      )}
      {status === "downloading" && (
        <button className="mdw-download-btn mdw-download-btn-busy" disabled>
          Downloading…
        </button>
      )}

      {/* ── note ─────────────────────────────────────────── */}
      <p className="mdw-note">
        Models are sourced from{" "}
        <strong>ggerganov/whisper.cpp</strong> on Hugging Face. After
        download, the file is verified by size before being promoted to
        the assets directory.
      </p>
    </div>
  );
};

export default ModelDownloadWizard;
