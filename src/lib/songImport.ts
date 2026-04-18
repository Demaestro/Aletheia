// Song import + chord utilities.
//
// These run entirely in the browser (no network, no worker) so they Just Work
// in offline mode. The goal is good-enough fidelity for most worship
// songbooks — not a 100 %-spec-compliant ChordPro/OpenLyrics parser.
//
//   - parseChordPro(text)  → ChordPro directive-aware: `{start_of_verse}` …
//     `{end_of_verse}`, `{chorus}`, `{title:…}`, `{author:…}`, `{key:…}`,
//     inline `[G]` chords are stripped for the projected lyric text but
//     preserved in a separate `chords` field per line for future use.
//   - parseOpenLyrics(xml) → OpenLyrics 0.8/0.9 `<verse name="v1"><lines>…`
//   - transpose(text, from, to) → shifts inline `[CHORD]` tokens.
//   - autoSplit(lyrics)     → quick-n-dirty "Verse / Chorus / Bridge" splitter
//     from plain lyric paste when the uploader didn't label sections.

import type { Song, SongSection } from "../types";

export type ParsedSong = {
  title: string;
  author: string;
  ccliNumber: string | null;
  copyright: string;
  language: string;
  songKey: string | null;
  bpm: number | null;
  sections: SongSection[];
};

// ---------------------------------------------------------------------------
// Chord math
// ---------------------------------------------------------------------------

const SHARP_SCALE = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;
const FLAT_SCALE = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"] as const;

