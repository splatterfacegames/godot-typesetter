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

# --- rich text: every documented tag
RT = {"fontSize": 30, "textWrappingMode": "Normal"}
NOTO = {"NotoJP": {"font": "NotoSansJP-Subset.ttf"}}
SPRITES = {"name": "TypesetterSprites", "width": 256, "height": 64,
           "sprites": [{"name": "circle", "rect": [0, 0, 64, 64], "metrics": [64, 64, 0, 52, 64]},
                       {"name": "square", "rect": [64, 0, 64, 64], "metrics": [64, 64, 0, 52, 64]},
                       {"name": "star", "rect": [128, 0, 64, 64], "metrics": [64, 64, 4, 56, 72], "scale": 1.5},
                       {"name": "heart", "unicode": 0x2665, "rect": [192, 0, 64, 64], "metrics": [64, 64, 0, 52, 64]}]}


def rt(name, text, props=None, rect=(600, 300), **kw):
    case("rt_" + name, text, {**RT, **(props or {})}, rect=rect, **kw)


rt("align", "<align=left>Left</align>\n<align=center>Center</align>\n<align=right>Right</align>\n"
   "<align=justified>Justified text that wraps onto a second line here and more</align>\n"
   "<align=flush>Flush text that wraps onto another line too</align>\nafter", rect=(400, 500))
rt("allcaps", "Mixed <allcaps>All Caps Text</allcaps> back <uppercase>upper</uppercase> <lowercase>LOWER Case</lowercase> end")
rt("alpha", "<alpha=#FF>Opaque <alpha=#80>half <alpha=#20>faint</alpha> done <color=#FF000080>red<alpha=#FF>full</color> z")
rt("color", "<color=#F00>R3</color> <color=#0F08>G4</color> <color=#0000FF>B6</color> <color=#FFFF0080>Y8</color> "
   "<color=red>red</color> <color=\"blue\">blue</color> <#00FFFF>cyan</color> <#F0F>mag</color> <color=orange>o</color>"
   "<color=purple>p</color><color=black>k</color><color=white>w</color><color=green>g</color><color=yellow>y</color> "
   "<color=#12345>bad</color> <color=#ABCDEF12>a</color>")
rt("cspace", "<cspace=10>Spaced</cspace> <cspace=1em>em</cspace> <cspace=-5>tight</cspace> normal <cspace=0.5em>wrap this long spaced run</cspace>")
rt("font", "Inter <font=\"NotoJP\">Noto 日本語 text</font> Inter <font=NotoJP>again</font> <font=\"Nope\">unknown</font> end",
   fonts=NOTO)
rt("font_weight", "<font-weight=700>Bold weight</font-weight> <font-weight=300>light</font-weight> <font-weight=900>black</font-weight> normal")
rt("gradient", "<gradient=\"Fire\">Gradient text</gradient> plain <gradient=Sea>sea</gradient>",
   gradients={"Fire": ["#FF0000", "#FFFF00", "#800000", "#FF8000"], "Sea": ["#00FFFF", "#0000FF", "#004080", "#00FF80"]})
rt("bold_italic", "<b>Bold text</b> <i>Italic text</i> <b><i>Both</i></b> <B>caps tag</B> plain")
rt("br", "one<br>two<br><br>four <br/>five")
rt("indent", "<indent=15%>Indented paragraph that wraps across several lines to show the hanging.</indent> Back to normal\n"
   "Next <indent=40>px indent here and it wraps around too</indent> x <indent=1em>em</indent>", rect=(500, 400))
rt("line_height", "Line one<line-height=50%>\nLine two\nLine three</line-height>\nLine four<line-height=80>\nfive<line-height=2em>\nsix</line-height>\nseven",
   rect=(400, 500))
rt("line_indent", "<line-indent=15%>This is the first line of a paragraph that wraps.\nAnd a second paragraph, which also wraps around.</line-indent>\nNo indent",
   rect=(400, 400))
