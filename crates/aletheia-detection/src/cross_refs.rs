//! Cross-reference graph for biblical context priors (Stage C).
//!
//! Holds an adjacency map of canonical references → strongly related
//! references. Sourced from a public-domain dataset such as the Treasury
//! of Scripture Knowledge (~340k links). The graph is loaded once at
//! startup from `offline-assets/cross-refs.json` and queried in O(1)
//! during ranking.
//!
//! When the file is absent, the graph is empty and all multipliers are
//! 1.0 — the system degrades to the base ranker without code changes.
//!
//! ## File format
//!
//! ```json
//! {
//!   "John 3:16": [
//!     { "ref": "1 John 4:9", "weight": 0.85 },
//!     { "ref": "Romans 5:8",  "weight": 0.78 }
//!   ],
//!   ...
//! }
//! ```
//!
//! Weights are clamped to `[0.0, 1.0]`. Higher weight = stronger relation.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One outgoing edge in the cross-reference graph.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CrossRefEdge {
    /// Target reference in canonical form (e.g. `"1 John 4:9"`).
    #[serde(rename = "ref")]
    pub target: String,
    /// `[0.0, 1.0]` — strength of relation. Cosmic-load TSK numerics map
    /// to this range during import; defaults to 0.5 when unknown.
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 {
    0.5
}

/// Read-only cross-reference adjacency map. Construct via [`load_from_file`]
/// or [`from_map`]; query via [`neighbours`] and [`relation_weight`].
#[derive(Clone, Debug, Default)]
pub struct CrossRefGraph {
    edges: HashMap<String, Vec<CrossRefEdge>>,
}

impl CrossRefGraph {
    /// Empty graph (returns 1.0 multipliers for every query).
    pub fn empty() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Build from a pre-parsed map. Used by tests and import scripts.
    pub fn from_map(edges: HashMap<String, Vec<CrossRefEdge>>) -> Self {
        Self { edges }
    }

    /// Load from a JSON file. Returns `None` if the file is missing,
    /// unparseable, or empty — never panics. Caller is expected to fall
    /// back to [`Self::empty`].
    pub fn load_from_file(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        let map: HashMap<String, Vec<CrossRefEdge>> = serde_json::from_slice(&bytes).ok()?;
        if map.is_empty() {
            return None;
        }
        Some(Self::from_map(map))
    }

    /// Total number of source references in the graph.
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// True when no edges are loaded.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Returns every outgoing edge from `reference`. Empty when the
    /// reference is not in the graph.
    pub fn neighbours(&self, reference: &str) -> &[CrossRefEdge] {
        self.edges
            .get(reference)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Symmetric weight lookup: returns `max(weight(a→b), weight(b→a))`,
    /// clamped to `[0.0, 1.0]`. Returns 0.0 when `a` and `b` are unrelated.
    pub fn relation_weight(&self, a: &str, b: &str) -> f32 {
        let forward = self
            .neighbours(a)
            .iter()
            .find(|e| e.target == b)
            .map(|e| e.weight.clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let backward = self
            .neighbours(b)
            .iter()
            .find(|e| e.target == a)
            .map(|e| e.weight.clamp(0.0, 1.0))
            .unwrap_or(0.0);
        forward.max(backward)
    }
}

/// A small ring buffer of references that recently fired live. Used as
/// the "context" side of the cross-reference prior — when a candidate
/// ranks adjacent to one of these, its score is lifted by
/// [`apply_recent_prior`].
#[derive(Clone, Debug)]
pub struct RecentVerseHistory {
    capacity: usize,
    /// Newest first.
    items: Vec<String>,
}

impl RecentVerseHistory {
    /// New buffer with the given capacity (typical: 8).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            items: Vec::new(),
        }
    }

    /// Record that `reference` just fired live. Newest entries push older
    /// entries out the back.
    pub fn record(&mut self, reference: impl Into<String>) {
        let reference = reference.into();
        // Move to front if already present (LRU semantics).
        self.items.retain(|r| r != &reference);
        self.items.insert(0, reference);
        if self.items.len() > self.capacity {
            self.items.truncate(self.capacity);
        }
    }

