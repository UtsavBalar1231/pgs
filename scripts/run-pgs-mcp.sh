#!/bin/sh
# run-pgs-mcp.sh — Execute the cached pgs-mcp binary.
# If binary is missing or not executable, triggers install-binary.sh first.

set -u

BINARY="${PGS_PLUGIN_DATA}/bin/pgs-mcp"

if [ ! -x "$BINARY" ]; then
    "${PGS_PLUGIN_ROOT}/scripts/install-binary.sh" >&2
fi

exec "$BINARY" "$@"
