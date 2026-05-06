//! Grammar-based explicit scripture reference parser.
//!
//! Consumes the output of [`crate::normalize::TranscriptNormalizer`] — a
//! lower-cased, filler-stripped, digit-normalized string — and scans for
//! canonical `<book> <chapter>[:<verse_start>[-<verse_end>]]` references.
//!
//! Design goals:
//!
//! * **Deterministic.** No regex backtracking, no ML. Operator can trust that
//!   the same transcript always produces the same candidates.
//! * **Zero false positives on common false friends.** Bare numbers without
//!   a preceding book ("we read chapter three yesterday") are never promoted.
//! * **Longest book alias wins.** `"1 John"` beats `"John"` when both match.
//! * **Chapter sanity-clamped.** A parsed chapter greater than the book's
//!   `max_chapter` is discarded.
//!
//! Recognized shapes (after normalization):
//!
//! * `john 3:16`
//! * `john 3 16`             (chapter & verse space-separated)
//! * `john 3`                (chapter-only — emitted with `verse_start = 1`, lower confidence)
//! * `john 3:16-20`          (range)
//! * `john chapter 3 verse 16`
//! * `john chapter 3 verse 16 through 20`
//! * `john chapter 3 verses 16 to 20`
//! * `1 john 4:8`
//!
//! Confidence is a coarse prior in [0.70, 0.98] that the retrieval/decision
//! layer can combine with transcript confidence later.

use crate::books::{BookEntry, longest_match_at, lookup_ambiguous, lookup_canonical};

/// One scripture reference resolved from normalized text.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedReference {
    /// Canonical book name (e.g. `"1 John"`, `"Song of Solomon"`).
    pub book: &'static str,
    pub chapter: u16,
    /// First verse of the range. Defaults to 1 when only the chapter was
    /// mentioned ("John 3").
    pub verse_start: u16,
    /// Optional end of an inclusive verse range.
    pub verse_end: Option<u16>,
    /// Coarse grammar confidence prior.
    pub confidence: f32,
    /// The alias text (normalized) that matched the book. Useful for audit
    /// logs and UI "why did you pick this?" tooltips.
    pub alias_matched: String,
    /// Whether the reference was explicit (chapter AND verse given) or
    /// chapter-only.
    pub explicit_verse: bool,
    /// True when the original text used a bare book name with multiple
    /// canonical resolutions (e.g. "Chronicles 7:14" → both 1Chr and 2Chr).
    /// The UI must offer `disambiguation_options` as chips instead of
    /// auto-picking. When `true`, the candidate's `book` field holds ONE
    /// of the valid options as a default; the rest are in
    /// `disambiguation_options`.
    pub needs_disambiguation: bool,
    /// Canonical book names this reference could legitimately resolve to.
    /// Always empty when `needs_disambiguation` is false.
    pub disambiguation_options: Vec<&'static str>,
}

impl ParsedReference {
    /// Human-readable rendering in the canonical `"Book C:V"` form used by the
    /// verse storage layer.
    pub fn as_reference_string(&self) -> String {
        match (self.explicit_verse, self.verse_end) {
            (true, Some(end)) if end > self.verse_start => {
                format!(
                    "{} {}:{}-{}",
                    self.book, self.chapter, self.verse_start, end
                )
            }
            (true, _) => format!("{} {}:{}", self.book, self.chapter, self.verse_start),
            (false, _) => format!("{} {}", self.book, self.chapter),
        }
    }
}

/// Deterministic grammar parser.
#[derive(Default, Clone, Copy, Debug)]
pub struct GrammarReferenceParser;