function noteIndex(note: string): number {
  const normalised = note.replace(/^([A-G])(b|#)?.*/, "$1$2");
  for (let i = 0; i < SHARP_SCALE.length; i++) {
    if (SHARP_SCALE[i] === normalised || FLAT_SCALE[i] === normalised) return i;
  }
  // Handle enharmonic oddities like Cb → B, E# → F.
  const oddities: Record<string, number> = {
    Cb: 11,
    "E#": 5,
    Fb: 4,
    "B#": 0,
  };
  if (oddities[normalised] !== undefined) return oddities[normalised];
  return -1;
}

function preferFlats(key: string | null | undefined): boolean {
  if (!key) return false;
  return /^(F|Bb|Eb|Ab|Db|Gb|Cb|Dm|Gm|Cm|Fm|Bbm|Ebm)$/.test(key);
}

/** Shift a single chord token like "G/B" or "F#m7" by `semitones`. */
export function shiftChord(token: string, semitones: number, targetKey?: string | null): string {
  const match = token.match(/^([A-G][b#]?)(.*)$/);
  if (!match) return token;
  const [, root, rest] = match;
  const i = noteIndex(root);
  if (i < 0) return token;
  const scale = preferFlats(targetKey) ? FLAT_SCALE : SHARP_SCALE;
  const shifted = scale[(i + ((semitones % 12) + 12)) % 12];
  // Handle slash chords recursively ("G/B").
  const restShifted = rest.replace(/\/([A-G][b#]?)/, (_m, bass: string) => {
    const bi = noteIndex(bass);
    return bi < 0 ? `/${bass}` : `/${scale[(bi + ((semitones % 12) + 12)) % 12]}`;
  });
  return shifted + restShifted;
}

/** Transpose all `[CHORD]` tokens in ChordPro-style text. */
export function transposeChordProText(
  text: string,
  fromKey: string,
  toKey: string
): string {
  const from = noteIndex(fromKey);
  const to = noteIndex(toKey);
  if (from < 0 || to < 0) return text;
  const semitones = to - from;
  return text.replace(/\[([^\]]+)\]/g, (_m, chord: string) =>
    `[${shiftChord(chord, semitones, toKey)}]`
  );
}

// ---------------------------------------------------------------------------
// ChordPro parser
// ---------------------------------------------------------------------------

/** Strip `[CHORD]` inline markers so projected lyrics stay clean. */
export function stripChordMarkers(line: string): string {
  return line.replace(/\[[^\]]+\]/g, "").replace(/\s{2,}/g, " ").trim();
}

const SECTION_DIRECTIVES: Record<string, string> = {
  sov: "Verse",
  start_of_verse: "Verse",
  soc: "Chorus",
  start_of_chorus: "Chorus",
  sob: "Bridge",
  start_of_bridge: "Bridge",
  sot: "Tag",
  start_of_tag: "Tag",
};

/** Parse a ChordPro string into a ParsedSong. Inline `[G]` chords are stripped
 *  from `section.text` so sending a section live shows clean lyrics. */
export function parseChordPro(source: string): ParsedSong {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const song: ParsedSong = {
    title: "",
    author: "",
    ccliNumber: null,
    copyright: "",
    language: "en",
    songKey: null,
    bpm: null,
    sections: [],
  };
  let current: SongSection | null = null;
  let verseCounter = 0;
  let chorusCounter = 0;

  const pushSection = () => {
    if (current && current.text.trim().length > 0) {
      song.sections.push({
        ...current,
        text: current.text.trim(),
      });
    }
    current = null;
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    // Metadata directives.
    const meta = line.match(/^\{\s*([a-zA-Z_]+)\s*:\s*(.+?)\s*\}\s*$/);
    if (meta) {
      const key = meta[1].toLowerCase();
      const value = meta[2];
      switch (key) {
        case "t":
        case "title":
          song.title = value;
          break;
        case "st":
        case "subtitle":
        case "author":
        case "artist":
          song.author = value;
          break;
        case "ccli":
          song.ccliNumber = value;
          break;
        case "copyright":
          song.copyright = value;
          break;
        case "language":
          song.language = value;
          break;
        case "key":
          song.songKey = value;
          break;
        case "tempo":
        case "bpm": {
          const n = Number.parseInt(value, 10);
          if (Number.isFinite(n)) song.bpm = n;
          break;
        }
        case "comment":
        case "c":
          if (current) current.text += `\n(${value})`;
          break;
        default:
          // Section-start directives.
          if (SECTION_DIRECTIVES[key]) {
            pushSection();
            const kind = SECTION_DIRECTIVES[key];
            if (kind === "Verse") verseCounter += 1;
            if (kind === "Chorus") chorusCounter += 1;
            const label =
              kind === "Verse"
                ? `Verse ${verseCounter}`
                : kind === "Chorus" && chorusCounter > 1
                  ? `Chorus ${chorusCounter}`
                  : kind;
            current = { label, text: "" };
          } else if (/^(eov|eoc|eob|eot|end_of_)/.test(key)) {
            pushSection();
          }
      }
      continue;
    }

    if (!current) {
      current = { label: "Intro", text: "" };
    }
    if (line.trim() === "" && current.text.trim() === "") continue;
    current.text += (current.text ? "\n" : "") + stripChordMarkers(line);
  }
  pushSection();

  if (song.sections.length === 0) {
    song.sections.push({ label: "Verse 1", text: source.trim() });
  }
  if (!song.title) song.title = "Imported song";
  return song;
}

// ---------------------------------------------------------------------------
// OpenLyrics parser (subset)
// ---------------------------------------------------------------------------

/** Parse an OpenLyrics XML document. Tries DOMParser first; falls back to a
 *  regex-based reader for environments without a DOM (e.g. tests). */
export function parseOpenLyrics(xml: string): ParsedSong {
  const song: ParsedSong = {
    title: "",
    author: "",
    ccliNumber: null,
    copyright: "",
    language: "en",
    songKey: null,
    bpm: null,
    sections: [],
  };

  try {
    const doc = new DOMParser().parseFromString(xml, "application/xml");
    const err = doc.querySelector("parsererror");
    if (err) throw new Error(err.textContent ?? "xml parse error");

    song.title = doc.querySelector("properties > titles > title")?.textContent?.trim() ?? "";
    const authors = Array.from(doc.querySelectorAll("properties > authors > author"))
      .map((el) => el.textContent?.trim())
      .filter(Boolean) as string[];
    song.author = authors.join(", ");
    song.copyright = doc.querySelector("properties > copyright")?.textContent?.trim() ?? "";
    song.ccliNumber = doc.querySelector("properties > ccliNo")?.textContent?.trim() ?? null;
    const langEl = doc.querySelector("lyrics")?.getAttribute("language");
    if (langEl) song.language = langEl;
    const keyEl = doc.querySelector("properties > key")?.textContent?.trim();
    if (keyEl) song.songKey = keyEl;

    const verses = Array.from(doc.querySelectorAll("lyrics > verse"));
    for (const v of verses) {
      const name = v.getAttribute("name") ?? `v${song.sections.length + 1}`;
      const linesNodes = Array.from(v.querySelectorAll("lines"));
      const lines = linesNodes
        .map((node) => {
          // OpenLyrics allows `<br/>` separators between lines.
          const html = node.innerHTML.replace(/<br\s*\/?>/gi, "\n");
          const tmp = document.createElement("div");
          tmp.innerHTML = html;
          return tmp.textContent ?? "";
        })
        .join("\n");
      song.sections.push({
        label: prettifyOpenLyricsLabel(name),
        text: lines.trim(),
      });
    }
  } catch {
    // Regex fallback — extracts <verse name="…"><lines>…</lines></verse>
    const titleMatch = xml.match(/<title[^>]*>([\s\S]*?)<\/title>/i);
    if (titleMatch) song.title = stripXml(titleMatch[1]);
    const authorMatch = xml.match(/<author[^>]*>([\s\S]*?)<\/author>/i);
    if (authorMatch) song.author = stripXml(authorMatch[1]);
    const ccliMatch = xml.match(/<ccliNo[^>]*>([\s\S]*?)<\/ccliNo>/i);
    if (ccliMatch) song.ccliNumber = stripXml(ccliMatch[1]);
    const verseRegex = /<verse[^>]*name="([^"]+)"[^>]*>([\s\S]*?)<\/verse>/g;
    let m: RegExpExecArray | null;
    while ((m = verseRegex.exec(xml)) !== null) {
      const name = m[1];
      const inner = m[2];
      const text = inner
        .replace(/<br\s*\/?>/gi, "\n")
        .replace(/<[^>]+>/g, "")
        .replace(/&amp;/g, "&")
        .replace(/&lt;/g, "<")
        .replace(/&gt;/g, ">")
        .replace(/&quot;/g, '"')
        .trim();
      song.sections.push({ label: prettifyOpenLyricsLabel(name), text });
    }
  }

  if (song.sections.length === 0) {
    song.sections.push({ label: "Verse 1", text: xml.trim() });
  }
  if (!song.title) song.title = "Imported song";
  return song;
}

function prettifyOpenLyricsLabel(name: string): string {
  // Names like "v1", "c1", "b1", "v1a"
  const m = name.match(/^([vcbpt])(\d+)([a-z])?$/i);
  if (!m) return name;
  const type = { v: "Verse", c: "Chorus", b: "Bridge", p: "Pre-chorus", t: "Tag" }[m[1].toLowerCase()];
  return `${type ?? name} ${m[2]}${m[3] ? m[3].toUpperCase() : ""}`;
}

function stripXml(s: string): string {
  return s
    .replace(/<[^>]+>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .trim();
}

// ---------------------------------------------------------------------------
// Plain-text "auto-split" by blank-line groups and common labels
// ---------------------------------------------------------------------------

const LABEL_RE = /^(verse|chorus|bridge|pre[-\s]?chorus|intro|outro|tag|refrain|interlude)(\s*\d+)?\s*:?\s*$/i;

export function autoSplitPlainText(lyrics: string): SongSection[] {
  const lines = lyrics.replace(/\r\n?/g, "\n").split("\n");
  const sections: SongSection[] = [];
  let current: SongSection = { label: "Verse 1", text: "" };
  let verseN = 1;
  let chorusN = 0;

  const commit = () => {
    if (current.text.trim()) sections.push({ ...current, text: current.text.trim() });
  };

  for (const raw of lines) {
    const line = raw.trim();
    const labelMatch = line.match(LABEL_RE);
    if (labelMatch) {
      commit();
      const kind = labelMatch[1].toLowerCase();
      let label = line.replace(/:$/, "");
      if (kind.startsWith("verse") && !/\d/.test(label)) {
        verseN += 1;
        label = `Verse ${verseN}`;
      } else if (kind.startsWith("chorus") && !/\d/.test(label)) {
        chorusN += 1;
        label = chorusN > 1 ? `Chorus ${chorusN}` : "Chorus";
      }
      current = { label, text: "" };
      continue;
    }
    if (!line && !current.text) continue;
    current.text += (current.text ? "\n" : "") + raw;
  }
  commit();

  // No labels detected — split on double blank lines as a fallback.
  if (sections.length <= 1) {
    const chunks = lyrics.split(/\n{2,}/).map((s) => s.trim()).filter(Boolean);
    if (chunks.length > 1) {
      return chunks.map((text, i) => ({
        label: i === 1 ? "Chorus" : `Verse ${i + 1 - (i > 0 ? 1 : 0)}`,
        text,
      }));
    }
  }
  return sections;
}

// ---------------------------------------------------------------------------
// High-level helpers
// ---------------------------------------------------------------------------

export function detectFormat(source: string): "chordpro" | "openlyrics" | "plain" {
  const head = source.slice(0, 400).toLowerCase();
  if (head.includes("<?xml") || head.includes("<song ") || head.includes("xmlns=\"http://openlyrics.info")) {
    return "openlyrics";
  }
  if (/\{\s*(title|t|author|artist|key|ccli|start_of_|soc|sov)\b/i.test(source) || /\[[A-G][b#]?/.test(source)) {
    return "chordpro";
  }
  return "plain";
}

export function parseAny(source: string): ParsedSong {
  switch (detectFormat(source)) {
    case "openlyrics":
      return parseOpenLyrics(source);
    case "chordpro":
      return parseChordPro(source);
    default: {
      const sections = autoSplitPlainText(source);
      return {
        title: "Imported song",
        author: "",
        ccliNumber: null,
        copyright: "",
        language: "en",
        songKey: null,
        bpm: null,
        sections: sections.length ? sections : [{ label: "Verse 1", text: source.trim() }],
      };
    }
  }
}

export function toSongRecord(parsed: ParsedSong, idPrefix = "song"): Song {
  const now = Date.now();
  return {
    id: `${idPrefix}-${now}-${Math.random().toString(36).slice(2, 8)}`,
    title: parsed.title,
    author: parsed.author,
    ccliNumber: parsed.ccliNumber,
    copyright: parsed.copyright,
    language: parsed.language || "en",
    songKey: parsed.songKey,
    bpm: parsed.bpm,
    sections: parsed.sections,
    createdAtMs: now,
    updatedAtMs: now,
  };
}
