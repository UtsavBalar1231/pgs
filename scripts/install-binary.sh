#!/bin/sh
# install-binary.sh - Download pgs-mcp binary from GitHub Releases.
# Called by run-pgs-mcp.sh fallback.
# NEVER exits non-zero on download failure; it must not crash the session.

set -u

# Resolve plugin root: prefer env var, fall back to one dir above this script.
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
PLUGIN_ROOT="${PGS_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)}}"
PLUGIN_DATA="${PGS_PLUGIN_DATA:-${CLAUDE_PLUGIN_DATA:-${XDG_DATA_HOME:-${HOME}/.local/share}/pgs-plugin}}"

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
installed_version() {
    if [ -r "$DATA_VERSION_FILE" ]; then
        tr -d '[:space:]' < "$DATA_VERSION_FILE"
    fi
}

needs_install() {
    INSTALLED_VERSION="$(installed_version)"
    [ ! -x "$BINARY" ] || [ "$INSTALLED_VERSION" != "$VERSION" ]
}

if ! needs_install; then
    exit 0
fi

if ! mkdir -p "$PLUGIN_DATA"; then
    printf 'pgs-install: failed to create data directory: %s\n' "$PLUGIN_DATA" >&2
    exit 0
fi

LOCK_DIR="${PLUGIN_DATA}/.install.lock"
HAVE_LOCK=0
while [ "$HAVE_LOCK" -eq 0 ]; do
    if mkdir "$LOCK_DIR" 2>/dev/null; then
        HAVE_LOCK=1
        trap 'rmdir "$LOCK_DIR"' EXIT INT TERM
    elif needs_install; then
        sleep 0.1
    else
        exit 0
    fi
done

if ! needs_install; then
    exit 0
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
TMP_BINARY="${BIN_DIR}/.pgs-mcp.$$.tmp"
TMP_VERSION="${PLUGIN_DATA}/.VERSION.$$.tmp"
rm -f "$TMP_BINARY" "$TMP_VERSION"

if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_BINARY"; then
    printf 'pgs-install: download failed (URL: %s)\n' "$DOWNLOAD_URL" >&2
    rm -f "$TMP_BINARY" "$TMP_VERSION"
    exit 0
fi

# Make executable.
if ! chmod +x "$TMP_BINARY"; then
    printf 'pgs-install: failed to chmod +x %s\n' "$TMP_BINARY" >&2
    rm -f "$TMP_BINARY" "$TMP_VERSION"
    exit 0
fi

if ! mv -f "$TMP_BINARY" "$BINARY"; then
    printf 'pgs-install: failed to move %s to %s\n' "$TMP_BINARY" "$BINARY" >&2
    rm -f "$TMP_BINARY" "$TMP_VERSION"
    exit 0
fi

# Record installed version.
if ! printf '%s\n' "$VERSION" > "$TMP_VERSION"; then
    printf 'pgs-install: failed to write version file: %s\n' "$TMP_VERSION" >&2
    rm -f "$TMP_VERSION"
    exit 0
fi

if ! mv -f "$TMP_VERSION" "$DATA_VERSION_FILE"; then
    printf 'pgs-install: failed to move %s to %s\n' "$TMP_VERSION" "$DATA_VERSION_FILE" >&2
    rm -f "$TMP_VERSION"
    exit 0
fi

printf 'pgs-install: installed pgs-mcp v%s\n' "$VERSION" >&2