rt("link", "Go to <link=\"id_a\">first link</link> or <link=id_b>second <b>bold</b></link> or <a href=\"x\">anchor</a>. "
   "<link=\"wrap\">A link that wraps across a line break here</link> end", rect=(400, 300))
rt("margin", "<margin=2em>Both margins of this wrapped text are set</margin>\n<margin-left=20%>Left margin text that wraps here</margin-left>\n"
   "<margin-right=100>Right margin text also wraps around</margin-right>\nplain line", rect=(450, 500))
rt("mark", "Normal <mark=#ffff00aa>marked text</mark> and <mark=#00ff0080 padding=\"10,20,5,5\">padded mark</mark> <mark>default</mark> end")
rt("mspace", "<mspace=20>Monospace 1.5</mspace> <mspace=0.8em>Em mono</mspace> back <mspace=2.5em>W i</mspace>")
rt("nobr", "Some text where <nobr>this phrase must not break</nobr> even when wrapping is needed here", rect=(300, 300))
rt("noparse", "<noparse><b>not bold</b> <color=red></noparse> <b>bold</b> <noparse></noparse>x")
rt("page", "Page one text here<page>Page two text<page>Three", {"overflowMode": "Page", "pageToDisplay": 2}, rect=(400, 200))
rt("page_first", "Page one text here<page>Page two text<page>Three", {"overflowMode": "Page"}, rect=(400, 200))
rt("pos", "Name<pos=30%>Value<pos=60%>More\nA<pos=100>B<pos=2em>C<pos=10>back", rect=(500, 200))
rt("rotate", "<rotate=45>Rot</rotate> <rotate=-30>neg</rotate> x <rotate=90>V</rotate>")
rt("strike_under", "<s>strike</s> <strikethrough>also</strikethrough> <u>under line</u> <underline>also</underline> "
   "<u><s>both</s></u> <u color=#FF0000>red u</u> <s color=#00FF00>green s</s> <size=50><u>big</u></size>")
rt("size", "<size=50>px50</size> <size=2em>2em</size> <size=50%>half</size> <size=+10>plus</size> <size=-10>minus</size> "
   "<size=+1em>pem</size> <size=150%><size=50%>nest</size></size>", rect=(900, 300))
rt("space", "A<space=20>B<space=2em>C<space=-10>D <space=50%>E")
rt("sprite", "Icons <sprite=0> <sprite index=1> <sprite name=\"star\"> <sprite=3 tint=1> <sprite name=\"heart\" color=#00FF0080> "
   "<sprite=\"TypesetterSprites\" index=2> ♥ end", spriteAsset=SPRITES, rect=(700, 200))
rt("sprite_face", "Icons <sprite=0> <size=60><sprite=1></size> text", spriteAsset={**SPRITES, "faceInfo": {
    "pointSize": 64, "scale": 1, "ascentLine": 56, "descentLine": -8, "lineHeight": 64, "baseline": 0}}, rect=(700, 200))
rt("style", "<style=H1>Heading</style> <style=\"Quote\">quoted text that may wrap</style> <style=Link>link</style> "
   "<style=C1>c1</style> <style=H2>h2</style> <style=C3>c3</style> <style=Title>Title</style> <style=Unknown>unk</style> end",
   rect=(500, 500))
rt("sub_sup", "H<sub>2</sub>O E=mc<sup>2</sup> x<sup>a<sup>b</sup></sup> y<sub>i<sub>j</sub></sub> <size=50>S<sup>p</sup></size>")
rt("smallcaps", "<smallcaps>Small Caps Text</smallcaps> Normal <smallcaps>éà 12</smallcaps>")
rt("voffset", "Base <voffset=10>up</voffset> <voffset=-0.5em>down</voffset> <voffset=1em>em</voffset> end")
rt("width", "<width=60%>This text is limited to sixty percent of the width and wraps</width>\nFull width line here that also wraps around\n"
   "<width=200>Two hundred px wide text</width>", rect=(500, 500))
