//! Canonical Bible book table and alias index.
//!
//! Every entry lists the canonical name (Protestant 66-book canon) plus all
//! aliases the grammar parser will accept. Aliases are stored lower-cased and
//! diacritic-folded so [`fold_for_matching`](crate::fold_for_matching) output
//! can be compared directly.
//!
//! Aliases cover:
//! * canonical long name (`"1 John"`, `"Song of Solomon"`)
//! * numbered-prefix variants (`"1st john"`, `"first john"`, `"i john"`)
//! * common short forms (`"1jn"`, `"1 jn"`, `"jn"`, `"phil"`, `"eph"`)
//! * "Psalm" singular alias for `Psalms`
//! * "Song of Songs" for `Song of Solomon`

/// One canonical book plus all accepted aliases (lower-cased, diacritic-folded).
///
/// `max_chapter` is the highest chapter number for this book; the grammar
/// parser uses it as a sanity clamp when the STT engine mis-hears a number.
#[derive(Clone, Debug)]
pub struct BookEntry {
    pub canonical: &'static str,
    pub aliases: &'static [&'static str],
    pub max_chapter: u16,
}

/// All 66 canonical books, Old Testament first. Aliases are intentionally
/// exhaustive: the parser picks the *longest* matching alias so numbered
/// books ("1 John", "first john") win over their unnumbered base ("John").
pub const BOOKS: &[BookEntry] = &[
    // ---------- Pentateuch ----------
    BookEntry { canonical: "Genesis",   aliases: &["genesis", "gen", "ge", "gn"], max_chapter: 50 },
    BookEntry { canonical: "Exodus",    aliases: &["exodus", "exod", "exo", "ex"], max_chapter: 40 },
    BookEntry { canonical: "Leviticus", aliases: &["leviticus", "lev", "lv"], max_chapter: 27 },
    BookEntry { canonical: "Numbers",   aliases: &["numbers", "num", "nm", "nu"], max_chapter: 36 },
    BookEntry { canonical: "Deuteronomy", aliases: &["deuteronomy", "deut", "deu", "dt",
        "dutronomy", "deutoronomy", "deutronomy", // Nigerian Whisper phonetics
    ], max_chapter: 34 },

    // ---------- History ----------
    BookEntry { canonical: "Joshua", aliases: &["joshua", "josh", "jos", "jsh"], max_chapter: 24 },
    BookEntry { canonical: "Judges", aliases: &["judges", "judg", "jdg", "jg"], max_chapter: 21 },
    BookEntry { canonical: "Ruth",   aliases: &["ruth", "rth", "ru"], max_chapter: 4 },

    BookEntry { canonical: "1 Samuel", aliases: &[
        "1 samuel", "1samuel", "1 sam", "1sam", "1 sa", "1sa", "1 sm",
        "first samuel", "1st samuel", "i samuel", "i sam",
    ], max_chapter: 31 },
    BookEntry { canonical: "2 Samuel", aliases: &[
        "2 samuel", "2samuel", "2 sam", "2sam", "2 sa", "2sa", "2 sm",
        "second samuel", "2nd samuel", "ii samuel", "ii sam",
    ], max_chapter: 24 },

    BookEntry { canonical: "1 Kings", aliases: &[
        "1 kings", "1kings", "1 kgs", "1kgs", "1 ki", "1ki",
        "first kings", "1st kings", "i kings",
    ], max_chapter: 22 },
    BookEntry { canonical: "2 Kings", aliases: &[
        "2 kings", "2kings", "2 kgs", "2kgs", "2 ki", "2ki",
        "second kings", "2nd kings", "ii kings",
    ], max_chapter: 25 },

    BookEntry { canonical: "1 Chronicles", aliases: &[
        "1 chronicles", "1chronicles", "1 chron", "1chron", "1 chr", "1chr", "1 ch", "1ch",
        "first chronicles", "1st chronicles", "i chronicles", "i chron",
    ], max_chapter: 29 },
    BookEntry { canonical: "2 Chronicles", aliases: &[
        "2 chronicles", "2chronicles", "2 chron", "2chron", "2 chr", "2chr", "2 ch", "2ch",
        "second chronicles", "2nd chronicles", "ii chronicles", "ii chron",
    ], max_chapter: 36 },

    BookEntry { canonical: "Ezra",     aliases: &["ezra", "ezr", "ez"], max_chapter: 10 },
    BookEntry { canonical: "Nehemiah", aliases: &["nehemiah", "neh", "ne"], max_chapter: 13 },
    BookEntry { canonical: "Esther",   aliases: &["esther", "esth", "est"], max_chapter: 10 },

    // ---------- Poetry & Wisdom ----------
    BookEntry { canonical: "Job",    aliases: &["job", "jb"], max_chapter: 42 },
    BookEntry { canonical: "Psalms", aliases: &["psalms", "psalm", "psa", "pss", "ps"], max_chapter: 150 },
    BookEntry { canonical: "Proverbs",     aliases: &["proverbs", "prov", "prv", "pr"], max_chapter: 31 },
    BookEntry { canonical: "Ecclesiastes", aliases: &["ecclesiastes", "eccl", "ecc", "ec", "qoh"], max_chapter: 12 },
    BookEntry { canonical: "Song of Solomon", aliases: &[
        "song of solomon", "song of songs", "canticles",
        "song", "sos", "song sol", "song of sol",
    ], max_chapter: 8 },

    // ---------- Major Prophets ----------
    BookEntry { canonical: "Isaiah",       aliases: &["isaiah", "isa", "is",
        "izaya", "ezaya", "isaya", "ezaia",  // Nigerian/West African Whisper variants
    ], max_chapter: 66 },
    BookEntry { canonical: "Jeremiah",     aliases: &["jeremiah", "jer", "je", "jr"], max_chapter: 52 },
    BookEntry { canonical: "Lamentations", aliases: &["lamentations", "lam", "la"], max_chapter: 5 },
    BookEntry { canonical: "Ezekiel",      aliases: &["ezekiel", "ezek", "eze", "ezk"], max_chapter: 48 },
    BookEntry { canonical: "Daniel",       aliases: &["daniel", "dan", "dn", "da"], max_chapter: 12 },

    // ---------- Minor Prophets ----------
    BookEntry { canonical: "Hosea",     aliases: &["hosea", "hos", "ho"], max_chapter: 14 },
    BookEntry { canonical: "Joel",      aliases: &["joel", "jl"], max_chapter: 3 },
    BookEntry { canonical: "Amos",      aliases: &["amos", "am"], max_chapter: 9 },
    BookEntry { canonical: "Obadiah",   aliases: &["obadiah", "obad", "oba", "ob"], max_chapter: 1 },
    BookEntry { canonical: "Jonah",     aliases: &["jonah", "jon", "jnh"], max_chapter: 4 },
    BookEntry { canonical: "Micah",     aliases: &["micah", "mic", "mi"], max_chapter: 7 },
    BookEntry { canonical: "Nahum",     aliases: &["nahum", "nah", "na"], max_chapter: 3 },
    BookEntry { canonical: "Habakkuk",  aliases: &["habakkuk", "hab", "hb"], max_chapter: 3 },
    BookEntry { canonical: "Zephaniah", aliases: &["zephaniah", "zeph", "zep", "zp"], max_chapter: 3 },
    BookEntry { canonical: "Haggai",    aliases: &["haggai", "hag", "hg"], max_chapter: 2 },
    BookEntry { canonical: "Zechariah", aliases: &["zechariah", "zech", "zec", "zc"], max_chapter: 14 },
    BookEntry { canonical: "Malachi",   aliases: &["malachi", "mal", "ml"], max_chapter: 4 },

    // ---------- Gospels & Acts ----------
    BookEntry { canonical: "Matthew", aliases: &["matthew", "matt", "mat", "mt",
        "matius", "matiyu", "matu",  // Nigerian Whisper phonetics
    ], max_chapter: 28 },
    BookEntry { canonical: "Mark",    aliases: &["mark", "mrk", "mk", "mr"], max_chapter: 16 },
    BookEntry { canonical: "Luke",    aliases: &["luke", "luk", "lk"], max_chapter: 24 },
    BookEntry { canonical: "John",    aliases: &["john", "jhn", "jn", "jo"], max_chapter: 21 },
    BookEntry { canonical: "Acts",    aliases: &["acts", "act", "ac", "acts of the apostles"], max_chapter: 28 },

    // ---------- Pauline Epistles ----------
    BookEntry { canonical: "Romans",      aliases: &["romans", "rom", "ro", "rm"], max_chapter: 16 },
    BookEntry { canonical: "1 Corinthians", aliases: &[
        "1 corinthians", "1corinthians", "1 cor", "1cor", "1 co", "1co",
        "first corinthians", "1st corinthians", "i corinthians", "i cor",
    ], max_chapter: 16 },
    BookEntry { canonical: "2 Corinthians", aliases: &[
        "2 corinthians", "2corinthians", "2 cor", "2cor", "2 co", "2co",
        "second corinthians", "2nd corinthians", "ii corinthians", "ii cor",
    ], max_chapter: 13 },
    BookEntry { canonical: "Galatians",   aliases: &["galatians", "gal", "ga"], max_chapter: 6 },
    BookEntry { canonical: "Ephesians",   aliases: &["ephesians", "eph", "ep",
        "efishans", "efesians",  // Nigerian Whisper phonetics
    ], max_chapter: 6 },
    BookEntry { canonical: "Philippians", aliases: &["philippians", "phil", "php", "pp",
        "filipians", "phillipians", "filipins",  // Nigerian Whisper phonetics
    ], max_chapter: 4 },
    BookEntry { canonical: "Colossians",  aliases: &["colossians", "col", "co"], max_chapter: 4 },
    BookEntry { canonical: "1 Thessalonians", aliases: &[
        "1 thessalonians", "1thessalonians", "1 thess", "1thess", "1 thes", "1thes", "1 th", "1th",
        "first thessalonians", "1st thessalonians", "i thessalonians", "i thess",
    ], max_chapter: 5 },
    BookEntry { canonical: "2 Thessalonians", aliases: &[
        "2 thessalonians", "2thessalonians", "2 thess", "2thess", "2 thes", "2thes", "2 th", "2th",
        "second thessalonians", "2nd thessalonians", "ii thessalonians", "ii thess",
    ], max_chapter: 3 },
    BookEntry { canonical: "1 Timothy", aliases: &[
        "1 timothy", "1timothy", "1 tim", "1tim", "1 ti", "1ti",
        "first timothy", "1st timothy", "i timothy", "i tim",
    ], max_chapter: 6 },
    BookEntry { canonical: "2 Timothy", aliases: &[
        "2 timothy", "2timothy", "2 tim", "2tim", "2 ti", "2ti",
        "second timothy", "2nd timothy", "ii timothy", "ii tim",
    ], max_chapter: 4 },
    BookEntry { canonical: "Titus",    aliases: &["titus", "tit", "ti"], max_chapter: 3 },
    BookEntry { canonical: "Philemon", aliases: &["philemon", "phlm", "phm", "pm"], max_chapter: 1 },

    // ---------- General Epistles & Revelation ----------
    BookEntry { canonical: "Hebrews", aliases: &["hebrews", "heb", "he"], max_chapter: 13 },
    BookEntry { canonical: "James",   aliases: &["james", "jas", "jm"], max_chapter: 5 },
    BookEntry { canonical: "1 Peter", aliases: &[
        "1 peter", "1peter", "1 pet", "1pet", "1 pe", "1pe", "1 pt", "1pt",
        "first peter", "1st peter", "i peter", "i pet",
    ], max_chapter: 5 },
    BookEntry { canonical: "2 Peter", aliases: &[
        "2 peter", "2peter", "2 pet", "2pet", "2 pe", "2pe", "2 pt", "2pt",
        "second peter", "2nd peter", "ii peter", "ii pet",
    ], max_chapter: 3 },
    BookEntry { canonical: "1 John", aliases: &[
        "1 john", "1john", "1 jn", "1jn", "1 jo", "1jo",
        "first john", "1st john", "i john", "i jn",
    ], max_chapter: 5 },
    BookEntry { canonical: "2 John", aliases: &[
        "2 john", "2john", "2 jn", "2jn", "2 jo", "2jo",
        "second john", "2nd john", "ii john", "ii jn",
    ], max_chapter: 1 },
    BookEntry { canonical: "3 John", aliases: &[
        "3 john", "3john", "3 jn", "3jn", "3 jo", "3jo",
        "third john", "3rd john", "iii john", "iii jn",
    ], max_chapter: 1 },
    BookEntry { canonical: "Jude",       aliases: &["jude", "jud", "jd"], max_chapter: 1 },
    BookEntry { canonical: "Revelation", aliases: &[
        "revelation", "revelations", "rev", "re", "rv",
        "apocalypse", "apoc",
        "revelashan", "revelatian",  // Nigerian Whisper phonetics
    ], max_chapter: 22 },
];