impl GrammarReferenceParser {
    /// Returns every reference the grammar can extract from `normalized`.
    /// Duplicates (same book/chapter/verse) are deduplicated — the highest
    /// confidence representative wins.
    pub fn parse_all(&self, normalized: &str) -> Vec<ParsedReference> {
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        let mut out: Vec<ParsedReference> = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            // 1. Standard longest-alias match (handles "1 John", "first samuel").
            if let Some((book, consumed, alias)) = longest_match_at(&tokens, i) {
                if let Some(reference) = parse_tail(book, alias, &tokens, i + consumed) {
                    out.push(reference);
                    i += consumed;
                    continue;
                }
            }

            // 2. Bare ambiguous book token: "chronicles 7:14",
            //    "samuel 16:7", etc. Emit one ParsedReference per valid
            //    canonical option (canon-range filtered) with
            //    needs_disambiguation = true so the UI can chip-pick.
            if let Some(token) = tokens.get(i) {
                if let Some(options) = lookup_ambiguous(token) {
                    let alias = (*token).to_string();
                    let valid: Vec<&'static str> = options
                        .iter()
                        .copied()
                        .filter(|canonical| {
                            // Peek the tail: only retain books whose canon
                            // range can host the parsed chapter:verse. We
                            // call parse_tail per option and discard misses.
                            if let Some(book_entry) = lookup_canonical(canonical) {
                                parse_tail(book_entry, alias.clone(), &tokens, i + 1).is_some()
                            } else {
                                false
                            }
                        })
                        .collect();

                    if !valid.is_empty() {
                        let auto_unambiguous = valid.len() == 1;
                        for canonical in &valid {
                            if let Some(book_entry) = lookup_canonical(canonical) {
                                if let Some(mut reference) =
                                    parse_tail(book_entry, alias.clone(), &tokens, i + 1)
                                {
                                    if !auto_unambiguous {
                                        reference.needs_disambiguation = true;
                                        reference.disambiguation_options =
                                            valid.iter().copied().collect();
                                        // Lower confidence for ambiguous picks.
                                        reference.confidence =
                                            (reference.confidence - 0.10).max(0.50);
                                    }
                                    out.push(reference);
                                }
                            }
                        }
                        i += 1;
                        continue;
                    }
                }
            }

            i += 1;
        }

        dedupe(out)
    }
}

/// Attempts to parse the chapter/verse tail starting at `tail_start`.
/// Returns `None` when the tail is missing (bare book name with no chapter).
fn parse_tail(
    book: &'static BookEntry,
    alias: String,
    tokens: &[&str],
    tail_start: usize,
) -> Option<ParsedReference> {
    let mut cursor = tail_start;

    // Skip optional "chapter" marker.
    let mut saw_chapter_marker = false;
    if tokens.get(cursor).copied() == Some("chapter") {
        saw_chapter_marker = true;
        cursor += 1;
    }

    // Chapter can be either a composite "3:16[-20]" token or a bare digit.
    let chapter: u16;
    let mut verse_start: Option<u16> = None;
    let mut verse_end: Option<u16> = None;

    if let Some((chap, vs, ve, after)) = read_colon_range(tokens, cursor) {
        chapter = chap;
        verse_start = Some(vs);
        verse_end = ve;
        cursor = after;
    } else if let Some((chap, after)) = read_digit(tokens, cursor) {
        chapter = chap;
        cursor = after;
    } else {
        return None;
    }

    // Chapter sanity clamp.
    if chapter == 0 || chapter > book.max_chapter {
        return None;
    }

    // If we don't yet have a verse, look for one via a "verse N" marker, a
    // composite "3:16" that immediately follows, or a space-separated digit
    // (only accepted when "chapter" marker was seen — avoids false positives
    // on trailing numbers).
    if verse_start.is_none() {
        if let Some(peek) = tokens.get(cursor).copied() {
            if matches!(peek, "verse" | "verses" | "v" | "vs") {
                cursor += 1;
                if let Some((vs, after)) = read_digit(tokens, cursor) {
                    verse_start = Some(vs);
                    cursor = after;
                }
            } else if let Some((_chap2, vs, ve, after)) = read_colon_range(tokens, cursor) {
                // "chapter 3 3:16" — defensive.
                verse_start = Some(vs);
                verse_end = ve;
                cursor = after;
            } else if let Some((vs, after)) = read_digit(tokens, cursor) {
                // Space-separated verse: "daniel 3 10", "acts 11 8",
                // "john 3 16". Always accept here — we already have a
                // book + valid chapter, so the next pure-digit token is
                // overwhelmingly the verse (even without the operator
                // saying the literal word "chapter"). Sanity-check
                // verse later. The saw_chapter_marker guard was too
                // conservative and caused the Nigerian-style
                // "<book> <chapter> <verse>" form to silently lose its
                // verse and degrade to chapter-only.
                let _ = saw_chapter_marker;
                verse_start = Some(vs);
                cursor = after;
            }
        }
    }

    // Optional verse range extension: "through|to|thru|- <digit>".
    if verse_start.is_some() && verse_end.is_none() {
        if let Some(sep) = tokens.get(cursor).copied() {
            if matches!(sep, "-" | "through" | "to" | "thru") {
                if let Some((ve, after)) = read_digit(tokens, cursor + 1) {
                    verse_end = Some(ve);
                    cursor = after;
                }
            }
        }
    }
    let _ = cursor; // cursor no longer needed past this point

    // Verse sanity.
    if let Some(vs) = verse_start {
        if vs == 0 || vs > 200 {
            return None;
        }
    }
    if let Some(ve) = verse_end {
        if ve == 0 || ve > 200 {
            return None;
        }
    }
    // Ensure range is non-decreasing.
    if let (Some(vs), Some(ve)) = (verse_start, verse_end) {
        if ve < vs {
            verse_end = None;
        }
    }

    let explicit_verse = verse_start.is_some();
    let confidence = if explicit_verse { 0.95 } else { 0.76 };

    Some(ParsedReference {
        book: book.canonical,
        chapter,
        verse_start: verse_start.unwrap_or(1),
        verse_end,
        confidence,
        alias_matched: alias,
        explicit_verse,
        needs_disambiguation: false,
        disambiguation_options: Vec::new(),
    })
}

