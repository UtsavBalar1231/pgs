use serde_json::Value;

fn read_json(path: &str) -> Value {
    let contents = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&contents).unwrap()
}

fn read_trimmed(path: &str) -> String {
    std::fs::read_to_string(path).unwrap().trim().to_owned()
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
fn shared_mcp_manifest_uses_codex_native_cached_binary_launcher() {
    let mcp_manifest = read_json(".mcp.json");
    let server = &mcp_manifest["mcpServers"]["pgs"];
    let args = server["args"].as_array().unwrap();
    let launcher = args[1].as_str().unwrap();

    assert_eq!(server["command"], "sh");
    assert_eq!(args[0], "-c");
    assert_eq!(args[2], "pgs-mcp");
    assert!(server.get("env").is_none());
    assert!(!launcher.contains("CLAUDE_PLUGIN_ROOT"));
    assert!(launcher.contains("CLAUDE_PLUGIN_DATA"));
    assert!(launcher.contains("${XDG_DATA_HOME:-$HOME/.local/share}/pgs-plugin"));
    assert!(launcher.contains("releases/download/v$VERSION/$BINARY_NAME"));
    assert!(!launcher.contains("releases/download/v${VERSION}/${BINARY_NAME}"));
    assert!(launcher.contains("exec \"$BINARY\" \"$@\""));
    assert!(launcher.contains(&format!("VERSION=\"{}\"", read_trimmed("VERSION"))));
}

#[test]
fn launcher_scripts_refresh_stale_cached_plugin_binary() {
    let runner = std::fs::read_to_string("scripts/run-pgs-mcp.sh").unwrap();
    let installer = std::fs::read_to_string("scripts/install-binary.sh").unwrap();

    assert!(runner.contains("CLAUDE_PLUGIN_ROOT"));
    assert!(runner.contains("CLAUDE_PLUGIN_DATA"));
    assert!(runner.contains("DATA_VERSION_FILE="));
    assert!(runner.contains("INSTALLED_VERSION="));
    assert!(runner.contains("[ \"$INSTALLED_VERSION\" != \"$VERSION\" ]"));

    assert!(installer.contains("CLAUDE_PLUGIN_ROOT"));
    assert!(installer.contains("CLAUDE_PLUGIN_DATA"));
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
    assert!(marketplace["description"].is_string());
    assert!(marketplace.get("metadata").is_none());
    assert_eq!(pgs["source"], "./plugins/pgs");
    assert_eq!(
        pgs["version"],
        read_json("plugins/pgs/.claude-plugin/plugin.json")["version"]
    );
    assert_eq!(pgs["category"], "Coding");
    assert_eq!(pgs["strict"], true);
    assert!(
        pgs["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "mcp")
    );
    assert_regular_file("plugins/pgs/.codex-plugin/plugin.json");
    assert_regular_file("plugins/pgs/.claude-plugin/plugin.json");
    assert_regular_file("plugins/pgs/.mcp.json");
    assert_regular_file("plugins/pgs/skills/git-commit-staging/SKILL.md");
    assert_regular_file("plugins/pgs/skills/git-commit-staging/references/capability-table.md");
    assert_regular_file("plugins/pgs/skills/git-commit-staging/references/tool-reference.md");
    assert_regular_file("plugins/pgs/skills/git-commit-staging/references/commit-message-guide.md");
    assert_regular_file("plugins/pgs/scripts/run-pgs-mcp.sh");
    assert_regular_file("plugins/pgs/scripts/install-binary.sh");
    assert_regular_file("plugins/pgs/VERSION");
    assert!(!std::path::Path::new("hooks/hooks.json").exists());
    assert!(!std::path::Path::new("plugins/pgs/hooks/hooks.json").exists());
}

#[test]
fn packaged_codex_plugin_matches_repo_sources() {
    for (source, packaged) in [
        (
            ".codex-plugin/plugin.json",
            "plugins/pgs/.codex-plugin/plugin.json",
        ),
        (".mcp.json", "plugins/pgs/.mcp.json"),
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

#[test]
fn root_claude_directory_is_marketplace_only() {
    assert!(
        !std::path::Path::new(".claude-plugin/plugin.json").exists(),
        "the repository root must not also be a Claude plugin; use plugins/pgs as the plugin root"
    );
}

#[test]
fn packaged_claude_plugin_manifest_uses_latest_metadata_fields() {
    let manifest = read_json("plugins/pgs/.claude-plugin/plugin.json");

    assert_eq!(manifest["name"], "pgs");
    assert_eq!(manifest["displayName"], "pgs");
    assert_eq!(manifest["keywords"][0], "git");
    assert!(manifest.get("skills").is_none());
    assert!(manifest.get("hooks").is_none());
    assert!(manifest.get("mcpServers").is_none());
}
