#!/usr/bin/env python3
"""Regenerate the atlas behind the product site's live hero (needs Pillow)."""
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parent.parent


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        pgm = Path(tmp) / "atlas.pgm"
        js = Path(tmp) / "atlas.json"
        font = REPO / "tests" / "fonts" / "Inter-Regular.ttf"
        r = subprocess.run(
            ["cargo", "run", "-q", "--release", "--example", "site_atlas", "--", str(font), str(pgm), str(js)],
            cwd=REPO / "native",
        )
        if r.returncode != 0:
            return r.returncode
        Image.open(pgm).save(REPO / "site" / "hero-atlas.png", optimize=True)
        shutil.copy2(js, REPO / "site" / "hero-atlas.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
