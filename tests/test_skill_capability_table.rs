//! Contract guards for the published pgs commit-staging skill. Asserts the skill
//! docs leak no internal source citations (`src/...:NNN`) or internal symbol
//! names, the packaged skill layout stays intact, `allowed-tools` matches the
//! MCP tool surface, the `structuredContent` naming is correct, and the
//! commit-message gate stays explicit.

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
            "failed to read {}: {e}. The skill file is required for the contract test.",
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
        read_skill_doc("references/tool-reference.md"),
        read_skill_doc("references/commit-message-guide.md"),
    ]
    .join("\n")
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
fn skill_docs_leak_no_internal_implementation_details() {
    let docs = read_all_skill_docs();

    // `src/...:NNN` source citations expose implementation internals and rot on
    // every refactor. Line-range selector examples like `src/main.rs:10-20` are
    // legitimate — a trailing `-` or `,` marks a range, not a citation.
    let citation_re = Regex::new(r"src/[a-z_/]+\.rs:(\d+)").expect("static regex is valid");
    let mut leaks: Vec<String> = Vec::new();
    for cap in citation_re.captures_iter(&docs) {
        let m = cap.get(0).expect("group 0 always present");
        let after = &docs[m.end()..];
        if after.starts_with('-') || after.starts_with(',') {
            continue;
        }
        leaks.push(m.as_str().to_owned());
    }
    assert!(
        leaks.is_empty(),
        "skill docs leak internal source citations (describe the MCP contract, not file:line): {leaks:?}"
    );

    // Internal symbol names must never appear in agent-facing skill docs.
    for symbol in [
        "compute_hunk_id",
        "extract_hunks",
        "suggest_splits",
        "validate_freshness",
        "preview_stage",
        "structured_tool_result",
        "define_tool_output",
        "build_index_entry",
    ] {
        assert!(
            !docs.contains(symbol),
            "skill docs leak internal symbol `{symbol}` — describe the MCP contract, not the code"
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
