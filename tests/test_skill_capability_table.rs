//! Anti-drift assertion for SKILL.md §0 Capability Truth Table. Extracts every
//! `src/...:NNN` citation, asserts each cited line is non-empty, and asserts
//! known load-bearing symbols appear within a ±5-line window of the cited line
//! (exact-line grep is brittle under benign refactors; ±5 still catches the
//! renames and deletions the test exists to prevent).

use std::fs;
use std::path::PathBuf;

use regex::Regex;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_skill_md() -> String {
    let path = manifest_dir().join("skills/git-commit-staging/SKILL.md");
    fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {e}. The skill file is required for the anti-drift test.",
            path.display()
        )
    })
}

fn read_source(rel: &str) -> String {
    let path = manifest_dir().join(rel);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read cited source file {}: {e}", path.display()))
}

/// (file, line, expected symbol) anchors kept in sync with SKILL.md §0.
const KNOWN_ANCHORS: &[(&str, u32, &str)] = &[
    ("src/selection/resolve.rs", 248, "validate_freshness"),
    ("src/mcp/contract.rs", 764, "structured_tool_result"),
    ("src/mcp/contract.rs", 281, "define_tool_output"),
    ("src/git/diff.rs", 270, "extract_hunks"),
];

#[test]
fn skill_capability_table_anchors_still_resolve() {
    let skill = read_skill_md();
    let citation_re = Regex::new(r"src/[a-z_/]+\.rs:(\d+)").expect("static regex is valid");

    let mut citations: Vec<(String, u32)> = Vec::new();
    for cap in citation_re.captures_iter(&skill) {
        let m = cap.get(0).expect("group 0 always present");
        let full = m.as_str();
        let after = &skill[m.end()..];
        if after.starts_with('-') || after.starts_with(',') {
            continue;
        }
        let (file, line_str) = full.rsplit_once(':').expect("regex guarantees ':'");
        let line: u32 = line_str.parse().expect("regex guarantees digits");
        citations.push((file.to_string(), line));
    }
    assert!(
        !citations.is_empty(),
        "SKILL.md contains no `src/...:NNN` citations — truth table missing or malformed"
    );
    for (file, line) in &citations {
        let source = read_source(file);
        let lines: Vec<&str> = source.lines().collect();
        let idx = (*line as usize).saturating_sub(1);
        assert!(
            idx < lines.len(),
            "citation {file}:{line} is out of range (file has {} lines) — stale anchor",
            lines.len()
        );
        assert!(
            !lines[idx].trim().is_empty(),
            "citation {file}:{line} points at an empty line — probable anchor rot"
        );
    }

    for (file, line, symbol) in KNOWN_ANCHORS {
        let source = read_source(file);
        let lines: Vec<&str> = source.lines().collect();
        let center = (*line as usize).saturating_sub(1);
        let start = center.saturating_sub(5);
        let end = (center + 5).min(lines.len().saturating_sub(1));
        let found = (start..=end).any(|i| lines[i].contains(symbol));
        assert!(
            found,
            "symbol `{symbol}` not found within ±5 lines of {file}:{line} — rename or deletion? \
             window contents:\n{}",
            (start..=end)
                .map(|i| format!("  {}: {}", i + 1, lines[i]))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
