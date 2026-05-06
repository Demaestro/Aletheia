//! Local Bible knowledge phrase catalog.
//!
//! This layer is intentionally deterministic and offline. It expands curated
//! Bible events, people, places, and themes into many natural sermon phrases
//! so live detection can resolve "the Beatitudes", "dry bones", or "woman
//! with the issue of blood" without waiting for a cloud model.

use crate::fold_for_matching;

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogPhraseMatch {
    pub reference: &'static str,
    pub alias: String,
    pub score: f32,
}

#[derive(Clone, Copy, Debug)]
struct CatalogEntry {
    reference: &'static str,
    title: &'static str,
    aliases: &'static [&'static str],
    themes: &'static [&'static str],
    people: &'static [&'static str],
    places: &'static [&'static str],
    score: f32,
}

/// Convenience wrapper: folds the raw query before delegating to
/// [`detect_catalog_matches`]. Use this from outside the crate when you
/// have not already folded the input.
pub fn detect_catalog_matches_raw(text: &str) -> Vec<CatalogPhraseMatch> {
    detect_catalog_matches(&fold_for_matching(text))
}

pub fn detect_catalog_matches(text_folded: &str) -> Vec<CatalogPhraseMatch> {
    let mut matches = Vec::new();

    for entry in CATALOG {
        let mut best_alias: Option<String> = None;
        for alias in expanded_aliases(entry) {
            let folded = fold_for_matching(&alias);
            if text_folded.contains(&folded) {
                best_alias = Some(alias);
                break;
            }
        }

        if let Some(alias) = best_alias {
            matches.push(CatalogPhraseMatch {
                reference: entry.reference,
                alias,
                score: entry.score,
            });
        }
    }

    for phrase_match in detect_semantic_matches(text_folded) {
        if matches
            .iter()
            .any(|existing| existing.reference == phrase_match.reference)
        {
            continue;
        }
        matches.push(phrase_match);
    }

    matches.sort_by(|left, right| right.score.total_cmp(&left.score));
    matches
}

#[cfg(test)]
pub(crate) fn catalog_phrase_count() -> usize {
    CATALOG
        .iter()
        .map(|entry| expanded_aliases(entry).len())
        .sum()
}

/// A passage-shaped catalog entry: the reference plus the curated aliases,
/// titles, themes, and people. The semantic precompute lane uses this to
/// build a pseudo-document per passage so a paraphrase like "the story of
/// the prodigal son returning home" matches the entire pericope, not just
/// the single anchor verse.
#[derive(Clone, Debug)]
pub struct PassageDescriptor {
    pub reference: &'static str,
    pub title: &'static str,
    pub aliases: Vec<String>,
    pub themes: &'static [&'static str],
    pub people: &'static [&'static str],
    pub places: &'static [&'static str],
    pub score: f32,
}

/// Returns every catalog entry as a `PassageDescriptor`. Order matches
/// `CATALOG`. Used by the scripture-search semantic precompute to seed
/// passage-level pseudo-documents.
pub fn passage_descriptors() -> Vec<PassageDescriptor> {
    CATALOG
        .iter()
        .map(|entry| PassageDescriptor {
            reference: entry.reference,
            title: entry.title,
            aliases: expanded_aliases(entry),
            themes: entry.themes,
            people: entry.people,
            places: entry.places,
            score: entry.score,
        })
        .collect()
}

fn expanded_aliases(entry: &CatalogEntry) -> Vec<String> {
    let mut out = Vec::new();
    push_unique(&mut out, entry.title);
    push_folded_variant(&mut out, entry.title);
    for alias in entry.aliases {
        push_unique(&mut out, alias);
        push_folded_variant(&mut out, alias);
    }

    for seed in std::iter::once(entry.title).chain(entry.aliases.iter().copied()) {
        push_seed_templates(
            &mut out,
            seed,
            &[
                "the {seed}",
                "story of {seed}",
                "scripture about {seed}",
                "bible verse about {seed}",
                "preaching about {seed}",
                "sermon about {seed}",
                "teaching on {seed}",
                "message on {seed}",
                "{seed} in the bible",
                "where the bible talks about {seed}",
                "open the passage about {seed}",
                "turn to the passage about {seed}",
                "bring up the scripture about {seed}",
                "show the verse about {seed}",
                "find the verse for {seed}",
                "where is {seed} in scripture",
                "where do we find {seed}",
                "the place where it says {seed}",
                "the verse where it says {seed}",
                "the chapter about {seed}",
                "bible story about {seed}",
                "bible story where {seed}",
                "scripture where {seed}",
                "scripture that says {seed}",
                "scripture that talks about {seed}",
                "scripture connected to {seed}",
                "scripture reference for {seed}",
                "sermon illustration about {seed}",
                "message about {seed}",
                "lesson from {seed}",
                "what happened with {seed}",
                "what the bible says concerning {seed}",
                "God's word about {seed}",
                "Gods word about {seed}",
                "Jesus teaching about {seed}",
                "teaching from {seed}",
                "preaching from {seed}",
                "remember {seed}",
                "when the preacher mentions {seed}",
                "when pastor talks about {seed}",
                "the account of {seed}",
                "the narrative of {seed}",
                "the miracle of {seed}",
                "the parable of {seed}",
                "the doctrine of {seed}",
                "the prophecy about {seed}",
                "the promise about {seed}",
                "the command about {seed}",
                "the warning about {seed}",
                "the example of {seed}",
                "the testimony of {seed}",
                "the revelation of {seed}",
                "the prayer about {seed}",
                "the worship text about {seed}",
                "the service reading about {seed}",
                "call up {seed}",
                "pull up {seed}",
                "display {seed}",
                "project {seed}",
                "take us to {seed}",
                "let us read {seed}",
                "let's read {seed}",
                "I want {seed}",
                "I need {seed}",
            ],
        );
    }

    for theme in entry.themes {
        if theme.split_whitespace().count() > 1 {
            push_unique(&mut out, theme);
            push_folded_variant(&mut out, theme);
        }
        push_seed_templates(
            &mut out,
            theme,
            &[
                "scripture about {seed}",
                "bible verse about {seed}",
                "preaching on {seed}",
                "sermon on {seed}",
                "teaching about {seed}",
                "what the bible says about {seed}",
                "{seed} in the bible",
                "passage about {seed}",
                "verse about {seed}",
                "message about {seed}",
                "lesson about {seed}",
                "God's word concerning {seed}",
                "Gods word concerning {seed}",
                "where scripture teaches {seed}",
                "where the bible explains {seed}",
                "where Jesus teaches {seed}",
                "scripture for a sermon on {seed}",
                "scripture for preaching on {seed}",
                "bible passage for {seed}",
                "bible reference for {seed}",
                "context for {seed}",
                "theme of {seed}",
                "spiritual lesson about {seed}",
                "church teaching about {seed}",
                "Sunday message about {seed}",
                "worship reading about {seed}",
                "devotional reading about {seed}",
                "Bible answer about {seed}",
                "biblical example of {seed}",
                "biblical promise about {seed}",
                "biblical warning about {seed}",
                "biblical command about {seed}",
                "biblical story about {seed}",
            ],
        );
    }

    for person in entry.people {
        for seed in std::iter::once(entry.title)
            .chain(entry.aliases.iter().copied())
            .take(4)
        {
            push_unique(&mut out, &format!("{person} and {seed}"));
            push_unique(&mut out, &format!("{seed} with {person}"));
            push_unique(&mut out, &format!("where {person} is connected to {seed}"));
            push_unique(&mut out, &format!("when {person} experienced {seed}"));
            push_unique(&mut out, &format!("the story of {person} and {seed}"));
            push_unique(&mut out, &format!("what happened to {person} with {seed}"));
            push_unique(&mut out, &format!("what happened when {person} met {seed}"));
        }
        for theme in entry.themes {
            push_unique(&mut out, &format!("{person} and {theme}"));
            push_unique(&mut out, &format!("{person} with {theme}"));
            push_unique(&mut out, &format!("story of {person} and {theme}"));
            push_unique(
                &mut out,
                &format!("what happened to {person} about {theme}"),
            );
        }
        for place in entry.places {
            push_unique(&mut out, &format!("{person} in {place}"));
            push_unique(&mut out, &format!("{person} at {place}"));
            push_unique(&mut out, &format!("what happened to {person} in {place}"));
            push_unique(&mut out, &format!("what happened to {person} at {place}"));
        }
    }

    for place in entry.places {
        push_unique(&mut out, &format!("what happened at {place}"));
        push_unique(&mut out, &format!("what happened in {place}"));
        push_unique(&mut out, &format!("scripture about {place}"));
        push_unique(&mut out, &format!("bible story at {place}"));
    }

    out
}