rt("zwsp", "Long<zwsp>Compound<zwsp>Word<zwsp>Joined<zwsp>Together forever", rect=(250, 300))
rt("nbsp_tag", "Keep<nbsp>these<nbsp>words<nbsp>together and these break", rect=(250, 300))
rt("shy_tag", "Extra<shy>ordi<shy>nary hyphen<shy>ation in<shy>side<shy>words", rect=(220, 300))
rt("escapes", "Tab\\there\\nNew line\\r\\nCRLF \\u00E9\\u00C9 \\U0001F600 back\\\\slash \\x", rect=(600, 300))
rt("nested", "<b><i>bi</b> i only</i> <color=red><size=40>A<color=blue>B</color>C</size>D</color> <size=40><b>x</size>y</b> "
   "<u><s>us</u>s</s> </b>orphan</i>")
rt("unknown", "<foo>bar</foo> <b=1>x</b> < spaced > <color>nocolor</color> <size>nosize</size> <#zzz>bad <> a<b c> </")
rt("upper_lower_nest", "<lowercase>LOW <uppercase>up</uppercase> LOW</lowercase> <smallcaps>Sm <b>Bo</b></smallcaps>")

# --- text processing
CJK_JP = "吾輩は猫である。名前はまだ無い。どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。"
CJK_JP2 = "「こんにちは」と彼は言った。（括弧）や『二重』、そして…！？ーーー終わり。ッァィゥェォャュョ「始まり」"
CJK_ZH = "我能吞下玻璃而不伤身体。这是一个测试，包括标点符号：逗号、句号。"
CJK_MIX = "Typesetter は TextMesh Pro と同じ結果を出すべきです。English words mixed with 日本語の文章、and more text."
JP = {"font": "NotoSansJP-Subset.ttf"}


def tp(name, text, props=None, rect=(400, 300), **kw):
    case("tp_" + name, text, {**RT, **(props or {})}, rect=rect, **kw)


tp("cjk_jp", CJK_JP, {"fontSize": 24}, rect=(300, 400), **JP)
tp("cjk_jp_punct", CJK_JP2, {"fontSize": 24}, rect=(250, 400), **JP)
tp("cjk_zh", CJK_ZH, {"fontSize": 24}, rect=(260, 300), **JP)
tp("cjk_mixed", CJK_MIX, {"fontSize": 24}, rect=(300, 400), **JP)
tp("cjk_fallback", "Inter text with 日本語のフォールバック文字、混在。", {"fontSize": 24}, rect=(300, 300), fonts=NOTO, fallbacks=["NotoJP"])
tp("missing_glyph", "Inter lacks 日本 and ☃ \U0001F600 chars", {"fontSize": 24}, rect=(500, 200))
SHY, NBSP, HYPH, FIGSP, NNBSP, LSEP, PSEP, VT, ZWSP, CR = (chr(x) for x in (0xAD, 0xA0, 0x2010, 0x2007, 0x202F, 0x2028, 0x2029, 0x0B, 0x200B, 0x0D))
tp("hyphen", "well-known state-of-the-art hyphen-separated words and en" + HYPH + "dash", {"fontSize": 24}, rect=(200, 400))
tp("soft_hyphen", SHY.join(["Ex", "tra", "or", "di", "nary hy", "phen", "ation in", "side long", "er words"]), {"fontSize": 24}, rect=(180, 400))
tp("nbsp", NBSP.join(["word", "word", "word and more", "words here"]) + FIGSP + "figure" + NNBSP + "narrow", {"fontSize": 24}, rect=(200, 400))
tp("mixed_sizes_wrap", "Small <size=60>BIG</size> small <size=16>tiny words here</size> back to <size=48>large</size> and normal text wraps",
   {"fontSize": 24}, rect=(260, 400))
tp("tabs", "a\tb\tc\n\tindented\tx\nlonger text\tz\n<size=48>\tbig</size>", {"fontSize": 24}, rect=(600, 300))
tp("rtl", "Hello world right to left", {"isRightToLeftText": True}, rect=(500, 200))
tp("rtl_wrap", "Hello world right to left text that wraps", {"isRightToLeftText": True, "fontSize": 24}, rect=(200, 300))
for flag in ("Bold", "Italic", "Underline", "LowerCase", "UpperCase", "SmallCaps", "Strikethrough", "Superscript", "Subscript", "Highlight"):
    tp("style_" + flag.lower(), "Style Flag Test qy 12", {"fontStyle": flag}, rect=(500, 200))
