#!/usr/bin/env bash
# Usage: bash tests/manual/symlink_audit.sh [repo_path]
# Lists every symlink entry in HEAD and reports whether the blob looks corrupt
# (size > 4096 bytes is suspicious; symlink targets are usually short paths).
set -euo pipefail
REPO="${1:-.}"
cd "$REPO"
git ls-tree -r HEAD | awk '$1=="120000"' | while read -r mode type sha path; do
  size=$(git cat-file -s "$sha")
  if [ "$size" -gt 4096 ]; then
    printf "SUSPECT  size=%-8s sha=%s path=%s\n" "$size" "$sha" "$path"
  else
    target=$(git cat-file blob "$sha")
    printf "OK       size=%-8s target=%-40s path=%s\n" "$size" "$target" "$path"
  fi
done
