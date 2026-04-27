//! Scripture detection service boundary.

pub mod books;
pub mod grammar;
pub mod normalize;

pub use grammar::{GrammarReferenceParser, ParsedReference};
pub use normalize::{NormalizedText, TranscriptNormalizer};

use aletheia_core::{Millis, ServiceSessionId};

/// Source tier that produced a scripture candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetectionTier {
    ExactReference,
    SpokenNumberReference,
    LocalLanguageAlias,
    VerseQuotation,
    ThematicCoreference,
    SermonContextRerank,
    CloudEnhancement,
}

/// Safety bucket used by the approval UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfidenceBucket {
    Certain,
    Strong,
    Likely,
    Ambiguous,
    Unsafe,
}

impl ConfidenceBucket {
    /// Maps a score to the operator-facing bucket.
    pub fn from_score(score: f32) -> Self {
        match score {
            value if value >= 0.92 => Self::Certain,
            value if value >= 0.82 => Self::Strong,
            value if value >= 0.68 => Self::Likely,
            value if value >= 0.45 => Self::Ambiguous,
            _ => Self::Unsafe,
        }
    }
}

/// Transcript segment normalized by the STT boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptSegment {
    pub id: String,
    pub session_id: ServiceSessionId,
    pub started_at_ms: Millis,
    pub ended_at_ms: Millis,
    pub speaker_label: Option<String>,
    pub language: String,
    pub text: String,
    pub confidence: f32,
    pub adapter: String,
    pub latency_ms: Millis,
}

/// Evidence attached to a scripture candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectionEvidence {
    pub tier: DetectionTier,
    pub segment_id: String,
    pub excerpt: String,
    pub weight: f32,
}

/// Language detected from a transcript segment before scripture matching.
#[derive(Clone, Debug, PartialEq)]
pub struct LanguageDetection {
    pub code: &'static str,
    pub name: &'static str,
    pub confidence: f32,
    pub matched_terms: Vec<String>,
}

/// Language pack metadata exposed to the UI and readiness checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportedLanguage {
    pub code: &'static str,
    pub name: &'static str,
    pub stt_locale: &'static str,
    pub scripture_aliases_ready: bool,
    pub offline_stt_ready: bool,
    pub cloud_stt_ready: bool,
}

struct LanguageProfile {
    code: &'static str,
    name: &'static str,
    stt_locale: &'static str,
    terms: &'static [&'static str],
    aliases_ready: bool,
    offline_stt_ready: bool,
    cloud_stt_ready: bool,
}

/// Candidate emitted by scripture detection.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptureCandidate {
    pub id: String,
    pub session_id: ServiceSessionId,
    pub reference: String,
    pub translation: String,
    pub language: String,
    pub score: f32,
    pub bucket: ConfidenceBucket,
    pub reasons: Vec<String>,
    pub evidence: Vec<DetectionEvidence>,
}

/// Stateless confidence policy.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfidencePolicy {
    pub auto_preview_min_score: f32,
    pub live_requires_operator: bool,
}

/// A labeled transcript sample used to measure detector behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct AccuracyFixture {
    pub id: &'static str,
    pub language: &'static str,
    pub text: &'static str,
    pub expected_reference: Option<&'static str>,
}

/// Precision and recall summary for local detector fixtures.
#[derive(Clone, Debug, PartialEq)]
pub struct AccuracyEvaluation {
    pub total: u16,
    pub true_positives: u16,
    pub false_positives: u16,
    pub false_negatives: u16,
    pub precision: f32,
    pub recall: f32,
}

impl Default for ConfidencePolicy {
    fn default() -> Self {
        Self {
            auto_preview_min_score: 0.95,
            live_requires_operator: true,
        }
    }
}

impl ConfidencePolicy {
    /// Returns whether the candidate can move to Preview without live output.
    pub fn can_auto_preview(&self, candidate: &ScriptureCandidate) -> bool {
        candidate.score >= self.auto_preview_min_score
            && matches!(
                candidate.bucket,
                ConfidenceBucket::Certain | ConfidenceBucket::Strong
            )
    }

    /// Live output is deliberately operator-owned by default.
    pub fn can_auto_live(&self) -> bool {
        !self.live_requires_operator
    }
}

/// Scripture detection contract.
pub trait ScriptureDetector {
    /// Detect candidates from a transcript segment and rolling context.
    fn detect(
        &self,
        segment: &TranscriptSegment,
        context: &[TranscriptSegment],
    ) -> Vec<ScriptureCandidate>;
}

/// Language detector contract.
pub trait LanguageDetector {
    /// Detects language from text with an optional STT-declared prior.
    fn detect_language(&self, text: &str, declared_language: Option<&str>) -> LanguageDetection;

    /// Returns languages covered by the local alias and STT policy.
    fn supported_languages(&self) -> Vec<SupportedLanguage>;
}

/// Evaluates a detector against labeled transcript fixtures.
pub fn evaluate_accuracy_fixtures<D: ScriptureDetector>(
    detector: &D,
    fixtures: &[AccuracyFixture],
) -> AccuracyEvaluation {
    let session_id = ServiceSessionId::new("accuracy-fixture").expect("static session id is valid");
    let mut true_positives = 0_u16;
    let mut false_positives = 0_u16;
    let mut false_negatives = 0_u16;

    for fixture in fixtures {
        let segment = TranscriptSegment {
            id: fixture.id.to_string(),
            session_id: session_id.clone(),
            started_at_ms: 0,
            ended_at_ms: 1_000,
            speaker_label: Some("Fixture".to_string()),
            language: fixture.language.to_string(),
            text: fixture.text.to_string(),
            confidence: 0.95,
            adapter: "fixture".to_string(),
            latency_ms: 0,
        };
        let best_candidate = detector
            .detect(&segment, &[])
            .into_iter()
            .max_by(|left, right| left.score.total_cmp(&right.score));

        match (fixture.expected_reference, best_candidate) {
            (Some(expected), Some(candidate)) if candidate.reference == expected => {
                true_positives += 1;
            }
            (Some(_), Some(_)) => {
                false_positives += 1;
                false_negatives += 1;
            }
            (Some(_), None) => {
                false_negatives += 1;
            }
            (None, Some(_)) => {
                false_positives += 1;
            }
            (None, None) => {}
        }
    }

    let precision_denominator = true_positives + false_positives;
    let recall_denominator = true_positives + false_negatives;

    AccuracyEvaluation {
        total: fixtures.len() as u16,
        true_positives,
        false_positives,
        false_negatives,
        precision: if precision_denominator == 0 {
            1.0
        } else {
            f32::from(true_positives) / f32::from(precision_denominator)
        },
        recall: if recall_denominator == 0 {
            1.0
        } else {
            f32::from(true_positives) / f32::from(recall_denominator)
        },
    }
}

/// Deterministic language detector for local-first routing.
#[derive(Default)]
pub struct KeywordLanguageDetector;

impl LanguageDetector for KeywordLanguageDetector {
    fn detect_language(&self, text: &str, declared_language: Option<&str>) -> LanguageDetection {
        let normalized = fold_for_matching(text);
        let declared = declared_language.map(fold_for_matching);
        let mut best_profile = &LANGUAGE_PROFILES[0];
        let mut best_score = 0.0_f32;
        let mut best_terms = Vec::new();

        for profile in LANGUAGE_PROFILES {
            let mut score = 0.0_f32;
            let mut matched_terms = Vec::new();

            if let Some(declared) = declared.as_deref() {
                if declared == fold_for_matching(profile.name)
                    || declared == fold_for_matching(profile.code)
                    || declared == fold_for_matching(profile.stt_locale)
                {
                    score += 0.42;
                    matched_terms.push(format!("declared:{}", profile.name));
                }
            }

            for term in profile.terms {
                let folded = fold_for_matching(term);
                if normalized.contains(&folded) {
                    score += 0.16;
                    matched_terms.push((*term).to_string());
                }
            }

            if score > best_score {
                best_score = score;
                best_profile = profile;
                best_terms = matched_terms;
            }
        }

        let confidence = if best_score == 0.0 {
            0.36
        } else {
            best_score.clamp(0.0, 0.98)
        };

        LanguageDetection {
            code: best_profile.code,
            name: best_profile.name,
            confidence,
            matched_terms: best_terms,
        }
    }

