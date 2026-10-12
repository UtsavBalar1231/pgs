use serde_json::Value;

fn read_json(path: &str) -> Value {
    let contents = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&contents).unwrap()
}

fn read_trimmed(path: &str) -> String {
    std::fs::read_to_string(path).unwrap().trim().to_owned()
}

fn relative_file_paths(root: &std::path::Path) -> std::collections::BTreeSet<std::path::PathBuf> {
    let mut found = std::collections::BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.insert(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    found
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
    assert!(launcher.contains("DATA=\"$PGS_PLUGIN_DATA\""));
    assert!(launcher.contains("DATA=\"$PLUGIN_DATA\""));
    assert!(launcher.contains("DATA=\"$CLAUDE_PLUGIN_DATA\""));
    let data_chain = [
        "\"$PGS_PLUGIN_DATA\"",
        "DATA=\"$PLUGIN_DATA\"",
        "DATA=\"$CLAUDE_PLUGIN_DATA\"",
        "BASE=\"$XDG_DATA_HOME\"",
    ]
    .map(|needle| launcher.find(needle).unwrap());
    assert!(
        data_chain.is_sorted(),
        "data-dir fallbacks are out of order"
    );
    assert!(launcher.contains("MINGW*|MSYS*|CYGWIN*) BINARY=\"$DATA/bin/pgs-mcp.exe\""));
    assert!(launcher.contains("*) BINARY=\"$DATA/bin/pgs-mcp\""));
    assert!(
        launcher.contains(
            "Linux:aarch64|Linux:arm64) BINARY_NAME=\"pgs-mcp-aarch64-unknown-linux-musl\""
        )
    );
    assert!(launcher.contains(
        "MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64) BINARY_NAME=\"pgs-mcp-x86_64-pc-windows-msvc.exe\""
    ));
    assert!(launcher.contains("MINGW*:arm64|MSYS*:arm64|CYGWIN*:arm64) BINARY_NAME=\"pgs-mcp-aarch64-pc-windows-msvc.exe\""));
    assert!(launcher.contains("MINGW*:aarch64|MSYS*:aarch64|CYGWIN*:aarch64|"));
    assert!(launcher.contains("BASE=\"$XDG_DATA_HOME\""));
    assert!(launcher.contains("BASE=\"$HOME/.local/share\""));
    assert!(launcher.contains("DATA=\"$BASE/pgs-plugin\""));
    assert!(!launcher.contains(":-${"));
    assert!(launcher.contains("releases/download/v$VERSION/$BINARY_NAME"));
    assert!(!launcher.contains("releases/download/v${VERSION}/${BINARY_NAME}"));
    assert!(launcher.contains("needs_install()"));
    assert!(launcher.contains("[ -r \"$VERSION_FILE\" ]"));
    assert!(launcher.contains("LOCK_DIR=\"$DATA/.install.lock\""));
    assert!(launcher.contains("mkdir \"$LOCK_DIR\""));
    assert!(launcher.contains("TMP_BINARY=\"$DATA/bin/.pgs-mcp.$$.tmp\""));
    assert!(launcher.contains("mv -f \"$TMP_BINARY\" \"$BINARY\""));
    assert!(launcher.contains("TMP_VERSION=\"$DATA/.VERSION.$$.tmp\""));
    assert!(launcher.contains("mv -f \"$TMP_VERSION\" \"$VERSION_FILE\""));
    assert!(launcher.contains("exec \"$BINARY\" \"$@\""));
    assert!(launcher.contains(&format!("VERSION=\"{}\"", read_trimmed("VERSION"))));
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
    assert_regular_file("plugins/pgs/skills/git-commit-staging/references/tool-reference.md");
    assert_regular_file("plugins/pgs/skills/git-commit-staging/references/commit-message-guide.md");
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
        ("VERSION", "plugins/pgs/VERSION"),
    ] {
        assert_eq!(
            std::fs::read_to_string(source).unwrap(),
            std::fs::read_to_string(packaged).unwrap(),
            "{packaged} must match {source}"
        );
    }

    let source = std::path::Path::new("skills");
    let packaged = std::path::Path::new("plugins/pgs/skills");

    assert!(
        !std::fs::symlink_metadata(source)
            .unwrap()
            .file_type()
            .is_symlink(),
        "skills must be a real directory: Gemini CLI copies extensions with fs.cp and \
         Windows checkouts without developer mode turn symlinks into text files"
    );
    let source_files = relative_file_paths(source);
    assert_eq!(
        source_files,
        relative_file_paths(packaged),
        "skills and plugins/pgs/skills must hold the same set of files"
    );
    assert!(!source_files.is_empty());
    for relative in &source_files {
        assert_eq!(
            std::fs::read(source.join(relative)).unwrap(),
            std::fs::read(packaged.join(relative)).unwrap(),
            "plugins/pgs/skills/{} must match skills/{}",
            relative.display(),
            relative.display()
        );
    }
}

// A release bump touches every file below. A stale manifest version strands
// plugin users on the previous binary, so CI checks all of them on every push
// rather than leaving it to the tag-time gate in release.yml.
#[test]
fn every_version_anchor_matches_the_crate_version() {
    let expected = env!("CARGO_PKG_VERSION");

    for path in ["VERSION", "plugins/pgs/VERSION"] {
        assert_eq!(read_trimmed(path), expected, "{path}");
    }

    let marketplace = read_json(".claude-plugin/marketplace.json");
    assert_eq!(
        marketplace["plugins"][0]["version"], expected,
        "marketplace.json"
    );

    for path in [
        ".codex-plugin/plugin.json",
        "plugins/pgs/.codex-plugin/plugin.json",
        "plugins/pgs/.claude-plugin/plugin.json",
    ] {
        assert_eq!(read_json(path)["version"], expected, "{path}");
    }

    for path in [".mcp.json", "plugins/pgs/.mcp.json"] {
        let launcher = read_json(path)["mcpServers"]["pgs"]["args"][1]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            launcher.contains(&format!("VERSION=\"{expected}\"")),
            "{path} launcher must download v{expected}"
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
