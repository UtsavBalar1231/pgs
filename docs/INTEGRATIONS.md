# Using pgs with coding agents

`pgs-mcp` is a local stdio MCP server. Every harness below needs the same two
things: the `pgs-mcp` binary, and a config entry that launches it. The agent
skill is optional but recommended; it teaches the scan, plan, stage, commit
workflow.

Install `pgs-mcp` first (see the README), and put it on your `PATH`. If you keep
it somewhere else, replace `pgs-mcp` with the absolute path in every snippet.
Each config below is the minimal form; the harness's own documentation covers
optional fields such as environment variables and timeouts.

Plugin installs (Claude Code, Codex, Copilot CLI, Factory Droid) read the
marketplace in this repository. The plugin's launcher downloads a matching
`pgs-mcp` on first start for Linux (x86_64, aarch64), macOS, and Windows
(x86_64, ARM64). On Windows it runs under the `sh` that ships with Git for
Windows; without it on `PATH`, install the binary yourself and use a manual
setup.

## Plugin and extension installs

### Claude Code

```text
/plugin marketplace add UtsavBalar1231/pgs
/plugin install pgs@pgs-marketplace
```

Or register the server without the plugin:

```bash
claude mcp add --transport stdio --scope user pgs -- pgs-mcp
```

### Codex

```bash
codex plugin marketplace add UtsavBalar1231/pgs
codex plugin add pgs@pgs-marketplace
```

Or register only the server:

```bash
codex mcp add pgs -- pgs-mcp
```

Equivalent `~/.codex/config.toml` entry:

```toml
[mcp_servers.pgs]
command = "pgs-mcp"
```

### Gemini CLI

```bash
gemini extensions install https://github.com/UtsavBalar1231/pgs
```

The repository root carries a `gemini-extension.json` starting with 0.8.0. Or
register only the server:

```bash
gemini mcp add -s user pgs pgs-mcp
```

### GitHub Copilot CLI

```bash
copilot plugin install UtsavBalar1231/pgs
```

Copilot CLI reads `.claude-plugin/marketplace.json` from the repository.

### Factory Droid

```bash
droid plugin marketplace add UtsavBalar1231/pgs
droid plugin install pgs@pgs-marketplace
```

## Editors and manual MCP configuration

### VS Code

```bash
code --add-mcp '{"name":"pgs","command":"pgs-mcp"}'
```

Or open this link in a browser to install through VS Code:

```text
vscode:mcp/install?%7B%22name%22%3A%22pgs%22%2C%22command%22%3A%22pgs-mcp%22%7D
```

### Cursor

Install link:

```text
cursor://anysphere.cursor-deeplink/mcp/install?name=pgs&config=eyJjb21tYW5kIjoicGdzLW1jcCJ9
```

Or add the server to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "pgs": { "command": "pgs-mcp" }
  }
}
```

### Kiro

Install link:

```text
https://kiro.dev/launch/mcp/add?name=pgs&config=%7B%22command%22%3A%22pgs-mcp%22%7D
```

Or add the server to `~/.kiro/settings/mcp.json`:

```json
{
  "mcpServers": {
    "pgs": { "command": "pgs-mcp" }
  }
}
```

### Windsurf and Devin

Add the server to `~/.config/devin/mcp_config.json` (`%APPDATA%\devin\mcp_config.json`
on Windows):

```json
{
  "mcpServers": {
    "pgs": { "command": "pgs-mcp", "args": [] }
  }
}
```

### Cline

Add the server to `cline_mcp_settings.json`:

```json
{
  "mcpServers": {
    "pgs": { "command": "pgs-mcp", "args": [] }
  }
}
```

### Zed

Add the server to `settings.json`:

```json
{
  "context_servers": {
    "pgs": { "command": "pgs-mcp", "args": [] }
  }
}
```

### JetBrains Junie

Add the server to `.junie/mcp/mcp.json` in the project:

```json
{
  "mcpServers": {
    "pgs": { "command": "pgs-mcp", "args": [] }
  }
}
```

### Warp

Add the server to `.warp/.mcp.json` in the project:

```json
{
  "mcpServers": {
    "pgs": { "command": "pgs-mcp", "args": [] }
  }
}
```

### OpenCode

Add the server to `opencode.json`:

```json
{
  "mcp": {
    "pgs": {
      "type": "local",
      "command": ["pgs-mcp"],
      "enabled": true
    }
  }
}
```

### Goose

Add an entry under `extensions` in `~/.config/goose/config.yaml`:

```yaml
extensions:
  pgs:
    name: pgs
    type: stdio
    cmd: pgs-mcp
    args: []
    enabled: true
