/**
 * TranscriptSearchPanel
 *
 * Full-text search over the persisted transcript_segments table.
 * Uses the `search_transcript_history` Tauri command (SQLite FTS5 with
 * graceful LIKE fallback).
 *
 * Highlight rendering: the backend wraps matched terms with «…» guillemets
 * which this component converts to <mark> elements for browser highlight.
 */
import React, {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  searchTranscriptHistory,
  TranscriptSearchResult,
} from "../services/desktopApi";

const DEBOUNCE_MS = 350;

/** Converts backend guillemet highlights → <mark> spans. */
function renderSnippet(snippet: string): React.ReactNode {
  const parts = snippet.split(/(«[^»]*»)/g);
  return parts.map((part, i) => {
    if (part.startsWith("«") && part.endsWith("»")) {
      return (
        <mark key={i} className="tsp-highlight">
          {part.slice(1, -1)}
        </mark>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

const TranscriptSearchPanel: React.FC = () => {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<TranscriptSearchResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const runSearch = useCallback((q: string) => {
    const trimmed = q.trim();
    if (trimmed.length < 2) {
      setResults([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    searchTranscriptHistory(trimmed, 60)
      .then((res) => {
        setResults(res);
        setLoading(false);
      })
      .catch((e) => {
        setError(String(e));
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => runSearch(query), DEBOUNCE_MS);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, runSearch]);

  const confidenceColor = (c: number) => {
    if (c >= 80) return "#4ade80";
    if (c >= 55) return "#facc15";
    return "#f87171";
  };

  return (
    <div className="tsp-root">
      {/* ── search bar ────────────────────────────────────── */}
      <div className="tsp-search-bar">
        <span className="tsp-icon">🔍</span>
        <input
          id="transcript-search-input"
          type="text"
          className="tsp-input"
          placeholder="Search transcript history… e.g. 'John' or 'by his stripes'"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
        {query && (
          <button
            id="transcript-search-clear"
            className="tsp-clear"
            onClick={() => setQuery("")}
            aria-label="Clear search"
          >
            ×
          </button>
        )}
      </div>

      {/* ── state feedback ────────────────────────────────── */}
      {loading && (
        <div className="tsp-status">
          <span className="tsp-spinner" /> Searching…
        </div>
      )}
      {error && <div className="tsp-error">⚠ {error}</div>}
      {!loading && !error && query.trim().length >= 2 && results.length === 0 && (
        <div className="tsp-empty">No transcript matches for "{query.trim()}"</div>
      )}

      {/* ── results ───────────────────────────────────────── */}
      {results.length > 0 && (
        <div className="tsp-results">
          <div className="tsp-results-header">
            {results.length} result{results.length !== 1 ? "s" : ""}
          </div>
          <ul className="tsp-list">
            {results.map((r) => (
              <li key={r.segmentId} className="tsp-item">
                <div className="tsp-item-meta">
                  <span className="tsp-time">{r.time}</span>
                  <span className="tsp-speaker">{r.speaker}</span>
                  <span className="tsp-lang">{r.language}</span>
                  <span
                    className="tsp-confidence"
                    style={{ color: confidenceColor(r.confidence) }}
                    title={`${r.confidence}% confidence`}
                  >
                    {r.confidence}%
                  </span>
                </div>
                <p className="tsp-snippet">{renderSnippet(r.snippet)}</p>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
};

export default TranscriptSearchPanel;
