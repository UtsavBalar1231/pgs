#!/bin/sh
# run-pgs-mcp.sh - Execute the cached pgs-mcp binary.
# If binary is missing or cached at a different version, triggers install-binary.sh first.

set -u

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
PLUGIN_ROOT="${PGS_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)}}"
PLUGIN_DATA="${PGS_PLUGIN_DATA:-${CLAUDE_PLUGIN_DATA:-${XDG_DATA_HOME:-${HOME}/.local/share}/pgs-plugin}}"
VERSION_FILE="${PLUGIN_ROOT}/VERSION"
DATA_VERSION_FILE="${PLUGIN_DATA}/VERSION"
BINARY="${PLUGIN_DATA}/bin/pgs-mcp"

VERSION=""
if [ -f "$VERSION_FILE" ]; then
    VERSION="$(tr -d '[:space:]' < "$VERSION_FILE")"
fi

INSTALLED_VERSION=""
if [ -f "$DATA_VERSION_FILE" ]; then
    INSTALLED_VERSION="$(tr -d '[:space:]' < "$DATA_VERSION_FILE")"
fi

if [ ! -x "$BINARY" ] || { [ -n "$VERSION" ] && [ "$INSTALLED_VERSION" != "$VERSION" ]; }; then
    PGS_PLUGIN_ROOT="$PLUGIN_ROOT" PGS_PLUGIN_DATA="$PLUGIN_DATA" \
        "${PLUGIN_ROOT}/scripts/install-binary.sh" >&2
fi

exec "$BINARY" "$@"