/// Parses a digit token starting at `start`. Returns the value and the next
/// index. Returns `None` if the token is not a pure non-negative integer
/// fitting in u16 (max 65,535 — plenty for chapter/verse).
fn read_digit(tokens: &[&str], start: usize) -> Option<(u16, usize)> {
    let tok = tokens.get(start)?;
    if tok.is_empty() || !tok.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let value: u16 = tok.parse().ok()?;
    Some((value, start + 1))
}

/// Parses a composite `"<chapter>:<verse>[-<verse_end>]"` token at `start`.
/// Returns `(chapter, verse_start, Option<verse_end>, next_index)`.
fn read_colon_range(tokens: &[&str], start: usize) -> Option<(u16, u16, Option<u16>, usize)> {
    let tok = tokens.get(start)?;
    let (chapter_str, rest) = tok.split_once(':')?;
    let chapter: u16 = chapter_str.parse().ok()?;
    let (vs_str, ve_opt) = match rest.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let verse_start: u16 = vs_str.parse().ok()?;
    let verse_end: Option<u16> = match ve_opt {
        Some(b) => Some(b.parse().ok()?),
        None => None,
    };
    Some((chapter, verse_start, verse_end, start + 1))
}

/// Keeps the highest-confidence representative per (book, chapter, verse_start).
fn dedupe(mut refs: Vec<ParsedReference>) -> Vec<ParsedReference> {
    refs.sort_by(|a, b| {
        (
            a.book,
            a.chapter,
            a.verse_start,
            a.verse_end,
            a.explicit_verse,
        )
            .cmp(&(
                b.book,
                b.chapter,
                b.verse_start,
                b.verse_end,
                b.explicit_verse,
            ))
            .then_with(|| b.confidence.total_cmp(&a.confidence))
    });
    refs.dedup_by(|a, b| {
        a.book == b.book
            && a.chapter == b.chapter
            && a.verse_start == b.verse_start
            && a.verse_end == b.verse_end
            && a.explicit_verse == b.explicit_verse
    });
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::TranscriptNormalizer;

    fn parse_raw(raw: &str) -> Vec<ParsedReference> {
        let n = TranscriptNormalizer.normalize(raw);
        GrammarReferenceParser.parse_all(&n.text)
    }

    #[test]
    fn parses_space_separated_chapter_verse_no_marker() {
        // Nigerian-style: "Daniel 3 10", "Acts 11 8", "John 3 16" with no
        // literal "chapter" word. Must be parsed as <book> chapter:verse.
        let r = parse_raw("daniel 3 10");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].book, "Daniel");
        assert_eq!(r[0].chapter, 3);
        assert_eq!(r[0].verse_start, 10);
        let r = parse_raw("acts 11 8");
        assert_eq!(r[0].book, "Acts");
        assert_eq!(r[0].chapter, 11);
        assert_eq!(r[0].verse_start, 8);
        let r = parse_raw("john 3 16");
        assert_eq!(r[0].book, "John");
        assert_eq!(r[0].chapter, 3);
        assert_eq!(r[0].verse_start, 16);
    }

    #[test]
    fn parses_basic_colon_reference() {
        let refs = parse_raw("Open John 3:16 please.");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].book, "John");
        assert_eq!(refs[0].chapter, 3);
        assert_eq!(refs[0].verse_start, 16);
        assert!(refs[0].explicit_verse);
    }

    #[test]
    fn parses_numbered_book_with_ordinal_prefix() {
        let refs = parse_raw("first john 4:8");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].book, "1 John");
        assert_eq!(refs[0].chapter, 4);
        assert_eq!(refs[0].verse_start, 8);
    }

    #[test]
    fn parses_chapter_verse_word_form() {
        let refs = parse_raw("romans chapter eight verse twenty eight");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].book, "Romans");
        assert_eq!(refs[0].chapter, 8);
        assert_eq!(refs[0].verse_start, 28);
    }

    #[test]
    fn parses_range() {
        let refs = parse_raw("romans 8:28-30");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].verse_start, 28);
        assert_eq!(refs[0].verse_end, Some(30));
    }

    #[test]
    fn parses_range_with_words() {
        let refs = parse_raw("john chapter 3 verses 16 through 18");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].chapter, 3);
        assert_eq!(refs[0].verse_start, 16);
        assert_eq!(refs[0].verse_end, Some(18));
    }

    #[test]
    fn longest_book_alias_wins() {
        let refs = parse_raw("song of solomon 2:1");
        assert_eq!(refs[0].book, "Song of Solomon");
    }

    #[test]
    fn chapter_only_lower_confidence() {
        let refs = parse_raw("please turn to psalm 23");
        assert_eq!(refs[0].book, "Psalms");
        assert_eq!(refs[0].chapter, 23);
        assert!(!refs[0].explicit_verse);
        assert!(refs[0].confidence < 0.9);
    }

    #[test]
    fn rejects_impossible_chapter() {
        // John only has 21 chapters.
        let refs = parse_raw("john 99:1");
        assert!(refs.is_empty());
    }

    #[test]
    fn no_false_positive_on_bare_chapter_word() {
        let refs = parse_raw("we talked about chapter three yesterday");
        assert!(refs.is_empty());
    }

    #[test]
    fn handles_two_references_same_sentence() {
        let refs = parse_raw("compare john 3:16 with romans 8:28");
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn dedupes_identical_parses() {
        let refs = parse_raw("john 3:16 and again john 3:16");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn parses_psalm_119() {
        let refs = parse_raw("psalm one hundred and nineteen verse eleven");
        assert_eq!(refs[0].book, "Psalms");
        assert_eq!(refs[0].chapter, 119);
        assert_eq!(refs[0].verse_start, 11);
    }

    #[test]
    fn reference_string_renders_canonical_form() {
        let refs = parse_raw("first peter 5:7");
        assert_eq!(refs[0].as_reference_string(), "1 Peter 5:7");
    }

    #[test]
    fn reference_string_range_form() {
        let refs = parse_raw("romans 8:28-30");
        assert_eq!(refs[0].as_reference_string(), "Romans 8:28-30");
    }
}
