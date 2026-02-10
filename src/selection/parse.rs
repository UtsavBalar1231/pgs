/// Auto-detection parser for v2 positional selection arguments.
///
/// Converts a bare string (file path, hunk ID, or `path:ranges`) into
/// a [`SelectionSpec`] without requiring explicit `--file`/`--hunk`/`--lines`
/// flags.
use crate::error::AgstageError;
use crate::models::{LineRange, SelectionSpec};

/// Auto-detect a positional argument into a [`SelectionSpec`].
///
/// Detection rules (applied in order):
/// 1. If the string contains `:` and the character immediately after the
///    **last** `:` is an ASCII digit → parse as `Lines { path, ranges }`.
/// 2. If the string is exactly 12 hexadecimal characters → `Hunk { hunk_id }`.
/// 3. Otherwise → `File { path }`.
///
/// The `--exclude` flag reuses this same function.
///
/// # Errors
///
/// Returns [`AgstageError::InvalidSelection`] for an empty argument, and
/// [`AgstageError::InvalidLineRange`] when a range segment is malformed or
/// has `start > end` / `start < 1`.
pub fn detect_selection(arg: &str) -> Result<SelectionSpec, AgstageError> {
    if arg.is_empty() {
        return Err(AgstageError::InvalidSelection {
            detail: "selection argument must not be empty".into(),
        });
    }

    // Rule 1: colon whose successor is a digit → Lines
    if let Some(colon_pos) = arg.rfind(':') {
        let after = &arg[colon_pos + 1..];
        if after.starts_with(|c: char| c.is_ascii_digit()) {
            let path = arg[..colon_pos].to_owned();
            let ranges = parse_ranges(&path, after)?;
            return Ok(SelectionSpec::Lines { path, ranges });
        }
    }

    // Rule 2: exactly 12 hex characters → Hunk
    if arg.len() == 12 && arg.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(SelectionSpec::Hunk {
            hunk_id: arg.to_owned(),
        });
    }

    // Rule 3: everything else → File
    Ok(SelectionSpec::File {
        path: arg.to_owned(),
    })
}

/// Parse line ranges from the portion after the last `:`.
///
/// Format: `START-END[,START-END,...]` (1-indexed, inclusive, `START <= END`).
///
/// # Errors
///
/// Returns [`AgstageError::InvalidSelection`] when a segment is not in
/// `START-END` form, and [`AgstageError::InvalidLineRange`] when the
/// numbers are out of range or `start > end`.
fn parse_ranges(path: &str, ranges_str: &str) -> Result<Vec<LineRange>, AgstageError> {
    let mut ranges = Vec::new();

    for segment in ranges_str.split(',') {
        let segment = segment.trim();
        let Some(dash_pos) = segment.find('-') else {
            return Err(AgstageError::InvalidSelection {
                detail: format!("expected START-END range, got {segment:?} in {path}"),
            });
        };

        let start_str = &segment[..dash_pos];
        let end_str = &segment[dash_pos + 1..];

        let start: u32 = start_str
            .parse()
            .map_err(|_| AgstageError::InvalidSelection {
                detail: format!("invalid line number {start_str:?} in {path}"),
            })?;
        let end: u32 = end_str
            .parse()
            .map_err(|_| AgstageError::InvalidSelection {
                detail: format!("invalid line number {end_str:?} in {path}"),
            })?;

        if start < 1 {
            return Err(AgstageError::InvalidLineRange {
                path: path.to_owned(),
                start,
                end,
            });
        }
        if start > end {
            return Err(AgstageError::InvalidLineRange {
                path: path.to_owned(),
                start,
                end,
            });
        }

        ranges.push(LineRange { start, end });
    }

    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SelectionSpec;

    // ── File detection ────────────────────────────────────────────

    #[test]
    fn detect_file_path_returns_file() {
        let spec = detect_selection("src/main.rs").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::File {
                path: "src/main.rs".into()
            }
        );
    }

    #[test]
    fn detect_bare_filename_returns_file() {
        let spec = detect_selection("main.rs").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::File {
                path: "main.rs".into()
            }
        );
    }

    // ── Hunk detection ────────────────────────────────────────────

    #[test]
    fn detect_12_hex_returns_hunk() {
        let spec = detect_selection("abc123def456").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::Hunk {
                hunk_id: "abc123def456".into()
            }
        );
    }

    #[test]
    fn detect_uppercase_hex_returns_hunk() {
        let spec = detect_selection("ABC123DEF456").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::Hunk {
                hunk_id: "ABC123DEF456".into()
            }
        );
    }

    #[test]
    fn detect_11_char_hex_returns_file() {
        // Not exactly 12 chars → treated as file
        let spec = detect_selection("abc123def45").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::File {
                path: "abc123def45".into()
            }
        );
    }

    #[test]
    fn detect_13_char_hex_returns_file() {
        let spec = detect_selection("abc123def4567").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::File {
                path: "abc123def4567".into()
            }
        );
    }

    // ── Lines detection ───────────────────────────────────────────

    #[test]
    fn detect_single_range_returns_lines() {
        let spec = detect_selection("src/main.rs:10-20").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::Lines {
                path: "src/main.rs".into(),
                ranges: vec![LineRange { start: 10, end: 20 }],
            }
        );
    }

    #[test]
    fn detect_multi_range_returns_lines() {
        let spec = detect_selection("src/lib.rs:1-5,10-15").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::Lines {
                path: "src/lib.rs".into(),
                ranges: vec![
                    LineRange { start: 1, end: 5 },
                    LineRange { start: 10, end: 15 },
                ],
            }
        );
    }

    #[test]
    fn detect_single_line_range_start_equals_end() {
        let spec = detect_selection("file.rs:42-42").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::Lines {
                path: "file.rs".into(),
                ranges: vec![LineRange { start: 42, end: 42 }],
            }
        );
    }

    // ── Windows path edge cases ───────────────────────────────────

    #[test]
    fn detect_windows_path_no_range_returns_file() {
        // Last ':' is after 'C' — next char is '\', not a digit → File
        let spec = detect_selection(r"C:\Users\test\file.rs").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::File {
                path: r"C:\Users\test\file.rs".into()
            }
        );
    }

    #[test]
    fn detect_windows_path_with_range_returns_lines() {
        let spec = detect_selection(r"C:\Users\test\file.rs:5-10").unwrap();
        assert_eq!(
            spec,
            SelectionSpec::Lines {
                path: r"C:\Users\test\file.rs".into(),
                ranges: vec![LineRange { start: 5, end: 10 }],
            }
        );
    }

    // ── Error cases ───────────────────────────────────────────────

    #[test]
    fn detect_start_greater_than_end_returns_invalid_line_range() {
        let err = detect_selection("file.rs:20-10").unwrap_err();
        assert!(
            matches!(
                err,
                AgstageError::InvalidLineRange {
                    start: 20,
                    end: 10,
                    ..
                }
            ),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn detect_start_zero_returns_invalid_line_range() {
        let err = detect_selection("file.rs:0-5").unwrap_err();
        assert!(
            matches!(err, AgstageError::InvalidLineRange { start: 0, .. }),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn detect_empty_arg_returns_invalid_selection() {
        let err = detect_selection("").unwrap_err();
        assert!(
            matches!(err, AgstageError::InvalidSelection { .. }),
            "unexpected error: {err}"
        );
    }
}
