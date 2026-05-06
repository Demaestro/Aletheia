/**
 * BibleReaderPanel — offline Bible reader for Aletheia.
 *
 * Features:
 *  - Translation tabs (KJV, NKJV, NLT, MSG + any imported translation)
 *  - Book list (OT / NT) with live filter
 *  - Chapter navigation with prev / next / picker
 *  - Verse display with per-verse hover actions → preview or send live
 *  - Quick-reference jump input ("Jn 3:16", "Rom 8 28", etc.)
 *  - Native file-picker Bible import (thiagobodruk JSON or Beblia XML schema)
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import {
  AlertCircle,
  BookMarked,
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Download,
  Search,
  Send,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import {
  getBibleChapter,
  importBibleTranslation,
  isTauriRuntime,
  listBibleTranslations,
  type BibleTranslationStatus,
  type BibleVerse,
} from "../services/desktopApi";

// ---------------------------------------------------------------------------
// Canon
// ---------------------------------------------------------------------------

const OT_BOOKS = [
  "Genesis","Exodus","Leviticus","Numbers","Deuteronomy",
  "Joshua","Judges","Ruth","1 Samuel","2 Samuel",
  "1 Kings","2 Kings","1 Chronicles","2 Chronicles",
  "Ezra","Nehemiah","Esther","Job","Psalm","Proverbs",
  "Ecclesiastes","Song of Solomon","Isaiah","Jeremiah",
  "Lamentations","Ezekiel","Daniel","Hosea","Joel",
  "Amos","Obadiah","Jonah","Micah","Nahum","Habakkuk",
  "Zephaniah","Haggai","Zechariah","Malachi",
];

const NT_BOOKS = [
  "Matthew","Mark","Luke","John","Acts",
  "Romans","1 Corinthians","2 Corinthians","Galatians",
  "Ephesians","Philippians","Colossians","1 Thessalonians",
  "2 Thessalonians","1 Timothy","2 Timothy","Titus",
  "Philemon","Hebrews","James","1 Peter","2 Peter",
  "1 John","2 John","3 John","Jude","Revelation",
];

const ALL_BOOKS = [...OT_BOOKS, ...NT_BOOKS];

const CHAPTER_COUNTS: number[] = [
  50,40,27,36,34,24,21,4,31,24,22,25,29,36,10,13,10,42,150,31,
  12,8,66,52,5,48,12,14,3,9,1,4,7,3,3,3,2,14,4,
  28,16,24,21,28,16,16,13,6,6,4,4,5,3,6,4,3,1,13,5,5,3,5,1,1,1,22,
];

function maxChapter(book: string): number {
  const idx = ALL_BOOKS.indexOf(book);
  return idx >= 0 ? (CHAPTER_COUNTS[idx] ?? 1) : 1;
}

// ---------------------------------------------------------------------------
// Translation labels
// ---------------------------------------------------------------------------

const TRANSLATION_LABELS: Record<string, string> = {
  kjv:  "King James Version",
  nkjv: "New King James Version",
  nlt:  "New Living Translation",
  msg:  "The Message",
  gnb:  "Good News Bible / Good News Translation",
  esv:  "English Standard Version",
  niv:  "New International Version",
  csb:  "Christian Standard Bible",
  amp:  "Amplified Bible",
  tlb:  "The Living Bible",
};

const PREFERRED = ["kjv", "web", "bbe", "nkjv", "nlt", "gnb", "esv", "niv", "csb", "amp", "tlb", "msg"];

const BEBLIA_FILENAME_IDS: Record<string, string> = {
  englishnkjbible: "nkjv",
  englishnltbible: "nlt",
  englishgntbible: "gnb",
  englishgnbbible: "gnb",
  englishesvbible: "esv",
  englishnivbible: "niv",
  englishcsbbible: "csb",
  englishamplifiedbible: "amp",
  englishtlbible: "tlb",
};

function deriveTranslationId(filename: string): string {
  const base = filename.replace(/\.(json|xml)$/i, "").toLowerCase().trim();
  return BEBLIA_FILENAME_IDS[base] ?? base;
}

// ---------------------------------------------------------------------------
// Quick-reference parser ("Jn 3 16", "Romans 8:28")
// ---------------------------------------------------------------------------

const BOOK_ALIASES: Record<string, string> = {
  gen:"Genesis",exo:"Exodus",lev:"Leviticus",num:"Numbers",
  deu:"Deuteronomy",deut:"Deuteronomy",josh:"Joshua",jdg:"Judges",
  jud:"Judges",ruth:"Ruth","1sa":"1 Samuel","1sm":"1 Samuel",
  "2sa":"2 Samuel","2sm":"2 Samuel","1ki":"1 Kings","1kgs":"1 Kings",
  "2ki":"2 Kings","2kgs":"2 Kings","1ch":"1 Chronicles","1chr":"1 Chronicles",
  "2ch":"2 Chronicles","2chr":"2 Chronicles",ezra:"Ezra",neh:"Nehemiah",
  est:"Esther",esth:"Esther",job:"Job",ps:"Psalm",psa:"Psalm",
  psalm:"Psalm",psalms:"Psalm",pro:"Proverbs",prov:"Proverbs",
  ecc:"Ecclesiastes",eccl:"Ecclesiastes",song:"Song of Solomon",
  sos:"Song of Solomon",isa:"Isaiah",jer:"Jeremiah",lam:"Lamentations",
  eze:"Ezekiel",ezek:"Ezekiel",dan:"Daniel",hos:"Hosea",joel:"Joel",
  amo:"Amos",oba:"Obadiah",jon:"Jonah",mic:"Micah",nah:"Nahum",
  hab:"Habakkuk",zep:"Zephaniah",zeph:"Zephaniah",hag:"Haggai",
  zec:"Zechariah",zech:"Zechariah",mal:"Malachi",
  mat:"Matthew",matt:"Matthew",mk:"Mark",mar:"Mark",luk:"Luke",
  jn:"John",jhn:"John",act:"Acts",acts:"Acts",rom:"Romans",
  "1co":"1 Corinthians","1cor":"1 Corinthians","2co":"2 Corinthians",
  "2cor":"2 Corinthians",gal:"Galatians",eph:"Ephesians",
  phi:"Philippians",php:"Philippians",phil:"Philippians",col:"Colossians",
  "1th":"1 Thessalonians","1thes":"1 Thessalonians","2th":"2 Thessalonians",
  "2thes":"2 Thessalonians","1ti":"1 Timothy","1tim":"1 Timothy",
  "2ti":"2 Timothy","2tim":"2 Timothy",tit:"Titus",phm:"Philemon",
  heb:"Hebrews",jas:"James",jam:"James","1pe":"1 Peter","1pet":"1 Peter",
  "2pe":"2 Peter","2pet":"2 Peter","1jn":"1 John","1jo":"1 John",
  "2jn":"2 John","2jo":"2 John","3jn":"3 John","3jo":"3 John",
  jude:"Jude",rev:"Revelation",
};

interface ParsedRef { book: string; chapter: number; verse?: number }

function parseQuickRef(raw: string): ParsedRef | null {
  const s = raw.trim();
  const m = s.match(/^(\d?\s*[a-zA-Z]+(?:\s+[a-zA-Z]+)?)\s+(\d+)(?:[:\s]+(\d+))?$/);
  if (!m) return null;
  const [, bookRaw, chapterStr, verseStr] = m;
  const bookKey = bookRaw.toLowerCase().replace(/\s+/g, "");
  const resolved =
    BOOK_ALIASES[bookKey] ??
    ALL_BOOKS.find((b) => b.toLowerCase().startsWith(bookRaw.toLowerCase().trim()));
  if (!resolved) return null;
  const chapter = parseInt(chapterStr, 10);
  if (isNaN(chapter) || chapter < 1) return null;
  return {
    book: resolved,
    chapter: Math.min(chapter, maxChapter(resolved)),
    verse: verseStr ? parseInt(verseStr, 10) : undefined,
  };
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface BibleReaderPanelProps {
  initialTranslation?: string;
  initialBook?: string;
  initialChapter?: number;
  onPreviewVerse?: (reference: string, text: string, translation: string) => void;
  onLiveVerse?: (reference: string, text: string, translation: string) => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function BibleReaderPanel({
  initialTranslation = "kjv",
  initialBook = "John",
  initialChapter = 3,
  onPreviewVerse,
  onLiveVerse,
}: BibleReaderPanelProps) {
  const [translations, setTranslations] = useState<BibleTranslationStatus[]>([]);
  const [activeTranslation, setActiveTranslation] = useState(initialTranslation);
  const [translationsLoading, setTranslationsLoading] = useState(true);
  const [, startTranslationTransition] = useTransition();
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  const [book, setBook] = useState(initialBook);
  const [chapter, setChapter] = useState(initialChapter);
  const [verses, setVerses] = useState<BibleVerse[]>([]);
  const [loadingVerses, setLoadingVerses] = useState(false);
  const [verseError, setVerseError] = useState<string | null>(null);

  const [bookFilter, setBookFilter] = useState("");
  const [quickRef, setQuickRef] = useState("");
  const [highlightVerse, setHighlightVerse] = useState<number | null>(null);
  const [fontSize, setFontSize] = useState(16);
  const [hoveredVerse, setHoveredVerse] = useState<number | null>(null);

  const verseEls = useRef<Record<number, HTMLElement | null>>({});
  const chapterViewRef = useRef<HTMLDivElement>(null);
  const chapterRequestSeqRef = useRef(0);

  // Load translations on mount
  useEffect(() => {
    setTranslationsLoading(true);
    listBibleTranslations()
      .then((list) => {
        setTranslations(list);
        const available = list.filter((t) => t.versesLoaded > 0 && t.fullCanon);
        if (available.length > 0) {
          const preferred = available.find(
            (t) => t.id.toLowerCase() === initialTranslation.toLowerCase()
          );
          setActiveTranslation(preferred?.id ?? available[0].id);
        }
      })
      .catch(console.warn)
      .finally(() => setTranslationsLoading(false));
  }, [initialTranslation]);

  // Load chapter
  const loadChapter = useCallback(
    (tid: string, bk: string, ch: number) => {
      const requestSeq = ++chapterRequestSeqRef.current;
      setLoadingVerses(true);
      setVerseError(null);
      setHighlightVerse(null);
      getBibleChapter(tid, bk, ch)
        .then((data) => {
          if (requestSeq !== chapterRequestSeqRef.current) return;
          setVerses(data);
          if (data.length === 0)
            setVerseError(
              `No verses found for ${bk} ${ch} in ${tid.toUpperCase()}. Import this translation to read offline.`
            );
          chapterViewRef.current?.scrollTo({ top: 0, behavior: "auto" });
        })
        .catch((err: unknown) => {
          if (requestSeq !== chapterRequestSeqRef.current) return;
          setVerses([]);
          setVerseError(err instanceof Error ? err.message : "Failed to load chapter.");
        })
        .finally(() => {
          if (requestSeq === chapterRequestSeqRef.current) setLoadingVerses(false);
        });
    },
    []
  );

  useEffect(() => {
    if (translations.length === 0 && translationsLoading) return;
    const timer = window.setTimeout(() => loadChapter(activeTranslation, book, chapter), 100);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTranslation, book, chapter]);

  // Chapter navigation
  const goPrev = () => {
    if (chapter > 1) { setChapter((c) => c - 1); return; }
    const idx = ALL_BOOKS.indexOf(book);
    if (idx > 0) { const b = ALL_BOOKS[idx - 1]; setBook(b); setChapter(maxChapter(b)); }
  };
  const goNext = () => {
    if (chapter < maxChapter(book)) { setChapter((c) => c + 1); return; }
    const idx = ALL_BOOKS.indexOf(book);
    if (idx < ALL_BOOKS.length - 1) { setBook(ALL_BOOKS[idx + 1]); setChapter(1); }
  };

  // Quick-reference jump
  const jumpToRef = () => {
    const parsed = parseQuickRef(quickRef);
    if (!parsed) {
      setImportStatus(`Could not parse "${quickRef}". Try "John 3:16" or "Ps 23".`);
      return;
    }
    setBook(parsed.book);
    setChapter(parsed.chapter);
    if (parsed.verse) {
      setTimeout(() => {
        setHighlightVerse(parsed.verse!);
        verseEls.current[parsed.verse!]?.scrollIntoView({ behavior: "smooth", block: "center" });
      }, 600);
    }
    setQuickRef("");
    setImportStatus(null);
  };

  // Filtered books
  const filteredOT = useMemo(
    () => OT_BOOKS.filter((b) => b.toLowerCase().includes(bookFilter.toLowerCase())),
    [bookFilter]
  );
  const filteredNT = useMemo(
    () => NT_BOOKS.filter((b) => b.toLowerCase().includes(bookFilter.toLowerCase())),
    [bookFilter]
  );

  // Import translation using a hidden <input type="file"> (plugin-dialog not available)
  const fileInputRef = useRef<HTMLInputElement>(null);
  const pendingFileResolve = useRef<((path: string) => void) | null>(null);

  const pickFile = (): Promise<string | null> =>
    new Promise((resolve) => {
      pendingFileResolve.current = resolve;
      fileInputRef.current?.click();
    });

  const onFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) { pendingFileResolve.current?.(null as unknown as string); return; }
    // In Tauri the file object has a path property (webkitRelativePath or a Tauri-specific one)
    // Use the name as the translation id derivation; the actual path comes from the File object.
    // Tauri v2 exposes the native FS path via (file as any).path
    const nativePath = (file as unknown as { path?: string }).path ?? file.name;
    pendingFileResolve.current?.(nativePath);
    e.target.value = "";
  };

  const handleImport = async () => {
    if (!isTauriRuntime()) {
      setImportStatus("Bible import requires the desktop runtime (Tauri).");
      return;
    }
    try {
      const selected = await pickFile();
      if (!selected) return;

      const filename = selected.split(/[\\/]/).pop() ?? selected;
      const derivedId = deriveTranslationId(filename);
      const label = TRANSLATION_LABELS[derivedId] ?? derivedId.toUpperCase();

      setImporting(true);
      setImportStatus(`Importing ${label}… this may take up to 60 seconds.`);

      const result = await importBibleTranslation(derivedId, label, "Licensed", selected);
      setImportStatus(`✓ ${label} imported — ${result.versesInserted.toLocaleString()} verses.`);
      setActiveTranslation(derivedId);
      const list = await listBibleTranslations();
      setTranslations(list);
    } catch (err) {
      setImportStatus(err instanceof Error ? err.message : "Import failed.");
    } finally {
      setImporting(false);
    }
  };

  const loadedTranslations = useMemo(() => {
    return translations
      .filter((t) => t.versesLoaded > 0 && t.fullCanon)
      .sort((a, b) => {
        if (a.fullCanon !== b.fullCanon) return a.fullCanon ? -1 : 1;
        return a.id.localeCompare(b.id);
      });
  }, [translations]);
  const sampleTranslations = useMemo(
    () =>
      translations
        .filter((t) => t.versesLoaded > 0 && !t.fullCanon)
        .sort((a, b) => a.id.localeCompare(b.id)),
    [translations]
  );
  const missingPreferred = PREFERRED.filter(
    (id) => !loadedTranslations.some((t) => t.id.toLowerCase() === id)
  );

  // ── Render ──────────────────────────────────────────────────────────────
  return (
    <>
      {/* Hidden file input for Bible JSON/XML import */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".json,.xml,application/json,application/xml,text/xml"
        className="hidden"
        aria-hidden="true"
        onChange={onFileInputChange}
      />

    <div className="flex flex-col h-full overflow-hidden bg-transparent">

      {/* ── Header ───────────────────────────────────────────────────────── */}
      <div className="flex items-center gap-3 px-5 py-3 border-b border-[var(--c-line)] flex-wrap gap-y-2">
        {/* Title */}
        <div className="flex items-center gap-2 min-w-0">
          <BookOpen size={17} className="text-violet-400 shrink-0" />
          <span className="font-semibold text-[var(--c-ink)] text-sm whitespace-nowrap">Scripture Library</span>
          <span className="text-xs text-[var(--c-muted)] hidden sm:block">· Offline</span>
        </div>

        {/* Quick reference */}
        <div className="flex items-center gap-2 flex-1 min-w-[200px] max-w-xs ml-auto">
          <div className="relative flex-1">
            <Search size={13} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--c-muted)]" />
            <input
              id="bible-quickref"
              className="w-full bg-[var(--c-mist)] border border-[var(--c-line)] rounded-lg pl-8 pr-3 py-1.5 text-sm text-[var(--c-ink)] placeholder-[var(--c-muted)] focus:outline-none focus:border-violet-500 transition"
              placeholder="Jump to… e.g. John 3:16"
              value={quickRef}
              onChange={(e) => setQuickRef(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") jumpToRef(); }}
              aria-label="Quick reference jump"
            />
          </div>
          <button
            onClick={jumpToRef}
            className="px-3 py-1.5 bg-violet-600 hover:bg-violet-700 text-white text-xs font-medium rounded-lg transition whitespace-nowrap"
          >
            Go
          </button>
        </div>

        {/* Font size */}
        <div className="flex items-center gap-1.5 text-[var(--c-muted)]">
          <button onClick={() => setFontSize((s) => Math.max(12, s - 2))} className="hover:text-[var(--c-ink)] transition" aria-label="Decrease font size">
            <ZoomOut size={15} />
          </button>
          <span className="text-xs w-8 text-center tabular-nums">{fontSize}px</span>
          <button onClick={() => setFontSize((s) => Math.min(28, s + 2))} className="hover:text-[var(--c-ink)] transition" aria-label="Increase font size">
            <ZoomIn size={15} />
          </button>
        </div>
      </div>

      {/* ── Status / import notice ────────────────────────────────────────── */}
      {importStatus && (
        <div className={`flex items-center gap-2 px-5 py-2 text-xs border-b border-[var(--c-line)] ${
          importStatus.startsWith("✓")
            ? "bg-emerald-950/40 text-emerald-300 border-emerald-800/30"
            : "bg-[var(--c-mist)] text-[var(--c-graphite)]"
        }`}>
          <span className="flex-1">{importStatus}</span>
          <button onClick={() => setImportStatus(null)} className="shrink-0 hover:text-[var(--c-ink)] transition" aria-label="Dismiss">
            <X size={13} />
          </button>
        </div>
      )}

      {/* ── Missing translations notice ───────────────────────────────────── */}
      {missingPreferred.length > 0 && !importStatus && (
        <div className="flex items-center gap-2 px-5 py-2 text-xs bg-amber-950/30 text-amber-300 border-b border-amber-800/20">
          <AlertCircle size={13} className="shrink-0" />
          <span className="flex-1">
            {missingPreferred.map((id) => id.toUpperCase()).join(", ")} not imported.
            Import a Bible JSON to read offline.
          </span>
          <button
            onClick={handleImport}
            disabled={importing}
            className="flex items-center gap-1 px-2 py-0.5 rounded bg-amber-900/50 hover:bg-amber-800/50 transition disabled:opacity-50"
          >
            <Download size={11} /> Import
          </button>
        </div>
      )}

      {/* ── Translation tabs row ─────────────────────────────────────────── */}
      <div className="flex items-center gap-2 px-5 py-2 border-b border-[var(--c-line)] overflow-x-auto scrollbar-none">
        {translationsLoading ? (
          <span className="text-xs text-[var(--c-muted)] animate-pulse">Loading translations…</span>
        ) : loadedTranslations.length === 0 ? (
          <span className="text-xs text-[var(--c-muted)]">No translations imported yet</span>
        ) : (
          loadedTranslations.map((t) => {
            const active = activeTranslation.toLowerCase() === t.id.toLowerCase();
            return (
              <button
                key={t.id}
                role="tab"
                aria-selected={active}
                onClick={() => {
                  startTranslationTransition(() => setActiveTranslation(t.id));
                }}
                className={`flex items-center gap-1.5 shrink-0 px-3 py-1 rounded-lg text-xs font-medium transition ${
                  active
                    ? "bg-violet-600 text-white shadow-md shadow-violet-900/40"
                    : "bg-[var(--c-mist)] text-[var(--c-graphite)] hover:text-[var(--c-ink)] border border-[var(--c-line)]"
                }`}
              >
                {t.id.toUpperCase()}
                <span className={`text-[10px] ${active ? "text-violet-200" : "text-[var(--c-muted)]"}`}>
                  {(t.versesLoaded / 1000).toFixed(0)}k
                </span>
              </button>
            );
          })
        )}
        <button
          onClick={handleImport}
          disabled={importing}
          className="ml-auto flex items-center gap-1 shrink-0 px-3 py-1 text-xs text-[var(--c-muted)] hover:text-[var(--c-ink)] border border-[var(--c-line)] rounded-lg transition disabled:opacity-50 bg-[var(--c-mist)]"
        >
          <Download size={12} />
          {importing ? "Importing…" : "Import"}
        </button>
      </div>
      {!translationsLoading && sampleTranslations.length > 0 ? (
        <div className="border-b border-[var(--c-line)] px-5 py-2 text-[11px] text-[var(--c-muted)]">
          Sample packs available for phrase detection only:{" "}
          {sampleTranslations.map((translation) => translation.id.toUpperCase()).join(", ")}.
          Import the full Bible before reading chapters in them.
        </div>
      ) : null}

      {/* ── Body ─────────────────────────────────────────────────────────── */}
      <div className="flex flex-1 min-h-0 overflow-hidden">

        {/* Book sidebar */}
        <div className="w-44 shrink-0 border-r border-[var(--c-line)] flex flex-col bg-[var(--c-mist)]/40">
          {/* Filter */}
          <div className="px-3 pt-3 pb-2">
            <div className="relative">
              <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--c-muted)]" />
              <input
                className="w-full bg-[var(--c-mist)] border border-[var(--c-line)] rounded-md pl-7 pr-2 py-1 text-xs text-[var(--c-ink)] placeholder-[var(--c-muted)] focus:outline-none focus:border-violet-500 transition"
                placeholder="Filter books…"
                value={bookFilter}
                onChange={(e) => setBookFilter(e.target.value)}
                aria-label="Filter book list"
              />
            </div>
          </div>

          {/* Book list */}
          <div className="flex-1 overflow-y-auto px-1 pb-3 text-xs">
            {filteredOT.length > 0 && (
              <div className="px-2 py-1.5 text-[10px] font-semibold tracking-widest uppercase text-[var(--c-muted)]">
                Old Testament
              </div>
            )}
            {filteredOT.map((b) => (
              <button
                key={b}
                onClick={() => { setBook(b); setChapter(1); }}
                className={`w-full text-left px-2 py-1 rounded-md truncate transition ${
                  book === b
                    ? "bg-violet-600/20 text-violet-300 font-medium"
                    : "text-[var(--c-graphite)] hover:bg-[var(--c-line)] hover:text-[var(--c-ink)]"
                }`}
                title={b}
              >
                {b}
              </button>
            ))}
            {filteredNT.length > 0 && (
              <div className="px-2 pt-3 pb-1.5 text-[10px] font-semibold tracking-widest uppercase text-[var(--c-muted)]">
                New Testament
              </div>
            )}
            {filteredNT.map((b) => (
              <button
                key={b}
                onClick={() => { setBook(b); setChapter(1); }}
                className={`w-full text-left px-2 py-1 rounded-md truncate transition ${
                  book === b
                    ? "bg-violet-600/20 text-violet-300 font-medium"
                    : "text-[var(--c-graphite)] hover:bg-[var(--c-line)] hover:text-[var(--c-ink)]"
                }`}
                title={b}
              >
                {b}
              </button>
            ))}
            {filteredOT.length === 0 && filteredNT.length === 0 && (
              <p className="px-3 py-4 text-[var(--c-muted)] text-[11px]">
                No books match "{bookFilter}"
              </p>
            )}
          </div>
        </div>

        {/* Chapter column */}
        <div className="flex flex-col flex-1 min-w-0 overflow-hidden">

          {/* Chapter nav bar */}
          <div className="flex items-center gap-3 px-4 py-2.5 border-b border-[var(--c-line)] bg-[var(--c-mist)]/20">
            <button
              onClick={goPrev}
              className="p-1.5 rounded-lg text-[var(--c-muted)] hover:text-[var(--c-ink)] hover:bg-[var(--c-line)] transition"
              aria-label="Previous chapter"
            >
              <ChevronLeft size={17} />
            </button>

            <div className="flex items-center gap-2 flex-1 justify-center">
              <div className="flex items-center gap-1.5 font-semibold text-[var(--c-ink)] text-sm">
                <BookMarked size={14} className="text-violet-400" />
                {book}
              </div>

              <select
                id="bible-chapter-select"
                value={chapter}
                onChange={(e) => setChapter(parseInt(e.target.value, 10))}
                aria-label="Select chapter"
                className="bg-[var(--c-mist)] border border-[var(--c-line)] text-[var(--c-ink)] text-xs rounded-lg px-2 py-1 focus:outline-none focus:border-violet-500 transition cursor-pointer"
              >
                {Array.from({ length: maxChapter(book) }, (_, i) => i + 1).map((c) => (
                  <option key={c} value={c}>Ch. {c}</option>
                ))}
              </select>
            </div>

            <button
              onClick={goNext}
              className="p-1.5 rounded-lg text-[var(--c-muted)] hover:text-[var(--c-ink)] hover:bg-[var(--c-line)] transition"
              aria-label="Next chapter"
            >
              <ChevronRight size={17} />
            </button>
          </div>

          {/* Scrollable verse area */}
          <div
            ref={chapterViewRef}
            className="flex-1 overflow-y-auto px-6 py-4"
            style={{ fontSize: `${fontSize}px` }}
          >
            {loadingVerses ? (
              <div className="flex flex-col items-center justify-center h-48 gap-3 text-[var(--c-muted)]">
                <div className="w-7 h-7 rounded-full border-2 border-violet-500 border-t-transparent animate-spin" />
                <span className="text-sm">Loading {book} {chapter}…</span>
              </div>
            ) : verseError ? (
              <div className="flex flex-col items-center justify-center h-48 gap-4 text-[var(--c-muted)] text-center px-6">
                <AlertCircle size={28} className="text-amber-400" />
                <p className="text-sm leading-relaxed">{verseError}</p>
                <button
                  onClick={handleImport}
                  disabled={importing}
                  className="flex items-center gap-2 px-4 py-2 bg-violet-600 hover:bg-violet-700 text-white text-sm rounded-lg transition disabled:opacity-50"
                >
                  <Download size={14} /> Import Translation
                </button>
              </div>
            ) : (
              <div>
                {/* Chapter heading */}
                <h2 className="mb-4 flex items-center gap-3 border-b border-[var(--c-line)] pb-3 text-lg font-semibold tracking-tight text-[var(--c-ink)]">
                  {book} {chapter}
                  <span className="ml-auto rounded-[6px] border border-[var(--c-line)] bg-[var(--c-mist)] px-2 py-1 text-xs font-semibold text-[var(--c-muted)]">
                    {activeTranslation.toUpperCase()}
                  </span>
                </h2>

                {/* Verses */}
                {verses.map((v) => (
                  <div
                    key={v.verse}
                    ref={(el) => { verseEls.current[v.verse] = el; }}
                    onMouseEnter={() => setHoveredVerse(v.verse)}
                    onMouseLeave={() => setHoveredVerse(null)}
                    className={`group relative -mx-1 mb-1 grid grid-cols-[42px_minmax(0,1fr)_auto] items-center gap-3 rounded-[6px] border px-3 py-3 leading-relaxed transition-colors ${
                      highlightVerse === v.verse
                        ? "border-amber-500/50 bg-amber-950/35 shadow-[inset_3px_0_0_rgba(245,158,11,0.85)]"
                        : "border-transparent bg-[var(--c-mist)]/20 hover:border-[var(--c-line)] hover:bg-[var(--c-mist)]/45"
                    }`}
                  >
                    {/* Verse number */}
                    <span
                      className={`select-none text-right font-mono font-semibold ${
                        highlightVerse === v.verse ? "text-amber-200" : "text-[var(--c-muted)]"
                      }`}
                      style={{ fontSize: `${Math.max(11, fontSize - 4)}px` }}
                    >
                      {v.verse}
                    </span>

                    {/* Verse text */}
                    <span
                      className={`pr-24 ${
                        highlightVerse === v.verse ? "font-semibold text-[var(--c-ink)]" : "text-[var(--c-graphite)]"
                      }`}
                      style={{ lineHeight: "1.7" }}
                    >
                      {v.text}
                    </span>

                    {/* Hover actions */}
                    {hoveredVerse === v.verse && (
                      <div className="absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-1.5 animate-fade-in">
                        <button
                          onClick={() => onPreviewVerse?.(v.reference, v.text, v.translation)}
                          title={`Preview ${v.reference}`}
                          className="flex items-center gap-1 whitespace-nowrap rounded-[6px] border border-[var(--c-line)] bg-[#111113] px-2 py-1 text-[11px] text-slate-100 transition hover:bg-[#18181b]"
                        >
                          <BookOpen size={11} /> Preview
                        </button>
                        <button
                          onClick={() => onLiveVerse?.(v.reference, v.text, v.translation)}
                          title={`Send ${v.reference} live`}
                          className="flex items-center gap-1 whitespace-nowrap rounded-[6px] bg-amber-600 px-2 py-1 text-[11px] text-white transition hover:bg-amber-500"
                        >
                          <Send size={11} /> Live
                        </button>
                      </div>
                    )}
                  </div>
                ))}

                {verses.length === 0 && !verseError && (
                  <div className="flex flex-col items-center gap-2 py-12 text-[var(--c-muted)] text-sm">
                    <AlertCircle size={20} />
                    <span>No verses loaded</span>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Footer bar */}
          <div className="flex items-center gap-3 px-4 py-2 border-t border-[var(--c-line)] text-xs text-[var(--c-muted)] bg-[var(--c-mist)]/20">
            <button onClick={goPrev} className="flex items-center gap-1 hover:text-[var(--c-ink)] transition">
              <ChevronLeft size={13} /> Prev
            </button>
            <span className="flex-1 text-center tabular-nums">
              {book} {chapter} · {activeTranslation.toUpperCase()}
              {verses.length > 0 ? ` · ${verses.length} verses` : ""}
            </span>
            <button onClick={goNext} className="flex items-center gap-1 hover:text-[var(--c-ink)] transition">
              Next <ChevronRight size={13} />
            </button>
          </div>
        </div>
      </div>
    </div>
    </>
  );
}
