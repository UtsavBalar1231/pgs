use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use super::scan::LineOrigin;
use super::selection::LineRange;

// ─── Preview (dry-run --explain) ──────────────────────────────────

/// Per-file exact-content preview produced by `pgs stage --dry-run --explain`.
///
/// Binary files produce `preview_lines: []` and `reason: Some("binary")`.
/// `truncated` fires per-file independently when `limit_applied` is exceeded.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OperationPreview {
    /// Original selection string as passed on the CLI (e.g. `src/main.rs:10-20`).
    pub selection: String,
    /// File path the preview applies to.
    pub file_path: String,
    /// Line ranges that were resolved for this file. Empty for whole-file / binary entries.
    pub resolved_ranges: Vec<LineRange>,
    /// Exact content lines that would land in the index (capped by `limit_applied`).
    pub preview_lines: Vec<PreviewLine>,
    /// `true` when this file's preview exceeded `limit_applied` and was truncated.
    pub truncated: bool,
    /// Per-file preview cap as passed on the CLI. `0` means unlimited.
    pub limit_applied: u32,
    /// Non-empty only for entries where the preview could not render concrete line
    /// content. Currently only `Some("binary")` for binary files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A single line inside an [`OperationPreview`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PreviewLine {
    /// 1-indexed line number in the new (workdir) file.
    pub line_number: u32,
    /// Classification of the line (addition / deletion / context).
    pub origin: LineOrigin,
    /// Text content of the line (no +/- prefix).
    pub content: String,
}
