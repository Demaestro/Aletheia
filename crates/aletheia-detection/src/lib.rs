//! Scripture detection service boundary.

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

/// Minimal deterministic detector used until the full parser lands.
#[derive(Default)]
pub struct ReferenceKeywordDetector;

impl ScriptureDetector for ReferenceKeywordDetector {
    fn detect(
        &self,
        segment: &TranscriptSegment,
        _context: &[TranscriptSegment],
    ) -> Vec<ScriptureCandidate> {
        let lower = fold_for_matching(&segment.text);

        REFERENCE_PATTERNS
            .iter()
            .filter_map(|pattern| {
                let reference = pattern.reference;
                let aliases = pattern.aliases;
                aliases
                    .iter()
                    .find(|alias| lower.contains(&fold_for_matching(alias)))
                    .map(|alias| {
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

                        ScriptureCandidate {
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
                        }
                    })
            })
            .collect()
    }
}

struct ReferencePattern {
    reference: &'static str,
    aliases: &'static [&'static str],
    localized_aliases: &'static [&'static str],
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

fn fold_for_matching(input: &str) -> String {
    input
        .to_lowercase()
        .replace('á', "a")
        .replace('à', "a")
        .replace('â', "a")
        .replace('ã', "a")
        .replace('ä', "a")
        .replace('é', "e")
        .replace('è', "e")
        .replace('ê', "e")
        .replace('ë', "e")
        .replace('í', "i")
        .replace('ì', "i")
        .replace('î', "i")
        .replace('ï', "i")
        .replace('ó', "o")
        .replace('ò', "o")
        .replace('ô', "o")
        .replace('õ', "o")
        .replace('ö', "o")
        .replace('ú', "u")
        .replace('ù', "u")
        .replace('û', "u")
        .replace('ü', "u")
        .replace('ñ', "n")
        .replace('ç', "c")
        .replace('ẹ', "e")
        .replace('ɛ', "e")
        .replace('ọ', "o")
        .replace('ɔ', "o")
        .replace('ṣ', "s")
        .replace('ş', "s")
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
