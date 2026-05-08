//! Transcript normalization.
//!
//! Converts a raw STT transcript window into a canonical form the grammar
//! parser can scan cheaply:
//!
//! 1. Lower-case, diacritic-fold.
//! 2. Strip filler words and hesitation markers.
//! 3. Map English spoken ordinals ("first", "1st", "i") immediately before a
//!    book-ish token to the digit form (`"first john"` → `"1 john"`).
//! 4. Convert English spoken numbers ("three", "sixteen", "one hundred
//!    nineteen", "twenty three") to digits.
//! 5. Collapse redundant whitespace.
//!
//! The result is *lossy by design*. It is not meant to be shown to a user —
//! it is fed into the grammar parser and then discarded.

use crate::fold_for_matching;

/// Words that carry no meaning for scripture detection and distort both
/// number-word sequences and alias matching. Kept small on purpose to avoid
/// stripping preacher phrasing that the retrieval layer may want later.
const FILLERS: &[&str] = &[
    // Universal hesitation markers
    "uh", "um", "uhh", "umm", "er", "eh", "mm", "hmm", "ah",
    // Discourse markers that appear between book names and numbers
    "like", "you know", "i mean", "basically", "actually", "sort of",
    "kind of", "gonna", "wanna", "gotta", "erm",
    // Sermon-specific filler phrases (common in Nigerian Pentecostal preaching)
    "turn with me to", "please turn with me to", "let us look at",
    "let me show you", "the bible says in", "the scripture says",
    "as we read in", "as it is written in",
];


/// Ordinal markers the parser should rewrite to digit prefixes before a book
/// alias. Case- and diacritic-folded. "i"/"ii"/"iii" deliberately included
/// because Roman-numeral book prefixes are common in printed Bibles.
fn ordinal_prefix(word: &str) -> Option<&'static str> {
    match word {
        "first" | "1st" | "i" => Some("1"),
        "second" | "2nd" | "ii" => Some("2"),
        "third" | "3rd" | "iii" => Some("3"),
        _ => None,
    }
}

/// English spoken numerals 0–19.
fn small_number(word: &str) -> Option<u32> {
    Some(match word {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        // ── Nigerian-accent Whisper phonetic variants ─────────────────────
        // Whisper frequently transcribes Nigerian-accented speech with these
        // substitutions. Accepted only inside the number-merging pass so
        // the risk of false positives on real words is negligible.
        "wan" => 1,          // "one"  → /wʌn/ → "wan"
        "tree" => 3,         // "three" elides the /θ/
        "fif" | "fiff" => 5, // "five"  shortened
        "nain" => 9,         // "nine"  → /naɪn/ → "nain"
        "twen" => 20,        // "twenty" clipped
        _ => return None,
    })
}

/// English tens 20–90.
fn tens_number(word: &str) -> Option<u32> {
    Some(match word {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    })
}

/// Result of normalizing a transcript window.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedText {
    /// Lower-cased, folded, filler-stripped, digit-normalized text.
    pub text: String,
}

/// Pure-function normalizer.
#[derive(Default, Clone, Copy, Debug)]
pub struct TranscriptNormalizer;

impl TranscriptNormalizer {
    /// Normalizes `raw` for grammar parsing.
    pub fn normalize(&self, raw: &str) -> NormalizedText {
        // Step 1: fold diacritics & lowercase, then separate reference
        // punctuation (":" and "-") into standalone tokens so they survive
        // tokenization.
        let folded = fold_for_matching(raw);
        let punctuation_spaced = separate_reference_punctuation(&folded);

        // Step 2: tokenize on whitespace. `words` keeps original-ish tokens.
        let words: Vec<String> = punctuation_spaced
            .split_whitespace()
            .map(strip_trailing_punctuation)
            .filter(|t| !t.is_empty())
            .collect();

        // Step 3: strip multi-word and single-word fillers.
        let words = strip_fillers(&words);

        // Step 4: merge consecutive spelled numbers into digits.
        let words = merge_spelled_numbers(&words);

        // Step 5: rewrite ordinal prefixes ("first", "1st", "i") immediately
        // before a likely book-ish token to the digit form.
        let words = rewrite_ordinal_prefixes(&words);

        NormalizedText {
            text: words.join(" "),
        }
    }
}

/// Inserts spaces around ":" and "-" so they tokenize cleanly.
/// Does NOT split "3:16" into "3 : 16" — callers will still see "3:16" intact.
/// Instead this splits them only if they are adjacent to letters to keep
/// reference punctuation attached to numbers.
fn separate_reference_punctuation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        match ch {
            ',' | '.' | ';' | '!' | '?' | '(' | ')' | '"' | '\'' => out.push(' '),
            ':' | '-' => {
                let between_digits = prev.map(|c| c.is_ascii_digit()).unwrap_or(false)
                    && next.map(|c| c.is_ascii_digit()).unwrap_or(false);
                if between_digits {
                    out.push(*ch);
                } else {
                    out.push(' ');
                }
            }
            _ => out.push(*ch),
        }
    }
    out
}

fn strip_trailing_punctuation(token: &str) -> String {
    token
        .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')' | '"' | '\''))
        .to_string()
}

/// Strips fillers. Handles two-word fillers ("you know", "i mean") first.
fn strip_fillers(words: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        // Try 2-word filler.
        if i + 1 < words.len() {
            let pair = format!("{} {}", words[i], words[i + 1]);
            if FILLERS.contains(&pair.as_str()) {
                i += 2;
                continue;
            }
        }
        if FILLERS.contains(&words[i].as_str()) {
            i += 1;
            continue;
        }
        out.push(words[i].clone());
        i += 1;
    }
    out
}

