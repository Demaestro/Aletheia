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
///
/// Boundaries follow the architectural-vision contract:
/// * `Certain`  — score ≥ 0.92, eligible for auto-send when other rules pass
/// * `Strong`   — 0.75 ≤ score < 0.92, prepared as next suggestion (operator confirms)
/// * `Likely`   — 0.55 ≤ score < 0.75, requires explicit operator approval
/// * `Unsafe`   — score < 0.55, dropped from the operator queue
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfidenceBucket {
    Certain,
    Strong,
    Likely,
    Unsafe,
}

impl ConfidenceBucket {
    /// Maps a score to the operator-facing bucket.
    pub fn from_score(score: f32) -> Self {
        match score {
            value if value >= 0.92 => Self::Certain,
            value if value >= 0.75 => Self::Strong,
            value if value >= 0.55 => Self::Likely,
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

/// A scripture candidate after ranking, carrying the tiers that produced it.
///
/// A single reference may be backed by several signals (e.g. quote match +
/// thematic catalog hit). The `tiers` vector records every signal that fired
/// so the auto-open evaluator can apply the architectural-vision rules
/// (quote needs lexical+semantic, never semantic-alone, etc.).
#[derive(Clone, Debug, PartialEq)]
pub struct RankedCandidate {
    pub reference: String,
    pub score: f32,
    pub tiers: Vec<DetectionTier>,
}

/// Decision returned by [`ConfidencePolicy::evaluate_auto_open`].
///
/// `Open` is the only state in which the projection layer is allowed to push
/// content to live without operator interaction; every other state surfaces in
/// the operator UI for confirmation or approval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoOpenDecision {
    Open,
    Prepare,
    RequireApproval,
    Ignore,
}

/// First-class operating mode that gates how detector output reaches outputs.
///
/// The architectural vision distinguishes five distinct postures:
/// * `Manual`   — detector is advisory only; nothing auto-sends. Every send is
///   an operator click.
/// * `Assisted` — detector may prepare next-up, but the operator must confirm
///   before anything goes live. Auto-open decisions are downgraded to
///   `Prepare`.
/// * `Auto`     — trusts the safety policy. `AutoOpenDecision::Open` is taken
///   as-is; everything else still surfaces for approval.
/// * `Rehearsal` — same gating as `Auto` but the live output adapter is
///   replaced with a no-op (mock vMix). Used for end-to-end dry runs.
/// * `Mock`     — entire output stack is simulated. No external systems are
///   touched. Dev/demo only.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OperatingMode {
    Manual,
    Assisted,
    Auto,
    Rehearsal,
    Mock,
}

impl Default for OperatingMode {
    /// `Assisted` is the safe default: detection helps, operator confirms.
    fn default() -> Self {
        Self::Assisted
    }
}

impl OperatingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Assisted => "assisted",
            Self::Auto => "auto",
            Self::Rehearsal => "rehearsal",
            Self::Mock => "mock",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "manual" => Some(Self::Manual),
            "assisted" => Some(Self::Assisted),
            "auto" => Some(Self::Auto),
            "rehearsal" => Some(Self::Rehearsal),
            "mock" => Some(Self::Mock),
            _ => None,
        }
    }

    /// True when the mode allows real external side effects (vMix, projector).
    /// `Rehearsal` and `Mock` swap in inert adapters and return false.
    pub fn allows_real_outputs(&self) -> bool {
        matches!(self, Self::Manual | Self::Assisted | Self::Auto)
    }

    /// True when the mode permits taking `AutoOpenDecision::Open` literally.
    /// Manual and Assisted always require the operator, so `Open` is downgraded.
    pub fn permits_auto_send(&self) -> bool {
        matches!(self, Self::Auto | Self::Rehearsal)
    }

    /// Applies the mode's gating to a safety-policy decision.
    ///
    /// The safety policy decides what's *safe* to auto-send; the mode decides
    /// whether the operator has *authorized* auto-send. Both must agree.
    pub fn gate(&self, decision: AutoOpenDecision) -> AutoOpenDecision {
        match self {
            Self::Manual => match decision {
                AutoOpenDecision::Ignore => AutoOpenDecision::Ignore,
                _ => AutoOpenDecision::RequireApproval,
            },
            Self::Assisted => match decision {
                AutoOpenDecision::Open => AutoOpenDecision::Prepare,
                other => other,
            },
            Self::Auto | Self::Rehearsal | Self::Mock => decision,
        }
    }
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

    /// Applies the five architectural-vision auto-open rules to a ranked list.
    ///
    /// Rules enforced (in order):
    /// 1. Score below 0.55 → `Ignore`.
    /// 2. Score 0.55..0.75 → `RequireApproval`.
    /// 3. Top result within 0.10 of the runner-up → `RequireApproval` (never
    ///    auto-open when multiple candidates are close).
    /// 4. Topic match alone (only `ThematicCoreference`) → `RequireApproval`.
    /// 5. Semantic alone (only `SermonContextRerank` and/or `CloudEnhancement`)
    ///    → `RequireApproval`. A quote match (`VerseQuotation`) needs at least
    ///    one corroborating tier (semantic OR exact reference) before it can
    ///    auto-open.
    /// 6. Score ≥ 0.92 with at least one explicit-reference signal
    ///    (`ExactReference`, `SpokenNumberReference`, or
    ///    `LocalLanguageAlias`), or a quote match with corroboration, may
    ///    `Open`. Otherwise `Prepare` for the operator.
    ///
    /// The caller is still responsible for honouring the operating mode
    /// (Manual / Assisted / Auto / Rehearsal / Mock) — this method only states
    /// what the *safety policy* permits.
    pub fn evaluate_auto_open(&self, ranked: &[RankedCandidate]) -> AutoOpenDecision {
        let Some(top) = ranked.first() else {
            return AutoOpenDecision::Ignore;
        };

        let bucket = ConfidenceBucket::from_score(top.score);
        if matches!(bucket, ConfidenceBucket::Unsafe) {
            return AutoOpenDecision::Ignore;
        }
        if matches!(bucket, ConfidenceBucket::Likely) {
            return AutoOpenDecision::RequireApproval;
        }

        // Rule 5: never auto-open when multiple candidates are close.
        if let Some(runner_up) = ranked.get(1) {
            if (top.score - runner_up.score).abs() < 0.10 {
                return AutoOpenDecision::RequireApproval;
            }
        }

        let has_explicit = top.tiers.iter().any(|tier| {
            matches!(
                tier,
                DetectionTier::ExactReference
                    | DetectionTier::SpokenNumberReference
                    | DetectionTier::LocalLanguageAlias
            )
        });
        let has_quote = top.tiers.contains(&DetectionTier::VerseQuotation);
        let has_topic = top.tiers.contains(&DetectionTier::ThematicCoreference);
        let has_semantic = top.tiers.iter().any(|tier| {
            matches!(
                tier,
                DetectionTier::SermonContextRerank | DetectionTier::CloudEnhancement
            )
        });

        // Rule 3: topic-only requires approval.
        if has_topic && !has_explicit && !has_quote {
            return AutoOpenDecision::RequireApproval;
        }

        // Rule 4: never auto-open from semantic alone.
        if has_semantic && !has_explicit && !has_quote && !has_topic {
            return AutoOpenDecision::RequireApproval;
        }

        // Rule 2: quote needs lexical + semantic agreement before auto-open.
        if has_quote && !has_explicit && !has_semantic {
            return AutoOpenDecision::RequireApproval;
        }

        // Strong (0.75..0.92) → prepare as next, operator confirms.
        if matches!(bucket, ConfidenceBucket::Strong) {
            return AutoOpenDecision::Prepare;
        }

        // Certain (≥0.92) and the gating rules above are satisfied.
        if has_explicit || (has_quote && has_semantic) {
            AutoOpenDecision::Open
        } else {
            AutoOpenDecision::Prepare
        }
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

pub mod books;
pub mod calibration;
pub mod cross_encoder;
pub mod cross_refs;
pub mod embeddings;
pub mod grammar;
pub mod normalize;
pub mod phrase_catalog;

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

        assert_eq!(candidate.bucket, ConfidenceBucket::Likely);
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

    fn ranked(reference: &str, score: f32, tiers: &[DetectionTier]) -> RankedCandidate {
        RankedCandidate {
            reference: reference.to_string(),
            score,
            tiers: tiers.to_vec(),
        }
    }

    #[test]
    fn auto_open_requires_explicit_or_corroborated_quote() {
        let policy = ConfidencePolicy::default();

        // Explicit reference at high confidence: Open.
        let explicit = ranked("Psalm 23:4", 0.95, &[DetectionTier::ExactReference]);
        assert_eq!(
            policy.evaluate_auto_open(&[explicit]),
            AutoOpenDecision::Open
        );

        // Quote alone at high confidence: still requires approval.
        let quote_only = ranked("Romans 8:28", 0.94, &[DetectionTier::VerseQuotation]);
        assert_eq!(
            policy.evaluate_auto_open(&[quote_only]),
            AutoOpenDecision::RequireApproval
        );

        // Quote + semantic agreement: Open.
        let quote_and_semantic = ranked(
            "Romans 8:28",
            0.94,
            &[
                DetectionTier::VerseQuotation,
                DetectionTier::SermonContextRerank,
            ],
        );
        assert_eq!(
            policy.evaluate_auto_open(&[quote_and_semantic]),
            AutoOpenDecision::Open
        );
    }

    #[test]
    fn auto_open_rejects_topic_or_semantic_alone() {
        let policy = ConfidencePolicy::default();

        let topic = ranked(
            "Matthew 25:14-30",
            0.94,
            &[DetectionTier::ThematicCoreference],
        );
        assert_eq!(
            policy.evaluate_auto_open(&[topic]),
            AutoOpenDecision::RequireApproval
        );

        let semantic = ranked("John 14:6", 0.95, &[DetectionTier::SermonContextRerank]);
        assert_eq!(
            policy.evaluate_auto_open(&[semantic]),
            AutoOpenDecision::RequireApproval
        );
    }

    #[test]
    fn auto_open_blocks_when_top_two_are_close() {
        let policy = ConfidencePolicy::default();
        let top = ranked("John 3:16", 0.95, &[DetectionTier::ExactReference]);
        let runner_up = ranked("John 3:36", 0.93, &[DetectionTier::ExactReference]);

        assert_eq!(
            policy.evaluate_auto_open(&[top, runner_up]),
            AutoOpenDecision::RequireApproval
        );
    }

    #[test]
    fn auto_open_buckets_by_score() {
        let policy = ConfidencePolicy::default();

        let strong = ranked("Psalm 23:4", 0.80, &[DetectionTier::ExactReference]);
        assert_eq!(
            policy.evaluate_auto_open(&[strong]),
            AutoOpenDecision::Prepare
        );

        let likely = ranked("Psalm 23:4", 0.60, &[DetectionTier::ExactReference]);
        assert_eq!(
            policy.evaluate_auto_open(&[likely]),
            AutoOpenDecision::RequireApproval
        );

        let unsafe_low = ranked("Psalm 23:4", 0.40, &[DetectionTier::ExactReference]);
        assert_eq!(
            policy.evaluate_auto_open(&[unsafe_low]),
            AutoOpenDecision::Ignore
        );
    }

    #[test]
    fn operating_mode_default_is_assisted() {
        assert_eq!(OperatingMode::default(), OperatingMode::Assisted);
    }

    #[test]
    fn operating_mode_round_trips_str() {
        for mode in [
            OperatingMode::Manual,
            OperatingMode::Assisted,
            OperatingMode::Auto,
            OperatingMode::Rehearsal,
            OperatingMode::Mock,
        ] {
            assert_eq!(OperatingMode::from_str(mode.as_str()), Some(mode));
        }
        assert_eq!(OperatingMode::from_str("garbage"), None);
    }

    #[test]
    fn manual_mode_downgrades_every_send_to_approval() {
        let mode = OperatingMode::Manual;
        assert_eq!(
            mode.gate(AutoOpenDecision::Open),
            AutoOpenDecision::RequireApproval
        );
        assert_eq!(
            mode.gate(AutoOpenDecision::Prepare),
            AutoOpenDecision::RequireApproval
        );
        // Ignore stays Ignore — no point asking the operator about noise.
        assert_eq!(
            mode.gate(AutoOpenDecision::Ignore),
            AutoOpenDecision::Ignore
        );
        assert!(!mode.permits_auto_send());
    }

    #[test]
    fn assisted_mode_downgrades_only_open_to_prepare() {
        let mode = OperatingMode::Assisted;
        assert_eq!(mode.gate(AutoOpenDecision::Open), AutoOpenDecision::Prepare);
        assert_eq!(
            mode.gate(AutoOpenDecision::Prepare),
            AutoOpenDecision::Prepare
        );
        assert_eq!(
            mode.gate(AutoOpenDecision::RequireApproval),
            AutoOpenDecision::RequireApproval
        );
        assert!(!mode.permits_auto_send());
    }

    #[test]
    fn auto_and_rehearsal_pass_decisions_through() {
        for mode in [OperatingMode::Auto, OperatingMode::Rehearsal] {
            assert_eq!(mode.gate(AutoOpenDecision::Open), AutoOpenDecision::Open);
            assert!(mode.permits_auto_send());
        }
    }

    #[test]
    fn rehearsal_and_mock_block_real_outputs() {
        assert!(OperatingMode::Manual.allows_real_outputs());
        assert!(OperatingMode::Assisted.allows_real_outputs());
        assert!(OperatingMode::Auto.allows_real_outputs());
        assert!(!OperatingMode::Rehearsal.allows_real_outputs());
        assert!(!OperatingMode::Mock.allows_real_outputs());
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
