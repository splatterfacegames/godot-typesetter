#!/usr/bin/env python3
"""Generates the TMP_Text API of Typesetter's text nodes and the API map.

    python tools/make_api.py

Reads tools/api/text_common.gd.in (the shared TMP_Text surface: properties,
read-only properties, methods) and tools/api/<node>.gd.in (node-specific code),
and writes addons/typesetter/runtime/<node>.gd plus docs/api-map.md.

Template directives:
  #@group Name
  #@prop name: Type = default | export | dirty | core | TMP name | doc
      dirty: comma list of layout, vertices, material, text, watch, none
      core:  "=" copies the value into TypesetterTextCore.<name>; "-" does not
  #@ro name: Type | TMP name | doc     (getter _ts_get_<name>, defined below it)
  #@rw name: Type | TMP name | doc     (getter and setter _ts_get/_ts_set_<name>)
"""
import os
import re

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = os.path.join(REPO, "tools", "api")
OUT = os.path.join(REPO, "addons", "typesetter", "runtime")
NODES = [
    # (template, output, TMP class, Typesetter class)
    ("label.gd.in", "typesetter_label.gd", "TextMeshProUGUI", "TypesetterLabel"),
    ("text_3d.gd.in", "typesetter_text_3d.gd", "TextMeshPro", "TypesetterText3D"),
]
KIND = {"layout": 1, "vertices": 2, "material": 4, "text": 8, "watch": 0, "none": 0}
RESOURCE_TYPES = ("TypesetterFontAsset", "TypesetterVertexGradient", "TypesetterColorGradient",
                  "TypesetterSpriteAsset", "TypesetterStyleSheet", "TypesetterStyle")


def expand(src, api_rows):
    out = []
    core = []
    for line in src.splitlines():
        if line.startswith("#@group"):
            name = line[len("#@group"):].strip()
            out.append('@export_group("%s")' % name if name else '@export_group("")')
            continue
        m = re.match(r"#@prop (\w+): ([\w.]+) = (.*?) \| (.*?) \| (.*?) \| (.*?) \| (.*?) \| (.*)$", line)
        if m:
            name, typ, default, export, dirty, corev, tmp, doc = [x.strip() for x in m.groups()]
            kinds = [k.strip() for k in dirty.split(",")]
            mask = sum(KIND[k] for k in kinds)
            for d in doc.split("\\n"):
                out.append("## " + d)
            out.append("## TMP: " + tmp)
            ann = "" if export == "-" else export + " "
            out.append("%svar %s: %s = %s:" % (ann, name, typ, default))
            out.append("\tset(value):")
            out.append("\t\tif %s == value:" % name)
            out.append("\t\t\treturn")
            if "watch" in kinds:
                out.append("\t\t_ts_watch(%s, value)" % name)
            out.append("\t\t%s = value" % name)
            out.append("\t\t_ts_on_changed(&\"%s\", %d)" % (name, mask))
            if corev == "=":
                core.append(name)
            api_rows.append((tmp, name, typ, "property", doc))
            continue
        m = re.match(r"#@(ro|rw) (\w+): ([\w.]+) \| (.*?) \| (.*)$", line)
        if m:
            mode, name, typ, tmp, doc = [x.strip() for x in m.groups()]
            out.append("## " + doc)
            out.append("## TMP: " + tmp)
            if mode == "ro":
                out.append("var %s: %s:" % (name, typ))
                out.append("\tget:")
                out.append("\t\treturn _ts_get_%s()" % name)
                out.append("\tset(_value):")
                out.append('\t\tpush_error("%s is read-only")' % name)
            else:
                out.append("var %s: %s:" % (name, typ))
                out.append("\tget:")
                out.append("\t\treturn _ts_get_%s()" % name)
                out.append("\tset(value):")
                out.append("\t\t_ts_set_%s(value)" % name)
            out.append("")
            api_rows.append((tmp, name, typ, "property (read-only)" if mode == "ro" else "property", doc))
            continue
        out.append(line)
    # Methods for the API map: "## TMP: Name(...)" right above "func name(".
    lines = src.splitlines()
    for i, line in enumerate(lines):
        mm = re.match(r"func ([a-z]\w*)\(", line)
        if mm and i > 0:
            j = i - 1
            tmp = None
            doc = []
            while j >= 0 and lines[j].startswith("##"):
                t = lines[j][2:].strip()
                if t.startswith("TMP:"):
                    tmp = t[4:].strip()
                else:
                    doc.insert(0, t)
                j -= 1
            if tmp:
                api_rows.append((tmp, mm.group(1) + "()", "", "method", " ".join(doc)))
    return "\n".join(out) + "\n", core


