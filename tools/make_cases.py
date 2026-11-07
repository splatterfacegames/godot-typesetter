#!/usr/bin/env python3
"""Writes the ground-truth case files in bench/cases/ (shared by the Unity dump
harness and Typesetter's parity tests). Property names are TextMesh Pro's
public TMP_Text property names; Typesetter maps them mechanically to snake_case.

    python tools/make_cases.py
"""
import json
import os

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(REPO, "bench", "cases")
ASCII = "".join(chr(c) for c in range(32, 127))
LOREM = ("The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs! "
         "Sphinx of black quartz, judge my vow; how vexingly quick daft zebras jump.")
cases = {}


def case(name, text, props=None, rect=(600, 200), component="ugui", asset=None, **kw):
    c = {"font": "Inter-Regular.ttf", "component": component, "rect": list(rect), "text": text,
         "props": {"fontSize": 36, "alignment": "TopLeft", **(props or {})}}
    if asset:
        c["asset"] = asset
    c.update(kw)
    cases[name] = c


# --- basic layout
case("single_line", "Hello, world!", {"textWrappingMode": "NoWrap"})
case("kerning_pairs", "AVATAR To WAVE Yo! LT", {"fontSize": 48, "textWrappingMode": "NoWrap"}, rect=(800, 120))
for fs in (12, 18, 24, 72):
    case("size_%d" % fs, "Sphinx of black quartz", {"fontSize": fs, "textWrappingMode": "NoWrap"}, rect=(1200, 200))
case("newlines", "Line one\nLine two\n\nLine four", {"fontSize": 30}, rect=(500, 300))
case("wrap_normal", LOREM, {"fontSize": 24, "textWrappingMode": "Normal"}, rect=(300, 400),
     preferred=[[300, 0], [0, 0], [150, 0], [1000, 0]])
case("wrap_long_word", "Supercalifragilisticexpialidocious fits nowhere", {"fontSize": 30, "textWrappingMode": "Normal"}, rect=(200, 400))
case("wrap_nowrap_overflow", LOREM, {"fontSize": 24, "textWrappingMode": "NoWrap"}, rect=(300, 100))
case("wrap_preserve_ws", "a  b   c\t d    e", {"fontSize": 30, "textWrappingMode": "PreserveWhitespace"}, rect=(400, 200))

# --- alignment
for al in ("TopLeft", "Top", "TopRight", "TopJustified", "TopFlush", "TopGeoAligned",
           "Left", "Center", "Right", "BottomLeft", "Bottom", "BottomRight",
           "BaselineLeft", "Baseline", "MidlineLeft", "Midline", "CaplineLeft", "Capline"):
    case("align_" + al, LOREM[:90], {"fontSize": 24, "alignment": al, "textWrappingMode": "Normal"}, rect=(320, 240))

# --- spacing
case("spacing_character", LOREM[:60], {"fontSize": 24, "characterSpacing": 12, "textWrappingMode": "Normal"}, rect=(400, 300))
case("spacing_character_neg", LOREM[:60], {"fontSize": 24, "characterSpacing": -8, "textWrappingMode": "Normal"}, rect=(400, 300))
case("spacing_word", LOREM[:60], {"fontSize": 24, "wordSpacing": 30, "textWrappingMode": "Normal"}, rect=(400, 300))
case("spacing_line", LOREM[:90], {"fontSize": 24, "lineSpacing": 40, "textWrappingMode": "Normal"}, rect=(300, 400))
case("spacing_line_neg", LOREM[:90], {"fontSize": 24, "lineSpacing": -20, "textWrappingMode": "Normal"}, rect=(300, 400))
case("spacing_paragraph", "First paragraph here.\nSecond one.\nThird.", {"fontSize": 24, "paragraphSpacing": 25}, rect=(400, 300))
case("margins", LOREM[:90], {"fontSize": 24, "margin": [10, 20, 30, 40], "textWrappingMode": "Normal"}, rect=(320, 300))

# --- overflow
for mode in ("Overflow", "Ellipsis", "Masking", "Truncate", "ScrollRect", "Page"):
    case("overflow_" + mode, LOREM, {"fontSize": 24, "overflowMode": mode, "textWrappingMode": "Normal"}, rect=(260, 100))
case("overflow_ellipsis_nowrap", LOREM, {"fontSize": 24, "overflowMode": "Ellipsis", "textWrappingMode": "NoWrap"}, rect=(260, 60))

# --- auto size
case("autosize_shrink", LOREM, {"enableAutoSizing": True, "fontSizeMin": 8, "fontSizeMax": 72, "textWrappingMode": "Normal"}, rect=(300, 120))
case("autosize_grow", "Hi", {"enableAutoSizing": True, "fontSizeMin": 8, "fontSizeMax": 200, "textWrappingMode": "Normal"}, rect=(300, 120))

# --- visibility
case("max_visible_characters", LOREM[:80], {"fontSize": 24, "maxVisibleCharacters": 20, "textWrappingMode": "Normal"}, rect=(320, 200))
case("max_visible_lines", LOREM, {"fontSize": 24, "maxVisibleLines": 2, "textWrappingMode": "Normal"}, rect=(320, 300))

# --- 3D component
case("tmp3d_basic", "Hello 3D", {"fontSize": 36, "textWrappingMode": "NoWrap"}, rect=(20, 5), component="3d")
case("tmp3d_wrap", LOREM[:90], {"fontSize": 12, "textWrappingMode": "Normal"}, rect=(10, 10), component="3d")

# --- rich text (first batch)
case("rich_basic", "<b>Bold</b> <i>Italic</i> <u>Under</u> <s>Strike</s> <size=50>Big</size> <color=#FF0000>Red</color>",
     {"fontSize": 30, "textWrappingMode": "Normal"}, rect=(700, 200))

# --- atlas generation per render mode (ASCII, static set)
for mode in ("SDF", "SDFAA", "SDFAA_HINTED", "SDF8", "SDF16", "SDF32", "SMOOTH", "SMOOTH_HINTED", "RASTER", "RASTER_HINTED"):
    case("atlas_" + mode.lower(), "AVg@", {"fontSize": 48, "textWrappingMode": "NoWrap"}, rect=(400, 100),
         asset={"pointSize": 48, "padding": 5, "renderMode": mode, "atlas": [512, 512], "characters": ASCII})

os.makedirs(OUT, exist_ok=True)
for name, c in cases.items():
    with open(os.path.join(OUT, name + ".json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(c, f, indent=1, ensure_ascii=False)
        f.write("\n")
print("wrote %d cases to %s" % (len(cases), OUT))