fn push_seed_templates(out: &mut Vec<String>, seed: &str, templates: &[&str]) {
    for template in templates {
        push_unique(out, &template.replace("{seed}", seed));
    }
}

fn push_folded_variant(out: &mut Vec<String>, value: &str) {
    let folded = fold_for_matching(value);
    if folded != value {
        push_unique(out, &folded);
    }
}

fn detect_semantic_matches(text_folded: &str) -> Vec<CatalogPhraseMatch> {
    let query_tokens = normalized_tokens(text_folded);
    if query_tokens.len() < 2 {
        return Vec::new();
    }

    let mut scored: Vec<CatalogPhraseMatch> = CATALOG
        .iter()
        .filter_map(|entry| {
            let semantic = semantic_score(&query_tokens, entry)?;
            Some(CatalogPhraseMatch {
                reference: entry.reference,
                alias: format!("semantic: {}", entry.title),
                score: semantic.min(entry.score - 0.02),
            })
        })
        .collect();

    scored.sort_by(|left, right| right.score.total_cmp(&left.score));
    scored.truncate(3);
    scored
}

fn semantic_score(query_tokens: &[String], entry: &CatalogEntry) -> Option<f32> {
    let entry_tokens = entry_tokens(entry);
    let matched = query_tokens
        .iter()
        .filter(|token| entry_tokens.iter().any(|entry_token| entry_token == *token))
        .count();

    if matched < 2 {
        return None;
    }

    let coverage = matched as f32 / query_tokens.len() as f32;
    let entity_bonus = entity_context_bonus(query_tokens, entry).unwrap_or(0.0);
    let title_tokens = normalized_tokens(entry.title);
    let title_hits = query_tokens
        .iter()
        .filter(|token| title_tokens.iter().any(|title| title == *token))
        .count();
    let title_bonus = if title_hits >= 2 {
        0.1
    } else if title_hits == 1 {
        0.04
    } else {
        0.0
    };
    let rare_bonus = if query_tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "beatitude"
                | "bartimaeus"
                | "zacchaeus"
                | "joseph"
                | "slavery"
                | "slave"
                | "gideon"
                | "naaman"
                | "lazarus"
                | "pentecost"
                | "goliath"
                | "jericho"
                | "moriah"
                | "carmel"
                | "samaria"
                | "samaritan"
        )
    }) {
        0.12
    } else {
        0.0
    };

    let score = 0.58 + (coverage * 0.24) + title_bonus + rare_bonus + entity_bonus;
    if score >= 0.74 { Some(score) } else { None }
}

fn entity_context_bonus(query_tokens: &[String], entry: &CatalogEntry) -> Option<f32> {
    let person_tokens = people_tokens(entry);
    if person_tokens.is_empty() {
        return None;
    }

    let context_tokens = entry_context_tokens(entry);
    let person_hits = query_tokens
        .iter()
        .filter(|token| person_tokens.iter().any(|person| person == *token))
        .count();
    if person_hits == 0 {
        return None;
    }

    let context_hits = query_tokens
        .iter()
        .filter(|token| context_tokens.iter().any(|context| context == *token))
        .count();
    if context_hits == 0 {
        return None;
    }

    // General Bible-character resolver: a named person plus story/theme/place
    // evidence is stronger than broad lexical overlap, but still below exact
    // reference and quote lanes.
    let bonus = 0.10 + (context_hits.min(3) as f32 * 0.045) + (person_hits.min(2) as f32 * 0.025);
    Some(bonus.min(0.24))
}

