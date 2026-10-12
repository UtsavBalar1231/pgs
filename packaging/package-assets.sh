#!/usr/bin/env bash
# Build the per-target install archives and MCP Bundles from the release
# binaries in <asset-dir>, which must already hold pgs-<target>[.exe] and
# pgs-mcp-<target>[.exe] for every target in packaging/targets.txt.
#
# Writes next to the binaries and leaves them untouched:
#   pgs-<target>.tar.gz   (zip on Windows)  pgs, pgs-mcp, LICENSE, README.md
#   pgs-mcp-<target>.mcpb                   manifest.json, server/pgs-mcp[.exe]
#
# Needs GNU tar and zip, both present on ubuntu runners.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <asset-dir>" >&2
  exit 2
fi

ASSET_DIR=$(cd "$1" && pwd)
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

while read -r target; do
  case "$target" in
    *windows*) ext=.exe platform=win32 ;;
    *darwin*) ext= platform=darwin ;;
    *) ext= platform=linux ;;
  esac

  for name in pgs pgs-mcp; do
    if [ ! -f "$ASSET_DIR/$name-$target$ext" ]; then
      echo "missing binary: $name-$target$ext" >&2
      exit 1
    fi
  done

  archive="$WORK/$target/archive"
  mkdir -p "$archive"
  install -m 755 "$ASSET_DIR/pgs-$target$ext" "$archive/pgs$ext"
  install -m 755 "$ASSET_DIR/pgs-mcp-$target$ext" "$archive/pgs-mcp$ext"
  install -m 644 "$ROOT/LICENSE" "$ROOT/README.md" "$archive/"

  if [ -n "$ext" ]; then
    rm -f "$ASSET_DIR/pgs-$target.zip"
    (cd "$archive" && zip -X -q "$ASSET_DIR/pgs-$target.zip" "pgs$ext" "pgs-mcp$ext" LICENSE README.md)
  else
    tar --owner=0 --group=0 --numeric-owner -czf "$ASSET_DIR/pgs-$target.tar.gz" \
      -C "$archive" pgs pgs-mcp LICENSE README.md
  fi

  bundle="$WORK/$target/bundle"
  mkdir -p "$bundle/server"
  install -m 755 "$ASSET_DIR/pgs-mcp-$target$ext" "$bundle/server/pgs-mcp$ext"
  sed -e "s/@VERSION@/$VERSION/g" -e "s/@PLATFORM@/$platform/g" -e "s/@BINARY@/pgs-mcp$ext/g" \
    "$ROOT/packaging/mcpb/manifest.json" > "$bundle/manifest.json"
  if grep -q '@[A-Z_]*@' "$bundle/manifest.json"; then
    echo "unfilled placeholder in the manifest for $target" >&2
    exit 1
  fi
  rm -f "$ASSET_DIR/pgs-mcp-$target.mcpb"
  (cd "$bundle" && zip -X -q "$ASSET_DIR/pgs-mcp-$target.mcpb" manifest.json "server/pgs-mcp$ext")
done < "$ROOT/packaging/targets.txt"
