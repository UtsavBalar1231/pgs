//! Anti-drift assertion for SKILL.md §0 Capability Truth Table. Extracts every
//! `src/...:NNN` citation, asserts each cited line is non-empty, and asserts
//! known load-bearing symbols appear within a ±5-line window of the cited line
//! (exact-line grep is brittle under benign refactors; ±5 still catches the
//! renames and deletions the test exists to prevent).

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use regex::Regex;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn skill_root() -> PathBuf {
    manifest_dir().join("plugins/pgs/skills/git-commit-staging")
}

fn read_skill_md() -> String {
    let path = skill_root().join("SKILL.md");
    fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {e}. The skill file is required for the anti-drift test.",
            path.display()
        )
    })
}

fn read_skill_doc(rel: &str) -> String {
    let path = skill_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {e}. The skill reference is required.",
            path.display()
        )
    })
}

fn read_all_skill_docs() -> String {
    [
        read_skill_md(),
        read_skill_doc("references/capability-table.md"),
        read_skill_doc("references/tool-reference.md"),
        read_skill_doc("references/commit-message-guide.md"),
    ]
    .join("\n")
}

fn read_source(rel: &str) -> String {
    let path = manifest_dir().join(rel);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read cited source file {}: {e}", path.display()))
}

fn parse_allowed_tools(skill: &str) -> BTreeSet<String> {
    let mut tools = BTreeSet::new();
    let mut in_allowed_tools = false;
    for line in skill.lines() {
        if line == "allowed-tools:" {
            in_allowed_tools = true;
            continue;
        }
        if in_allowed_tools {
            if let Some(tool) = line.trim().strip_prefix("- ") {
                tools.insert(tool.to_owned());
            } else if line == "---" || !line.starts_with(' ') {
                break;
            }
        }
    }
    tools
}

/// (file, line, expected symbol) anchors kept in sync with SKILL.md §0.
const KNOWN_ANCHORS: &[(&str, u32, &str)] = &[
    ("src/selection/resolve.rs", 247, "validate_freshness"),
    ("src/mcp/contract.rs", 712, "structured_tool_result"),
    ("src/mcp/contract.rs", 320, "define_tool_output"),
    ("src/git/diff.rs", 270, "extract_hunks"),
    ("src/git/diff.rs", 211, "suggest_splits"),
    ("src/git/diff.rs", 377, "compute_hunk_id"),
    ("src/git/staging.rs", 250, "preview_stage"),
    ("src/models/preview.rs", 14, "OperationPreview"),
    ("src/models/plan.rs", 11, "CommitPlan"),
    ("src/models/scan.rs", 101, "whitespace_only"),
    ("src/cmd/overview.rs", 9, "execute"),
    ("src/cmd/plan_check.rs", 35, "execute"),
    ("src/cmd/plan_diff.rs", 36, "execute"),
    ("src/cmd/commit.rs", 15, "amend"),
    ("src/mcp/contract.rs", 162, "amend"),
    ("src/cmd/commit.rs", 62, "args.message"),
    ("src/cmd/commit.rs", 70, "args.message"),
];

#[test]
fn top_level_skill_path_is_a_symlink_to_the_packaged_skill_tree() {
    let root_skills = manifest_dir().join("skills");
    let metadata = fs::symlink_metadata(&root_skills)
        .unwrap_or_else(|e| panic!("failed to stat {}: {e}", root_skills.display()));
    let target = fs::read_link(&root_skills)
        .unwrap_or_else(|e| panic!("failed to read {} symlink: {e}", root_skills.display()));

    assert!(
        metadata.file_type().is_symlink(),
        "{} must be a symlink, not a second maintained skill directory",
        root_skills.display()
    );
    assert_eq!(target, PathBuf::from("plugins/pgs/skills"));
}

#[test]
fn root_claude_directory_does_not_define_a_second_plugin_root() {
    assert!(
        !manifest_dir().join(".claude-plugin/plugin.json").exists(),
        "root .claude-plugin is the marketplace; the Claude plugin root is plugins/pgs"
    );
}

#[test]
fn skill_capability_table_anchors_still_resolve() {
    let skill = read_all_skill_docs();
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

#[test]
fn skill_allowed_tools_cover_every_mcp_tool() {
    let actual = parse_allowed_tools(&read_skill_md());
    let expected = [
        pgs::mcp::contract::PGS_SCAN_TOOL,
        pgs::mcp::contract::PGS_STATUS_TOOL,
        pgs::mcp::contract::PGS_STAGE_TOOL,
        pgs::mcp::contract::PGS_UNSTAGE_TOOL,
        pgs::mcp::contract::PGS_COMMIT_TOOL,
        pgs::mcp::contract::PGS_LOG_TOOL,
        pgs::mcp::contract::PGS_OVERVIEW_TOOL,
        pgs::mcp::contract::PGS_SPLIT_HUNK_TOOL,
        pgs::mcp::contract::PGS_PLAN_CHECK_TOOL,
        pgs::mcp::contract::PGS_PLAN_DIFF_TOOL,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();

    assert_eq!(
        actual, expected,
        "skill allowed-tools must stay in sync with the MCP tool surface"
    );
}

#[test]
fn skill_guidance_has_no_cli_only_or_stale_structured_content_paths() {
    let docs = read_all_skill_docs();

    assert!(
        !docs.contains("pgs stage --dry-run --explain"),
        "agents must use MCP pgs_stage explain/limit, not CLI-only preview"
    );
    assert!(
        !docs.contains("structured_content"),
        "agent-facing skill docs should use JSON-RPC structuredContent naming"
    );
    assert!(
        !docs.contains("src/mcp/contract.rs:835") && !docs.contains("src/mcp/contract.rs:304"),
        "skill docs should not contain stale structured result anchors"
    );
    assert!(
        docs.contains("structuredContent"),
        "skill docs should name the JSON-RPC structured content field"
    );
}

#[test]
fn skill_commit_message_gate_is_explicit() {
    let skill = read_skill_md();

    for required in [
        "Message quality gate",
        "pgs_log",
        "pgs_status",
        "repo style first",
        "Conventional Commits fallback",
        "Body is required",
    ] {
        assert!(
            skill.contains(required),
            "SKILL.md should explicitly require `{required}` before pgs_commit"
        );
    }
}