fn people_tokens(entry: &CatalogEntry) -> Vec<String> {
    let mut tokens = Vec::new();
    for person in entry.people {
        for token in normalized_tokens(person) {
            push_token_unique(&mut tokens, token);
        }
    }
    tokens
}

fn entry_context_tokens(entry: &CatalogEntry) -> Vec<String> {
    let mut tokens = Vec::new();
    for text in std::iter::once(entry.title)
        .chain(entry.aliases.iter().copied())
        .chain(entry.themes.iter().copied())
        .chain(entry.places.iter().copied())
    {
        for token in normalized_tokens(text) {
            push_token_unique(&mut tokens, token);
        }
    }
    tokens
}

fn entry_tokens(entry: &CatalogEntry) -> Vec<String> {
    let mut tokens = Vec::new();
    for text in std::iter::once(entry.title)
        .chain(entry.aliases.iter().copied())
        .chain(entry.themes.iter().copied())
        .chain(entry.people.iter().copied())
        .chain(entry.places.iter().copied())
    {
        for token in normalized_tokens(text) {
            push_token_unique(&mut tokens, token);
        }
    }
    tokens
}

fn normalized_tokens(input: &str) -> Vec<String> {
    let folded = fold_for_matching(input);
    let mut tokens = Vec::new();
    for raw in folded
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        if let Some(token) = semantic_token(raw) {
            push_token_unique(&mut tokens, token);
        }
    }
    tokens
}

fn semantic_token(raw: &str) -> Option<String> {
    if STOPWORDS.contains(&raw) {
        return None;
    }

    let canonical = match raw {
        "ye" | "thou" | "thee" => "you",
        "thy" | "thine" => "your",
        "hast" => "have",
        "hath" => "has",
        "dost" | "doest" | "doth" => "do",
        "unto" => "to",
        "beattiudes" | "beatitudes" | "attitudes" => "beatitude",
        "blessed" | "blessing" | "blessings" => "bless",
        "teachings" | "teaching" | "taught" => "teach",
        "preaching" | "preached" => "preach",
        "scriptures" | "verses" => "scripture",
        "stories" => "story",
        "people" => "person",
        "children" => "child",
        "women" => "woman",
        "shepherds" => "shepherd",
        "prayers" | "praying" | "prayed" => "prayer",
        "healed" | "healing" => "heal",
        "washed" | "washing" => "wash",
        "delivered" | "deliverance" => "deliver",
        "forgiven" | "forgiveness" => "forgive",
        "slavery" | "slaves" => "slave",
        "selling" | "sold" => "sell",
        "betrayed" | "betrayal" => "betray",
        "thrown" | "throwing" | "cast" | "casting" => "throw",
        "killed" | "slew" | "slain" | "murdered" | "murder" => "kill",
        "died" | "death" => "die",
        "ran" | "running" | "fled" | "fleeing" => "run",
        "swallowed" | "swallowing" => "swallow",
        "climbed" | "climbing" => "climb",
        "prisons" | "imprisoned" => "prison",
        "lions" => "lion",
        "mouths" => "mouth",
        "fires" | "fiery" => "fire",
        "furnaces" => "furnace",
        "widows" => "widow",
        "oils" => "oil",
        "vessels" => "vessel",
        "whirlwinds" => "whirlwind",
        "chariots" => "chariot",
        "rivers" => "river",
        "seas" => "sea",
        "storms" => "storm",
        "waves" => "wave",
        "trees" => "tree",
        "wells" => "well",
        "dreamer" | "dreams" => "dream",
        "brothers" => "brother",
        "egyptian" | "egyptians" => "egypt",
        "pit" | "pits" => "pit",
        "faithful" => "faith",
        "peaceful" => "peace",
        "merciful" => "mercy",
        _ => raw,
    };

    let stemmed = stem_token(canonical);
    if stemmed.len() < 3 {
        None
    } else {
        Some(stemmed)
    }
}

fn stem_token(token: &str) -> String {
    for suffix in ["ing", "ed", "es", "s"] {
        if token.len() > suffix.len() + 3 && token.ends_with(suffix) {
            return token[..token.len() - suffix.len()].to_string();
        }
    }
    token.to_string()
}

fn push_token_unique(out: &mut Vec<String>, value: String) {
    if !out.iter().any(|existing| existing == &value) {
        out.push(value);
    }
}