def main():
    common = open(os.path.join(API, "text_common.gd.in"), encoding="utf-8").read()
    api_rows = []
    common_out = None
    for tpl, outname, tmp_cls, ts_cls in NODES:
        rows = []
        node = open(os.path.join(API, tpl), encoding="utf-8").read()
        node_out, core_a = expand(node, rows)
        c, core_b = expand(common, rows)
        c += "\n\nfunc _ts_sync_core_generated(c: TypesetterTextCore) -> void:\n"
        body = "".join("\tc.%s = %s\n" % (n, n) for n in core_a + core_b)
        c += body if body else "\tpass\n"
        common_out = c
        head, sep, tail = node_out.partition("# @@COMMON@@\n")
        if not sep:
            raise SystemExit("%s lacks the # @@COMMON@@ marker" % tpl)
        text = head + c + "\n\n" + tail
        text = re.sub(r"\n{3,}", "\n\n\n", text)
        with open(os.path.join(OUT, outname), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        api_rows.append(("__node__", tmp_cls, ts_cls, "", ""))
        api_rows.extend([r for r in rows if r not in api_rows])
        print("wrote", outname)
    write_map(api_rows)


def write_map(rows):
    md = ["# API map: TextMesh Pro → Typesetter", "",
          "Generated by `tools/make_api.py` from the node templates. Every public TMP_Text",
          "member is listed with its Typesetter name. Naming rule: TMP's PascalCase/camelCase",
          "identifiers become snake_case, `TMP_`/`TextMeshPro` class prefixes become `Typesetter`.", "",
          "## Classes", "",
          "| TextMesh Pro | Typesetter |", "|---|---|"]
    classes = [
        ("TextMeshProUGUI", "TypesetterLabel (Control)"), ("TextMeshPro", "TypesetterText3D (GeometryInstance3D)"),
        ("TMP_Text", "shared API of both nodes; engine in TypesetterTextCore"),
        ("TMP_FontAsset", "TypesetterFontAsset"), ("FaceInfo", "TypesetterFaceInfo"),
        ("Glyph / TMP_Character", "TypesetterGlyph / TypesetterCharacter"),
        ("TMP_SpriteAsset", "TypesetterSpriteAsset"), ("TMP_SpriteGlyph", "TypesetterSpriteGlyph"),
        ("TMP_SpriteCharacter", "TypesetterSpriteCharacter"), ("TMP_SpriteAnimator", "TypesetterSpriteAnimator"),
        ("TMP_StyleSheet", "TypesetterStyleSheet"), ("TMP_Style", "TypesetterStyle"),
        ("TMP_ColorGradient", "TypesetterColorGradient"), ("VertexGradient", "TypesetterVertexGradient"),
        ("TMP_Settings", "TypesetterSettings (ProjectSettings `typesetter/*`)"),
        ("TMP_TextInfo", "TypesetterTextInfo"), ("TMP_CharacterInfo", "TypesetterCharacterInfo"),
        ("TMP_WordInfo", "TypesetterWordInfo"), ("TMP_LineInfo", "TypesetterLineInfo"),
        ("TMP_LinkInfo", "TypesetterLinkInfo"), ("TMP_PageInfo", "TypesetterPageInfo"),
        ("TMP_MeshInfo", "TypesetterMeshInfo"), ("TMP_TextUtilities", "TypesetterTextUtilities"),
        ("TMP_TextEventHandler", "TypesetterTextEventHandler"), ("TMP_InputField", "TypesetterInputField"),
        ("TMP_InputValidator", "TypesetterInputValidator"), ("TMP_Dropdown", "TypesetterDropdown"),
        ("TMP_MaterialManager", "TypesetterMaterialManager"), ("ShaderUtilities", "TypesetterShaderUtilities"),
        ("TMPro_EventManager / static events", "TypesetterEvents (TypesetterSettings.events)"),
        ("ITextPreprocessor", "any Object with preprocess_text(String) -> String"),
        ("TMP_FontAssetUtilities", "TypesetterFontAsset methods"),
        ("Font Asset Creator window", "Typesetter Font Asset Creator dock (editor)"),
        ("Distance Field / Mobile / Overlay / Bitmap / Sprite shaders", "typesetter_*.gdshader"),
    ]
    for a, b in classes:
        md.append("| `%s` | `%s` |" % (a, b))
    current = None
    seen = set()
    for tmp, name, typ, kind, doc in rows:
        if tmp == "__node__":
            current = (name, typ)
            md += ["", "## %s → %s" % (name, typ), "", "| TMP | Typesetter | Kind | Notes |", "|---|---|---|---|"]
            seen = set()
            continue
        key = (tmp, name)
        if key in seen:
            continue
        seen.add(key)
        md.append("| `%s` | `%s` | %s | %s |" % (tmp, name, kind, doc.replace("|", "\\|")))
    with open(os.path.join(REPO, "docs", "api-map.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(md) + "\n")
    print("wrote docs/api-map.md")


if __name__ == "__main__":
    main()
