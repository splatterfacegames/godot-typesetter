#!/usr/bin/env python3
"""Builds tests/fonts/NotoSansJP-Subset.ttf, the small CJK test font used by the
CJK line-breaking and <font> cases.

Source: Noto Sans JP (SIL OFL 1.1) from the Google Fonts repository,
https://github.com/google/fonts/tree/main/ofl/notosansjp (NotoSansJP[wght].ttf).
The variable font is instanced at wght=400 and subset (fontTools) to ASCII,
CJK punctuation, kana, half/full-width forms and the ideographs that appear in
bench/cases texts that use the font.

    python tools/make_cjk_font.py path/to/NotoSansJP[wght].ttf
"""
import glob
import json
import os
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "tests", "fonts", "NotoSansJP-Subset.ttf")


def wanted():
    cps = set(range(0x20, 0x7F))
    cps |= set(range(0x3000, 0x3100))  # CJK punctuation, hiragana, katakana
    cps |= set(range(0xFF00, 0xFFF0))  # half/full-width forms
    cps |= {0xA0, 0xAD, 0x2010, 0x2013, 0x2014, 0x2026, 0x25A1, 0x2015, 0x30FC}
    for path in glob.glob(os.path.join(REPO, "bench", "cases", "*.json")):
        c = json.load(open(path, encoding="utf-8"))
        uses = "NotoSansJP" in c.get("font", "") or any("NotoSansJP" in f.get("font", "") for f in c.get("fonts", {}).values())
        if uses:
            cps |= {ord(ch) for ch in c["text"] if ord(ch) >= 0x2E80}
    return cps


def main():
    src = sys.argv[1]
    font = TTFont(src)
    # Subset first (instancing all ~17k glyphs is very slow), then instance.
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    opts.hinting = False
    s = subset.Subsetter(opts)
    s.populate(unicodes=sorted(wanted()))
    s.subset(font)
    if "fvar" in font:
        font = instancer.instantiateVariableFont(font, {"wght": 400})
    font.save(OUT)
    print("wrote %s (%d bytes)" % (OUT, os.path.getsize(OUT)))


if __name__ == "__main__":
    main()