    /// Newest first.
    pub fn entries(&self) -> &[String] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Default for RecentVerseHistory {
    fn default() -> Self {
        Self::new(8)
    }
}

/// Apply the cross-reference prior to a single candidate.
///
/// For each reference in `recent` (newest first), we look up the
/// strongest relation to `candidate` in the graph and combine them with
/// a recency decay. The resulting multiplier is in `[1.0, 1.0 + max_lift]`,
/// then applied to the score and clamped to `[0.0, 1.0]`.
///
/// `max_lift` of 0.10 is a sensible default — this prior is meant to
/// nudge expository sequences (`John 3:16` → `1 John 4:9`), not override
/// the base ranker.
pub fn apply_recent_prior(
    candidate_reference: &str,
    candidate_score: f32,
    recent: &RecentVerseHistory,
    graph: &CrossRefGraph,
    max_lift: f32,
) -> f32 {
    if graph.is_empty() || recent.is_empty() {
        return candidate_score;
    }
    let max_lift = max_lift.clamp(0.0, 0.5);
    let mut best: f32 = 0.0;
    // Recency decay: 1.0, 0.7, 0.49, ... (geometric with r=0.7).
    let mut decay: f32 = 1.0;
    for recent_ref in recent.entries() {
        if recent_ref == candidate_reference {
            decay *= 0.7;
            continue;
        }
        let weight = graph.relation_weight(recent_ref, candidate_reference);
        let lifted = weight * decay;
        if lifted > best {
            best = lifted;
        }
        decay *= 0.7;
    }
    let multiplier = 1.0 + max_lift * best;
    (candidate_score * multiplier).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn john_316_graph() -> CrossRefGraph {
        let mut m = HashMap::new();
        m.insert(
            "John 3:16".to_string(),
            vec![
                CrossRefEdge {
                    target: "1 John 4:9".to_string(),
                    weight: 0.85,
                },
                CrossRefEdge {
                    target: "Romans 5:8".to_string(),
                    weight: 0.78,
                },
            ],
        );
        CrossRefGraph::from_map(m)
    }

    #[test]
    fn empty_graph_is_no_op() {
        let g = CrossRefGraph::empty();
        let mut h = RecentVerseHistory::new(4);
        h.record("John 3:16");
        let lifted = apply_recent_prior("1 John 4:9", 0.7, &h, &g, 0.1);
        assert_eq!(lifted, 0.7);
    }

    #[test]
    fn empty_recent_is_no_op() {
        let g = john_316_graph();
        let h = RecentVerseHistory::new(4);
        let lifted = apply_recent_prior("1 John 4:9", 0.7, &h, &g, 0.1);
        assert_eq!(lifted, 0.7);
    }

    #[test]
    fn related_recent_lifts_score() {
        let g = john_316_graph();
        let mut h = RecentVerseHistory::new(4);
        h.record("John 3:16");
        let base = 0.70_f32;
        let lifted = apply_recent_prior("1 John 4:9", base, &h, &g, 0.10);
        // multiplier = 1 + 0.10 * 0.85 * 1.0 = 1.085
        // expected ≈ 0.7595
        assert!((lifted - 0.70 * 1.085).abs() < 1e-4);
    }

    #[test]
    fn unrelated_recent_does_not_lift() {
        let g = john_316_graph();
        let mut h = RecentVerseHistory::new(4);
        h.record("John 3:16");
        let base = 0.70_f32;
        let lifted = apply_recent_prior("Genesis 1:1", base, &h, &g, 0.10);
        assert_eq!(lifted, base);
    }

    #[test]
    fn recency_decays() {
        let g = john_316_graph();
        let mut h = RecentVerseHistory::new(4);
        h.record("Older verse 1:1");
        h.record("Older verse 2:2");
        h.record("Older verse 3:3");
        h.record("John 3:16"); // Newest after all the others; see record() semantics below.
        // Order in `entries` is newest first: ["John 3:16", "Older verse 3:3", ...]
        let lifted = apply_recent_prior("1 John 4:9", 0.70, &h, &g, 0.10);
        // John 3:16 is newest → decay 1.0 → multiplier 1.085
        assert!((lifted - 0.70 * 1.085).abs() < 1e-4);
    }

    #[test]
    fn lru_dedup_in_history() {
        let mut h = RecentVerseHistory::new(3);
        h.record("A");
        h.record("B");
        h.record("A");
        assert_eq!(h.entries(), &["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn multiplier_clamped() {
        let g = john_316_graph();
        let mut h = RecentVerseHistory::new(4);
        h.record("John 3:16");
        // Even with absurd score, output stays within [0, 1].
        let lifted = apply_recent_prior("1 John 4:9", 0.99, &h, &g, 0.5);
        assert!(lifted <= 1.0);
        assert!(lifted >= 0.99);
    }

    #[test]
    fn relation_weight_symmetric() {
        let g = john_316_graph();
        assert_eq!(g.relation_weight("John 3:16", "1 John 4:9"), 0.85);
        assert_eq!(g.relation_weight("1 John 4:9", "John 3:16"), 0.85);
        assert_eq!(g.relation_weight("Genesis 1:1", "John 3:16"), 0.0);
    }
}
