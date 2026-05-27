#!/bin/sh
# run-pgs-mcp.sh - Execute the cached pgs-mcp binary.
# If binary is missing or not executable, triggers install-binary.sh first.

set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PLUGIN_ROOT="${PGS_PLUGIN_ROOT:-$(CDPATH= cd -- "${SCRIPT_DIR}/.." && pwd)}"
PLUGIN_DATA="${PGS_PLUGIN_DATA:-${XDG_DATA_HOME:-${HOME}/.local/share}/pgs-plugin}"
BINARY="${PLUGIN_DATA}/bin/pgs-mcp"

if [ ! -x "$BINARY" ]; then
    PGS_PLUGIN_ROOT="$PLUGIN_ROOT" PGS_PLUGIN_DATA="$PLUGIN_DATA" \
        "${PLUGIN_ROOT}/scripts/install-binary.sh" >&2
fi

exec "$BINARY" "$@"
