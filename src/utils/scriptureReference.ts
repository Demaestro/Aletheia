/**
 * scriptureReference.ts
 *
 * Pure TypeScript utility that mirrors the logic used in:
 *   - referenceToApiBibleId() in desktopApi.ts (TS frontend)
 *   - parse_reference() + canonical_book() in lib.rs (Rust backend)
 *
 * Extracted here so both the fetch helper and unit tests can import
 * the same map without duplicating the table or touching Tauri APIs.
 */

/**
 * Maps lower-cased book names (and common abbreviations) to their
 * three-letter uppercase API.Bible codes.
 */
export const BOOK_MAP: Readonly<Record<string, string>> = {
  // Pentateuch
  genesis: "GEN", gen: "GEN",
  exodus: "EXO", ex: "EXO", exo: "EXO", exod: "EXO",
  leviticus: "LEV", lev: "LEV",
  numbers: "NUM", num: "NUM",
  deuteronomy: "DEU", deut: "DEU", deu: "DEU", dt: "DEU",
  // Historical
  joshua: "JOS", josh: "JOS", jos: "JOS",
  judges: "JDG", judg: "JDG", jdg: "JDG",
  ruth: "RUT",
  "1 samuel": "1SA", "1sam": "1SA", "first samuel": "1SA",
  "2 samuel": "2SA", "2sam": "2SA", "second samuel": "2SA",
  "1 kings": "1KI", "1kgs": "1KI", "first kings": "1KI",
  "2 kings": "2KI", "2kgs": "2KI", "second kings": "2KI",
  "1 chronicles": "1CH", "1chr": "1CH", "first chronicles": "1CH",
  "2 chronicles": "2CH", "2chr": "2CH", "second chronicles": "2CH",
  ezra: "EZR",
  nehemiah: "NEH", neh: "NEH",
  esther: "EST", est: "EST",
  // Wisdom
  job: "JOB",
  psalms: "PSA", psalm: "PSA", ps: "PSA", psa: "PSA",
  proverbs: "PRO", prov: "PRO", pro: "PRO",
  ecclesiastes: "ECC", eccl: "ECC", ecc: "ECC",
  "song of solomon": "SNG", "song of songs": "SNG", song: "SNG", sos: "SNG",
  // Major prophets
  isaiah: "ISA", isa: "ISA", is: "ISA",
  jeremiah: "JER", jer: "JER",
  lamentations: "LAM", lam: "LAM",
  ezekiel: "EZK", ezek: "EZK", eze: "EZK",
  daniel: "DAN", dan: "DAN",
  // Minor prophets
  hosea: "HOS", hos: "HOS",
  joel: "JOL",
  amos: "AMO",
  obadiah: "OBA", obad: "OBA",
  jonah: "JON", jon: "JON",
  micah: "MIC", mic: "MIC",
  nahum: "NAM", nah: "NAM",
  habakkuk: "HAB", hab: "HAB",
  zephaniah: "ZEP", zeph: "ZEP", zep: "ZEP",
  haggai: "HAG", hag: "HAG",
  zechariah: "ZEC", zech: "ZEC", zec: "ZEC",
  malachi: "MAL", mal: "MAL",
  // Gospels & Acts
  matthew: "MAT", matt: "MAT", mat: "MAT", mt: "MAT",
  mark: "MRK", mrk: "MRK", mk: "MRK", mar: "MRK",
  luke: "LUK", luk: "LUK", lk: "LUK",
  john: "JHN", jhn: "JHN", jn: "JHN",
  acts: "ACT", act: "ACT",
  // Epistles
  romans: "ROM", rom: "ROM",
  "1 corinthians": "1CO", "1cor": "1CO", "1 cor": "1CO", "first corinthians": "1CO",
  "2 corinthians": "2CO", "2cor": "2CO", "2 cor": "2CO", "second corinthians": "2CO",
  galatians: "GAL", gal: "GAL",
  ephesians: "EPH", eph: "EPH",
  philippians: "PHP", phil: "PHP",
  colossians: "COL", col: "COL",
  "1 thessalonians": "1TH", "1th": "1TH",
  "2 thessalonians": "2TH", "2th": "2TH",
  "1 timothy": "1TI", "1ti": "1TI", "1tim": "1TI",
  "2 timothy": "2TI", "2ti": "2TI", "2tim": "2TI",
  titus: "TIT", tit: "TIT",
  philemon: "PHM", phlm: "PHM",
  hebrews: "HEB", heb: "HEB",
  james: "JAS", jas: "JAS",
  "1 peter": "1PE", "1pe": "1PE", "1pet": "1PE", "1 pet": "1PE",
  "2 peter": "2PE", "2pe": "2PE", "2pet": "2PE", "2 pet": "2PE",
  "1 john": "1JN", "1jn": "1JN", "first john": "1JN",
  "2 john": "2JN", "2jn": "2JN",
  "3 john": "3JN", "3jn": "3JN",
  jude: "JUD",
  revelation: "REV", rev: "REV", revelations: "REV",
};

