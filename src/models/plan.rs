use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

// ─── Plan-check input (agent-supplied) ────────────────────────────

fn default_plan_version() -> String {
    "v1".to_owned()
}

/// Agent-supplied commit plan consumed by `pgs plan-check` and `pgs plan-diff`.
///
/// Unknown input fields are silently ignored; additive fields use
/// `#[serde(default)]` so A6 plan-diff extends v1 without breaking A3 consumers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CommitPlan {
    /// Schema version. Only `"v1"` is currently recognized. Defaults to `"v1"` when
    /// omitted so agents that follow the MCP schema (which does not advertise this
    /// field) do not receive a parse error.
    #[serde(default = "default_plan_version")]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A [`CommitPlan`] JSON without a `version` field must deserialize successfully
    /// and default `version` to `"v1"`. Agents following the MCP schema (which omits
    /// `version`) must not receive a parse error.
    #[test]
    fn commit_plan_without_version_deserializes_as_v1() {
        let json = r#"{"commits":[{"id":"c1","selections":["src/main.rs"]}]}"#;
        let plan: CommitPlan =
            serde_json::from_str(json).expect("must deserialize without version");
        assert_eq!(plan.version, "v1", "version must default to \"v1\"");
        assert_eq!(plan.commits.len(), 1);
        assert_eq!(plan.commits[0].id.as_deref(), Some("c1"));
    }

    /// A [`CommitPlan`] with an explicit `"version":"v1"` must still deserialize correctly.
    #[test]
    fn commit_plan_with_explicit_version_v1_deserializes_correctly() {
        let json = r#"{"version":"v1","commits":[{"selections":["x.rs"]}]}"#;
        let plan: CommitPlan = serde_json::from_str(json).expect("must deserialize with version");
        assert_eq!(plan.version, "v1");
        assert_eq!(plan.commits.len(), 1);
    }
}