/// Returns the canonical book for a folded alias, or `None` if no match.
/// Uses exact equality — callers should pass a single token or whitespace-joined
/// multi-token alias already folded with [`crate::fold_for_matching`].
pub fn lookup_exact(alias_folded: &str) -> Option<&'static BookEntry> {
    BOOKS.iter().find(|book| book.aliases.contains(&alias_folded))
}

/// Bare aliases (no numeric prefix) that legitimately resolve to MULTIPLE
/// canonical books. The grammar parser uses this to surface a disambiguation
/// candidate set instead of silently dropping the input or guessing.
///
/// Example: a transcript saying "Chronicles 7:14" — there is no canonical
/// "Chronicles" book, but both 1 Chr (29 chap) and 2 Chr (36 chap) have a
/// chapter 7. The parser will return both candidates with
/// `needs_disambiguation = true`.
///
/// `john` is intentionally excluded — the unprefixed form is the canonical
/// gospel; numeric epistles already require a prefix to match.
pub const BARE_AMBIGUOUS_BOOKS: &[(&str, &[&str])] = &[
    ("chronicles",      &["1 Chronicles", "2 Chronicles"]),
    ("samuel",          &["1 Samuel", "2 Samuel"]),
    ("kings",           &["1 Kings", "2 Kings"]),
    ("corinthians",     &["1 Corinthians", "2 Corinthians"]),
    ("thessalonians",   &["1 Thessalonians", "2 Thessalonians"]),
    ("timothy",         &["1 Timothy", "2 Timothy"]),
    ("peter",           &["1 Peter", "2 Peter"]),
];