const MAX_CHAPTER_BY_BOOK_CODE: Readonly<Record<string, number>> = {
  GEN: 50, EXO: 40, LEV: 27, NUM: 36, DEU: 34, JOS: 24, JDG: 21, RUT: 4,
  "1SA": 31, "2SA": 24, "1KI": 22, "2KI": 25, "1CH": 29, "2CH": 36,
  EZR: 10, NEH: 13, EST: 10, JOB: 42, PSA: 150, PRO: 31, ECC: 12, SNG: 8,
  ISA: 66, JER: 52, LAM: 5, EZK: 48, DAN: 12, HOS: 14, JOL: 3, AMO: 9,
  OBA: 1, JON: 4, MIC: 7, NAM: 3, HAB: 3, ZEP: 3, HAG: 2, ZEC: 14, MAL: 4,
  MAT: 28, MRK: 16, LUK: 24, JHN: 21, ACT: 28, ROM: 16, "1CO": 16, "2CO": 13,
  GAL: 6, EPH: 6, PHP: 4, COL: 4, "1TH": 5, "2TH": 3, "1TI": 6, "2TI": 4,
  TIT: 3, PHM: 1, HEB: 13, JAS: 5, "1PE": 5, "2PE": 3, "1JN": 5, "2JN": 1,
  "3JN": 1, JUD: 1, REV: 22,
};

export type ParsedReference = {
  bookCode: string;   // e.g. "HAB"
  chapter: number;
  verse: number;
  /** API.Bible passage ID format — e.g. "HAB.3.17" */
  apiBibleId: string;
};

/**
 * Parses a human-readable scripture reference (e.g. "Habakkuk 3:17") into
 * a structured object. Returns `null` for any input that cannot be matched.
 *
 * Accepted formats:
 *   - "Book Chapter:Verse"    — "Romans 8:28"
 *   - "Book Chapter Verse"    — "Matthew 11 12"
 *   - "Book JoinedDigits"     — "John 316", "Acts 1214"
 *   - "Book Chapter.Verse"    — "Romans 8.28"
 *   - "Book Chapter,Verse"    — "Romans 8,28"
 *   - Numbered books          — "1 Corinthians 13:4"
 *   - Abbreviated books       — "Hab 3:17", "Ps 23:1", "1 Cor 13:4"
 */
export function parseScriptureReference(input: string): ParsedReference | null {
  if (!input || typeof input !== "string") return null;

  let normalized = input
    .trim()
    .replace(/[.,]/g, ":") // normalise delimiters
    .replace(/\s{2,}/g, " ");

  normalized = normalizeJoinedChapterVerse(normalized);

  // Match: everything before the last "digit:digit" — allowing numbered books
  const m = normalized.match(/^(.+?)\s+(\d+)(?::|\s+)(\d+)$/i);
  if (!m) return null;

  const [, bookRaw, chapterStr, verseStr] = m;
  const bookKey = bookRaw.toLowerCase().trim();
  const bookCode = BOOK_MAP[bookKey];
  if (!bookCode) return null;

  const chapter = parseInt(chapterStr, 10);
  const verse = parseInt(verseStr, 10);
  if (isNaN(chapter) || isNaN(verse) || chapter < 1 || verse < 1) return null;

  return {
    bookCode,
    chapter,
    verse,
    apiBibleId: `${bookCode}.${chapter}.${verse}`,
  };
}

function normalizeJoinedChapterVerse(input: string): string {
  const match = input.match(/^(.+?)\s+(\d{2,4})$/i);
  if (!match) return input;

  const [, bookRaw, digits] = match;
  const bookCode = BOOK_MAP[bookRaw.toLowerCase().trim()];
  if (!bookCode) return input;
  const maxChapter = MAX_CHAPTER_BY_BOOK_CODE[bookCode] ?? 150;
  const chapterOnly = Number.parseInt(digits, 10);
  if (chapterOnly <= maxChapter) return input;

  const splitPoints = digits.length === 2
    ? [1]
    : digits.length === 3
      ? [2, 1]
      : [2, 1];

  for (const splitAt of splitPoints) {
    const chapter = Number.parseInt(digits.slice(0, splitAt), 10);
    const verse = Number.parseInt(digits.slice(splitAt), 10);
    if (chapter > 0 && chapter <= maxChapter && verse > 0) {
      return `${bookRaw} ${chapter}:${verse}`;
    }
  }

  return input;
}