fn push_unique(out: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !out.iter().any(|existing| existing == trimmed) {
        out.push(trimmed.to_string());
    }
}

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "he", "her", "him", "his", "i",
    "in", "is", "it", "me", "my", "of", "on", "or", "our", "she", "that", "the", "their", "them",
    "they", "this", "to", "was", "we", "what", "when", "where", "who", "with", "you", "your",
    "about", "talking", "speaker", "preacher", "pastor", "message", "sermon", "today", "please",
    "open", "bring", "show", "like", "ye", "thou", "thee", "thy", "thine", "unto",
];

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        reference: "Matthew 5:3-12",
        title: "Beatitudes",
        aliases: &[
            "the beatitudes",
            "beattiudes",
            "be attitudes",
            "blessed are the poor in spirit",
            "blessed are they that mourn",
            "blessed are the meek",
            "blessed are the pure in heart",
            "blessed are the peacemakers",
            "sermon on the mount blessings",
            "Jesus teaching about blessed people",
            "blessed people",
            "kingdom blessed people",
        ],
        themes: &[
            "kingdom blessing",
            "blessed people",
            "blessed life",
            "humility",
            "mercy",
            "purity of heart",
            "peacemaking",
        ],
        people: &["Jesus"],
        places: &["mountain", "sermon on the mount"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Matthew 5:14",
        title: "salt and light",
        aliases: &[
            "light of the world",
            "city set on a hill",
            "salt of the earth",
        ],
        themes: &["witness", "influence", "public faith"],
        people: &["Jesus"],
        places: &[],
        score: 0.86,
    },
    CatalogEntry {
        reference: "Matthew 6:9-13",
        title: "Lord's Prayer",
        aliases: &[
            "our father who art in heaven",
            "teach us to pray",
            "model prayer",
        ],
        themes: &["prayer", "forgiveness", "daily bread"],
        people: &["Jesus"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Matthew 6:33",
        title: "seek first the kingdom",
        aliases: &["seek ye first", "all these things shall be added"],
        themes: &["priority", "kingdom first", "worry"],
        people: &["Jesus"],
        places: &[],
        score: 1.02,
    },
    CatalogEntry {
        reference: "Matthew 7:7",
        title: "ask seek knock",
        aliases: &[
            "ask and it shall be given",
            "seek and ye shall find",
            "knock and it shall be opened",
        ],
        themes: &["prayer", "persistence", "receiving"],
        people: &["Jesus"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Matthew 7:24-27",
        title: "wise man built his house upon the rock",
        aliases: &[
            "house on the rock",
            "foolish man built on sand",
            "rock and sand",
        ],
        themes: &["obedience", "foundation", "storms of life"],
        people: &["Jesus"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Matthew 14:29",
        title: "Peter walks on water",
        aliases: &[
            "walking on water",
            "Peter stepped out of the boat",
            "Lord bid me come",
        ],
        themes: &["faith", "fear", "focus on Jesus"],
        people: &["Peter", "Jesus"],
        places: &["sea of Galilee"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Matthew 16:18",
        title: "upon this rock I will build my church",
        aliases: &["gates of hell shall not prevail", "I will build my church"],
        themes: &["church", "authority", "kingdom keys"],
        people: &["Peter", "Jesus"],
        places: &["Caesarea Philippi"],
        score: 0.87,
    },
    CatalogEntry {
        reference: "Matthew 17:2",
        title: "transfiguration",
        aliases: &[
            "mount of transfiguration",
            "Jesus face shone",
            "Moses and Elijah appeared",
        ],
        themes: &["glory", "revelation", "sonship"],
        people: &["Jesus", "Moses", "Elijah", "Peter"],
        places: &["high mountain"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Matthew 25:1-13",
        title: "ten virgins",
        aliases: &[
            "wise and foolish virgins",
            "oil in their lamps",
            "bridegroom came",
        ],
        themes: &["readiness", "watchfulness", "oil"],
        people: &["Jesus"],
        places: &[],
        score: 0.86,
    },
    CatalogEntry {
        reference: "Matthew 25:31-46",
        title: "I was hungry and you gave me food",
        aliases: &[
            "least of these",
            "when did we see you hungry",
            "clothe the naked",
        ],
        themes: &["compassion", "service", "judgment"],
        people: &["Jesus"],
        places: &[],
        score: 0.86,
    },
    CatalogEntry {
        reference: "Matthew 28:18-20",
        title: "Great Commission",
        aliases: &[
            "go ye therefore",
            "make disciples of all nations",
            "baptizing them",
        ],
        themes: &["mission", "evangelism", "discipleship"],
        people: &["Jesus"],
        places: &["Galilee"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Mark 4:35-41",
        title: "peace be still",
        aliases: &[
            "Jesus calms the storm",
            "storm on the sea",
            "wind and waves obey",
        ],
        themes: &["peace", "fear", "authority"],
        people: &["Jesus"],
        places: &["sea"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Mark 5:9",
        title: "legion demons",
        aliases: &["my name is legion", "madman of Gadara", "demons into pigs"],
        themes: &["deliverance", "freedom", "spiritual warfare"],
        people: &["Jesus"],
        places: &["Gadara", "tombs"],
        score: 0.86,
    },
    CatalogEntry {
        reference: "Mark 5:34",
        title: "woman with the issue of blood",
        aliases: &[
            "touched the hem of his garment",
            "twelve years issue of blood",
            "who touched me",
        ],
        themes: &["healing", "faith", "restoration"],
        people: &["Jesus"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Mark 10:47",
        title: "blind Bartimaeus",
        aliases: &[
            "son of David have mercy",
            "Bartimaeus received sight",
            "blind man by the roadside",
        ],
        themes: &["mercy", "healing", "persistent faith"],
        people: &["Bartimaeus", "Jesus"],
        places: &["Jericho"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Luke 1:38",
        title: "be it unto me according to thy word",
        aliases: &[
            "Mary said be it unto me",
            "annunciation",
            "angel Gabriel came to Mary",
        ],
        themes: &["surrender", "obedience", "calling"],
        people: &["Mary", "Gabriel"],
        places: &["Nazareth"],
        score: 0.86,
    },
    CatalogEntry {
        reference: "Luke 2:11",
        title: "birth of Jesus",
        aliases: &[
            "unto you is born this day",
            "Christ the Lord born",
            "shepherds heard good tidings",
        ],
        themes: &["incarnation", "Christmas", "good news"],
        people: &["Jesus", "Mary", "Joseph", "shepherds"],
        places: &["Bethlehem"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Luke 10:25-37",
        title: "Good Samaritan",
        aliases: &[
            "certain Samaritan",
            "man fell among thieves",
            "neighbor parable",
        ],
        themes: &["mercy", "neighbor", "compassion"],
        people: &["Samaritan", "Jesus"],
        places: &["road to Jericho"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Luke 10:42",
        title: "Mary and Martha",
        aliases: &[
            "one thing is needful",
            "Martha was careful and troubled",
            "Mary sat at Jesus feet",
        ],
        themes: &["devotion", "distraction", "worship"],
        people: &["Mary", "Martha", "Jesus"],
        places: &["Bethany"],
        score: 0.87,
    },
    CatalogEntry {
        reference: "Luke 15:3-7",
        title: "lost sheep",
        aliases: &[
            "ninety nine sheep",
            "one lost sheep",
            "leaves the ninety nine",
        ],
        themes: &["restoration", "seeking the lost", "repentance"],
        people: &["Jesus"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Luke 15:11-32",
        title: "prodigal son",
        aliases: &[
            "lost son came home",
            "father ran to him",
            "robe ring and shoes",
        ],
        themes: &["repentance", "forgiveness", "restoration"],
        people: &["prodigal son", "father"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Luke 19:5",
        title: "Zacchaeus",
        aliases: &[
            "Zacchaeus climbed a sycamore tree",
            "come down for today",
            "chief tax collector",
        ],
        themes: &["salvation", "repentance", "encounter"],
        people: &["Zacchaeus", "Jesus"],
        places: &["Jericho", "sycamore tree"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 1:1",
        title: "in the beginning was the Word",
        aliases: &[
            "the word was with God",
            "the word was God",
            "word became flesh",
        ],
        themes: &["Jesus as word", "incarnation", "divinity"],
        people: &["Jesus", "John"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 2:7",
        title: "water into wine",
        aliases: &[
            "wedding at Cana",
            "Jesus turned water to wine",
            "first miracle of Jesus",
        ],
        themes: &["miracle", "obedience", "joy"],
        people: &["Jesus", "Mary"],
        places: &["Cana"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 3:16",
        title: "for God so loved the world",
        aliases: &[
            "God so loved the world",
            "whosoever believeth",
            "everlasting life",
        ],
        themes: &["salvation", "love of God", "eternal life"],
        people: &["Jesus", "Nicodemus"],
        places: &[],
        score: 0.92,
    },
    CatalogEntry {
        reference: "John 4:7",
        title: "woman at the well",
        aliases: &["Samaritan woman", "woman by the well", "give me to drink"],
        themes: &["evangelism", "living water", "worship in spirit and truth"],
        people: &["Jesus", "Samaritan woman"],
        places: &["Sychar", "Jacob's well", "Samaria"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 6:35",
        title: "bread of life",
        aliases: &[
            "I am the bread of life",
            "he that cometh to me shall never hunger",
        ],
        themes: &["satisfaction", "provision", "Jesus"],
        people: &["Jesus"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 8:12",
        title: "light of the world",
        aliases: &["I am the light of the world", "shall not walk in darkness"],
        themes: &["light", "guidance", "truth"],
        people: &["Jesus"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 10:11",
        title: "good shepherd",
        aliases: &[
            "I am the good shepherd",
            "good shepherd gives his life",
            "my sheep hear my voice",
        ],
        themes: &["shepherd", "sacrifice", "guidance"],
        people: &["Jesus"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 11:38-44",
        title: "Lazarus come forth",
        aliases: &[
            "raising of Lazarus",
            "Lazarus was dead four days",
            "Jesus wept",
        ],
        themes: &["resurrection", "grief", "power over death"],
        people: &["Jesus", "Lazarus", "Mary", "Martha"],
        places: &["Bethany"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "John 14:6",
        title: "way truth and life",
        aliases: &[
            "I am the way the truth and the life",
            "no man cometh unto the father",
        ],
        themes: &["salvation", "truth", "access to God"],
        people: &["Jesus", "Thomas"],
        places: &[],
        score: 0.92,
    },
    CatalogEntry {
        reference: "Acts 2:1-4",
        title: "day of Pentecost",
        aliases: &[
            "filled with the Holy Ghost",
            "cloven tongues of fire",
            "speaking in tongues",
        ],
        themes: &["Holy Spirit", "power", "revival"],
        people: &["Peter", "apostles"],
        places: &["upper room", "Jerusalem"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Acts 3:6",
        title: "silver and gold have I none",
        aliases: &[
            "rise up and walk",
            "lame man at beautiful gate",
            "Peter healed the lame man",
        ],
        themes: &["healing", "authority", "name of Jesus"],
        people: &["Peter", "John"],
        places: &["beautiful gate", "temple"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Acts 9:4",
        title: "Saul on the road to Damascus",
        aliases: &[
            "Damascus road",
            "Saul Saul why persecutest thou me",
            "conversion of Paul",
        ],
        themes: &["conversion", "calling", "encounter"],
        people: &["Saul", "Paul", "Jesus"],
        places: &["Damascus road"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Acts 16:25",
        title: "Paul and Silas in prison",
        aliases: &[
            "midnight prayer and praise",
            "prison doors opened",
            "jailer was saved",
        ],
        themes: &["praise", "deliverance", "salvation"],
        people: &["Paul", "Silas"],
        places: &["Philippi prison"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Romans 8:28",
        title: "all things work together for good",
        aliases: &[
            "called according to his purpose",
            "God works all things for good",
        ],
        themes: &["purpose", "providence", "hope"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "1 Corinthians 13:4-8",
        title: "love is patient love is kind",
        aliases: &[
            "charity suffereth long",
            "love chapter",
            "greatest of these is love",
        ],
        themes: &["love", "marriage", "character"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "2 Corinthians 5:17",
        title: "new creature in Christ",
        aliases: &["old things are passed away", "all things are become new"],
        themes: &["new creation", "identity", "salvation"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Galatians 5:22-23",
        title: "fruit of the Spirit",
        aliases: &[
            "love joy peace",
            "fruit of the holy spirit",
            "against such there is no law",
        ],
        themes: &["character", "Holy Spirit", "self control"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Ephesians 6:10-18",
        title: "armor of God",
        aliases: &[
            "put on the whole armour of God",
            "helmet of salvation",
            "shield of faith",
        ],
        themes: &["spiritual warfare", "prayer", "truth"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Philippians 4:6",
        title: "be anxious for nothing",
        aliases: &[
            "do not be anxious",
            "prayer and supplication",
            "peace of God",
        ],
        themes: &["anxiety", "prayer", "peace"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Philippians 4:13",
        title: "I can do all things through Christ",
        aliases: &[
            "through Christ which strengtheneth me",
            "Christ strengthens me",
        ],
        themes: &["strength", "contentment", "endurance"],
        people: &["Paul"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Hebrews 11:1-6",
        title: "faith is the substance",
        aliases: &[
            "substance of things hoped for",
            "evidence of things not seen",
            "faith chapter",
        ],
        themes: &["faith", "hope", "unseen"],
        people: &[],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "James 2:17",
        title: "faith without works is dead",
        aliases: &[
            "faith by itself is dead",
            "show me thy faith",
            "works and faith",
        ],
        themes: &["obedience", "faith", "works"],
        people: &["James"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "1 Peter 5:7",
        title: "casting all your care upon him",
        aliases: &["he careth for you", "cast your anxiety on him"],
        themes: &["anxiety", "care", "trust"],
        people: &["Peter"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Revelation 3:20",
        title: "behold I stand at the door and knock",
        aliases: &[
            "stand at the door and knock",
            "if any man hear my voice",
            "I will sup with him",
        ],
        themes: &["invitation", "fellowship", "repentance"],
        people: &["Jesus", "John"],
        places: &["Laodicea"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Genesis 1:1",
        title: "creation",
        aliases: &[
            "in the beginning God created",
            "God created the heavens and the earth",
        ],
        themes: &["beginning", "creator", "creation"],
        people: &["God"],
        places: &["Eden"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Genesis 3:6",
        title: "fall of man",
        aliases: &[
            "Adam and Eve sinned",
            "serpent deceived Eve",
            "forbidden fruit",
        ],
        themes: &["sin", "temptation", "disobedience"],
        people: &["Adam", "Eve", "serpent"],
        places: &["garden of Eden"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 4:9",
        title: "Cain and Abel",
        aliases: &[
            "am I my brother's keeper",
            "Cain killed Abel",
            "blood of Abel",
        ],
        themes: &["jealousy", "murder", "accountability"],
        people: &["Cain", "Abel"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 6:14",
        title: "Noah's ark",
        aliases: &["Noah built the ark", "flood came", "gopher wood ark"],
        themes: &["obedience", "judgment", "salvation"],
        people: &["Noah"],
        places: &["ark"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Genesis 11:4",
        title: "tower of Babel",
        aliases: &[
            "let us build a tower",
            "confusion of languages",
            "Babel tower",
        ],
        themes: &["pride", "language", "scattering"],
        people: &[],
        places: &["Babel"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 12:1",
        title: "call of Abraham",
        aliases: &[
            "get thee out of thy country",
            "Abraham called by God",
            "Abram leave your father's house",
        ],
        themes: &["calling", "faith", "obedience"],
        people: &["Abraham", "Abram"],
        places: &["Ur", "Canaan"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 22:1-14",
        title: "Abraham offers Isaac",
        aliases: &["sacrifice Isaac", "Jehovah Jireh", "ram in the thicket"],
        themes: &["faith", "sacrifice", "provision"],
        people: &["Abraham", "Isaac"],
        places: &["Moriah"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Genesis 28:12",
        title: "Jacob's ladder",
        aliases: &[
            "ladder reaching heaven",
            "angels ascending and descending",
            "Bethel dream",
        ],
        themes: &["encounter", "promise", "presence of God"],
        people: &["Jacob"],
        places: &["Bethel"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 37:3",
        title: "Joseph coat of many colors",
        aliases: &[
            "coat of many colours",
            "Joseph dreamer",
            "Joseph and his brothers",
        ],
        themes: &["favor", "jealousy", "dreams"],
        people: &["Joseph"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 37:28",
        title: "Joseph sold into slavery",
        aliases: &[
            "Joseph was sold to slavery",
            "Joseph was sold into slavery",
            "Joseph was sold into slavery by his brothers",
            "Joseph was sold as a slave by his brothers",
            "Joseph sold as a slave",
            "Joseph sold to slavery",
            "Joseph sold by his brothers",
            "Joseph was betrayed and sold",
            "Joseph brothers sold him",
            "Joseph sold into Egypt",
            "Joseph was thrown into the pit",
            "Joseph in the pit",
            "sold Joseph to the Ishmaelites",
            "sold Joseph for twenty pieces of silver",
            "the dreamer was sold",
            "brothers sold Joseph because of jealousy",
            "Joseph taken down to Egypt",
        ],
        themes: &[
            "betrayal",
            "jealousy",
            "slavery",
            "providence",
            "family conflict",
            "suffering before purpose",
        ],
        people: &["Joseph", "Jacob", "Judah", "Ishmaelites", "Midianites"],
        places: &["Dothan", "Egypt", "pit"],
        score: 0.93,
    },
    CatalogEntry {
        reference: "Genesis 39:20-23",
        title: "Joseph in prison",
        aliases: &[
            "Joseph was put in prison",
            "Joseph falsely accused",
            "Potiphar's wife accused Joseph",
            "Joseph suffered in prison",
            "Lord was with Joseph in prison",
        ],
        themes: &["false accusation", "faithfulness", "favor", "suffering"],
        people: &["Joseph", "Potiphar"],
        places: &["Egypt", "prison"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Genesis 50:20",
        title: "you meant it for evil but God meant it for good",
        aliases: &[
            "God meant it for good",
            "Joseph forgave his brothers",
            "what you meant for evil",
        ],
        themes: &["providence", "forgiveness", "purpose"],
        people: &["Joseph"],
        places: &["Egypt"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Exodus 3:5",
        title: "burning bush",
        aliases: &[
            "Moses and the burning bush",
            "holy ground",
            "take off your shoes",
        ],
        themes: &["calling", "holiness", "deliverance"],
        people: &["Moses"],
        places: &["Horeb"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Exodus 12:13",
        title: "Passover blood",
        aliases: &[
            "when I see the blood",
            "blood on the doorpost",
            "Passover lamb",
        ],
        themes: &["deliverance", "redemption", "blood"],
        people: &["Moses"],
        places: &["Egypt"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Exodus 14:21-31",
        title: "Red Sea parted",
        aliases: &[
            "crossing the Red Sea",
            "Moses stretched out his hand",
            "Israel crossed on dry ground",
        ],
        themes: &["deliverance", "miracle", "faith"],
        people: &["Moses"],
        places: &["Red Sea"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Exodus 16:4",
        title: "manna from heaven",
        aliases: &[
            "bread from heaven",
            "daily manna",
            "God fed Israel in wilderness",
        ],
        themes: &["provision", "daily bread", "wilderness"],
        people: &["Moses"],
        places: &["wilderness"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Exodus 17:6",
        title: "water from the rock",
        aliases: &[
            "Moses struck the rock",
            "rock brought water",
            "thirst in wilderness",
        ],
        themes: &["provision", "thirst", "miracle"],
        people: &["Moses"],
        places: &["Horeb"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Exodus 20:3",
        title: "Ten Commandments",
        aliases: &[
            "thou shalt have no other gods",
            "law on Sinai",
            "Moses received the commandments",
        ],
        themes: &["law", "holiness", "obedience"],
        people: &["Moses"],
        places: &["Sinai"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Numbers 21:9",
        title: "bronze serpent",
        aliases: &[
            "brazen serpent",
            "look and live",
            "serpent lifted in wilderness",
        ],
        themes: &["healing", "faith", "salvation"],
        people: &["Moses"],
        places: &["wilderness"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Joshua 6:1-21",
        title: "walls of Jericho fell",
        aliases: &[
            "Jericho walls came down",
            "march around Jericho",
            "shout and the walls fell",
        ],
        themes: &["obedience", "victory", "faith"],
        people: &["Joshua"],
        places: &["Jericho"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Judges 6:12",
        title: "Gideon mighty man of valor",
        aliases: &[
            "Gideon and the fleece",
            "mighty man of valor",
            "mighty man of valour",
            "thou mighty man of valor",
            "thou mighty man of valour",
            "the lord is with thee thou mighty man",
            "Gideon's army",
        ],
        themes: &["calling", "courage", "weakness"],
        people: &["Gideon"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Judges 16:30",
        title: "Samson pulls down the temple",
        aliases: &[
            "Samson and Delilah",
            "Samson's hair",
            "let me die with the Philistines",
        ],
        themes: &["strength", "failure", "restoration"],
        people: &["Samson", "Delilah"],
        places: &["Philistine temple"],
        score: 0.86,
    },
    CatalogEntry {
        reference: "Ruth 1:16",
        title: "where you go I will go",
        aliases: &[
            "Ruth and Naomi",
            "thy people shall be my people",
            "your God my God",
        ],
        themes: &["loyalty", "covenant", "family"],
        people: &["Ruth", "Naomi"],
        places: &["Moab", "Bethlehem"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "1 Samuel 3:10",
        title: "speak Lord for thy servant heareth",
        aliases: &["call of Samuel", "Samuel heard God", "Eli and Samuel"],
        themes: &["calling", "listening", "prophetic"],
        people: &["Samuel", "Eli"],
        places: &["Shiloh"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "1 Samuel 17:45-50",
        title: "David and Goliath",
        aliases: &[
            "David said to Goliath",
            "you come with sword and spear",
            "I come in the name of the Lord",
        ],
        themes: &["courage", "faith", "victory"],
        people: &["David", "Goliath"],
        places: &["valley of Elah"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "1 Kings 18:36-39",
        title: "Elijah fire on Mount Carmel",
        aliases: &[
            "fire fell from heaven",
            "prophets of Baal",
            "God that answers by fire",
        ],
        themes: &["revival", "idolatry", "power"],
        people: &["Elijah"],
        places: &["Mount Carmel"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "2 Kings 2:11",
        title: "Elijah taken up",
        aliases: &[
            "chariot of fire",
            "Elijah went up by whirlwind",
            "mantle of Elijah",
        ],
        themes: &["transition", "anointing", "double portion"],
        people: &["Elijah", "Elisha"],
        places: &["Jordan"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "2 Kings 4:2",
        title: "widow's oil",
        aliases: &[
            "borrow vessels not a few",
            "oil multiplied",
            "Elisha and the widow",
        ],
        themes: &["provision", "faith", "debt"],
        people: &["Elisha", "widow"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "2 Kings 5:14",
        title: "Naaman dipped seven times",
        aliases: &[
            "Naaman healed of leprosy",
            "wash in Jordan seven times",
            "Elisha and Naaman",
        ],
        themes: &["healing", "humility", "obedience"],
        people: &["Naaman", "Elisha"],
        places: &["Jordan"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Esther 4:14",
        title: "for such a time as this",
        aliases: &[
            "Esther before the king",
            "if I perish I perish",
            "Mordecai told Esther",
        ],
        themes: &["purpose", "courage", "deliverance"],
        people: &["Esther", "Mordecai"],
        places: &["Persia"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Job 1:21",
        title: "the Lord gave and the Lord hath taken away",
        aliases: &[
            "blessed be the name of the Lord",
            "Job lost everything",
            "naked came I out",
        ],
        themes: &["suffering", "worship", "loss"],
        people: &["Job"],
        places: &["Uz"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Psalm 23:1",
        title: "the Lord is my shepherd",
        aliases: &["I shall not want", "green pastures", "still waters"],
        themes: &["shepherd", "comfort", "guidance"],
        people: &["David"],
        places: &[],
        score: 0.92,
    },
    CatalogEntry {
        reference: "Psalm 23:4",
        title: "valley of the shadow of death",
        aliases: &[
            "though I walk through the valley",
            "thy rod and thy staff",
            "fear no evil",
        ],
        themes: &["comfort", "fear", "presence of God"],
        people: &["David"],
        places: &["valley"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Psalm 46:10",
        title: "be still and know",
        aliases: &[
            "be still and know that I am God",
            "God is our refuge",
            "refuge and strength",
        ],
        themes: &["stillness", "trust", "refuge"],
        people: &[],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Psalm 91:1",
        title: "secret place of the most high",
        aliases: &[
            "abide under the shadow",
            "shadow of the almighty",
            "Psalm ninety one protection",
        ],
        themes: &["protection", "refuge", "deliverance"],
        people: &[],
        places: &["secret place"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Proverbs 3:5",
        title: "trust in the Lord with all thine heart",
        aliases: &[
            "lean not on your own understanding",
            "acknowledge him",
            "he shall direct thy paths",
        ],
        themes: &["trust", "guidance", "wisdom"],
        people: &["Solomon"],
        places: &[],
        score: 0.92,
    },
    CatalogEntry {
        reference: "Isaiah 6:8",
        title: "here am I send me",
        aliases: &["Isaiah saw the Lord", "whom shall I send", "holy holy holy"],
        themes: &["calling", "holiness", "mission"],
        people: &["Isaiah"],
        places: &["temple"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Isaiah 9:6",
        title: "unto us a child is born",
        aliases: &["wonderful counsellor", "mighty God", "prince of peace"],
        themes: &["messiah", "Christmas", "peace"],
        people: &["Jesus", "Isaiah"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Isaiah 40:31",
        title: "they that wait upon the Lord",
        aliases: &[
            "mount up with wings as eagles",
            "run and not be weary",
            "renew their strength",
        ],
        themes: &["waiting", "strength", "hope"],
        people: &["Isaiah"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Isaiah 53:5",
        title: "by his stripes we are healed",
        aliases: &[
            "wounded for our transgressions",
            "bruised for our iniquities",
            "suffering servant",
        ],
        themes: &["healing", "atonement", "cross"],
        people: &["Jesus", "Isaiah"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Jeremiah 1:5",
        title: "before I formed thee in the belly",
        aliases: &[
            "called from the womb",
            "ordained thee a prophet",
            "Jeremiah called",
        ],
        themes: &["calling", "identity", "purpose"],
        people: &["Jeremiah"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Jeremiah 29:11",
        title: "plans to prosper you",
        aliases: &["thoughts of peace", "expected end", "future and hope"],
        themes: &["hope", "future", "purpose"],
        people: &["Jeremiah"],
        places: &["Babylon"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Ezekiel 37:1-14",
        title: "valley of dry bones",
        aliases: &[
            "dry bones live",
            "can these bones live",
            "prophesy to these bones",
        ],
        themes: &[
            "revival",
            "restoration",
            "prophecy",
            "prophetic restoration",
            "bones coming alive",
            "dead bones live again",
        ],
        people: &["Ezekiel"],
        places: &["valley of dry bones"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Daniel 3:19-27",
        title: "fourth man in the fire",
        aliases: &[
            "Shadrach Meshach and Abednego",
            "fiery furnace",
            "son of God in the fire",
        ],
        themes: &["deliverance", "persecution", "faith"],
        people: &["Shadrach", "Meshach", "Abednego", "Nebuchadnezzar"],
        places: &["fiery furnace", "Babylon"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Daniel 6:16-23",
        title: "Daniel in the lions den",
        aliases: &[
            "lions den",
            "God shut the lions mouths",
            "Daniel prayed three times",
        ],
        themes: &["prayer", "deliverance", "faithfulness"],
        people: &["Daniel"],
        places: &["lions den", "Babylon"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Jonah 1:15-17",
        title: "Jonah and the whale",
        aliases: &[
            "great fish swallowed Jonah",
            "Jonah ran from God",
            "three days in the fish",
        ],
        themes: &["obedience", "mercy", "repentance"],
        people: &["Jonah"],
        places: &["Nineveh", "Tarshish"],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Micah 6:8",
        title: "do justice love mercy walk humbly",
        aliases: &[
            "what does the Lord require",
            "walk humbly with thy God",
            "love mercy",
        ],
        themes: &["justice", "mercy", "humility"],
        people: &["Micah"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Habakkuk 2:2",
        title: "write the vision",
        aliases: &[
            "make it plain",
            "vision for an appointed time",
            "though it tarry wait for it",
        ],
        themes: &["vision", "waiting", "faith"],
        people: &["Habakkuk"],
        places: &[],
        score: 0.88,
    },
    CatalogEntry {
        reference: "Habakkuk 3:17",
        title: "although the fig tree shall not blossom",
        aliases: &[
            "yet I will rejoice in the Lord",
            "no fruit in the vines",
            "fields shall yield no meat",
            "Habakkuk rejoicing in famine",
            "rejoicing when nothing is working",
        ],
        themes: &[
            "joy",
            "faith",
            "worship in hardship",
            "trust during scarcity",
        ],
        people: &["Habakkuk"],
        places: &[],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Obadiah 1:17",
        title: "upon mount Zion shall be deliverance",
        aliases: &[
            "there shall be deliverance on mount Zion",
            "mount Zion deliverance",
            "the house of Jacob shall possess their possessions",
            "possess your possessions",
            "Obadiah deliverance",
        ],
        themes: &[
            "deliverance",
            "restoration",
            "inheritance",
            "judgment of Edom",
        ],
        people: &["Obadiah", "Jacob", "Edom"],
        places: &["Mount Zion", "Edom"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Malachi 3:10",
        title: "bring all the tithes into the storehouse",
        aliases: &[
            "prove me now herewith",
            "windows of heaven",
            "pour you out a blessing",
            "not room enough to receive it",
            "Malachi tithes and offering",
        ],
        themes: &["tithing", "giving", "stewardship", "blessing", "obedience"],
        people: &["Malachi"],
        places: &["storehouse"],
        score: 0.9,
    },
    CatalogEntry {
        reference: "Malachi 4:2",
        title: "sun of righteousness shall arise",
        aliases: &[
            "healing in his wings",
            "sun of righteousness",
            "Malachi healing in his wings",
            "arise with healing",
        ],
        themes: &["healing", "righteousness", "restoration", "hope"],
        people: &["Malachi"],
        places: &[],
        score: 0.88,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_joseph_sold_into_slavery_summary() {
        let hits = detect_catalog_matches_raw("Joseph was sold to slavery by his brothers");
        assert!(
            hits.iter()
                .any(|hit| hit.reference == "Genesis 37:28" && hit.score >= 0.88),
            "expected Joseph slavery summary to resolve to Genesis 37:28, got {hits:?}"
        );
    }

    #[test]
    fn detects_beatitudes_from_common_misspelling() {
        let hits = detect_catalog_matches_raw("the preacher mentioned the beattiudes");
        assert!(
            hits.iter().any(|hit| hit.reference == "Matthew 5:3-12"),
            "expected Beatitudes catalog hit, got {hits:?}"
        );
    }

    #[test]
    fn detects_named_bible_characters_from_contextual_summaries() {
        let cases = [
            ("Daniel was thrown into the lions den", "Daniel 6:16-23"),
            (
                "Elijah called fire down on mount carmel",
                "1 Kings 18:36-39",
            ),
            ("Naaman was healed after washing in Jordan", "2 Kings 5:14"),
            ("Ruth stayed with Naomi", "Ruth 1:16"),
            ("Moses stood before the burning bush", "Exodus 3:5"),
            ("Samson died with the Philistines", "Judges 16:30"),
        ];

        for (query, expected) in cases {
            let hits = detect_catalog_matches_raw(query);
            assert!(
                hits.iter().any(|hit| hit.reference == expected),
                "expected {query:?} to resolve to {expected}, got {hits:?}"
            );
        }
    }
}
