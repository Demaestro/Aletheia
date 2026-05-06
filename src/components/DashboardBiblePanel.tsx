import { memo, useEffect, useMemo, useRef, useState, useTransition } from "react";
import { ChevronLeft, ChevronRight, Mic, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { HealthItem, ScriptureCandidate } from "../types";
import { ActionButton, StatusPill, cn } from "./Primitives";
import {
  getBibleChapter,
  listBibleTranslations,
  type AudioLevel,
  type BibleTranslationStatus,
  type BibleVerse,
} from "../services/desktopApi";
import { captureStateLabel, type CaptureState } from "../hooks/useAlwaysOnCommandCapture";
import { VuMeter } from "./VuMeter";

const BOOKS = [
  "Genesis", "Exodus", "Leviticus", "Numbers", "Deuteronomy", "Joshua", "Judges", "Ruth",
  "1 Samuel", "2 Samuel", "1 Kings", "2 Kings", "1 Chronicles", "2 Chronicles", "Ezra", "Nehemiah",
  "Esther", "Job", "Psalm", "Proverbs", "Ecclesiastes", "Song of Solomon", "Isaiah", "Jeremiah",
  "Lamentations", "Ezekiel", "Daniel", "Hosea", "Joel", "Amos", "Obadiah", "Jonah", "Micah",
  "Nahum", "Habakkuk", "Zephaniah", "Haggai", "Zechariah", "Malachi", "Matthew", "Mark", "Luke",
  "John", "Acts", "Romans", "1 Corinthians", "2 Corinthians", "Galatians", "Ephesians",
  "Philippians", "Colossians", "1 Thessalonians", "2 Thessalonians", "1 Timothy", "2 Timothy",
  "Titus", "Philemon", "Hebrews", "James", "1 Peter", "2 Peter", "1 John", "2 John", "3 John",
  "Jude", "Revelation",
];

type ParsedDashboardReference = {
  book: string;
  chapter: number;
  verse: number;
};

function parseDashboardReference(reference?: string | null): ParsedDashboardReference | null {
  if (!reference) return null;
  const match = reference.trim().match(/^(.+?)\s+(\d+):(\d+)/);
  if (!match) return null;
  const [, book, chapter, verse] = match;
  const parsedChapter = Number.parseInt(chapter, 10);
  const parsedVerse = Number.parseInt(verse, 10);
  if (!book || !Number.isFinite(parsedChapter) || !Number.isFinite(parsedVerse)) return null;
  return { book: book === "Psalms" ? "Psalm" : book, chapter: parsedChapter, verse: parsedVerse };
}

export const DashboardBiblePanel = memo(function DashboardBiblePanel({
  activeCandidate,
  listenerStatus,
  onOpenCommand,
  onPreviewVerse,
  captureState = "idle",
  captureRunning = false,
  captureStarting = false,
  selectedAudioDevice,
  audioLevel,
  healthItems = [],
  onOpenCaptureDiagnostics,
}: {
  activeCandidate?: ScriptureCandidate | null;
  listenerStatus?: string;
  onOpenCommand?: (command: string, translationId?: string) => void;
  onPreviewVerse?: (reference: string, text: string, translation: string) => void;
  captureState?: CaptureState;
  captureRunning?: boolean;
  captureStarting?: boolean;
  selectedAudioDevice?: string;
  audioLevel?: AudioLevel | null;
  healthItems?: HealthItem[];
  onOpenCaptureDiagnostics?: () => void;
}) {
  const { t } = useTranslation();
  const activeReference = useMemo(
    () => parseDashboardReference(activeCandidate?.reference) ?? { book: "Genesis", chapter: 1, verse: 1 },
    [activeCandidate?.reference]
  );
  const [book, setBook] = useState("Genesis");
  const [chapter, setChapter] = useState(1);
  const [verse, setVerse] = useState(1);
  const [translationId, setTranslationId] = useState("kjv");
  const [translations, setTranslations] = useState<BibleTranslationStatus[]>([]);
  const [verses, setVerses] = useState<BibleVerse[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [, startTranslationTransition] = useTransition();
  const activeVerseRef = useRef<HTMLButtonElement | null>(null);
  const chapterRequestSeqRef = useRef(0);
  const didSkipInitialCandidateRef = useRef(false);
  const hasCriticalReadinessIssue = useMemo(
    () =>
      healthItems.some((item) => {
        const label = item.label.toLowerCase();
        return (
          ["offline", "degraded"].includes(item.state) &&
          (label.includes("scripture") || label.includes("stt") || label.includes("database"))
        );
      }),
    [healthItems],
  );

  const stableTranslations = useMemo(() => {
    const loaded = translations.filter((item) => item.versesLoaded > 0);
    const fullCanon = loaded.filter((item) => item.fullCanon);
    if (fullCanon.length > 0) return fullCanon;

    const safeSeeded = loaded.filter((item) =>
      ["kjv", "web", "bbe"].includes(item.id.toLowerCase())
    );
    if (safeSeeded.length > 0) return safeSeeded;

    return loaded.length
      ? loaded
      : [{ id: "kjv", name: "King James Version", versesLoaded: 0, fullCanon: false }];
  }, [translations]);
  useEffect(() => {
    if (!didSkipInitialCandidateRef.current) {
      didSkipInitialCandidateRef.current = true;
      return;
    }
    setBook(activeReference.book);
    setChapter(activeReference.chapter);
    setVerse(activeReference.verse);
    if (activeCandidate?.translation) {
      const nextTranslation = activeCandidate.translation.toLowerCase();
      const stable = stableTranslations.some((item) => item.id === nextTranslation && item.fullCanon);
      setTranslationId(stable ? nextTranslation : "kjv");
    }
  }, [activeCandidate?.translation, activeReference.book, activeReference.chapter, activeReference.verse, stableTranslations]);

  useEffect(() => {
    let cancelled = false;
    void listBibleTranslations().then((items) => {
      if (cancelled) return;
      setTranslations(items.filter((item) => item.versesLoaded > 0));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (stableTranslations.length === 0) return;
    if (stableTranslations.some((item) => item.id === translationId)) return;
    setTranslationId(stableTranslations[0].id);
  }, [stableTranslations, translationId]);

  useEffect(() => {
    let cancelled = false;
    const requestSeq = ++chapterRequestSeqRef.current;
    const timer = window.setTimeout(() => {
      setLoading(true);
      setError(null);
      void getBibleChapter(translationId, book, chapter)
      .then((chapterVerses) => {
        if (cancelled || requestSeq !== chapterRequestSeqRef.current) return;
        setVerses(chapterVerses);
        if (chapterVerses.length === 0) {
          setError(`${translationId.toUpperCase()} ${book} ${chapter} returned no verses. Check Bible library status or switch to KJV/WEB.`);
        }
      })
      .catch((err: unknown) => {
        if (cancelled || requestSeq !== chapterRequestSeqRef.current) return;
        setVerses([]);
        setError(err instanceof Error ? err.message : "Chapter lookup failed.");
      })
      .finally(() => {
        if (!cancelled && requestSeq === chapterRequestSeqRef.current) setLoading(false);
      });
    }, 100);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [translationId, book, chapter]);

  useEffect(() => {
    activeVerseRef.current?.scrollIntoView({ block: "center", behavior: "auto" });
  }, [book, chapter, verse, verses.length]);

  const openTypedReference = () => {
    const safeChapter = Math.max(1, Math.trunc(chapter || 1));
    const safeVerse = Math.max(1, Math.trunc(verse || 1));
    setChapter(safeChapter);
    setVerse(safeVerse);
    const chapterHit = verses.find((item) => item.verse === safeVerse);
    if (chapterHit) {
      onPreviewVerse?.(chapterHit.reference, chapterHit.text, chapterHit.translation);
      return;
    }
    onOpenCommand?.(`${book} ${safeChapter}:${safeVerse}`, translationId);
  };

  const goPreviousChapter = () => {
    setChapter((value) => Math.max(1, value - 1));
    setVerse(1);
  };

  const goNextChapter = () => {
    setChapter((value) => value + 1);
    setVerse(1);
  };

  return (
    <section className="overflow-hidden rounded-[8px] border border-line bg-paper shadow-[0_18px_60px_-44px_rgba(0,0,0,0.85)]">
      <div className="border-b border-line bg-paper px-4 py-3">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <StatusPill
            tone={
              captureRunning
                ? "live"
                : captureStarting || captureState === "missingModel" || captureState === "micUnavailable" || captureState === "recognitionDegraded"
                ? "degraded"
                : "neutral"
            }
            label={captureStateLabel(captureState)}
          />
          <button
            type="button"
            onClick={onOpenCaptureDiagnostics}
            className="truncate rounded-[6px] border border-line bg-mist px-2.5 py-1.5 text-xs font-medium text-muted transition hover:text-ink"
          >
            {listenerStatus ??
              t("dashboardBible.listenerManaged", {
                defaultValue: "Always-on command listener is managed by the desktop core.",
              })}
          </button>
          <div className="min-w-[180px] flex-1">
            <VuMeter
              active={captureRunning || captureStarting}
              deviceLabel={selectedAudioDevice}
              backendLevel={audioLevel ?? null}
              compact
            />
          </div>
        </div>

        <div className="grid min-w-0 grid-cols-[86px_minmax(170px,1fr)_64px_64px_auto] items-center gap-2 xl:justify-end">
          <select
            id="dashboard-bible-translation"
            value={translationId}
            onChange={(event) => {
              const nextTranslation = event.target.value;
              startTranslationTransition(() => setTranslationId(nextTranslation));
            }}
            className="h-9 rounded-[6px] border border-line bg-paper px-2.5 text-xs font-semibold text-ink outline-none focus:border-accent"
            aria-label={t("dashboardBible.translation", { defaultValue: "Bible translation" })}
          >
            {stableTranslations.map((item) => (
              <option key={item.id} value={item.id}>
                {item.id.toUpperCase()}
              </option>
            ))}
          </select>

          <select
            id="dashboard-bible-book"
            value={book}
            onChange={(event) => setBook(event.target.value)}
            className="h-9 min-w-0 rounded-[6px] border border-line bg-paper px-2.5 text-xs font-semibold text-ink outline-none focus:border-accent"
            aria-label={t("dashboardBible.book", { defaultValue: "Bible book" })}
          >
            {BOOKS.map((name) => (
              <option key={name} value={name}>{name}</option>
            ))}
          </select>

          <input
            id="dashboard-bible-chapter"
            type="number"
            min={1}
            value={chapter}
            onChange={(event) => setChapter(Number.parseInt(event.target.value, 10) || 1)}
            className="h-9 w-16 rounded-[6px] border border-line bg-paper px-2 text-center text-xs font-semibold text-ink outline-none focus:border-accent"
            aria-label={t("dashboardBible.chapter", { defaultValue: "Chapter" })}
          />
          <input
            id="dashboard-bible-verse"
            type="number"
            min={1}
            value={verse}
            onChange={(event) => setVerse(Number.parseInt(event.target.value, 10) || 1)}
            className="h-9 w-16 rounded-[6px] border border-line bg-paper px-2 text-center text-xs font-semibold text-ink outline-none focus:border-accent"
            aria-label={t("dashboardBible.verse", { defaultValue: "Verse" })}
          />

          <ActionButton className="h-9 whitespace-nowrap px-3 text-xs" onClick={openTypedReference}>
            <Search className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
            {t("dashboardBible.open", { defaultValue: "Open" })}
          </ActionButton>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-3 border-b border-line bg-paper px-4 py-3">
        <button
          type="button"
          onClick={goPreviousChapter}
          className="inline-flex h-8 w-8 items-center justify-center rounded-[6px] border border-line bg-mist text-muted transition hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          aria-label={t("dashboardBible.previousChapter", { defaultValue: "Previous chapter" })}
        >
          <ChevronLeft className="h-4 w-4" aria-hidden="true" />
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2 text-xs font-semibold uppercase tracking-[0.12em] text-muted">
            <span>{activeCandidate?.reference ?? `${book} ${chapter}:${verse}`}</span>
            <span className="rounded-[6px] border border-line bg-mist px-2 py-0.5 text-[10px] text-muted">
              {translationId.toUpperCase()}
            </span>
            {hasCriticalReadinessIssue ? (
              <span className="rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-200">
                {t("dashboardBible.checkDiagnostics", { defaultValue: "Check transcript diagnostics" })}
              </span>
            ) : null}
          </div>
          <p className="mt-1 truncate text-xs text-muted" aria-live="polite">
            {activeCandidate?.text ||
              t("dashboardBible.speakOrNavigate", {
                defaultValue: "Speak a scripture command or navigate manually.",
              })}
          </p>
        </div>
        <button
          type="button"
          onClick={goNextChapter}
          className="inline-flex h-8 w-8 items-center justify-center rounded-[6px] border border-line bg-mist text-muted transition hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          aria-label={t("dashboardBible.nextChapter", { defaultValue: "Next chapter" })}
        >
          <ChevronRight className="h-4 w-4" aria-hidden="true" />
        </button>
        <div className="flex items-center gap-2 rounded-[6px] border border-line bg-mist px-2.5 py-1.5 text-xs font-semibold text-muted">
          <Mic className={cn("h-3.5 w-3.5", captureRunning ? "text-emerald-300" : captureStarting ? "text-amber-300" : "text-muted")} aria-hidden="true" />
          <span>
            {t("dashboardBible.verseCount", {
              count: verses.length,
              defaultValue: `${verses.length} verses`,
            })}
          </span>
        </div>
      </div>

      <div className="max-h-[calc(100vh-275px)] overflow-y-auto px-3 py-3 sm:px-4" aria-live="polite">
        {error ? (
          <div className="mb-4 rounded-[6px] border border-amber-500/30 bg-amber-500/10 p-4 text-sm leading-6 text-amber-200">
            {error}
          </div>
        ) : null}

        <div className="space-y-1">
          {verses.map((item) => {
            const active = item.verse === verse;
            return (
              <button
                type="button"
                key={item.reference}
                ref={active ? activeVerseRef : undefined}
                onClick={() => {
                  setVerse(item.verse);
                  onPreviewVerse?.(item.reference, item.text, item.translation);
                }}
                className={cn(
                  "grid w-full grid-cols-[42px_minmax(0,1fr)_28px] items-start gap-4 rounded-[6px] border px-4 py-3 text-left transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent",
                  active
                    ? "border-accent/50 bg-accent/10 shadow-[inset_3px_0_0_var(--c-accent)]"
                    : "border-transparent bg-transparent hover:border-line hover:bg-mist/45"
                )}
                aria-label={`Open ${item.reference}`}
              >
                <span className={cn(
                  "text-right font-mono text-sm font-semibold",
                  active ? "text-accent" : "text-muted"
                )}>
                  {item.verse}
                </span>
                <span className={cn(
                  "text-[16px] leading-8",
                  active ? "font-semibold text-ink" : "text-graphite"
                )}>
                  {item.text}
                </span>
                <ChevronRight className={cn(
                  "h-4 w-4 justify-self-end",
                  active ? "text-accent" : "text-muted/70"
                )} aria-hidden="true" />
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
});
