export type ParsedScriptureCommand = {
  book: string;
  chapter: number;
  verse: number;
  explicitVerse: boolean;
};

export type ScriptureCommandKind =
  | "explicitReference"
  | "chapterReference"
  | "contextualFollowUp"
  | "none";

type BookAliasMatch = {
  alias: string;
  book: string;
  start: number;
  end: number;
};

const BOOK_ALIAS_ROWS = [
  ["song of solomon", "Song of Solomon"],
  ["song of songs", "Song of Solomon"],
  ["1 thessalonians", "1 Thessalonians"],
  ["first thessalonians", "1 Thessalonians"],
  ["2 thessalonians", "2 Thessalonians"],
  ["second thessalonians", "2 Thessalonians"],
  ["1 chronicles", "1 Chronicles"],
  ["first chronicles", "1 Chronicles"],
  ["2 chronicles", "2 Chronicles"],
  ["second chronicles", "2 Chronicles"],
  ["1 corinthians", "1 Corinthians"],
  ["first corinthians", "1 Corinthians"],
  ["2 corinthians", "2 Corinthians"],
  ["second corinthians", "2 Corinthians"],
  ["1 timothy", "1 Timothy"],
  ["first timothy", "1 Timothy"],
  ["2 timothy", "2 Timothy"],
  ["second timothy", "2 Timothy"],
  ["1 samuel", "1 Samuel"],
  ["first samuel", "1 Samuel"],
  ["2 samuel", "2 Samuel"],
  ["second samuel", "2 Samuel"],
  ["1 kings", "1 Kings"],
  ["first kings", "1 Kings"],
  ["2 kings", "2 Kings"],
  ["second kings", "2 Kings"],
  ["1 peter", "1 Peter"],
  ["first peter", "1 Peter"],
  ["2 peter", "2 Peter"],
  ["second peter", "2 Peter"],
  ["1 john", "1 John"],
  ["first john", "1 John"],
  ["2 john", "2 John"],
  ["second john", "2 John"],
  ["3 john", "3 John"],
  ["third john", "3 John"],
  ["genesis", "Genesis"],
  ["exodus", "Exodus"],
  ["leviticus", "Leviticus"],
  ["levitikus", "Leviticus"],
  ["numbers", "Numbers"],
  ["deuteronomy", "Deuteronomy"],
  ["deutronomy", "Deuteronomy"],
  ["joshua", "Joshua"],
  ["judges", "Judges"],
  ["ruth", "Ruth"],
  ["ezra", "Ezra"],
  ["nehemiah", "Nehemiah"],
  ["esther", "Esther"],
  ["job", "Job"],
  ["psalms", "Psalm"],
  ["psalm", "Psalm"],
  ["proverbs", "Proverbs"],
  ["ecclesiastes", "Ecclesiastes"],
  ["isaiah", "Isaiah"],
  ["jeremiah", "Jeremiah"],
  ["lamentations", "Lamentations"],
  ["ezekiel", "Ezekiel"],
  ["ezikel", "Ezekiel"],
  ["eziekiel", "Ezekiel"],
  ["ezekia", "Ezekiel"],
  ["ezekel", "Ezekiel"],
  ["daniel", "Daniel"],
  ["hosea", "Hosea"],
  ["joel", "Joel"],
  ["amos", "Amos"],
  ["obadiah", "Obadiah"],
  ["jonah", "Jonah"],
  ["micah", "Micah"],
  ["nahum", "Nahum"],
  ["habakkuk", "Habakkuk"],
  ["habbakuk", "Habakkuk"],
  ["habakuk", "Habakkuk"],
  ["zephaniah", "Zephaniah"],
  ["haggai", "Haggai"],
  ["zechariah", "Zechariah"],
  ["malachi", "Malachi"],
  ["matthew", "Matthew"],
  ["mathew", "Matthew"],
  ["matu", "Matthew"],
  ["matiu", "Matthew"],
  ["mathiew", "Matthew"],
  ["mark", "Mark"],
  ["luke", "Luke"],
  ["john", "John"],
  ["acts", "Acts"],
  ["romans", "Romans"],
  ["galatians", "Galatians"],
  ["ephesians", "Ephesians"],
  ["philippians", "Philippians"],
  ["colossians", "Colossians"],
  ["titus", "Titus"],
  ["philemon", "Philemon"],
  ["hebrews", "Hebrews"],
  ["james", "James"],
  ["jude", "Jude"],
  ["revelation", "Revelation"],
] satisfies Array<[string, string]>;

const BOOK_ALIASES: Array<[string, string]> = [...BOOK_ALIAS_ROWS].sort(
  (left, right) => right[0].length - left[0].length,
);

