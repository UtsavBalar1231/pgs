#!/bin/sh
# install-binary.sh — Download pgs-mcp binary from GitHub Releases.
# Called by SessionStart hook and run-pgs-mcp.sh fallback.
# NEVER exits non-zero on download failure — must not crash the session.

set -u

# Resolve plugin root: prefer env var, fall back to two dirs above this script.
PLUGIN_ROOT="${PGS_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}"
PLUGIN_DATA="${PGS_PLUGIN_DATA:-${HOME}/.local/share/pgs-plugin}"

VERSION_FILE="${PLUGIN_ROOT}/VERSION"
DATA_VERSION_FILE="${PLUGIN_DATA}/VERSION"
BIN_DIR="${PLUGIN_DATA}/bin"
BINARY="${BIN_DIR}/pgs-mcp"

# Read current version from plugin source.
if [ ! -f "$VERSION_FILE" ]; then
    printf 'pgs-install: VERSION file not found at %s\n' "$VERSION_FILE" >&2
    exit 0
fi
VERSION="$(cat "$VERSION_FILE" | tr -d '[:space:]')"

# Skip download if already at this version.
if [ -f "$DATA_VERSION_FILE" ]; then
    INSTALLED_VERSION="$(cat "$DATA_VERSION_FILE" | tr -d '[:space:]')"
    if [ "$INSTALLED_VERSION" = "$VERSION" ] && [ -x "$BINARY" ]; then
        exit 0
    fi
fi

# Detect OS.
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        BINARY_NAME="pgs-mcp-universal-apple-darwin"
        ;;
    Linux)
        case "$ARCH" in
            x86_64)
                BINARY_NAME="pgs-mcp-x86_64-unknown-linux-musl"
                ;;
            aarch64)
                BINARY_NAME="pgs-mcp-aarch64-unknown-linux-musl"
                ;;
            *)
                printf 'pgs-install: unsupported Linux architecture: %s\n' "$ARCH" >&2
                exit 0
                ;;
        esac
        ;;
    MINGW*|MSYS*|CYGWIN*)
        case "$ARCH" in
            x86_64)
                BINARY_NAME="pgs-mcp-x86_64-pc-windows-msvc.exe"
                ;;
            *)
                printf 'pgs-install: unsupported Windows architecture: %s\n' "$ARCH" >&2
                exit 0
                ;;
        esac
        ;;
    *)
        printf 'pgs-install: unsupported OS: %s\n' "$OS" >&2
        exit 0
        ;;
esac

DOWNLOAD_URL="https://github.com/UtsavBalar1231/pgs/releases/download/v${VERSION}/${BINARY_NAME}"

printf 'pgs-install: downloading %s -> %s\n' "$DOWNLOAD_URL" "$BINARY" >&2

# Create bin directory.
if ! mkdir -p "$BIN_DIR"; then
    printf 'pgs-install: failed to create bin directory: %s\n' "$BIN_DIR" >&2
    exit 0
fi

# Download binary.
if ! curl -fsSL "$DOWNLOAD_URL" -o "$BINARY"; then
    printf 'pgs-install: download failed (URL: %s)\n' "$DOWNLOAD_URL" >&2
    rm -f "$BINARY"
    exit 0
fi

# Make executable.
if ! chmod +x "$BINARY"; then
    printf 'pgs-install: failed to chmod +x %s\n' "$BINARY" >&2
    exit 0
fi

# Record installed version.
printf '%s\n' "$VERSION" > "$DATA_VERSION_FILE"

printf 'pgs-install: installed pgs-mcp v%s\n' "$VERSION" >&2