tp("style_combo", "Combined Styles Here", {"fontStyle": "Bold, Italic, Underline, Strikethrough"}, rect=(500, 200))
tp("style_bold_tag_off", "Plain <b>not bold</b> <i>not italic</i> end", {"fontStyle": "Bold, Italic"}, rect=(500, 200))
tp("font_weight", "Font weight bold property", {"fontWeight": "Bold"}, rect=(500, 200))
tp("max_visible_words", LOREM[:80], {"fontSize": 24, "maxVisibleWords": 5}, rect=(320, 200))
tp("first_visible_char", LOREM[:80], {"fontSize": 24, "firstVisibleCharacter": 10}, rect=(320, 200))
tp("page_to_display", LOREM, {"fontSize": 24, "overflowMode": "Page", "pageToDisplay": 2}, rect=(260, 100))
tp("page_to_display3", LOREM, {"fontSize": 24, "overflowMode": "Page", "pageToDisplay": 3, "alignment": "Center"}, rect=(260, 100))
tp("linked", LOREM, {"fontSize": 24, "overflowMode": "Linked"}, rect=(260, 100), linked={"rect": [260, 300]})
tp("max_desc_off", LOREM[:80], {"fontSize": 24, "maxVisibleCharacters": 20, "useMaxVisibleDescender": False}, rect=(320, 200))
tp("max_desc_lines", LOREM[:80], {"fontSize": 24, "maxVisibleLines": 1, "useMaxVisibleDescender": True, "alignment": "Bottom"}, rect=(320, 200))
for r in (0.0, 1.0):
    tp("wwr_%d" % int(r * 100), LOREM[:90], {"fontSize": 24, "alignment": "TopJustified", "wordWrappingRatios": r}, rect=(320, 240))
tp("autosize_adj", LOREM, {"enableAutoSizing": True, "fontSizeMin": 8, "fontSizeMax": 72, "characterWidthAdjustment": 20,
                           "lineSpacingAdjustment": -20}, rect=(300, 120))
tp("autosize_charw", LOREM[:40], {"enableAutoSizing": True, "fontSizeMin": 30, "fontSizeMax": 72, "characterWidthAdjustment": 30,
                                  "textWrappingMode": "NoWrap"}, rect=(300, 120))
tp("parse_ctrl_off", "Literal \\n and \\t here", {"parseCtrlCharacters": False}, rect=(500, 200))
tp("richtext_off", "<b>tags</b> <color=red>shown</color> \\n", {"richText": False}, rect=(600, 200))
tp("control_chars", "line1" + CR + "\nline2" + CR + "line3" + LSEP + "sep" + PSEP + "para" + VT + "vt" + ZWSP + "zw", {"fontSize": 24}, rect=(400, 400))
tp("char_spacing_wrap", "Character spacing with wrapping across lines", {"fontSize": 24, "characterSpacing": 20}, rect=(260, 300))

RSQUO = chr(0x2019)
tp("words_start_space", " hello world  ", {}, rect=(500, 200))
tp("words_start_dot", ".hello world. (x) 3.14 a_b", {}, rect=(500, 200))
tp("words_apos", "don't stop it's rock" + RSQUO + "n" + RSQUO + "roll 'quoted' end.", {}, rect=(600, 200))
tp("bold_pad5", "<b>Bold</b> x <b>W</b>", {"fontSize": 48, "textWrappingMode": "NoWrap"}, rect=(400, 100),
   asset={"pointSize": 48, "padding": 5, "renderMode": "SDFAA", "atlas": [512, 512]})
tp("italic_sizes", "<i>Ab<size=60>Cd</size><sup>e</sup><voffset=10>f</voffset></i>", {}, rect=(500, 200))

