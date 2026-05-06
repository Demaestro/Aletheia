//! Embedding contracts for Stage A (broad recall).
//!
//! The retrieval pipeline calls into a [`SentenceEmbedder`] that turns a
//! transcript window into a unit-length vector. Verse vectors are
//! pre-computed once at warm time and stored in SQLite.
//!
//! The default implementation, [`HashingEmbedder`], is a feature-hashed
//! bag-of-character-n-grams projector. It is deterministic, has no model
//! file, and is meaningfully better than word-overlap for paraphrase
//! recall because it captures sub-word morphology ("loved", "loveth",
//! "loving" all share trigrams). When a real embedding model file is
//! shipped (e.g. BGE-small ONNX), the desktop crate replaces this with
//! an ONNX-backed embedder behind the same trait.

use std::sync::Arc;

/// Produces unit-length embeddings of arbitrary text.
pub trait SentenceEmbedder: Send + Sync {
    /// Output dimensionality. Must be constant for the lifetime of the
    /// embedder (the verse index is rebuilt if this changes).
    fn dim(&self) -> usize;

    /// Stable identifier including dimension + model version. Used to
    /// invalidate the verse-vector cache when the embedder changes.
    fn version_tag(&self) -> &str;

    /// Embeds `text` to a unit-length vector of `dim()` floats.
    fn embed(&self, text: &str) -> Vec<f32>;

    /// Optional batch path. Default: serial calls. ONNX backends override
    /// this for throughput.
    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// Cosine similarity of two unit vectors. Falls back to dot product —
/// callers are expected to pass already-normalised vectors.
pub fn cosine_unit(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// L2-normalises a vector in place. Becomes a unit vector unless input
/// is all zeros, in which case it stays all zeros.
pub fn l2_normalise(vec: &mut [f32]) {
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vec.iter_mut() {
            *v /= norm;
        }
    }
}

/// Feature-hashed character-n-gram embedder. No model file, pure Rust,
/// deterministic. Default: 256-dim, 3..=5 character n-grams over
/// lower-cased input. Punctuation is dropped; whitespace becomes a
/// single space.
///
/// This is the fallback when no neural embedder is installed. It
/// dramatically out-performs token-overlap on paraphrase ("loved the
/// world" ↔ "lovingly cared for the world" share many trigrams).
pub struct HashingEmbedder {
    dim: usize,
    n_min: usize,
    n_max: usize,
    tag: String,
}

impl HashingEmbedder {
    pub fn new(dim: usize) -> Self {
        let n_min = 3;
        let n_max = 5;
        let tag = format!("hashed-charngram-{n_min}-{n_max}-{dim}-v1");
        Self {
            dim: dim.max(64),
            n_min,
            n_max,
            tag,
        }
    }
}

impl Default for HashingEmbedder {
    fn default() -> Self {
        Self::new(256)
    }
}

impl SentenceEmbedder for HashingEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn version_tag(&self) -> &str {
        &self.tag
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let normalised = normalise_for_ngrams(text);
        let mut vec = vec![0.0f32; self.dim];
        if normalised.is_empty() {
            return vec;
        }
        let chars: Vec<char> = normalised.chars().collect();
        for n in self.n_min..=self.n_max {
            if chars.len() < n {
                continue;
            }
            for window in chars.windows(n) {
                let h = fnv1a_chars(window);
                let bucket = (h as usize) % self.dim;
                let sign = if (h & 1) == 0 { 1.0_f32 } else { -1.0_f32 };
                vec[bucket] += sign;
            }
        }
        l2_normalise(&mut vec);
        vec
    }
}

fn normalise_for_ngrams(text: &str) -> String {
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

fn fnv1a_chars(window: &[char]) -> u64 {
    // FNV-1a 64-bit on the UTF-8 encoding of the window.
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = FNV_OFFSET;
    for &c in window {
        let mut buf = [0u8; 4];
        let bytes = c.encode_utf8(&mut buf).as_bytes();
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

/// A boxed embedder. Most callers should hold one of these so the
/// concrete impl can be swapped without ripple changes.
pub type SharedEmbedder = Arc<dyn SentenceEmbedder>;

/// Factory: returns the heuristic fallback. The desktop crate may
/// override this with an ONNX-backed model when a model file is
/// installed.
pub fn default_embedder() -> SharedEmbedder {
    Arc::new(HashingEmbedder::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_is_unit_length() {
        let emb = HashingEmbedder::new(128);
        let v = emb.embed("for God so loved the world");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3 || norm == 0.0);
        assert_eq!(v.len(), 128);
    }

    #[test]
    fn paraphrase_has_higher_similarity_than_unrelated() {
        let emb = HashingEmbedder::new(512);
        let target = emb.embed("for God so loved the world that he gave his only son");
        let para = emb.embed("God loved the world so much he gave his only son");
        let unrelated = emb.embed("the law was given through Moses on stone tablets");

        let s_para = cosine_unit(&target, &para);
        let s_unrelated = cosine_unit(&target, &unrelated);
        assert!(
            s_para > s_unrelated,
            "paraphrase ({s_para}) should score above unrelated ({s_unrelated})"
        );
    }

    #[test]
    fn empty_input_is_zero_vector() {
        let emb = HashingEmbedder::new(64);
        let v = emb.embed("");
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn version_tag_changes_with_dim() {
        let a = HashingEmbedder::new(128);
        let b = HashingEmbedder::new(256);
        assert_ne!(a.version_tag(), b.version_tag());
    }

    #[test]
    fn deterministic() {
        let emb = HashingEmbedder::new(128);
        let v1 = emb.embed("Be still and know that I am God");
        let v2 = emb.embed("Be still and know that I am God");
        assert_eq!(v1, v2);
    }
}
