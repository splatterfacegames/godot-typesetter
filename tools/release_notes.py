#!/usr/bin/env python3
"""Checks a release's version everywhere it is written and prints its changelog notes.

    python tools/release_notes.py 0.1.0        # or v0.1.0

Fails when addons/typesetter/plugin.cfg, native/Cargo.toml or CHANGELOG.md disagree
with the version, or when the changelog has no section for it.
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    version = sys.argv[1].removeprefix("v")
    problems = []
    plugin = (REPO / "addons/typesetter/plugin.cfg").read_text(encoding="utf8")
    m = re.search(r'^version="([^"]+)"', plugin, re.M)
    if not m or m.group(1) != version:
        problems.append("plugin.cfg has version %s" % (m.group(1) if m else "none"))
    cargo = (REPO / "native/Cargo.toml").read_text(encoding="utf8")
    m = re.search(r'^version = "([^"]+)"', cargo, re.M)
    if not m or m.group(1) != version:
        problems.append("native/Cargo.toml has version %s" % (m.group(1) if m else "none"))
    changelog = (REPO / "CHANGELOG.md").read_text(encoding="utf8")
    m = re.search(r"^## \[%s\][^\n]*\n(.*?)(?=^## \[|^\[[^\]]+\]: )" % re.escape(version), changelog, re.M | re.S)
    if not m:
        problems.append("CHANGELOG.md has no section for %s" % version)
    if problems:
        for p in problems:
            print("error: " + p, file=sys.stderr)
        return 1
    print(m.group(1).strip())
    return 0


if __name__ == "__main__":
    sys.exit(main())
