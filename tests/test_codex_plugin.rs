use serde_json::Value;

fn read_json(path: &str) -> Value {
    let contents = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&contents).unwrap()
}

fn assert_regular_file(path: &str) {
    let metadata = std::fs::symlink_metadata(path).unwrap();
    assert!(
        metadata.file_type().is_file(),
        "{path} must be a regular file"
    );
}

#[test]
fn codex_plugin_manifest_wires_shared_mcp_server_and_skill() {
    let manifest = read_json(".codex-plugin/plugin.json");

    assert_eq!(manifest["name"], "pgs");
    assert_eq!(manifest["skills"], "./skills/");
    assert_eq!(manifest["mcpServers"], "./.mcp.json");
    assert_eq!(manifest["interface"]["category"], "Coding");
    assert!(
        manifest["interface"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "Write")
    );
}

#[test]
fn shared_mcp_manifest_uses_launcher_with_install_fallback_env() {
    let mcp_manifest = read_json(".mcp.json");
    let server = &mcp_manifest["mcpServers"]["pgs"];

    assert_eq!(
        server["command"],
        "${CLAUDE_PLUGIN_ROOT}/scripts/run-pgs-mcp.sh"
    );
    assert_eq!(server["env"]["PGS_PLUGIN_ROOT"], "${CLAUDE_PLUGIN_ROOT}");
    assert_eq!(server["env"]["PGS_PLUGIN_DATA"], "${CLAUDE_PLUGIN_DATA}");
}

#[test]
fn marketplace_entry_points_to_codex_compatible_plugin_path() {
    let marketplace = read_json(".claude-plugin/marketplace.json");
    let plugins = marketplace["plugins"].as_array().unwrap();
    let pgs = plugins
        .iter()
        .find(|plugin| plugin["name"] == "pgs")
        .expect("pgs marketplace entry must exist");

    assert_eq!(marketplace["name"], "pgs-marketplace");
    assert_eq!(pgs["source"], "./plugins/pgs");
    assert_regular_file("plugins/pgs/.codex-plugin/plugin.json");
    assert_regular_file("plugins/pgs/.claude-plugin/plugin.json");
    assert_regular_file("plugins/pgs/.mcp.json");
    assert_regular_file("plugins/pgs/hooks/hooks.json");
    assert_regular_file("plugins/pgs/skills/git-commit-staging/SKILL.md");
    assert_regular_file("plugins/pgs/scripts/run-pgs-mcp.sh");
    assert_regular_file("plugins/pgs/scripts/install-binary.sh");
    assert_regular_file("plugins/pgs/VERSION");
}

#[test]
fn packaged_codex_plugin_matches_repo_sources() {
    for (source, packaged) in [
        (
            ".codex-plugin/plugin.json",
            "plugins/pgs/.codex-plugin/plugin.json",
        ),
        (
            ".claude-plugin/plugin.json",
            "plugins/pgs/.claude-plugin/plugin.json",
        ),
        (".mcp.json", "plugins/pgs/.mcp.json"),
        ("hooks/hooks.json", "plugins/pgs/hooks/hooks.json"),
        (
            "skills/git-commit-staging/SKILL.md",
            "plugins/pgs/skills/git-commit-staging/SKILL.md",
        ),
        (
            "scripts/run-pgs-mcp.sh",
            "plugins/pgs/scripts/run-pgs-mcp.sh",
        ),
        (
            "scripts/install-binary.sh",
            "plugins/pgs/scripts/install-binary.sh",
        ),
        ("VERSION", "plugins/pgs/VERSION"),
    ] {
        assert_eq!(
            std::fs::read_to_string(source).unwrap(),
            std::fs::read_to_string(packaged).unwrap(),
            "{packaged} must match {source}"
        );
    }
}
