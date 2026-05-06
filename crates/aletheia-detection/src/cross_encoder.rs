//! Cross-encoder reranker (Stage B).
//!
//! Where a [`SentenceEmbedder`](crate::embeddings::SentenceEmbedder)
//! scores `(query)` and `(candidate)` independently and combines them
//! with cosine similarity, a cross-encoder ingests `(query, candidate)`
//! together and produces one calibrated relevance score in `[0.0, 1.0]`.
//! Cross-encoders are ~10× more accurate at paraphrase ranking but
//! ~100× slower per pair — so we run them only on the top-N from
//! Stage A.
//!
//! The default impl, [`HeuristicCrossEncoder`], is a deterministic
//! reranker built from signals the bi-encoder cannot see directly:
//!
//! * Character-n-gram Jaccard (paraphrase-tolerant lexical overlap).
//! * Bigram order preservation (catches "world loved God" vs.
//!   "God loved world").
//! * Length ratio penalty (very long candidates against short queries
//!   are penalised).
//! * Stopword-stripped token coverage with positional weighting
//!   (matches at the start are more telling).
//!
//! Combined, these give a notable lift on paraphrase + partial-quote
//! recall above either FTS or pure cosine. When a real cross-encoder
//! ONNX model is shipped, the desktop crate replaces this trait impl
//! without touching callers.

use std::collections::HashSet;
use std::sync::Arc;

/// Scores `(query, passage)` pairs in `[0.0, 1.0]`.
pub trait CrossEncoder: Send + Sync {
    /// Stable identifier including model version. Used to invalidate
    /// any per-pair cache the caller may keep.
    fn version_tag(&self) -> &str;

    /// Score a single pair. Higher = more relevant.
    fn score(&self, query: &str, passage: &str) -> f32;

    /// Default batch path: serial. ONNX backends override for throughput.
    fn score_batch(&self, query: &str, passages: &[&str]) -> Vec<f32> {
        passages.iter().map(|p| self.score(query, p)).collect()
    }
}

/// Pure-Rust deterministic reranker. No model file. Tuned to lift
/// paraphrase + partial-quote pairs while keeping unrelated pairs near
/// the bottom of the range.
pub struct HeuristicCrossEncoder {
    tag: &'static str,
}

impl HeuristicCrossEncoder {
    pub fn new() -> Self {
        Self {
            tag: "heuristic-rerank-v1",
        }
    }
}

impl Default for HeuristicCrossEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossEncoder for HeuristicCrossEncoder {
    fn version_tag(&self) -> &str {
        self.tag
    }

    fn score(&self, query: &str, passage: &str) -> f32 {
        if query.trim().is_empty() || passage.trim().is_empty() {
            return 0.0;
        }
        let q_norm = normalise(query);
        let p_norm = normalise(passage);

        let jaccard_3 = char_ngram_jaccard(&q_norm, &p_norm, 3);
        let jaccard_4 = char_ngram_jaccard(&q_norm, &p_norm, 4);
        let bigram_order = ordered_bigram_overlap(&q_norm, &p_norm);
        let token_cov = positional_token_coverage(&q_norm, &p_norm);
        let length_penalty = length_ratio_penalty(&q_norm, &p_norm);

        let raw = 0.32 * jaccard_3 + 0.20 * jaccard_4 + 0.18 * bigram_order + 0.30 * token_cov;
        (raw * length_penalty).clamp(0.0, 1.0)
    }
}

fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

fn char_ngrams(text: &str, n: usize) -> HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < n {
        return HashSet::new();
    }
    let mut out = HashSet::with_capacity(chars.len().saturating_sub(n).saturating_add(1));
    for window in chars.windows(n) {
        out.insert(window.iter().collect());
    }
    out
}

fn char_ngram_jaccard(a: &str, b: &str, n: usize) -> f32 {
    let aa = char_ngrams(a, n);
    let bb = char_ngrams(b, n);
    if aa.is_empty() && bb.is_empty() {
        return 0.0;
    }
    let inter = aa.intersection(&bb).count() as f32;
    let union = aa.union(&bb).count().max(1) as f32;
    inter / union
}

fn tokenise_no_stop(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .filter(|t| t.len() >= 3 && !is_stopword(t))
        .collect()
}

fn is_stopword(t: &str) -> bool {
    matches!(
        t,
        "the"
            | "and"
            | "for"
            | "but"
            | "with"
            | "from"
            | "that"
            | "this"
            | "his"
            | "her"
            | "him"
            | "she"
            | "they"
            | "them"
            | "their"
            | "you"
            | "your"
            | "are"
            | "was"
            | "were"
            | "have"
            | "has"
            | "had"
            | "not"
            | "shall"
            | "will"
            | "unto"
            | "all"
            | "any"
            | "who"
            | "which"
            | "what"
            | "thy"
            | "thee"
            | "thou"
            | "into"
    )
}

