"""Render the MCP Registry server.json for one release.

usage: render-server-json.py SERVER_JSON_TEMPLATE SHA256SUMS TAG > server.json

Takes the repo-root server.json as the template, sets `version` to the tag
without its leading "v", and replaces `packages` with one `mcpb` entry per
target in TARGETS (the per-architecture macOS bundles are not listed). The
registry has no platform field, so the bundles are told apart only by the
target in their file name.
"""

import json
import re
import sys

REPOSITORY = "UtsavBalar1231/pgs"

# Order is the order in the rendered `packages` array.
TARGETS = (
    "universal-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
)

# `sha256sum` output: "<hash>  <name>", with "*" before the name in binary mode.
CHECKSUM_LINE = re.compile(r"^([0-9a-fA-F]{64}) [ *](.+)$")


def read_checksums(path: str) -> dict[str, str]:
    checksums = {}
    with open(path, encoding="utf-8") as sums:
        for line in sums:
            match = CHECKSUM_LINE.match(line.rstrip("\n"))
            if match:
                checksums[match.group(2)] = match.group(1).lower()
    return checksums


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    template_path, sums_path, tag = sys.argv[1:]
    if not tag.startswith("v") or len(tag) == 1:
        print(f"tag must start with 'v' and name a version, got {tag!r}", file=sys.stderr)
        return 2

    with open(template_path, encoding="utf-8") as template_file:
        server = json.load(template_file)
    checksums = read_checksums(sums_path)

    packages = []
    for target in TARGETS:
        bundle = f"pgs-mcp-{target}.mcpb"
        if bundle not in checksums:
            print(f"{sums_path} has no checksum for {bundle}", file=sys.stderr)
            return 1
        packages.append(
            {
                "registryType": "mcpb",
                "identifier": f"https://github.com/{REPOSITORY}/releases/download/{tag}/{bundle}",
                "fileSha256": checksums[bundle],
                "transport": {"type": "stdio"},
            }
        )

    server["version"] = tag[1:]
    server["packages"] = packages
    json.dump(server, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
