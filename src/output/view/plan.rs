use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::models::OriginMix;

use super::common::{OUTPUT_VERSION, OutputCommand};

/// Classification label for a split-hunk range, rendered as a lowercase string.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OriginMixView {
    Addition,
    Deletion,
    Mixed,
}

impl OriginMixView {
    /// Total mapping from the split-classification type to the rendered view.
    #[must_use]
    pub const fn from_origin_mix(origin: OriginMix) -> Self {
        match origin {
            OriginMix::Addition => Self::Addition,
            OriginMix::Deletion => Self::Deletion,
            OriginMix::Mixed => Self::Mixed,
        }
    }
}

/// A single classified range inside a split-hunk result.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
pub struct SplitRangeView {
    pub start: u32,
    pub end: u32,
    pub origin_mix: OriginMixView,
}

/// Output for `pgs split-hunk` — descriptive classification of runs inside a single hunk.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct SplitHunkOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub hunk_id: String,
    pub ranges: Vec<SplitRangeView>,
}

impl SplitHunkOutput {
    #[must_use]
    pub const fn new(hunk_id: String, ranges: Vec<SplitRangeView>) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::SplitHunk,
            hunk_id,
            ranges,
        }
    }
}

/// A hunk covered by two or more planned commits.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PlanOverlap {
    pub hunk_id: String,
    pub commits: Vec<String>,
}

/// A hunk's identity inside an uncovered record (`file_path` + `hunk_id`).
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct HunkRef {
    pub file_path: String,
    pub hunk_id: String,
}

/// A selector that plan-check rejects as unsafe (e.g. a line range crossing a hunk boundary).
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct UnsafeSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    pub selection: String,
    pub reason: String,
}

/// A `SelectionSpec::Hunk` id absent from the fresh scan — the agent should re-scan.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct UnknownHunkId {
    /// The planned commit that referenced this hunk id, if the commit had a label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    /// The 12-hex hunk id that could not be matched in the current scan.
    pub hunk_id: String,
}

/// Output for `pgs plan-check` — surfaces every issue the plan has against a fresh scan.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PlanCheckOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub overlaps: Vec<PlanOverlap>,
    pub uncovered: Vec<HunkRef>,
    pub unsafe_selectors: Vec<UnsafeSelector>,
    pub unknown_paths: Vec<String>,
    /// Stale 12-hex hunk ids absent from the fresh scan (not path misses — see `unknown_paths`).
    pub unknown_hunk_ids: Vec<UnknownHunkId>,
}

impl PlanCheckOutput {
    #[must_use]
    pub const fn new(
        overlaps: Vec<PlanOverlap>,
        uncovered: Vec<HunkRef>,
        unsafe_selectors: Vec<UnsafeSelector>,
        unknown_paths: Vec<String>,
        unknown_hunk_ids: Vec<UnknownHunkId>,
    ) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::PlanCheck,
            overlaps,
            uncovered,
            unsafe_selectors,
            unknown_paths,
            unknown_hunk_ids,
        }
    }

    /// `true` when plan-check found any issue and the CLI should exit 1.
    #[must_use]
    pub const fn has_issues(&self) -> bool {
        !self.overlaps.is_empty()
            || !self.uncovered.is_empty()
            || !self.unsafe_selectors.is_empty()
            || !self.unknown_paths.is_empty()
            || !self.unknown_hunk_ids.is_empty()
    }
}

/// Confidence classification for a `shifted` plan-diff match.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanDiffMatchConfidence {
    /// `expected_checksum` at plan time matches a current hunk's content checksum.
    High,
    /// Old planned range overlaps the current hunk's new range by ≥50%.
    Medium,
}

/// A plan entry classified as `still_valid` or `gone` by plan-diff.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PlanDiffEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    pub selection: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hunk_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A plan entry that plan-diff fuzzy-matched to a new hunk id (`shifted`).
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PlanDiffShift {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    pub selection: String,
    pub file_path: String,
    /// The stale hunk id from the plan, when one was available. `None` when
    /// only `expected_checksum` drove the match (no `captured_hunk_id` set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_hunk_id: Option<String>,
    pub new_hunk_id: String,
    pub match_confidence: PlanDiffMatchConfidence,
}

/// Output for `pgs plan-diff` — reconciles a saved `CommitPlan` against a fresh scan.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PlanDiffOutput {
    pub version: &'static str,
    pub command: OutputCommand,
    pub still_valid: Vec<PlanDiffEntry>,
    pub shifted: Vec<PlanDiffShift>,
    pub gone: Vec<PlanDiffEntry>,
}

impl PlanDiffOutput {
    #[must_use]
    pub const fn new(
        still_valid: Vec<PlanDiffEntry>,
        shifted: Vec<PlanDiffShift>,
        gone: Vec<PlanDiffEntry>,
    ) -> Self {
        Self {
            version: OUTPUT_VERSION,
            command: OutputCommand::PlanDiff,
            still_valid,
            shifted,
            gone,
        }
    }

    /// `true` when plan-diff found any shifted or gone entries (exit 1).
    #[must_use]
    pub const fn has_drift(&self) -> bool {
        !self.shifted.is_empty() || !self.gone.is_empty()
    }
}