fn ordered_bigram_overlap(query: &str, passage: &str) -> f32 {
    let q_tokens = tokenise_no_stop(query);
    let p_tokens = tokenise_no_stop(passage);
    if q_tokens.len() < 2 || p_tokens.len() < 2 {
        return 0.0;
    }
    let q_bigrams: HashSet<(String, String)> = q_tokens
        .windows(2)
        .map(|w| (w[0].to_string(), w[1].to_string()))
        .collect();
    if q_bigrams.is_empty() {
        return 0.0;
    }
    let p_bigrams: HashSet<(String, String)> = p_tokens
        .windows(2)
        .map(|w| (w[0].to_string(), w[1].to_string()))
        .collect();
    let inter = q_bigrams.intersection(&p_bigrams).count() as f32;
    inter / q_bigrams.len() as f32
}

fn positional_token_coverage(query: &str, passage: &str) -> f32 {
    let q_tokens = tokenise_no_stop(query);
    if q_tokens.is_empty() {
        return 0.0;
    }
    let p_set: HashSet<&str> = tokenise_no_stop(passage).into_iter().collect();
    if p_set.is_empty() {
        return 0.0;
    }
    let mut hit = 0.0_f32;
    let mut weight_sum = 0.0_f32;
    let n = q_tokens.len() as f32;
    for (i, tok) in q_tokens.iter().enumerate() {
        // Tokens at the start are slightly weightier — common preacher
        // pattern is to lead with the most distinctive word.
        let pos_weight = 1.0 + 0.4 * (1.0 - (i as f32) / n);
        weight_sum += pos_weight;
        if p_set.contains(tok) {
            hit += pos_weight;
        }
    }
    if weight_sum == 0.0 {
        0.0
    } else {
        hit / weight_sum
    }
}

fn length_ratio_penalty(query: &str, passage: &str) -> f32 {
    let q_len = query.split_whitespace().count() as f32;
    let p_len = passage.split_whitespace().count() as f32;
    if q_len == 0.0 || p_len == 0.0 {
        return 0.0;
    }
    let ratio = (q_len.min(p_len)) / (q_len.max(p_len));
    // ratio == 1.0 → penalty 1.0. ratio == 0.1 → penalty ~0.55.
    0.5 + 0.5 * ratio
}

/// A boxed cross-encoder. Held as `Arc<dyn>` so the concrete impl can be
/// swapped at runtime when a model file is dropped in.
pub type SharedCrossEncoder = Arc<dyn CrossEncoder>;

pub fn default_cross_encoder() -> SharedCrossEncoder {
    Arc::new(HeuristicCrossEncoder::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paraphrase_outranks_unrelated() {
        let ce = HeuristicCrossEncoder::new();
        let q = "God loved the world so much he gave his only son";
        let target = "For God so loved the world, that he gave his only begotten Son";
        let unrelated = "Moses received the law on Mount Sinai engraved on stone tablets";
        let s_target = ce.score(q, target);
        let s_unrelated = ce.score(q, unrelated);
        assert!(
            s_target > s_unrelated + 0.10,
            "target ({s_target}) should clearly beat unrelated ({s_unrelated})"
        );
    }

    #[test]
    fn empty_inputs_return_zero() {
        let ce = HeuristicCrossEncoder::new();
        assert_eq!(ce.score("", "anything"), 0.0);
        assert_eq!(ce.score("anything", ""), 0.0);
    }

    #[test]
    fn partial_quote_still_scores_above_unrelated() {
        let ce = HeuristicCrossEncoder::new();
        let q = "for God so loved";
        let target = "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.";
        let unrelated = "In the beginning God created the heaven and the earth.";
        assert!(ce.score(q, target) > ce.score(q, unrelated));
    }

    #[test]
    fn output_in_unit_range() {
        let ce = HeuristicCrossEncoder::new();
        let s = ce.score("a b c", "d e f");
        assert!((0.0..=1.0).contains(&s));
        let s2 = ce.score("a b c", "a b c");
        assert!((0.0..=1.0).contains(&s2));
    }

    #[test]
    fn deterministic() {
        let ce = HeuristicCrossEncoder::new();
        let q = "Be still and know that I am God";
        let p = "Be still, and know that I am God";
        assert_eq!(ce.score(q, p), ce.score(q, p));
    }
}
