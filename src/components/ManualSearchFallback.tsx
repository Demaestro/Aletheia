import { Keyboard, Search, SlidersHorizontal, X } from "lucide-react";
import { useEffect, useState } from "react";
import { manualSearchResults } from "../data/production";
import type { ManualSearchResult, ScriptureCandidate } from "../types";
import { ActionButton, SectionHeader, StatusPill } from "./Primitives";
import {
  listBibleTranslations,
  importBibleTranslation,
  deleteBibleTranslation,
  fetchVerseFromApiBible,
  type BibleTranslationStatus
} from "../services/desktopApi";

const BUNDLED_TRANSLATION_IDS = new Set(["kjv", "bbe"]);

const TRANSLATIONS = ["All", "KJV", "NIV", "ESV", "NKJV", "NLT", "AMP"];

export function ManualSearchFallback({
  query,
  results = manualSearchResults,
  onQueryChange,
  onPreview,
  onLive
}: {
  query: string;
  results?: ManualSearchResult[];
  onQueryChange: (value: string) => void;
  onPreview: (candidate: ScriptureCandidate) => void;
  onLive: (candidate: ScriptureCandidate) => void;
}) {
  const [showFilters, setShowFilters] = useState(false);
  const [translationFilter, setTranslationFilter] = useState("All");
  const [translations, setTranslations] = useState<BibleTranslationStatus[]>([]);
  const [libraryError, setLibraryError] = useState<string | null>(null);
  const [importPaths, setImportPaths] = useState<Record<string, string>>({});
  const [importingId, setImportingId] = useState<string | null>(null);
  const [importMessage, setImportMessage] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  // On-demand fetched verse bodies (for references missing from local DB)
  const [fetchedBodies, setFetchedBodies] = useState<Record<string, string>>({});
  const [fetchingRef, setFetchingRef] = useState<string | null>(null);

  const fetchOnline = (ref: string) => {
    setFetchingRef(ref);
    void fetchVerseFromApiBible(ref)
      .then((result) => {
        if (result?.text) {
          setFetchedBodies((p) => ({ ...p, [ref]: result.text }));
        } else {
          setFetchedBodies((p) => ({ ...p, [ref]: "(Not found in API.Bible — check spelling or add manually.)" }));
        }
      })
      .catch(() => setFetchedBodies((p) => ({ ...p, [ref]: "(API.Bible lookup failed — check internet connection.)" })))
      .finally(() => setFetchingRef(null));
  };

  const [loadingTranslations, setLoadingTranslations] = useState(true);

  const refreshTranslations = async () => {
    try {
      const list = await listBibleTranslations();
      setTranslations(list);
      setLibraryError(null);
    } catch (err) {
      setLibraryError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingTranslations(false);
    }
  };

  useEffect(() => {
    void refreshTranslations();
  }, []);

  const knownTranslations: { id: string; name: string; license: string }[] = [
    { id: "kjv", name: "King James Version", license: "Public Domain" },
    { id: "bbe", name: "Bible in Basic English", license: "Public Domain" },
    { id: "web", name: "World English Bible", license: "Public Domain" },
    { id: "nkjv", name: "New King James Version", license: "Thomas Nelson" },
    { id: "niv", name: "New International Version", license: "Biblica" },
    { id: "nlt", name: "New Living Translation", license: "Tyndale" },
    { id: "msg", name: "The Message", license: "NavPress" }
  ];

  const libraryRows = knownTranslations.map((meta) => {
    const status = translations.find((t) => t.id === meta.id);
    return { ...meta, status };
  });

  const handleDelete = async (id: string, name: string) => {
    if (BUNDLED_TRANSLATION_IDS.has(id)) return;
    if (typeof window !== "undefined" && !window.confirm(
      `Delete all loaded verses for ${name}? You can re-import the JSON file afterwards.`
    )) {
      return;
    }
    setDeletingId(id);
    setImportMessage(null);
    try {
      const removed = await deleteBibleTranslation(id);
      setImportMessage(`Removed ${removed.toLocaleString()} verses for ${name}.`);
      await refreshTranslations();
    } catch (err) {
      setImportMessage(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setDeletingId(null);
    }
  };

  const handleImport = async (id: string, name: string, license: string) => {
    const path = (importPaths[id] ?? "").trim();
    if (!path) {
      setImportMessage(`Paste an absolute path to the ${id.toUpperCase()} JSON file first.`);
      return;
    }
    setImportingId(id);
    setImportMessage(null);
    try {
      const result = await importBibleTranslation(id, name, license, path);
      setImportMessage(`Imported ${result.versesInserted.toLocaleString()} verses for ${name}.`);
      setImportPaths((prev) => ({ ...prev, [id]: "" }));
      await refreshTranslations();
    } catch (err) {
      setImportMessage(`Import failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setImportingId(null);
    }
  };

  const filteredResults =
    translationFilter === "All" ? results : results.filter((r) => r.translation === translationFilter);

  return (
    <section className="space-y-7">
      <SectionHeader
        eyebrow="Manual fallback"
        title="Fast local search when detection is uncertain"
        detail="Search works offline across references, phrases, abbreviations, and recent service context. Cloud semantic search can help later, but never blocks the operator."
        action={<StatusPill tone="healthy" label="SQLite FTS" detail="local" />}
      />

      <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
        <label htmlFor="scripture-search" className="text-sm font-semibold text-ink">
          Search reference, phrase, or abbreviation
        </label>
        <div className="mt-3 flex gap-3">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" aria-hidden="true" />
            <input
              id="scripture-search"
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              className="h-12 w-full rounded-[6px] border border-white/5 bg-paper pl-10 pr-4 text-base text-ink outline-none transition placeholder:text-muted focus:border-accent focus:bg-white/5"
              placeholder="Try 'jn 3 16' or 'the lord is my shepherd'"
              role="combobox"
              aria-expanded="true"
              aria-controls="search-results"
            />
          </div>
          <ActionButton tone={showFilters ? "primary" : "secondary"} onClick={() => setShowFilters((v) => !v)}>
            <SlidersHorizontal className="mr-2 h-4 w-4" aria-hidden="true" />
            Filters
          </ActionButton>
        </div>
        {showFilters && (
          <div className="mt-4 flex flex-wrap items-center gap-2 rounded-[6px] border border-white/5 bg-mist p-3">
            <span className="text-xs font-semibold uppercase tracking-[0.12em] text-muted">Translation</span>
            {TRANSLATIONS.map((t) => (
              <button
                key={t}
                type="button"
                onClick={() => setTranslationFilter(t)}
                className={`rounded-[6px] border px-3 py-1 text-xs font-semibold transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent ${
                  translationFilter === t
                    ? "border-accent bg-accent text-white"
                    : "border-white/5 bg-white/5 text-ink hover:border-accent/50"
                }`}
              >
                {t}
              </button>
            ))}
            {translationFilter !== "All" && (
              <button
                type="button"
                onClick={() => setTranslationFilter("All")}
                className="ml-auto inline-flex items-center gap-1 text-xs text-muted hover:text-ink"
              >
                <X className="h-3 w-3" aria-hidden="true" />
                Clear
              </button>
            )}
          </div>
        )}
        <p className="mt-3 text-sm text-muted" aria-live="polite">
          {filteredResults.length} results available. Use Tab to move through actions.
        </p>
      </div>

      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_340px]">
        <div id="search-results" className="rounded-[6px] border border-white/5 bg-white/5">
          {filteredResults.map((result) => {
            const bodyText = fetchedBodies[result.reference] ?? result.snippet;
            const isBodyMissing = !result.snippet || result.snippet.trim().length < 5;
            return (
            <div key={`${result.reference}-${result.source}`} className="group border-b border-white/5 p-5 last:border-0">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-lg font-semibold text-ink">{result.reference}</p>
                  <p className="mt-1 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
                    {result.translation} · {result.source}
                  </p>
                </div>
                <StatusPill tone={result.source === "Exact reference" ? "healthy" : "neutral"} label={result.source} />
              </div>
              {isBodyMissing && !fetchedBodies[result.reference] ? (
                <div style={{ marginTop: 10 }}>
                  <button
                    type="button"
                    disabled={fetchingRef === result.reference}
                    onClick={() => fetchOnline(result.reference)}
                    style={{
                      fontSize: 12, padding: "4px 12px",
                      background: "rgba(255,255,255,0.07)", color: "#8ffa",
                      border: "1px solid rgba(255,255,255,0.12)", borderRadius: 6, cursor: "pointer"
                    }}
                  >
                    {fetchingRef === result.reference ? "Fetching…" : "⬇ Fetch verse from API.Bible"}
                  </button>
                </div>
              ) : (
                <p className="mt-4 text-base leading-7 text-graphite">{bodyText}</p>
              )}
              <div className="mt-4 flex flex-wrap gap-2 opacity-100 transition md:opacity-0 md:group-hover:opacity-100 md:group-focus-within:opacity-100">
                <ActionButton tone="secondary" onClick={() => onPreview(toCandidate({ ...result, snippet: bodyText }))}>
                  Preview only
                </ActionButton>
                <ActionButton onClick={() => onLive(toCandidate({ ...result, snippet: bodyText }))}>Send live</ActionButton>
              </div>
            </div>
            );
          })}
        </div>

        <aside className="space-y-4">
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <p className="text-sm font-semibold text-ink">Bible library</p>
            <p className="mt-1 text-xs text-muted">
              KJV and BBE bundle on first launch (31,100+ verses each, full canon, public domain). Paste an
              absolute path to a thiagobodruk-format JSON file to import licensed translations.
            </p>
            {libraryError && (
              <p className="mt-2 text-xs text-red-400">Library status unavailable: {libraryError}</p>
            )}
            {loadingTranslations ? (
              <div className="mt-4 flex items-center gap-2 text-xs text-muted">
                <span className="h-3 w-3 animate-spin rounded-full border-2 border-muted border-t-accent" />
                Loading library status…
              </div>
            ) : (
            <ul className="mt-4 space-y-3">
              {libraryRows.map((row) => {
                const loaded = row.status?.versesLoaded ?? 0;
                const isFull = row.status?.fullCanon ?? false;
                return (
                  <li key={row.id} className="rounded-[6px] border border-line bg-paper p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="min-w-0">
                        <p className="text-sm font-semibold text-ink">{row.id.toUpperCase()}</p>
                        <p className="text-xs text-muted">{row.name}</p>
                      </div>
                      <span className="flex-none">
                        <StatusPill
                          tone={isFull ? "healthy" : loaded > 0 ? "neutral" : "degraded"}
                          label={isFull ? "Full Bible" : loaded > 0 ? `${loaded.toLocaleString()} verses` : "Not loaded"}
                          detail={loaded > 0 && !isFull ? "Curated" : undefined}
                        />
                      </span>
                    </div>
                    {!isFull && (
                      <div className="mt-2 flex gap-2">
                        <input
                          type="text"
                          value={importPaths[row.id] ?? ""}
                          onChange={(e) =>
                            setImportPaths((prev) => ({ ...prev, [row.id]: e.target.value }))
                          }
                          placeholder={`C:\\path\\to\\${row.id}.json`}
                          className="h-9 flex-1 min-w-0 rounded-[6px] border border-line bg-mist px-2 text-xs text-ink outline-none focus:border-accent"
                        />
                        <ActionButton
                          tone="secondary"
                          onClick={() => void handleImport(row.id, row.name, row.license)}
                          disabled={importingId === row.id}
                        >
                          {importingId === row.id ? "Importing…" : "Import"}
                        </ActionButton>
                      </div>
                    )}
                    {loaded > 0 && !BUNDLED_TRANSLATION_IDS.has(row.id) && (
                      <div className="mt-2 flex justify-end">
                        <ActionButton
                          tone="danger"
                          onClick={() => void handleDelete(row.id, row.name)}
                          disabled={deletingId === row.id}
                        >
                          {deletingId === row.id ? "Deleting…" : "Delete"}
                        </ActionButton>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
            )}
            {importMessage && (
              <p className="mt-3 text-xs text-muted" aria-live="polite">{importMessage}</p>
            )}
          </div>
          <div className="rounded-[6px] border border-white/5 bg-white/5 p-5">
            <Keyboard className="h-5 w-5 text-accent" aria-hidden="true" />
            <p className="mt-3 text-sm font-semibold text-ink">Operator shortcuts</p>
            <dl className="mt-4 space-y-3 text-sm">
              <Shortcut label="Focus search" value="Ctrl K" />
              <Shortcut label="Approve focused result" value="Enter" />
              <Shortcut label="Send preview live" value="Ctrl L" />
              <Shortcut label="Clear live" value="Esc Esc" />
            </dl>
          </div>
          <div className="rounded-[6px] border border-white/5 bg-mist p-5">
            <p className="text-sm font-semibold text-ink">Fallback policy</p>
            <p className="mt-2 text-sm leading-6 text-muted">
              Manual search bypasses AI confidence scoring but still respects preview-before-live and destination arming.
            </p>
          </div>
        </aside>
      </div>
    </section>
  );
}

function toCandidate(result: ManualSearchResult): ScriptureCandidate {
  return {
    id: `${result.reference}-${result.translation}`.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, ""),
    reference: result.reference,
    translation: result.translation,
    language: result.language ?? "English",
    text: result.snippet,
    confidence: result.source === "Exact reference" ? 99 : 72,
    source: "Manual search",
    reason: `Operator selected from ${result.source.toLowerCase()}.`,
    status: "preview"
  };
}

function Shortcut({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <dt className="text-muted">{label}</dt>
      <dd className="rounded-[6px] border border-white/5 bg-paper px-2 py-1 font-mono text-xs text-ink">{value}</dd>
    </div>
  );
}
