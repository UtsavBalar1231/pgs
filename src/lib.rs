pub mod cmd;
pub mod error;
pub mod git;
pub mod mcp;
pub mod models;
mod output;
pub mod safety;
pub mod selection;

/// Convert `usize` to `u32`, saturating at `u32::MAX` on overflow.
///
/// Used where git2 or output contracts require `u32` but Rust collection
/// methods return `usize`. In practice no git file exceeds 4 billion lines.
pub(crate) fn saturating_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
