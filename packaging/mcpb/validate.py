"""Validate the manifest.json inside each .mcpb bundle against the MCPB JSON schema.

usage: validate.py SCHEMA BUNDLE.mcpb [BUNDLE.mcpb ...]

Also checks that the entry point the manifest names is a file in the bundle.
"""

import json
import sys
import zipfile

import jsonschema


def main() -> int:
    schema = json.load(open(sys.argv[1], encoding="utf-8"))
    failed = False
    for bundle in sys.argv[2:]:
        with zipfile.ZipFile(bundle) as archive:
            manifest = json.loads(archive.read("manifest.json"))
            problems = [
                f"{'/'.join(map(str, e.absolute_path)) or '<root>'}: {e.message}"
                for e in jsonschema.Draft7Validator(schema).iter_errors(manifest)
            ]
            entry_point = manifest.get("server", {}).get("entry_point")
            if entry_point not in archive.namelist():
                problems.append(f"entry_point {entry_point!r} is not in the bundle")
        print(f"{'FAIL' if problems else 'ok  '} {bundle}")
        for problem in problems:
            print(f"     {problem}")
        failed = failed or bool(problems)
    return 1 if failed else 0


sys.exit(main())
