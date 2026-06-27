use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

// ─── Selection (internal, not serialized to JSON output) ──────────

/// Parsed selection from CLI positional args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionSpec {
    /// Select an entire file.
    File { path: String },
    /// Select a specific hunk by its content-based ID.
    Hunk { hunk_id: String },
    /// Select specific line ranges within a file.
    Lines {
        path: String,
        ranges: Vec<LineRange>,
    },
    /// Select all files under a directory prefix.
    Directory { prefix: String },
}

/// An inclusive line range [start, end] (1-indexed).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LineRange {
    /// Starting line number (inclusive).
    pub start: u32,
    /// Ending line number (inclusive).
    pub end: u32,
}

/// A resolved selection ready for staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSelection {
    /// Path to the file.
    pub file_path: String,
    /// Indices into the file's hunks vec.
    pub hunk_indices: Vec<usize>,
    /// Optional line ranges (for line-level staging).
    pub line_ranges: Option<Vec<LineRange>>,
}

/// Format a `SelectionSpec` as a display string.
pub fn format_selection(spec: &SelectionSpec) -> String {
    match spec {
        SelectionSpec::File { path } => path.clone(),
        SelectionSpec::Hunk { hunk_id } => hunk_id.clone(),
        SelectionSpec::Lines { path, ranges } => {
            let ranges_str: Vec<String> = ranges
                .iter()
                .map(|r| format!("{}-{}", r.start, r.end))
                .collect();
            format!("{path}:{}", ranges_str.join(","))
        }
        SelectionSpec::Directory { prefix } => format!("{prefix}/"),
    }
}