    fn supported_languages(&self) -> Vec<SupportedLanguage> {
        LANGUAGE_PROFILES
            .iter()
            .map(|profile| SupportedLanguage {
                code: profile.code,
                name: profile.name,
                stt_locale: profile.stt_locale,
                scripture_aliases_ready: profile.aliases_ready,
                offline_stt_ready: profile.offline_stt_ready,
                cloud_stt_ready: profile.cloud_stt_ready,
            })
            .collect()
    }
}

/// Production detector for Phase 1: runs the grammar parser over normalized
/// transcript text and emits one candidate per extracted reference, then
/// layers in the keyword detector's thematic matches (e.g. "jesus wept" →
/// John 11:35) as lower-confidence fallbacks.
///
/// The operator queue sees grammar matches at 0.95 (explicit verse) or 0.76
/// (chapter-only), and keyword/thematic matches at their original scores
/// (0.58–0.91 depending on tier).
#[derive(Default, Clone, Copy, Debug)]
pub struct GrammarScriptureDetector;

impl ScriptureDetector for GrammarScriptureDetector {
    fn detect(
        &self,
        segment: &TranscriptSegment,
        context: &[TranscriptSegment],
    ) -> Vec<ScriptureCandidate> {
        let normalized = TranscriptNormalizer.normalize(&segment.text);
        let mut out: Vec<ScriptureCandidate> = Vec::new();

        for parsed in GrammarReferenceParser.parse_all(&normalized.text) {
            let reference = parsed.as_reference_string();
            let tier = if parsed.explicit_verse {
                DetectionTier::ExactReference
            } else {
                DetectionTier::SpokenNumberReference
            };
            out.push(ScriptureCandidate {
                id: format!("{}:{}", segment.id, reference),
                session_id: segment.session_id.clone(),
                reference: reference.clone(),
                translation: "KJV".to_string(),
                language: segment.language.clone(),
                score: parsed.confidence,
                bucket: ConfidenceBucket::from_score(parsed.confidence),
                reasons: vec![format!(
                    "Grammar parser matched \"{}\" in transcript",
                    parsed.alias_matched
                )],
                evidence: vec![DetectionEvidence {
                    tier,
                    segment_id: segment.id.clone(),
                    excerpt: segment.text.clone(),
                    weight: parsed.confidence,
                }],
            });
        }

        // Layer in keyword/thematic matches for phrases the grammar cannot
        // catch (e.g. "jesus wept" → John 11:35). Skip duplicates of
        // references the grammar already produced.
        let keyword = ReferenceKeywordDetector.detect(segment, context);
        for cand in keyword {
            if !out.iter().any(|c| c.reference == cand.reference) {
                out.push(cand);
            }
        }

        out
    }
}

// ─── Keyword-window helpers ──────────────────────────────────────────────────

/// Stop words stripped before keyword-window matching so the matcher focuses
/// on semantically significant tokens.
const PHRASE_STOP_WORDS: &[&str] = &[
    "a", "an", "the", "in", "of", "for", "and", "or", "with", "to", "shall",
    "will", "is", "are", "was", "were", "be", "been", "that", "which", "who",
    "my", "your", "his", "her", "our", "their", "by", "at", "on", "we", "i",
    "you", "they", "he", "she", "it", "not", "no", "this", "these", "those",
    "me", "us", "so", "have", "has", "had", "do", "does", "did", "then",
    "when", "where", "what", "how", "if", "but", "from", "up", "about",
    "into", "through", "during", "unto", "upon", "ye", "thy", "thee",
    "all", "any", "every", "now", "yet", "even", "just", "thus", "o",
];

/// Extracts content-bearing keywords from a phrase alias.
/// Removes stop words and tokens shorter than 3 characters.
fn phrase_keywords(phrase: &str) -> Vec<String> {
    fold_for_matching(phrase)
        .split_whitespace()
        .filter(|tok| tok.len() >= 3 && !PHRASE_STOP_WORDS.contains(tok))
        .map(|tok| tok.to_string())
        .collect()
}

/// Window size (in tokens) for keyword proximity matching.
/// 14 tokens ≈ 7 s of natural speech — wide enough to absorb inserted
/// connector words, narrow enough to avoid cross-topic false positives.
const KEYWORD_WINDOW: usize = 14;

/// Returns `true` when **all** `keywords` appear in any contiguous
/// `KEYWORD_WINDOW`-wide slice of `transcript_tokens` (order-independent).
fn keyword_window_match(transcript_tokens: &[&str], keywords: &[String]) -> bool {
    if keywords.is_empty() || keywords.len() > KEYWORD_WINDOW {
        return false;
    }
    let n = transcript_tokens.len();
    for start in 0..n {
        let end = (start + KEYWORD_WINDOW).min(n);
        let window = &transcript_tokens[start..end];
        if keywords.iter().all(|kw| window.contains(&kw.as_str())) {
            return true;
        }
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────────

/// Minimal deterministic detector used for thematic phrase matches. The
/// grammar parser is the primary path; this detector supplements it with
/// "jesus wept" → John 11:35 style coreferences.
///
/// Detection runs in two phases:
/// 1. **Exact substring** — alias is a verbatim substring of the transcript
///    (high confidence, 0.86–0.95).
/// 2. **Keyword window** — all content-bearing keywords from any alias appear
///    within a 14-token window, regardless of order (0.72). Catches variant
///    phrasings, inserted connector words, and minor STT substitutions.
#[derive(Default)]
pub struct ReferenceKeywordDetector;

impl ScriptureDetector for ReferenceKeywordDetector {
    fn detect(
        &self,
        segment: &TranscriptSegment,
        _context: &[TranscriptSegment],
    ) -> Vec<ScriptureCandidate> {
        let lower = fold_for_matching(&segment.text);
        let mut out: Vec<ScriptureCandidate> = Vec::new();

        // ── Phase 1: Exact substring matching ────────────────────────────────
        for pattern in REFERENCE_PATTERNS {
            let reference = pattern.reference;
            let alias_match = pattern
                .aliases
                .iter()
                .find(|alias| lower.contains(&fold_for_matching(alias)));

            if let Some(alias) = alias_match {
                let folded_alias = fold_for_matching(alias);
                let tier = if pattern.localized_aliases.contains(alias) {
                    DetectionTier::LocalLanguageAlias
                } else if folded_alias.contains(':')
                    || folded_alias.contains("verse")
                    || folded_alias.contains("chapter")
                    || folded_alias.contains("capitulo")
                    || folded_alias.contains("chapitre")
                {
                    DetectionTier::ExactReference
                } else if *alias == "he wept" || *alias == "david said to goliath" {
                    DetectionTier::ThematicCoreference
                } else {
                    DetectionTier::VerseQuotation
                };
                let score = match tier {
                    DetectionTier::ExactReference => 0.95,
                    DetectionTier::LocalLanguageAlias => 0.91,
                    DetectionTier::VerseQuotation => 0.86,
                    DetectionTier::ThematicCoreference => 0.58,
                    _ => 0.72,
                };
                out.push(ScriptureCandidate {
                    id: format!("{}:{}", segment.id, reference),
                    session_id: segment.session_id.clone(),
                    reference: reference.to_string(),
                    translation: "KJV".to_string(),
                    language: segment.language.clone(),
                    score,
                    bucket: ConfidenceBucket::from_score(score),
                    reasons: vec![format!("Matched sermon phrase: {alias}")],
                    evidence: vec![DetectionEvidence {
                        tier,
                        segment_id: segment.id.clone(),
                        excerpt: segment.text.clone(),
                        weight: score,
                    }],
                });
            }
        }

        // ── Phase 2: Keyword window matching ─────────────────────────────────
        // Handles word-order variation, inserted connectors, partial phrase
        // recall, and minor STT errors (e.g. "stripes" present but KJV exact
        // phrasing not). Only fires for references not caught in Phase 1.
        let tokens: Vec<&str> = lower.split_whitespace().collect();
        'patterns: for pattern in REFERENCE_PATTERNS {
            let reference = pattern.reference;
            // Skip references already matched at higher confidence.
            if out.iter().any(|c| c.reference == reference) {
                continue;
            }
            for alias in pattern.aliases {
                let keywords = phrase_keywords(alias);
                // ≥ 2 content keywords required to avoid spurious hits on
                // very short aliases (e.g. single-word thematic coreferences).
                if keywords.len() >= 2 && keyword_window_match(&tokens, &keywords) {
                    let score = 0.72_f32;
                    out.push(ScriptureCandidate {
                        id: format!("{}:{}:kw", segment.id, reference),
                        session_id: segment.session_id.clone(),
                        reference: reference.to_string(),
                        translation: "KJV".to_string(),
                        language: segment.language.clone(),
                        score,
                        bucket: ConfidenceBucket::from_score(score),
                        reasons: vec![format!(
                            "Keyword proximity match for \u{00ab}{}\u{00bb} (keys: {})",
                            alias,
                            keywords.join(", ")
                        )],
                        evidence: vec![DetectionEvidence {
                            tier: DetectionTier::VerseQuotation,
                            segment_id: segment.id.clone(),
                            excerpt: segment.text.clone(),
                            weight: score,
                        }],
                    });
                    continue 'patterns;
                }
            }
        }

        out
    }
}

struct ReferencePattern {
    reference: &'static str,
    aliases: &'static [&'static str],
    localized_aliases: &'static [&'static str],
}

