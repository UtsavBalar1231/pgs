# Install pgs

[← Project overview](../README.md) · [Agent integrations](INTEGRATIONS.md)

Choose a plugin for your coding agent, a prebuilt binary for the CLI, or a source build.
Plugin and extension installs include the MCP server launcher and agent skill;
you do not need to install the CLI first. See [agent integrations](INTEGRATIONS.md).

### Prebuilt binaries

Each [GitHub release](https://github.com/UtsavBalar1231/pgs/releases/latest)
ships, for every platform:

- both binaries, named `pgs-<target>` and `pgs-mcp-<target>`;
- an archive, `pgs-<target>.tar.gz` (`pgs-<target>.zip` on Windows), holding
  `pgs`, `pgs-mcp`, `LICENSE`, and `README.md` at its root;
- an [MCP Bundle](https://github.com/modelcontextprotocol/mcpb),
  `pgs-mcp-<target>.mcpb`, for clients that install `.mcpb` files.

A `SHA256SUMS` file covers all of them.

| Platform | `<target>` |
|----------|------------|
| Linux x86_64 | `x86_64-unknown-linux-musl` |
| Linux ARM64 | `aarch64-unknown-linux-musl` |
| macOS, Intel and Apple Silicon | `universal-apple-darwin` (or `x86_64-apple-darwin`, `aarch64-apple-darwin`) |
| Windows x86_64 | `x86_64-pc-windows-msvc` (`.exe`) |
| Windows ARM64 | `aarch64-pc-windows-msvc` (`.exe`, new in 0.8.0) |

Linux and macOS:

```bash
TARGET=x86_64-unknown-linux-musl   # or aarch64-unknown-linux-musl, universal-apple-darwin
BASE=https://github.com/UtsavBalar1231/pgs/releases/latest/download
mkdir -p ~/.local/bin
curl -fsSL -o ~/.local/bin/pgs     "$BASE/pgs-$TARGET"
curl -fsSL -o ~/.local/bin/pgs-mcp "$BASE/pgs-mcp-$TARGET"
chmod +x ~/.local/bin/pgs ~/.local/bin/pgs-mcp
```

Windows (PowerShell):

```powershell
$Target = "x86_64-pc-windows-msvc"   # or aarch64-pc-windows-msvc
$Base = "https://github.com/UtsavBalar1231/pgs/releases/latest/download"
$Dir = "$env:LOCALAPPDATA\pgs\bin"
New-Item -ItemType Directory -Force $Dir | Out-Null
Invoke-WebRequest "$Base/pgs-$Target.exe" -OutFile "$Dir\pgs.exe"
Invoke-WebRequest "$Base/pgs-mcp-$Target.exe" -OutFile "$Dir\pgs-mcp.exe"
```

Add the install directory to your `PATH`.

To install from an archive instead, unpack it and copy both binaries onto your
`PATH`. On Linux and macOS, with `TARGET` and `BASE` set as above:

```bash
curl -fsSL "$BASE/pgs-$TARGET.tar.gz" | tar -xz -C ~/.local/bin pgs pgs-mcp
```

#### Verify a download

Every binary, archive, and bundle in a release carries a signed build-provenance
attestation that names this repository's release workflow. With the
[GitHub CLI](https://cli.github.com/):

```bash
gh attestation verify pgs-$TARGET.tar.gz -R UtsavBalar1231/pgs
```

The check passes only for a file that this repository's release workflow
published. Releases made before attestations were added have none.

### With cargo-binstall

`Cargo.toml` carries [cargo-binstall](https://github.com/cargo-bins/cargo-binstall)
metadata that points at the release archives. The crate name `pgs` is taken on
crates.io by an unrelated project, so `cargo binstall pgs` does not install this
tool. Point binstall at this repository instead:

```bash
cargo binstall --git https://github.com/UtsavBalar1231/pgs pgs
```

or, from a checkout, `cargo binstall --manifest-path . pgs`. Both install `pgs`
and `pgs-mcp`.

### From source

```bash
cargo install --git https://github.com/UtsavBalar1231/pgs --locked
```

This installs both `pgs` and `pgs-mcp`. It needs Rust 1.88+ and a C compiler
(for libgit2).