```

### Amp

```bash
amp mcp add pgs -- pgs-mcp
```

### Crush

Add a line to `crushrc` (`./.crushrc`, `./crushrc`, or `~/.config/crush/crushrc`):

```bash
mcp add pgs --command pgs-mcp
```

### Continue

Create `.continue/mcpServers/pgs.yaml`:

```yaml
name: pgs
version: 0.0.1
schema: v1
mcpServers:
  - name: pgs
    command: pgs-mcp
```

## MCP Bundles

Each release ships one MCP Bundle per platform, `pgs-mcp-<target>.mcpb`, for
clients that install `.mcpb` files (for example Claude Desktop). A bundle is a
zip holding `manifest.json` and the `pgs-mcp` binary under `server/`
(`pgs-mcp.exe` on Windows). The manifest follows the
[MCPB spec](https://github.com/modelcontextprotocol/mcpb) `0.3`, declares a
`binary` server, and restricts `compatibility.platforms` to the bundle's OS. The
release workflow fills the template in `packaging/mcpb/manifest.json` from
`Cargo.toml` and checks every generated manifest against the MCPB JSON schema
before the release assets are collected. Pick the bundle whose `<target>`
matches your machine; the `universal-apple-darwin` bundle covers Intel and
Apple Silicon Macs.

`server.json` still lists no `packages`, so pgs is not published to the MCP
Registry. Publishing the bundles would take:

1. A published GitHub release, since the registry accepts MCPB artifacts
   hosted on GitHub or GitLab releases.
2. One `packages` entry per bundle in `server.json`, with `registryType`
   `"mcpb"`, the bundle's release download URL as `identifier` (it must contain
   "mcp", which `pgs-mcp-<target>.mcpb` does), `transport` of `stdio`, and
   `fileSha256` set to the bundle's SHA-256, which is its line in
   `SHA256SUMS`. The registry does not check the hash; clients do before they
   install.
3. `mcp-publisher publish`, authenticated as the owner of the
   `io.github.UtsavBalar1231` namespace.

The fields are from the registry's
[package types](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx)
page. That page says nothing about how a client chooses between several `mcpb`
entries for different platforms, so check that before listing all seven.

## The agent skill

The `git-commit-staging` skill lives at `plugins/pgs/skills/git-commit-staging`
in this repository. Plugin installs ship it for you. For a manual MCP setup,
copy the directory into the skills folder your harness reads:

| Destination | Read by |
|-------------|---------|
| `~/.agents/skills/` | Codex, Gemini CLI, Cursor, Copilot, OpenCode, Amp, Goose, Junie, Factory, Warp, Windsurf, Zed, Crush |
| `~/.claude/skills/` | Claude Code, Cline |
| `~/.kiro/skills/` | Kiro |

```bash
git clone https://github.com/UtsavBalar1231/pgs
mkdir -p ~/.agents/skills
cp -r pgs/plugins/pgs/skills/git-commit-staging ~/.agents/skills/
```

The skill's `allowed-tools` lists the pgs tools under both their standalone
names (`mcp__pgs__<tool>`) and their Claude Code plugin names
(`mcp__plugin_pgs_pgs__<tool>`). Harnesses that do not use `allowed-tools`
ignore the field.

## Checking the connection

Ask the agent to call `pgs_overview` with the absolute path of a git repository.
Every pgs tool requires an explicit `repo_path`; nothing is inferred from the
working directory. See [MCP_SERVER.md](MCP_SERVER.md) for the protocol contract.