/// If `alias_folded` is a bare ambiguous book token (e.g. `"chronicles"`),
/// returns the list of canonical book names it could resolve to. Otherwise
/// returns `None`.
pub fn lookup_ambiguous(alias_folded: &str) -> Option<&'static [&'static str]> {
    BARE_AMBIGUOUS_BOOKS
        .iter()
        .find(|(alias, _)| *alias == alias_folded)
        .map(|(_, books)| *books)
}

/// Resolves a canonical book name (e.g. `"1 Chronicles"`) to its `BookEntry`
/// for canon-range validation. Linear scan; the table is small.
pub fn lookup_canonical(canonical: &str) -> Option<&'static BookEntry> {
    BOOKS.iter().find(|b| b.canonical == canonical)
}

/// Returns every (alias, book) pair whose alias starts at `start_token_idx`
/// in the input token slice. The longest match wins so `1 john` beats `john`.
///
/// `tokens` must already be lower-cased and folded. Multi-word aliases like
/// `"first john"` are matched by joining consecutive tokens with spaces.
pub fn longest_match_at(
    tokens: &[&str],
    start: usize,
) -> Option<(&'static BookEntry, usize /* tokens consumed */, String /* alias */)> {
    if start >= tokens.len() {
        return None;
    }
    // Try descending window sizes so longer aliases (e.g. "song of solomon",
    // "first john") beat single-token fallbacks ("song", "john").
    let max_window = (tokens.len() - start).min(4);
    for window in (1..=max_window).rev() {
        let joined = tokens[start..start + window].join(" ");
        if let Some(book) = lookup_exact(&joined) {
            return Some((book, window, joined));
        }
    }
    None
}
