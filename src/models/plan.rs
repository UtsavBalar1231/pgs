use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

// ─── Plan-check input (agent-supplied) ────────────────────────────

/// Agent-supplied commit plan consumed by `pgs plan-check` and `pgs plan-diff`.
///
/// Unknown input fields are silently ignored; additive fields use
/// `#[serde(default)]` so A6 plan-diff extends v1 without breaking A3 consumers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CommitPlan {
    /// Schema version. Only `"v1"` is currently recognized.
    pub version: String,
    /// Ordered planned commits. Empty is valid (every scan hunk surfaces as uncovered).
    pub commits: Vec<PlannedCommit>,
    /// A6 additive: optional ISO-8601 timestamp agents stamp at plan capture.
    #[serde(default)]
    pub captured_at: Option<String>,
}

/// A single planned commit inside a [`CommitPlan`]. A6 plan-diff additive
/// fields (`captured_hunk_id`, `expected_checksum`) let agents pin entries to
/// a scan moment for higher-confidence classification.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PlannedCommit {
    /// Optional agent-supplied label used to identify the commit in plan-check
    /// reports. Defaults to `None` when omitted.
    #[serde(default)]
    pub id: Option<String>,
    /// Positional selection strings (file paths, 12-hex hunk IDs, or `path:A-B` ranges).
    pub selections: Vec<String>,
    /// Optional selection strings to exclude from this commit's coverage.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Optional commit message preview. Not interpreted by plan-check.
    #[serde(default)]
    pub message: Option<String>,
    /// A6 additive: 12-hex hunk id captured at plan time (plan-diff equality).
    #[serde(default)]
    pub captured_hunk_id: Option<String>,
    /// A6 additive: hunk-content SHA-256 captured at plan time (plan-diff fuzzy match).
    #[serde(default)]
    pub expected_checksum: Option<String>,
}
