#!/usr/bin/env python3
"""Compares Typesetter's atlas texels with TextMesh Pro's, glyph by glyph.

    python tools/compare_atlases.py [MODE ...]

For each render mode, builds Typesetter's atlas for the bench case's character
set (native/examples/dump_atlas) and compares every glyph's padded rectangle
with the same glyph in bench/unity/atlases/*.png. Prints mean absolute texel
error (0-255) and the share of texels off by more than 8.
"""
import json
import os
import subprocess
import sys

from PIL import Image

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODES = {"SMOOTH": 0, "SMOOTH_HINTED": 1, "RASTER": 2, "RASTER_HINTED": 3, "SDF": 4, "SDF8": 5, "SDF16": 6,
         "SDF32": 7, "SDFAA": 8, "SDFAA_HINTED": 9}
OUT = os.path.join(REPO, "native", "target", "atlas_cmp")


def main():
    os.makedirs(OUT, exist_ok=True)
    modes = sys.argv[1:] or list(MODES)
    for m in modes:
        key = "Inter-Regular_48_5_%s_512x512" % m
        uj = json.load(open(os.path.join(REPO, "bench", "unity", "atlases", key + ".json")))
        uimg = Image.open(os.path.join(REPO, "bench", "unity", "atlases", key + "_0.png")).convert("L")
        H = uimg.height
        out = os.path.join(OUT, m + ".pgm")
        subprocess.run(["cargo", "run", "-q", "--release", "--example", "dump_atlas", "--",
                        os.path.join(REPO, "tests", "fonts", "Inter-Regular.ttf"), str(MODES[m]), "48", "5", out],
                       cwd=os.path.join(REPO, "native"), check=True, capture_output=True)
        ours = {g[0]: g[1:] for g in json.load(open(out + ".json"))}
        oimg = Image.open(out).convert("L")
        pad = 5
        tot = n = over = 0
        worst = (0, None)
        for g in uj["glyphs"]:
            gi = g["index"]
            if gi not in ours or g["rect"][2] == 0:
                continue
            x, y, w, h = g["rect"]
            ox, oy, ow, oh = ours[gi]
            if (ow, oh) != (w, h):
                print("  glyph %d size differs: unity %dx%d ours %dx%d" % (gi, w, h, ow, oh))
                continue
            ub = (x - pad, H - (y + h) - pad, x + w + pad, H - y + pad)
            ob = (ox - pad, oy - pad, ox + ow + pad, oy + oh + pad)
            ua = list(uimg.crop(ub).getdata())
            oa = list(oimg.crop(ob).getdata())
            e = sum(abs(a - b) for a, b in zip(ua, oa))
            tot += e
            n += len(ua)
            over += sum(1 for a, b in zip(ua, oa) if abs(a - b) > 8)
            if e / len(ua) > worst[0]:
                worst = (e / len(ua), gi)
        print("%-14s mae %6.3f  >8 %6.2f%%  worst glyph %s (%.2f)" % (m, tot / max(n, 1), 100 * over / max(n, 1), worst[1], worst[0]))


if __name__ == "__main__":
    main()