/// Returns the number of scripture reference patterns in the built-in phrase
/// library. Exported so the diagnostics screen can surface "N phrases loaded"
/// without the caller needing to import the private array.
#[inline]
pub fn reference_pattern_count() -> usize {
    REFERENCE_PATTERNS.len()
}

const REFERENCE_PATTERNS: &[ReferencePattern] = &[
    ReferencePattern {
        reference: "Romans 8:28",
        aliases: &[
            "romans 8:28",
            "romans chapter eight",
            "romawa 8:28",
            "romafo 8:28",
            "warumi 8:28",
            "kwabaseroma 8:28",
            "romanos 8:28",
            "romains 8:28",
            "all things work together for good",
        ],
        localized_aliases: &[
            "romawa 8:28",
            "romafo 8:28",
            "warumi 8:28",
            "kwabaseroma 8:28",
            "romanos 8:28",
            "romains 8:28",
        ],
    },
    ReferencePattern {
        reference: "1 Samuel 17:45",
        aliases: &[
            "1 samuel 17:45",
            "first samuel seventeen forty five",
            "1 samaila 17:45",
            "1 samweli 17:45",
            "1 samuweli 17:45",
            "david said to goliath",
        ],
        localized_aliases: &["1 samaila 17:45", "1 samweli 17:45", "1 samuweli 17:45"],
    },
    ReferencePattern {
        reference: "Psalm 23:4",
        aliases: &[
            "valley of the shadow of death",
            "psalm twenty three verse four",
            "psalm 23:4",
            "zabura 23:4",
            "zaburi 23:4",
            "iindumiso 23:4",
            "salmo 23:4",
            "psaume 23:4",
        ],
        localized_aliases: &[
            "zabura 23:4",
            "zaburi 23:4",
            "iindumiso 23:4",
            "salmo 23:4",
            "psaume 23:4",
        ],
    },
    ReferencePattern {
        reference: "John 11:35",
        aliases: &[
            "john 11:35",
            "yohanna 11:35",
            "yohane 11:35",
            "yohana 11:35",
            "juan 11:35",
            "jean 11:35",
            "jesus wept",
            "he wept",
        ],
        localized_aliases: &[
            "yohanna 11:35",
            "yohane 11:35",
            "yohana 11:35",
            "juan 11:35",
            "jean 11:35",
        ],
    },
    ReferencePattern {
        reference: "Isaiah 40:31",
        aliases: &[
            "they that wait upon the lord",
            "renew their strength",
            "mount up with wings as eagles",
            "isaiah forty verse thirty one",
            "isaiah 40:31",
            "ishaya 40:31",
            "isaya 40:31",
            "yesaia 40:31",
            "isaias 40:31",
            "esaie 40:31",
        ],
        localized_aliases: &[
            "ishaya 40:31",
            "isaya 40:31",
            "yesaia 40:31",
            "isaias 40:31",
            "esaie 40:31",
        ],
    },
    // ---- Next 25 high-frequency sermon verses ----
    ReferencePattern {
        reference: "Jeremiah 29:11",
        aliases: &[
            "jeremiah 29:11",
            "i know the plans i have for you",
            "plans to prosper you",
            "plans to give you hope and a future",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Philippians 4:13",
        aliases: &[
            "philippians 4:13",
            "i can do all things through christ",
            "all things through christ who strengthens me",
            "filipians 4:13",
            "filipins 4:13",
        ],
        localized_aliases: &["filipians 4:13", "filipins 4:13"],
    },
    ReferencePattern {
        reference: "John 3:16",
        aliases: &[
            "john 3:16",
            "john chapter three verse sixteen",
            "for god so loved the world",
            "he gave his only begotten son",
            "whosoever believeth in him",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 23:1",
        aliases: &[
            "psalm 23:1",
            "the lord is my shepherd",
            "i shall not want",
            "psalm twenty three verse one",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Proverbs 3:5",
        aliases: &[
            "proverbs 3:5",
            "trust in the lord with all your heart",
            "lean not on your own understanding",
            "proverbs chapter three verse five",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 8:1",
        aliases: &[
            "romans 8:1",
            "there is therefore now no condemnation",
            "no condemnation for those in christ jesus",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 6:33",
        aliases: &[
            "matthew 6:33",
            "seek first the kingdom of god",
            "seek ye first the kingdom",
            "and all these things shall be added",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ephesians 6:11",
        aliases: &[
            "ephesians 6:11",
            "put on the whole armour of god",
            "whole armor of god",
            "stand against the wiles of the devil",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Isaiah 53:5",
        aliases: &[
            "isaiah 53:5",
            "he was wounded for our transgressions",
            "by his stripes we are healed",
            "with his stripes we are healed",
            "ishaya 53:5",
        ],
        localized_aliases: &["ishaya 53:5"],
    },
    ReferencePattern {
        reference: "Psalm 91:1",
        aliases: &[
            "psalm 91:1",
            "he who dwells in the shelter of the most high",
            "he that dwelleth in the secret place",
            "under the shadow of the almighty",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "2 Chronicles 7:14",
        aliases: &[
            "2 chronicles 7:14",
            "second chronicles 7:14",
            "if my people who are called by my name",
            "if my people shall humble themselves",
            "will forgive their sin and heal their land",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Corinthians 13:4",
        aliases: &[
            "1 corinthians 13:4",
            "love is patient love is kind",
            "charity suffereth long and is kind",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Joshua 1:9",
        aliases: &[
            "joshua 1:9",
            "be strong and courageous",
            "be strong and of good courage",
            "do not be afraid do not be discouraged",
            "i will be with you wherever you go",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 1:1",
        aliases: &[
            "genesis 1:1",
            "in the beginning god created",
            "in the beginning god created the heaven",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Revelation 21:4",
        aliases: &[
            "revelation 21:4",
            "god will wipe away every tear",
            "no more death or sorrow or crying",
            "there shall be no more death",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Hebrews 11:1",
        aliases: &[
            "hebrews 11:1",
            "faith is the substance of things hoped for",
            "the evidence of things not seen",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 46:1",
        aliases: &[
            "psalm 46:1",
            "god is our refuge and strength",
            "a very present help in trouble",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Philippians 4:6",
        aliases: &[
            "philippians 4:6",
            "do not be anxious about anything",
            "be careful for nothing",
            "with thanksgiving present your requests to god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 10:9",
        aliases: &[
            "romans 10:9",
            "if you confess with your mouth",
            "confess that jesus is lord",
            "believe in your heart that god raised him",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Galatians 2:20",
        aliases: &[
            "galatians 2:20",
            "i have been crucified with christ",
            "it is no longer i who live",
            "christ lives in me",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "2 Timothy 1:7",
        aliases: &[
            "2 timothy 1:7",
            "god has not given us a spirit of fear",
            "spirit of power and of love and of a sound mind",
            "spirit of fear but of power",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Numbers 6:24",
        aliases: &[
            "numbers 6:24",
            "the lord bless you and keep you",
            "the lord make his face shine upon you",
            "the lord lift up his countenance",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 11:28",
        aliases: &[
            "matthew 11:28",
            "come to me all you who are weary",
            "come to me all ye that labour",
            "i will give you rest",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 119:105",
        aliases: &[
            "psalm 119:105",
            "your word is a lamp to my feet",
            "thy word is a lamp unto my feet",
            "a light unto my path",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 10:10",
        aliases: &[
            "john 10:10",
            "i am come that they might have life",
            "life and life more abundantly",
            "life abundantly",
            "the thief comes only to steal and kill",
        ],
        localized_aliases: &[],
    },
    // ─── Extended phrase library (Priority 1 additions) ───────────────────────
    ReferencePattern {
        reference: "John 14:6",
        aliases: &[
            "john 14:6",
            "i am the way the truth and the life",
            "i am the way and the truth and the life",
            "no one comes to the father except through me",
            "no man cometh unto the father but by me",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Isaiah 54:17",
        aliases: &[
            "isaiah 54:17",
            "no weapon formed against you shall prosper",
            "no weapon fashioned against you shall prosper",
            "no weapon that is formed against thee shall prosper",
            "ishaya 54:17",
        ],
        localized_aliases: &["ishaya 54:17"],
    },
    ReferencePattern {
        reference: "Deuteronomy 28:13",
        aliases: &[
            "deuteronomy 28:13",
            "the head and not the tail",
            "above only and not beneath",
            "lend to many nations and borrow from none",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 John 4:4",
        aliases: &[
            "1 john 4:4",
            "greater is he that is in you",
            "greater is he who is in you than he who is in the world",
            "greater is he that is in me than he that is in the world",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Acts 16:31",
        aliases: &[
            "acts 16:31",
            "believe on the lord jesus christ and you shall be saved",
            "believe on the lord jesus christ and thou shalt be saved",
            "you and your household shall be saved",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "2 Corinthians 5:17",
        aliases: &[
            "2 corinthians 5:17",
            "if any man be in christ he is a new creature",
            "if anyone is in christ the new creation has come",
            "old things are passed away all things have become new",
            "behold all things have become new",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 34:8",
        aliases: &[
            "psalm 34:8",
            "taste and see that the lord is good",
            "o taste and see that the lord is good",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Jeremiah 33:3",
        aliases: &[
            "jeremiah 33:3",
            "call unto me and i will answer thee",
            "call to me and i will answer you",
            "i will show you great and mighty things",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 1:37",
        aliases: &[
            "luke 1:37",
            "with god nothing shall be impossible",
            "for with god nothing is impossible",
            "nothing is impossible with god",
            "nothing shall be impossible with god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Isaiah 41:10",
        aliases: &[
            "isaiah 41:10",
            "fear not for i am with you",
            "do not fear for i am with you",
            "be not dismayed for i am your god",
            "i will strengthen you i will help you",
            "ishaya 41:10",
        ],
        localized_aliases: &["ishaya 41:10"],
    },
    ReferencePattern {
        reference: "3 John 1:2",
        aliases: &[
            "3 john 1:2",
            "i wish above all things that you prosper",
            "beloved i pray that you prosper and be in health",
            "prosper and be in health even as your soul prospers",
            "above all things that you prosper and be in health",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Zechariah 4:6",
        aliases: &[
            "zechariah 4:6",
            "not by might nor by power but by my spirit",
            "not by might nor by power says the lord",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Proverbs 18:21",
        aliases: &[
            "proverbs 18:21",
            "death and life are in the power of the tongue",
            "life and death are in the power of the tongue",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 8:31",
        aliases: &[
            "romans 8:31",
            "if god be for us who can be against us",
            "if god is for us who can be against us",
            "who then can be against us",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 8:36",
        aliases: &[
            "john 8:36",
            "if the son shall make you free you shall be free indeed",
            "if the son sets you free you are free indeed",
            "free indeed",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Mark 16:17",
        aliases: &[
            "mark 16:17",
            "these signs shall follow them that believe",
            "in my name they shall cast out devils",
            "in my name shall they cast out demons",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Acts 1:8",
        aliases: &[
            "acts 1:8",
            "you shall receive power when the holy spirit comes upon you",
            "you shall receive power after that the holy ghost is come upon you",
            "power to be my witnesses",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 4:18",
        aliases: &[
            "luke 4:18",
            "the spirit of the lord is upon me",
            "he has anointed me to preach the gospel to the poor",
            "anointed me to preach good news to the poor",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Galatians 6:7",
        aliases: &[
            "galatians 6:7",
            "whatsoever a man soweth that shall he also reap",
            "do not be deceived god is not mocked",
            "whatsoever a man sows he shall also reap",
            "a man reaps what he sows",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Deuteronomy 8:18",
        aliases: &[
            "deuteronomy 8:18",
            "it is god who gives you power to get wealth",
            "power to get wealth that he may establish his covenant",
            "gives you the ability to produce wealth",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Proverbs 4:7",
        aliases: &[
            "proverbs 4:7",
            "wisdom is the principal thing",
            "get wisdom get understanding",
            "wisdom is supreme therefore get wisdom",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 5:14",
        aliases: &[
            "matthew 5:14",
            "ye are the light of the world",
            "you are the light of the world",
            "a city set on a hill cannot be hidden",
            "a city on a hill cannot be hid",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Hebrews 4:12",
        aliases: &[
            "hebrews 4:12",
            "the word of god is living and powerful",
            "the word of god is quick and powerful",
            "sharper than any two edged sword",
            "sharper than any double edged sword",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Peter 5:7",
        aliases: &[
            "1 peter 5:7",
            "casting all your care upon him",
            "cast all your anxiety on him",
            "cast all your cares upon him for he cares for you",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 27:1",
        aliases: &[
            "psalm 27:1",
            "the lord is my light and my salvation",
            "whom shall i fear",
            "the lord is the strength of my life of whom shall i be afraid",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 23:6",
        aliases: &[
            "psalm 23:6",
            "surely goodness and mercy shall follow me",
            "goodness and mercy shall follow me all the days of my life",
            "i will dwell in the house of the lord forever",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "James 5:16",
        aliases: &[
            "james 5:16",
            "the effective fervent prayer of a righteous man avails much",
            "the prayer of a righteous man is powerful and effective",
            "effectual fervent prayer of a righteous man availeth much",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 John 1:9",
        aliases: &[
            "1 john 1:9",
            "if we confess our sins he is faithful and just to forgive us",
            "faithful and just to forgive us our sins",
            "cleanse us from all unrighteousness",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 8:37",
        aliases: &[
            "romans 8:37",
            "we are more than conquerors",
            "more than conquerors through him who loved us",
            "nay in all these things we are more than conquerors",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Psalm 121:2",
        aliases: &[
            "psalm 121:2",
            "my help comes from the lord",
            "my help cometh from the lord",
            "i will lift up my eyes to the hills",
            "i will lift up mine eyes unto the hills",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ephesians 3:20",
        aliases: &[
            "ephesians 3:20",
            "able to do exceeding abundantly above all that we ask or think",
            "immeasurably more than all we ask or imagine",
            "now unto him who is able to do exceedingly abundantly",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 7:7",
        aliases: &[
            "matthew 7:7",
            "ask and it shall be given to you",
            "seek and you shall find",
            "knock and the door shall be opened unto you",
            "ask and it will be given to you seek and you will find",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 6:38",
        aliases: &[
            "luke 6:38",
            "give and it shall be given unto you",
            "give and it will be given to you",
            "good measure pressed down shaken together running over",
            "pressed down shaken together and running over",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Isaiah 55:11",
        aliases: &[
            "isaiah 55:11",
            "my word shall not return to me void",
            "my word will not return to me empty",
            "it shall not return to me void but it shall accomplish",
            "ishaya 55:11",
        ],
        localized_aliases: &["ishaya 55:11"],
    },
    ReferencePattern {
        reference: "Psalm 103:3",
        aliases: &[
            "psalm 103:3",
            "who forgives all your iniquities",
            "who heals all your diseases",
            "who forgives all your sins and heals all your diseases",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 16:33",
        aliases: &[
            "john 16:33",
            "in this world you will have trouble",
            "in the world you shall have tribulation",
            "but be of good cheer i have overcome the world",
            "take heart i have overcome the world",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Philippians 4:19",
        aliases: &[
            "philippians 4:19",
            "my god shall supply all your needs",
            "my god will supply all your needs",
            "according to his riches in glory",
            "according to his glorious riches in christ jesus",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "2 Timothy 3:16",
        aliases: &[
            "2 timothy 3:16",
            "all scripture is given by inspiration of god",
            "all scripture is god breathed",
            "profitable for doctrine for reproof",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ephesians 6:10",
        aliases: &[
            "ephesians 6:10",
            "be strong in the lord and in the power of his might",
            "finally be strong in the lord",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 8:32",
        aliases: &[
            "john 8:32",
            "you shall know the truth and the truth shall make you free",
            "and the truth shall set you free",
            "the truth will set you free",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Mark 11:24",
        aliases: &[
            "mark 11:24",
            "whatever you desire when you pray believe that you receive it",
            "believe you have received it and it will be yours",
            "whatsoever ye desire when ye pray believe that ye receive them",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 10:19",
        aliases: &[
            "luke 10:19",
            "i give you authority to trample on serpents and scorpions",
            "power to tread on serpents and scorpions",
            "nothing shall by any means hurt you",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "James 1:5",
        aliases: &[
            "james 1:5",
            "if any of you lacks wisdom let him ask of god",
            "if any of you lack wisdom ask god",
            "who gives generously to all without finding fault",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Isaiah 26:3",
        aliases: &[
            "isaiah 26:3",
            "perfect peace whose mind is stayed on thee",
            "you will keep in perfect peace",
            "kept in perfect peace because they trust in you",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 15:5",
        aliases: &[
            "john 15:5",
            "i am the vine you are the branches",
            "apart from me you can do nothing",
            "without me ye can do nothing",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 28:19",
        aliases: &[
            "matthew 28:19",
            "go therefore and make disciples of all nations",
            "go ye therefore and teach all nations",
            "all authority has been given to me in heaven and on earth",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Malachi 3:10",
        aliases: &[
            "malachi 3:10",
            "bring all the tithes into the storehouse",
            "bring the whole tithe into the storehouse",
            "i will open for you the windows of heaven",
            "pour out such a blessing that you will not have room enough for it",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 12:2",
        aliases: &[
            "romans 12:2",
            "be not conformed to this world",
            "do not conform to the pattern of this world",
            "transformed by the renewing of your mind",
            "be transformed by the renewing of your mind",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 5:8",
        aliases: &[
            "romans 5:8",
            "while we were yet sinners christ died for us",
            "god demonstrates his love for us in this",
            "god commendeth his love toward us in that while we were yet sinners",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ephesians 2:8",
        aliases: &[
            "ephesians 2:8",
            "by grace you are saved through faith",
            "for it is by grace you have been saved through faith",
            "not of yourselves it is the gift of god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 4:17",
        aliases: &[
            "romans 4:17",
            "calls things that are not as though they were",
            "calleth those things which be not as though they were",
            "calls into being things that do not exist",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 18:18",
        aliases: &[
            "matthew 18:18",
            "whatever you bind on earth shall be bound in heaven",
            "whatsoever ye shall bind on earth shall be bound in heaven",
            "whatsoever you loose on earth shall be loosed in heaven",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Acts 10:38",
        aliases: &[
            "acts 10:38",
            "god anointed jesus of nazareth with the holy spirit and with power",
            "went about doing good and healing all",
            "went about doing good healing all who were oppressed",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "2 Corinthians 9:8",
        aliases: &[
            "2 corinthians 9:8",
            "god is able to make all grace abound to you",
            "god is able to make all grace abound toward you",
            "always having all sufficiency in all things",
        ],
        localized_aliases: &[],
    },
    // ─── Parables of Jesus ────────────────────────────────────────────────────
    ReferencePattern {
        reference: "Matthew 13:3",
        aliases: &[
            "matthew 13:3",
            "parable of the sower",
            "a sower went forth to sow",
            "behold a sower went out to sow",
            "the sower sows the word",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 13:24",
        aliases: &[
            "matthew 13:24",
            "parable of the wheat and tares",
            "parable of the wheat and the weeds",
            "an enemy hath done this",
            "an enemy has done this",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 13:31",
        aliases: &[
            "matthew 13:31",
            "parable of the mustard seed",
            "the kingdom of heaven is like a mustard seed",
            "the kingdom of heaven is like unto a grain of mustard seed",
            "smallest of all seeds",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 13:33",
        aliases: &[
            "matthew 13:33",
            "parable of the leaven",
            "parable of the yeast",
            "the kingdom of heaven is like leaven",
            "the kingdom of heaven is like yeast",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 13:44",
        aliases: &[
            "matthew 13:44",
            "parable of the hidden treasure",
            "treasure hid in a field",
            "treasure hidden in a field",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 13:45",
        aliases: &[
            "matthew 13:45",
            "parable of the pearl of great price",
            "pearl of great price",
            "merchant seeking goodly pearls",
            "merchant looking for fine pearls",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 18:23",
        aliases: &[
            "matthew 18:23",
            "parable of the unforgiving servant",
            "parable of the unmerciful servant",
            "ten thousand talents",
            "owed him ten thousand talents",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 20:1",
        aliases: &[
            "matthew 20:1",
            "parable of the workers in the vineyard",
            "labourers in the vineyard",
            "workers in the vineyard",
            "agreed with the labourers for a penny a day",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 21:33",
        aliases: &[
            "matthew 21:33",
            "parable of the wicked tenants",
            "parable of the wicked husbandmen",
            "a certain householder which planted a vineyard",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 22:1",
        aliases: &[
            "matthew 22:1",
            "parable of the wedding feast",
            "parable of the wedding banquet",
            "the kingdom of heaven is like a king who made a marriage for his son",
            "the kingdom of heaven is like unto a certain king",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 25:1",
        aliases: &[
            "matthew 25:1",
            "parable of the ten virgins",
            "ten virgins which took their lamps",
            "five were wise and five were foolish",
            "five of them were wise and five were foolish",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 25:14",
        aliases: &[
            "matthew 25:14",
            "parable of the talents",
            "the parable of the talents",
            "to one he gave five talents",
            "well done thou good and faithful servant",
            "well done good and faithful servant",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 25:31",
        aliases: &[
            "matthew 25:31",
            "the sheep and the goats",
            "parable of the sheep and the goats",
            "separate them one from another as a shepherd divideth",
            "as you have done it unto the least of these my brethren",
            "inasmuch as ye have done it unto one of the least of these",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 7:24",
        aliases: &[
            "matthew 7:24",
            "parable of the wise and foolish builders",
            "parable of the two builders",
            "built his house upon a rock",
            "built his house on the rock",
            "wise man which built his house upon a rock",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 10:30",
        aliases: &[
            "luke 10:30",
            "parable of the good samaritan",
            "good samaritan",
            "a certain man went down from jerusalem to jericho",
            "fell among thieves",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 12:16",
        aliases: &[
            "luke 12:16",
            "parable of the rich fool",
            "the ground of a certain rich man brought forth plentifully",
            "thou fool this night thy soul shall be required of thee",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 15:4",
        aliases: &[
            "luke 15:4",
            "parable of the lost sheep",
            "what man of you having an hundred sheep",
            "if a man owns a hundred sheep",
            "leave the ninety and nine",
            "leave the ninety nine in the wilderness",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 15:8",
        aliases: &[
            "luke 15:8",
            "parable of the lost coin",
            "what woman having ten pieces of silver",
            "if a woman has ten silver coins",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 15:11",
        aliases: &[
            "luke 15:11",
            "parable of the prodigal son",
            "prodigal son",
            "a certain man had two sons",
            "father i have sinned against heaven",
            "father give me the portion of goods that falleth to me",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 16:19",
        aliases: &[
            "luke 16:19",
            "parable of the rich man and lazarus",
            "rich man and lazarus",
            "lazarus full of sores",
            "a certain rich man clothed in purple and fine linen",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 18:1",
        aliases: &[
            "luke 18:1",
            "parable of the persistent widow",
            "parable of the unjust judge",
            "men ought always to pray and not to faint",
            "always pray and not give up",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 18:9",
        aliases: &[
            "luke 18:9",
            "the pharisee and the tax collector",
            "the pharisee and the publican",
            "two men went up into the temple to pray",
            "god be merciful to me a sinner",
            "god have mercy on me a sinner",
        ],
        localized_aliases: &[],
    },
    // ─── Old Testament narratives & passages ──────────────────────────────────
    ReferencePattern {
        reference: "Genesis 1:26",
        aliases: &[
            "genesis 1:26",
            "let us make man in our image",
            "in our image after our likeness",
            "let us make mankind in our image",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 3:15",
        aliases: &[
            "genesis 3:15",
            "her seed shall bruise thy head",
            "her offspring will crush your head",
            "i will put enmity between thee and the woman",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 4:9",
        aliases: &[
            "genesis 4:9",
            "am i my brothers keeper",
            "where is abel thy brother",
            "where is your brother abel",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 9:13",
        aliases: &[
            "genesis 9:13",
            "i do set my bow in the cloud",
            "i have set my rainbow in the clouds",
            "rainbow in the cloud",
            "token of a covenant between me and the earth",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 12:1",
        aliases: &[
            "genesis 12:1",
            "abraham was called",
            "get thee out of thy country",
            "leave your country your people and your fathers household",
            "go from your country to the land i will show you",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 22:2",
        aliases: &[
            "genesis 22:2",
            "abraham offer isaac",
            "take now thy son thine only son isaac",
            "take your son your only son whom you love isaac",
            "offer him there for a burnt offering",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Genesis 28:12",
        aliases: &[
            "genesis 28:12",
            "jacobs ladder",
            "a ladder set up on the earth and the top of it reached to heaven",
            "stairway resting on the earth with its top reaching to heaven",
            "angels of god ascending and descending",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Exodus 3:14",
        aliases: &[
            "exodus 3:14",
            "i am that i am",
            "i am who i am",
            "tell them i am has sent me",
            "burning bush",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Exodus 14:21",
        aliases: &[
            "exodus 14:21",
            "moses parted the red sea",
            "stretched out his hand over the sea",
            "the lord caused the sea to go back by a strong east wind",
            "and the waters were divided",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Exodus 20:3",
        aliases: &[
            "exodus 20:3",
            "the ten commandments",
            "thou shalt have no other gods before me",
            "you shall have no other gods before me",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Deuteronomy 6:4",
        aliases: &[
            "deuteronomy 6:4",
            "the shema",
            "hear o israel the lord our god the lord is one",
            "hear o israel the lord our god is one lord",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Joshua 6:20",
        aliases: &[
            "joshua 6:20",
            "walls of jericho fell down flat",
            "wall fell down flat",
            "the people shouted with a great shout",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ruth 1:16",
        aliases: &[
            "ruth 1:16",
            "whither thou goest i will go",
            "where you go i will go",
            "thy people shall be my people",
            "your people will be my people and your god my god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Kings 3:9",
        aliases: &[
            "1 kings 3:9",
            "first kings 3:9",
            "solomon asked for wisdom",
            "give therefore thy servant an understanding heart",
            "give your servant a discerning heart",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Kings 18:21",
        aliases: &[
            "1 kings 18:21",
            "first kings 18:21",
            "elijah on mount carmel",
            "how long halt ye between two opinions",
            "how long will you waver between two opinions",
            "if the lord be god follow him",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Daniel 3:25",
        aliases: &[
            "daniel 3:25",
            "shadrach meshach and abednego",
            "fourth man in the fire",
            "the form of the fourth is like the son of god",
            "the fourth looks like a son of the gods",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Daniel 6:22",
        aliases: &[
            "daniel 6:22",
            "daniel in the lions den",
            "my god hath sent his angel and hath shut the lions mouths",
            "my god sent his angel and he shut the mouths of the lions",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Jonah 2:1",
        aliases: &[
            "jonah 2:1",
            "jonah prayed unto the lord his god out of the fishs belly",
            "from inside the fish jonah prayed",
            "in the belly of the great fish",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Habakkuk 2:4",
        aliases: &[
            "habakkuk 2:4",
            "the just shall live by his faith",
            "the righteous will live by his faith",
            "the just shall live by faith",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Micah 6:8",
        aliases: &[
            "micah 6:8",
            "do justly love mercy walk humbly",
            "to act justly and to love mercy and to walk humbly with your god",
            "to do justly and to love mercy and to walk humbly with thy god",
            "what doth the lord require of thee",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Hosea 6:6",
        aliases: &[
            "hosea 6:6",
            "i desired mercy and not sacrifice",
            "i desire mercy not sacrifice",
            "knowledge of god more than burnt offerings",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Amos 5:24",
        aliases: &[
            "amos 5:24",
            "let justice roll down like waters",
            "let judgment run down as waters and righteousness as a mighty stream",
            "righteousness like a mighty stream",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Joel 2:28",
        aliases: &[
            "joel 2:28",
            "i will pour out my spirit upon all flesh",
            "i will pour out my spirit on all people",
            "your sons and your daughters shall prophesy",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Lamentations 3:23",
        aliases: &[
            "lamentations 3:23",
            "his mercies are new every morning",
            "they are new every morning great is thy faithfulness",
            "great is thy faithfulness",
            "great is your faithfulness",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ezekiel 36:26",
        aliases: &[
            "ezekiel 36:26",
            "a new heart also will i give you",
            "i will give you a new heart",
            "take away the stony heart",
            "remove from you the heart of stone",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Proverbs 22:6",
        aliases: &[
            "proverbs 22:6",
            "train up a child in the way he should go",
            "start children off on the way they should go",
            "when he is old he will not depart from it",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Proverbs 16:3",
        aliases: &[
            "proverbs 16:3",
            "commit thy works unto the lord",
            "commit to the lord whatever you do",
            "and thy thoughts shall be established",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Proverbs 27:17",
        aliases: &[
            "proverbs 27:17",
            "iron sharpeneth iron",
            "iron sharpens iron",
            "so a man sharpeneth the countenance of his friend",
            "as one man sharpens another",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ecclesiastes 3:1",
        aliases: &[
            "ecclesiastes 3:1",
            "to every thing there is a season",
            "there is a time for everything",
            "a time for every purpose under heaven",
        ],
        localized_aliases: &[],
    },
    // ─── New Testament narratives & teachings ─────────────────────────────────
    ReferencePattern {
        reference: "Matthew 5:3",
        aliases: &[
            "matthew 5:3",
            "the beatitudes",
            "blessed are the poor in spirit",
            "for theirs is the kingdom of heaven",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 5:4",
        aliases: &[
            "matthew 5:4",
            "blessed are they that mourn",
            "blessed are those who mourn",
            "for they shall be comforted",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 5:6",
        aliases: &[
            "matthew 5:6",
            "blessed are they which do hunger and thirst after righteousness",
            "blessed are those who hunger and thirst for righteousness",
            "for they shall be filled",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 5:7",
        aliases: &[
            "matthew 5:7",
            "blessed are the merciful",
            "for they shall obtain mercy",
            "for they will be shown mercy",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 5:8",
        aliases: &[
            "matthew 5:8",
            "blessed are the pure in heart",
            "for they shall see god",
            "for they will see god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 5:9",
        aliases: &[
            "matthew 5:9",
            "blessed are the peacemakers",
            "for they shall be called the children of god",
            "for they will be called sons of god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 6:9",
        aliases: &[
            "matthew 6:9",
            "the lords prayer",
            "our father which art in heaven",
            "our father in heaven",
            "hallowed be thy name",
            "hallowed be your name",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 6:10",
        aliases: &[
            "matthew 6:10",
            "thy kingdom come thy will be done",
            "your kingdom come your will be done",
            "on earth as it is in heaven",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 6:11",
        aliases: &[
            "matthew 6:11",
            "give us this day our daily bread",
            "give us today our daily bread",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 6:12",
        aliases: &[
            "matthew 6:12",
            "forgive us our trespasses",
            "forgive us our debts",
            "as we forgive those who trespass against us",
            "as we forgive our debtors",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 7:12",
        aliases: &[
            "matthew 7:12",
            "the golden rule",
            "do unto others as you would have them do unto you",
            "all things whatsoever ye would that men should do to you do ye even so to them",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 26:39",
        aliases: &[
            "matthew 26:39",
            "garden of gethsemane prayer",
            "let this cup pass from me",
            "may this cup be taken from me",
            "not my will but thine be done",
            "yet not as i will but as you will",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Matthew 28:6",
        aliases: &[
            "matthew 28:6",
            "he is not here for he is risen",
            "he is not here he has risen",
            "as he said come see the place where the lord lay",
            "the resurrection of jesus",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Mark 12:30",
        aliases: &[
            "mark 12:30",
            "love the lord thy god with all thy heart",
            "love the lord your god with all your heart",
            "with all thy soul and with all thy mind and with all thy strength",
            "the great commandment",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 2:7",
        aliases: &[
            "luke 2:7",
            "she brought forth her firstborn son",
            "wrapped him in swaddling clothes",
            "laid him in a manger",
            "no room for them in the inn",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 2:14",
        aliases: &[
            "luke 2:14",
            "glory to god in the highest",
            "and on earth peace good will toward men",
            "peace on earth to those on whom his favor rests",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Luke 23:34",
        aliases: &[
            "luke 23:34",
            "father forgive them for they know not what they do",
            "father forgive them they do not know what they are doing",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 1:1",
        aliases: &[
            "john 1:1",
            "in the beginning was the word",
            "and the word was with god and the word was god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 1:14",
        aliases: &[
            "john 1:14",
            "the word was made flesh",
            "the word became flesh",
            "and dwelt among us",
            "and made his dwelling among us",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 6:35",
        aliases: &[
            "john 6:35",
            "i am the bread of life",
            "he that cometh to me shall never hunger",
            "whoever comes to me will never go hungry",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 10:11",
        aliases: &[
            "john 10:11",
            "i am the good shepherd",
            "the good shepherd giveth his life for the sheep",
            "the good shepherd lays down his life for the sheep",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 11:25",
        aliases: &[
            "john 11:25",
            "i am the resurrection and the life",
            "he that believeth in me though he were dead yet shall he live",
            "the one who believes in me will live even though they die",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 13:34",
        aliases: &[
            "john 13:34",
            "a new commandment i give unto you",
            "a new command i give you",
            "love one another as i have loved you",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "John 19:30",
        aliases: &[
            "john 19:30",
            "it is finished",
            "he said it is finished and bowed his head",
            "he gave up the ghost",
            "he gave up his spirit",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Acts 2:4",
        aliases: &[
            "acts 2:4",
            "they were all filled with the holy ghost",
            "they were all filled with the holy spirit",
            "began to speak with other tongues",
            "began to speak in other tongues",
            "tongues of fire",
            "the day of pentecost",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Acts 4:12",
        aliases: &[
            "acts 4:12",
            "neither is there salvation in any other",
            "salvation is found in no one else",
            "no other name under heaven given among men",
            "no other name by which we must be saved",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Acts 9:4",
        aliases: &[
            "acts 9:4",
            "saul saul why persecutest thou me",
            "saul saul why do you persecute me",
            "the road to damascus",
            "damascus road",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 3:23",
        aliases: &[
            "romans 3:23",
            "all have sinned and come short of the glory of god",
            "for all have sinned and fall short of the glory of god",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 6:23",
        aliases: &[
            "romans 6:23",
            "the wages of sin is death",
            "the gift of god is eternal life",
            "but the gift of god is eternal life through jesus christ our lord",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Romans 12:1",
        aliases: &[
            "romans 12:1",
            "present your bodies a living sacrifice",
            "offer your bodies as a living sacrifice",
            "your reasonable service",
            "this is your true and proper worship",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Corinthians 13:13",
        aliases: &[
            "1 corinthians 13:13",
            "and now abideth faith hope charity",
            "and now these three remain faith hope and love",
            "the greatest of these is love",
            "the greatest of these is charity",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Corinthians 15:55",
        aliases: &[
            "1 corinthians 15:55",
            "o death where is thy sting",
            "where o death is your victory",
            "where o grave is thy victory",
            "death has been swallowed up in victory",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Galatians 5:22",
        aliases: &[
            "galatians 5:22",
            "the fruit of the spirit",
            "fruit of the spirit is love joy peace",
            "longsuffering gentleness goodness faith",
            "love joy peace forbearance kindness goodness faithfulness",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Ephesians 2:10",
        aliases: &[
            "ephesians 2:10",
            "we are his workmanship",
            "we are gods handiwork",
            "created in christ jesus unto good works",
            "created in christ jesus to do good works",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Colossians 3:23",
        aliases: &[
            "colossians 3:23",
            "whatsoever ye do do it heartily as to the lord",
            "whatever you do work at it with all your heart",
            "as working for the lord and not for human masters",
            "as to the lord and not unto men",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Thessalonians 5:17",
        aliases: &[
            "1 thessalonians 5:17",
            "first thessalonians 5:17",
            "pray without ceasing",
            "pray continually",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Thessalonians 5:18",
        aliases: &[
            "1 thessalonians 5:18",
            "first thessalonians 5:18",
            "in every thing give thanks",
            "give thanks in all circumstances",
            "for this is the will of god in christ jesus concerning you",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Hebrews 12:1",
        aliases: &[
            "hebrews 12:1",
            "great cloud of witnesses",
            "compassed about with so great a cloud of witnesses",
            "surrounded by such a great cloud of witnesses",
            "lay aside every weight and the sin which doth so easily beset us",
            "run with patience the race that is set before us",
            "run with perseverance the race marked out for us",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Hebrews 13:5",
        aliases: &[
            "hebrews 13:5",
            "i will never leave thee nor forsake thee",
            "never will i leave you never will i forsake you",
            "be content with such things as ye have",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Hebrews 13:8",
        aliases: &[
            "hebrews 13:8",
            "jesus christ the same yesterday today and forever",
            "jesus christ the same yesterday and today and for ever",
            "jesus christ is the same yesterday and today and forever",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "James 1:2",
        aliases: &[
            "james 1:2",
            "count it all joy when you fall into divers temptations",
            "consider it pure joy whenever you face trials",
            "the testing of your faith produces patience",
            "the testing of your faith produces perseverance",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 Peter 2:9",
        aliases: &[
            "1 peter 2:9",
            "first peter 2:9",
            "a chosen generation a royal priesthood",
            "you are a chosen people a royal priesthood",
            "an holy nation a peculiar people",
            "a holy nation gods special possession",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "1 John 4:8",
        aliases: &[
            "1 john 4:8",
            "first john 4:8",
            "god is love",
            "he that loveth not knoweth not god for god is love",
            "whoever does not love does not know god because god is love",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Revelation 3:20",
        aliases: &[
            "revelation 3:20",
            "behold i stand at the door and knock",
            "i stand at the door and knock",
            "if any man hear my voice and open the door",
            "if anyone hears my voice and opens the door",
            "i will come in to him and will sup with him",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "Revelation 22:13",
        aliases: &[
            "revelation 22:13",
            "i am alpha and omega",
            "i am the alpha and the omega",
            "the beginning and the end the first and the last",
        ],
        localized_aliases: &[],
    },
    ReferencePattern {
        reference: "2 Peter 3:9",
        aliases: &[
            "2 peter 3:9",
            "second peter 3:9",
            "the lord is not slack concerning his promise",
            "the lord is not slow in keeping his promise",
            "not willing that any should perish",
            "not wanting anyone to perish",
            "but that all should come to repentance",
        ],
        localized_aliases: &[],
    },
];



const LANGUAGE_PROFILES: &[LanguageProfile] = &[
    LanguageProfile {
        code: "en",
        name: "English",
        stt_locale: "en",
        terms: &[
            "romans", "psalm", "john", "isaiah", "chapter", "verse", "lord",
        ],
        aliases_ready: true,
        offline_stt_ready: true,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "yo",
        name: "Yoruba",
        stt_locale: "yo",
        terms: &["olorun", "awon", "romu", "johanu", "a mo pe", "sise po"],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "ig",
        name: "Igbo",
        stt_locale: "ig",
        terms: &["chineke", "ndi", "jọn", "abu oma", "rom"],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "ha",
        name: "Hausa",
        stt_locale: "ha",
        terms: &[
            "romawa", "zabura", "yohanna", "ishaya", "allah", "ubangiji", "bude",
        ],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "tw",
        name: "Twi",
        stt_locale: "ak",
        terms: &[
            "romafo", "nnwom", "yohane", "yesaia", "onyame", "yenhwɛ", "momma",
        ],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "sw",
        name: "Swahili",
        stt_locale: "sw",
        terms: &[
            "warumi", "zaburi", "yohana", "isaya", "bwana", "mungu", "tufungue",
        ],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "xh",
        name: "Xhosa",
        stt_locale: "xh",
        terms: &[
            "kwabaseroma",
            "iindumiso",
            "yohane",
            "isaya",
            "uthixo",
            "masivule",
        ],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "es",
        name: "Spanish",
        stt_locale: "es",
        terms: &[
            "romanos", "salmo", "juan", "isaias", "dios", "senor", "abramos",
        ],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
    LanguageProfile {
        code: "fr",
        name: "French",
        stt_locale: "fr",
        terms: &[
            "romains", "psaume", "jean", "esaie", "dieu", "seigneur", "ouvrons",
        ],
        aliases_ready: true,
        offline_stt_ready: false,
        cloud_stt_ready: true,
    },
];

pub(crate) fn fold_for_matching(input: &str) -> String {
    input
        .to_lowercase()
        .replace(['á', 'à', 'â', 'ã', 'ä'], "a")
        .replace(['é', 'è', 'ê', 'ë', 'ẹ', 'ɛ'], "e")
        .replace(['í', 'ì', 'î', 'ï'], "i")
        .replace(['ó', 'ò', 'ô', 'õ', 'ö', 'ọ', 'ɔ'], "o")
        .replace(['ú', 'ù', 'û', 'ü'], "u")
        .replace('ñ', "n")
        .replace('ç', "c")
        .replace(['ṣ', 'ş'], "s")
        .replace('’', "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: "seg-1".to_string(),
            session_id: ServiceSessionId::new("sunday-am").expect("valid session id"),
            started_at_ms: 1_000,
            ended_at_ms: 2_000,
            speaker_label: Some("Pastor".to_string()),
            language: "English".to_string(),
            text: text.to_string(),
            confidence: 0.94,
            adapter: "offline-whisper".to_string(),
            latency_ms: 420,
        }
    }

    #[test]
    fn exact_reference_becomes_certain_candidate() {
        let detector = ReferenceKeywordDetector;
        let candidates = detector.detect(&segment("Open Psalm 23:4 with me."), &[]);

        assert_eq!(candidates[0].reference, "Psalm 23:4");
        assert_eq!(candidates[0].bucket, ConfidenceBucket::Certain);
    }

    #[test]
    fn thematic_coreference_is_not_auto_safe() {
        let detector = ReferenceKeywordDetector;
        let candidate = detector
            .detect(&segment("And then he wept."), &[])
            .remove(0);
        let policy = ConfidencePolicy::default();

        assert_eq!(candidate.bucket, ConfidenceBucket::Ambiguous);
        assert!(!policy.can_auto_preview(&candidate));
        assert!(!policy.can_auto_live());
    }

    /// Smoke test for the expanded phrase library — parables, beatitudes,
    /// and major narrative passages should each surface a candidate when
    /// referenced by their canonical phrasing. Regression-guards against
    /// accidental removal of patterns or off-by-one in the alias arrays.
    #[test]
    fn expanded_phrase_library_covers_parables_and_beatitudes() {
        let detector = ReferenceKeywordDetector;
        let cases: &[(&str, &str)] = &[
            ("Today's text is the parable of the prodigal son.", "Luke 15:11"),
            ("Recall the parable of the good samaritan.", "Luke 10:30"),
            ("The parable of the talents teaches stewardship.", "Matthew 25:14"),
            ("Blessed are the peacemakers for they shall be called the children of god.", "Matthew 5:9"),
            ("Our father which art in heaven, hallowed be thy name.", "Matthew 6:9"),
            ("The fruit of the spirit is love joy peace.", "Galatians 5:22"),
            ("In the beginning was the word, and the word was with god.", "John 1:1"),
            ("Jesus said it is finished and bowed his head.", "John 19:30"),
            ("He is not here for he is risen, as he said.", "Matthew 28:6"),
            ("Behold I stand at the door and knock.", "Revelation 3:20"),
            ("To do justly and to love mercy and to walk humbly with thy god.", "Micah 6:8"),
            ("The just shall live by his faith.", "Habakkuk 2:4"),
            ("Train up a child in the way he should go.", "Proverbs 22:6"),
            ("Pray without ceasing — that's our charge tonight.", "1 Thessalonians 5:17"),
            ("Whither thou goest I will go.", "Ruth 1:16"),
            ("Iron sharpeneth iron.", "Proverbs 27:17"),
        ];
        for (text, expected) in cases {
            let candidates = detector.detect(&segment(text), &[]);
            assert!(
                candidates.iter().any(|c| c.reference == *expected),
                "expected to detect {expected:?} from {text:?}, got {:?}",
                candidates.iter().map(|c| &c.reference).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn hausa_reference_alias_is_detected() {
        let detector = ReferenceKeywordDetector;
        let mut hausa_segment = segment("Mu bude Romawa 8:28 yau.");
        hausa_segment.language = "Hausa".to_string();

        let candidate = detector.detect(&hausa_segment, &[]).remove(0);

        assert_eq!(candidate.reference, "Romans 8:28");
        assert_eq!(candidate.bucket, ConfidenceBucket::Strong);
    }

    #[test]
    fn language_detector_supports_requested_languages() {
        let detector = KeywordLanguageDetector;
        let supported = detector.supported_languages();
        let codes = supported
            .iter()
            .map(|language| language.code)
            .collect::<Vec<_>>();

        for expected in ["ha", "tw", "sw", "xh", "es", "fr"] {
            assert!(codes.contains(&expected));
        }
    }

    #[test]
    fn language_detector_identifies_swahili_and_spanish() {
        let detector = KeywordLanguageDetector;
        let swahili = detector.detect_language("Tufungue Warumi 8:28 pamoja.", None);
        let spanish = detector.detect_language("Abramos Romanos 8:28 juntos.", None);

        assert_eq!(swahili.code, "sw");
        assert!(swahili.confidence >= 0.3);
        assert_eq!(spanish.code, "es");
        assert!(spanish.confidence >= 0.3);
    }

    #[test]
    fn fixture_evaluation_reports_precision_and_recall() {
        let detector = ReferenceKeywordDetector;
        let fixtures = [
            AccuracyFixture {
                id: "english-romans",
                language: "English",
                text: "Please open Romans 8:28.",
                expected_reference: Some("Romans 8:28"),
            },
            AccuracyFixture {
                id: "hausa-romans",
                language: "Hausa",
                text: "Mu bude Romawa 8:28 tare da ikilisiya.",
                expected_reference: Some("Romans 8:28"),
            },
            AccuracyFixture {
                id: "swahili-romans",
                language: "Swahili",
                text: "Tufungue Warumi 8:28 pamoja.",
                expected_reference: Some("Romans 8:28"),
            },
            AccuracyFixture {
                id: "negative-prayer",
                language: "English",
                text: "We will pray after the song.",
                expected_reference: None,
            },
        ];

        let evaluation = evaluate_accuracy_fixtures(&detector, &fixtures);

        assert_eq!(evaluation.true_positives, 3);
        assert_eq!(evaluation.false_positives, 0);
        assert_eq!(evaluation.false_negatives, 0);
        assert!(evaluation.precision >= 0.95);
        assert!(evaluation.recall >= 0.90);
    }
}