/// Reads one English integer starting at `words[start]`. Returns the value
/// and the number of words consumed. Supports 0–999.
///
/// Grammar: `number := <small> | <tens> [<small>] | <small> hundred [and] <rest>`
///          where rest is a `number` ≤ 99.
fn read_english_int(words: &[String], start: usize) -> Option<(u32, usize)> {
    if start >= words.len() {
        return None;
    }
    // Digit literal — nothing to convert.
    if words[start].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut total: u32 = 0;
    let mut consumed: usize = 0;
    let mut matched_any = false;

    // Parse optional hundreds: "<small> hundred"
    if let Some(n) = small_number(&words[start]) {
        if words.get(start + 1).map(|w| w.as_str()) == Some("hundred") {
            total = n * 100;
            consumed = 2;
            matched_any = true;
            // Optional "and"
            if words.get(start + consumed).map(|w| w.as_str()) == Some("and") {
                consumed += 1;
            }
        }
    }

    // Parse tens + optional ones: "twenty[-five]" or "twenty five"
    let cursor = start + consumed;
    if cursor < words.len() {
        if let Some(t) = tens_number(&words[cursor]) {
            total += t;
            consumed += 1;
            matched_any = true;
            // Optional ones digit directly after tens.
            if let Some(ones) = words.get(start + consumed).and_then(|w| small_number(w)) {
                if ones < 10 {
                    total += ones;
                    consumed += 1;
                }
            }
        } else if let Some(s) = small_number(&words[cursor]) {
            total += s;
            consumed += 1;
            matched_any = true;
        }
    }

    if matched_any {
        Some((total, consumed))
    } else {
        None
    }
}

/// Rewrites sequences of spoken numbers into digit tokens.
fn merge_spelled_numbers(words: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if let Some((value, consumed)) = read_english_int(words, i) {
            out.push(value.to_string());
            i += consumed;
            continue;
        }
        out.push(words[i].clone());
        i += 1;
    }
    out
}

/// Rewrites ordinal prefixes ("first", "1st", "i") to digits when the
/// following token looks like it could start a book alias (alphabetic token,
/// not a number or operator). Bare "i" is only rewritten when the next token
/// is a known numbered-book stem to avoid clobbering the pronoun "I".
fn rewrite_ordinal_prefixes(words: &[String]) -> Vec<String> {
    // Stems that legitimately take ordinal prefixes in the 66-book canon.
    const NUMBERED_STEMS: &[&str] = &[
        "samuel", "sam", "sm", "sa",
        "kings", "kgs", "ki",
        "chronicles", "chron", "chr", "ch",
        "corinthians", "cor", "co",
        "thessalonians", "thess", "thes", "th",
        "timothy", "tim", "ti",
        "peter", "pet", "pe", "pt",
        "john", "jn", "jhn", "jo",
    ];

    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        if let Some(digit) = ordinal_prefix(word) {
            let next = words.get(i + 1).map(|w| w.as_str()).unwrap_or("");
            let next_is_stem = NUMBERED_STEMS.contains(&next);
            let is_pronoun_i = word == "i" && !next_is_stem;
            if next_is_stem && !is_pronoun_i {
                out.push(digit.to_string());
                i += 1;
                continue;
            }
            if word != "i" && !next_is_stem {
                // "first", "1st" with nothing scripture-ish following — keep
                // the raw word; grammar parser will ignore it.
                out.push(word.clone());
                i += 1;
                continue;
            }
        }
        out.push(word.clone());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(input: &str) -> String {
        TranscriptNormalizer.normalize(input).text
    }

    #[test]
    fn lowercases_and_folds_diacritics() {
        assert_eq!(norm("JOHN 3:16"), "john 3:16");
        assert_eq!(norm("Jesús said"), "jesus said");
    }

    #[test]
    fn strips_fillers() {
        assert_eq!(norm("um you know romans 8:28"), "romans 8:28");
    }

    #[test]
    fn converts_ordinal_prefix_before_numbered_book() {
        assert_eq!(norm("first john 4:8"), "1 john 4:8");
        assert_eq!(norm("second corinthians 5:17"), "2 corinthians 5:17");
        assert_eq!(norm("1st peter 5:7"), "1 peter 5:7");
    }

    #[test]
    fn does_not_rewrite_pronoun_i() {
        assert_eq!(norm("i am the way"), "i am the way");
    }

    #[test]
    fn rewrites_roman_numeral_before_book() {
        assert_eq!(norm("i john 4:8"), "1 john 4:8");
        assert_eq!(norm("ii timothy 3:16"), "2 timothy 3:16");
    }

    #[test]
    fn spells_numbers_to_digits() {
        assert_eq!(norm("chapter three verse sixteen"), "chapter 3 verse 16");
        assert_eq!(norm("romans eight twenty eight"), "romans 8 28");
        assert_eq!(
            norm("psalm one hundred and nineteen"),
            "psalm 119"
        );
        assert_eq!(norm("john three sixteen"), "john 3 16");
    }

    #[test]
    fn handles_twenty_five_combinations() {
        assert_eq!(norm("psalm twenty three verse four"), "psalm 23 verse 4");
    }

    #[test]
    fn preserves_colon_dash_inside_numbers() {
        assert_eq!(norm("romans 8:28-30"), "romans 8:28-30");
    }

    #[test]
    fn strips_surrounding_punctuation() {
        assert_eq!(norm("Open to John 3:16,"), "open to john 3:16");
        assert_eq!(norm("(John 3:16)"), "john 3:16");
    }
}