const SMALL: Record<string, number> = {
  zero: 0,
  one: 1,
  two: 2,
  to: 2,
  too: 2,
  three: 3,
  four: 4,
  for: 4,
  five: 5,
  six: 6,
  seven: 7,
  eight: 8,
  ate: 8,
  nine: 9,
  ten: 10,
  eleven: 11,
  twelve: 12,
  thirteen: 13,
  fourteen: 14,
  fifteen: 15,
  sixteen: 16,
  seventeen: 17,
  eighteen: 18,
  nineteen: 19,
};

const TENS: Record<string, number> = {
  twenty: 20,
  thirty: 30,
  forty: 40,
  fourty: 40,
  fifty: 50,
  sixty: 60,
  seventy: 70,
  eighty: 80,
  ninety: 90,
};

function normalizeCommand(input: string): string {
  return input
    .toLowerCase()
    .replace(/[.,;:!?()[\]"']/g, " ")
    .replace(
      /\b(?:open|turn|read|show|display|please|kindly|to|the|book|of|chapter|chapters|verse|verses|vs|v)\b/g,
      " ",
    )
    .replace(/\s+/g, " ")
    .trim();
}

function readNumber(tokens: string[], start: number): { value: number; next: number } | null {
  const token = tokens[start];
  if (!token) return null;
  if (/^\d+$/.test(token)) return { value: Number.parseInt(token, 10), next: start + 1 };
  if (TENS[token] !== undefined) {
    const ones = SMALL[tokens[start + 1] ?? ""];
    const hasOnes = ones !== undefined && ones > 0 && ones < 10;
    return {
      value: TENS[token] + (hasOnes ? ones : 0),
      next: start + (hasOnes ? 2 : 1),
    };
  }
  if (SMALL[token] !== undefined) return { value: SMALL[token], next: start + 1 };
  return null;
}

function extractNumbers(text: string): number[] {
  const tokens = text.split(/\s+/).filter(Boolean);
  const numbers: number[] = [];
  let index = 0;
  while (index < tokens.length) {
    const parsed = readNumber(tokens, index);
    if (parsed) {
      numbers.push(parsed.value);
      index = parsed.next;
    } else {
      index += 1;
    }
  }
  return numbers;
}

function findBookAliasMatches(normalized: string): BookAliasMatch[] {
  const padded = ` ${normalized} `;
  const matches: BookAliasMatch[] = [];

  for (const [alias, book] of BOOK_ALIASES) {
    const needle = ` ${alias} `;
    let fromIndex = 0;
    while (fromIndex < padded.length) {
      const index = padded.indexOf(needle, fromIndex);
      if (index === -1) break;
      const start = index + 1;
      matches.push({
        alias,
        book,
        start,
        end: start + alias.length,
      });
      fromIndex = index + needle.length - 1;
    }
  }

  matches.sort((left, right) => {
    if (left.start !== right.start) return left.start - right.start;
    return right.alias.length - left.alias.length;
  });

  return matches.filter((match, index, all) => {
    const previous = all[index - 1];
    return !(previous && previous.start === match.start);
  });
}

export function parseExplicitScriptureCommand(command: string): ParsedScriptureCommand | null {
  const normalized = normalizeCommand(command);
  if (!normalized) return null;

  const matches = findBookAliasMatches(normalized);
  for (const [index, match] of matches.entries()) {
    const nextBookStart = matches[index + 1]?.start;
    const tailEnd = nextBookStart ?? normalized.length;
    const tail = normalized.slice(match.end, tailEnd).trim();
    const numbers = extractNumbers(tail);
    if (numbers.length === 0) continue;
    const chapter = numbers[0];
    const verse = numbers.length >= 2 ? numbers[1] : 1;
    if (chapter < 1 || verse < 1) continue;
    return {
      book: match.book,
      chapter,
      verse,
      explicitVerse: numbers.length >= 2,
    };
  }

  return null;
}

export function classifyScriptureCommand(command: string): ScriptureCommandKind {
  const parsed = parseExplicitScriptureCommand(command);
  if (parsed) return parsed.explicitVerse ? "explicitReference" : "chapterReference";

  const normalized = command
    .toLowerCase()
    .replace(/[.,:;]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!normalized) return "none";

  if (
    /\b(next|continue)\b/.test(normalized) ||
    /\bkeep going\b/.test(normalized) ||
    /\bnext verse\b/.test(normalized) ||
    /\b(previous verse|take it back|go back)\b/.test(normalized) ||
    /\bprevious\b/.test(normalized) ||
    /\b(?:go to\s+)?verse\s+\d{1,3}\b/.test(normalized)
  ) {
    return "contextualFollowUp";
  }

  return "none";
}