CLOSE = ["pos", "space", "page", "br", "sprite", "zwsp", "nbsp", "shy", "width", "rotate", "gradient", "font-weight", "a", "mark",
         "cspace", "mspace", "voffset", "indent", "line-height", "line-indent", "align", "allcaps", "uppercase", "lowercase",
         "smallcaps", "nobr", "sup", "sub", "material", "link", "style", "font", "size", "color", "u", "s", "i", "b", "noparse",
         "margin", "alpha", "action", "fade", "zwj", "underline", "strikethrough", "margin-left", "margin-right", "foo"]
tp("close_tags", "x".join("</%s>" % t for t in CLOSE) + "x", {"fontSize": 16}, rect=(900, 400))
OPEN_BAD = ["size=abc", "cspace=x", "pos=a", "voffset", "align=foo", "rotate=x", "font-weight=abc", "width", "indent", "mspace",
            "line-height", "line-indent=q", "margin", "mark=zz", "alpha=#GG", "alpha=80", "sprite", "space", "link", "a", "font",
            "style", "gradient", "u=1", "i=2", "page=3", "br=1", "nobr=no", "size=", "size=10px", "size=1.5em", "color=#1234567",
            "color=#FFF5", "color=redd", "pos=10px", "cspace=2px", "B", "COLOR=red", "Size=40", "sup=1"]
tp("open_bad_values", "|".join("<%s>" % t for t in OPEN_BAD) + "|end", {"fontSize": 16}, rect=(900, 400))
tp("tags_spacing", "<b >a</b > < b>c</ b> <size = 40>d</size> <color='red'>e</color> <size=\"40\">f</size> <color=#F00 >g</color>", {"fontSize": 24}, rect=(700, 200))

for pct in (1, 4, 49, 51, 6):
    w = 417.83 / (1 + (pct / 1000 if pct > 9 else pct / 100))
    tp("justify_fit_%d" % pct, "Justified text that wraps onto the next line", {"alignment": "TopJustified"}, rect=(round(w, 2), 300))
tp("justify_fit_left", "Justified text that wraps onto the next line", {"alignment": "TopLeft"}, rect=(410, 300))
rt("page_multiline", "Line one of page one\nline two<page>Page two first\nsecond<page>Three", {"overflowMode": "Page", "pageToDisplay": 2}, rect=(400, 200))
rt("page_overflow_mode", "Page one text<page>Page two text<page>Three", {}, rect=(400, 200))

PREF_PROBES = ["<b=1>x</b>", "<b c>x", "<#zzz>x", "<color>x</color>", "<size>x</size>", "<foo>x</foo>", "< spaced >", "<>x", "x</",
               "<u=1>x", "<i=2>x", "<page=3>x", "<nobr=no>x", "<B>x", "<sup=1>x", "<COLOR=red>x", "<Size=40>x", "<size=10px>x",
               "<pos=10px>x", "<cspace=2px>x", "<size=1.5em>x", "<color=#FFF5>x", "<link>x</link>", "<a>x</a>", "<margin>x",
               "<mark=zz>x", "<alpha=#GG>x", "<rotate=x>x", "<size=abc>x", "<cspace=x>x", "<pos=a>x", "<font-weight=abc>x",
               "<line-indent=q>x", "<b>x</b>", "<size=40>x</size>", "<voffset>x", "<width>x", "<alpha=80>x", "<sprite>x", "<style>x"]
for k, t in enumerate(PREF_PROBES):
    tp("pref_%02d" % k, "a" + t + "a", {"fontSize": 20, "textWrappingMode": "NoWrap"}, rect=(900, 100))

UNK = ["<foo>bar</foo> ", "<b=1>x</b> ", "< spaced > ", "<color>nocolor</color> ", "<size>nosize</size> ", "<#zzz>bad ", "<> ", "a<b c> ", "</"]
for k in range(1, len(UNK) + 1):
    tp("prefctx_%d" % k, "".join(UNK[:k]), {"fontSize": 30, "textWrappingMode": "NoWrap"}, rect=(1500, 100))

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
