#!/usr/bin/env bash
# Usage: bash tests/manual/symlink_repro.sh
#
# Requires pgs on PATH, or set CARGO_TARGET_DIR to the directory containing
# the release binary (e.g. CARGO_TARGET_DIR=/path/to/project/target).
# Build first: cargo build --release
#
# This script creates an isolated /tmp repo, plants a symlink whose target is
# a 2 KiB binary file, stages the symlink via `pgs stage`, then asserts:
#   - index mode is 120000
#   - staged blob size is 10 bytes (length of "target.bin")
#   - staged blob content is exactly "target.bin"
# A failure before the fix prints "FAIL: ..." and exits non-zero.
set -euo pipefail

REPO=$(mktemp -d)
cd "$REPO"
git init -q
git config user.email t@t
git config user.name t
echo "initial" > seed.txt
git add seed.txt && git commit -q -m initial

# Create a "fat" target file so a corrupt read would be obviously wrong
dd if=/dev/urandom of=target.bin bs=1024 count=2 status=none  # 2 KiB
ln -s target.bin link

# Locate pgs binary
PGS=$(command -v pgs 2>/dev/null || echo "${CARGO_TARGET_DIR:-$PWD/target}/release/pgs")

"$PGS" --repo "$REPO" stage link --json

# Inspect the staged blob
SHA=$(git ls-files -s link | awk '{print $2}')
MODE=$(git ls-files -s link | awk '{print $1}')
SIZE=$(git cat-file -s "$SHA")
CONTENT=$(git cat-file blob "$SHA")

echo "mode=$MODE sha=$SHA size=$SIZE content=$CONTENT"

test "$MODE" = "120000" || { echo "FAIL: mode is not 120000 (got $MODE)" >&2; rm -rf "$REPO"; exit 1; }
test "$SIZE" = "10"     || { echo "FAIL: size is not 10 (got $SIZE; target.bin string length is 10)" >&2; rm -rf "$REPO"; exit 1; }
test "$CONTENT" = "target.bin" || { echo "FAIL: content is not 'target.bin' (got '$CONTENT')" >&2; rm -rf "$REPO"; exit 1; }

echo "PASS: pgs staged symlink correctly"
rm -rf "$REPO"
