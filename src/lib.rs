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

/// Encode bytes as a lowercase hex string.
///
/// `sha2` 0.11 returns `hybrid_array::Array`, which does not implement
/// `LowerHex`, so digest bytes cannot be formatted with `{:x}` directly.
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
