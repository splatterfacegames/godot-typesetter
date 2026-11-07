// Black-box ground truth for Typesetter.
//
// Drives TextMesh Pro (com.unity.ugui) through its public API only: creates font
// assets with TMP_FontAsset.CreateFontAsset, lays out text cases with
// TextMeshProUGUI / TextMeshPro, and writes what Unity produces — textInfo
// (every public field of the character / word / line / link / page structs),
// preferred and rendered values, mesh vertices, generated atlas textures and
// glyph tables, and optional PNG renders. No TextMesh Pro source is read.
//
// Run through tools/unity-dump/run.py, which stages fonts and cases and calls:
//   Unity.exe -batchmode -projectPath tools/unity-dump -executeMethod TypesetterDump.Setup -quit
//   Unity.exe -batchmode -projectPath tools/unity-dump -executeMethod TypesetterDump.Run
//             -dumpCases Assets/Cases -dumpOut <dir> [-render] -quit
//
// A case is Assets/Cases/<name>.json:
//   { "font": "Inter-Regular.ttf",
//     "asset": { "pointSize": 90, "padding": 9, "renderMode": "SDFAA", "atlas": [1024, 1024] },
//     "component": "ugui" | "3d",
//     "rect": [400, 200],            // RectTransform size (pixels for ugui, units for 3d)
//     "pivot": [0.5, 0.5],
//     "text": "Hello",
//     "props": { "fontSize": 36, "alignment": "TopLeft", ... },   // TMP_Text property names
//     "preferred": [[400, 0], [0, 0]],                             // GetPreferredValues(w, h) queries
//     "material": { "ID_OutlineWidth": 0.2, "ID_OutlineColor": "#FF0000FF" },
//     "keywords": ["Keyword_Outline"],
//     "render": { "size": [512, 256], "pixelsPerUnit": 10, "background": "#000000FF" } }

using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using TMPro;
using UnityEditor;
using UnityEngine;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

public static class TypesetterDump
{
    static string Arg(string name, string def = null)
    {
        var a = Environment.GetCommandLineArgs();
        for (int i = 0; i < a.Length - 1; i++) if (a[i] == name) return a[i + 1];
        return def;
    }

    static bool Flag(string name) => Environment.GetCommandLineArgs().Contains(name);

    // Runs without -quit: the essential-resources import is asynchronous, so the
    // editor exits from the import callbacks (or a timeout) instead.
    public static void Setup()
    {
        if (Resources.Load<TMP_Settings>("TMP Settings") != null)
        {
            Debug.Log("[typesetter-dump] setup done; settings present: True");
            EditorApplication.Exit(0);
            return;
        }
        Debug.Log("[typesetter-dump] importing TMP essential resources");
        AssetDatabase.importPackageCompleted += _ => Finish();
        AssetDatabase.importPackageFailed += (_, err) => { Debug.LogError("[typesetter-dump] import failed: " + err); EditorApplication.Exit(1); };
        double start = EditorApplication.timeSinceStartup;
        EditorApplication.update += () =>
        {
            if (Resources.Load<TMP_Settings>("TMP Settings") != null) Finish();
            else if (EditorApplication.timeSinceStartup - start > 600) { Debug.LogError("[typesetter-dump] import timed out"); EditorApplication.Exit(1); }
        };
        TMP_PackageResourceImporter.ImportResources(true, false, false);
    }

    static void Finish()
    {
        AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
        bool ok = Resources.Load<TMP_Settings>("TMP Settings") != null;
        Debug.Log("[typesetter-dump] setup done; settings present: " + ok);
        EditorApplication.Exit(ok ? 0 : 1);
    }

    static readonly Dictionary<string, TMP_FontAsset> assets = new Dictionary<string, TMP_FontAsset>();

    public static void Run()
    {
        string casesDir = Arg("-dumpCases", "Assets/Cases");
        string outDir = Arg("-dumpOut", "Dump");
        bool render = Flag("-render");
        string only = Arg("-only", "");
        Directory.CreateDirectory(outDir);
        Directory.CreateDirectory(Path.Combine(outDir, "atlases"));
        ShaderUtilities.GetShaderPropertyIDs();
        File.WriteAllText(Path.Combine(outDir, "_enums.json"), Json.Write(DumpEnums()));
        int ok = 0, failed = 0;
        foreach (var path in Directory.GetFiles(casesDir, "*.json").OrderBy(p => p))
        {
            string name = Path.GetFileNameWithoutExtension(path);
            if (only.Length > 0 && !only.Split(',').Contains(name)) continue;
            try
            {
                var c = (Dictionary<string, object>)Json.Parse(File.ReadAllText(path));
                var result = RunCase(name, c, outDir, render);
                File.WriteAllText(Path.Combine(outDir, name + ".json"), Json.Write(result));
                ok++;
            }
            catch (Exception e)
            {
                failed++;
                Debug.LogError("[typesetter-dump] case " + name + " failed: " + e);
                File.WriteAllText(Path.Combine(outDir, name + ".json"), Json.Write(new Dictionary<string, object> { { "error", e.ToString() } }));
            }
        }
        foreach (var kv in assets) DumpAsset(kv.Key, kv.Value, outDir);
        Debug.Log("[typesetter-dump] done: " + ok + " ok, " + failed + " failed");
    }

    // ---------------------------------------------------------------- assets

    static TMP_FontAsset GetAsset(Dictionary<string, object> c, out string key)
    {
        string font = Str(c, "font", "Inter-Regular.ttf");
        var a = c.ContainsKey("asset") ? (Dictionary<string, object>)c["asset"] : new Dictionary<string, object>();
        int pt = (int)Num(a, "pointSize", 90);
        int pad = (int)Num(a, "padding", 9);
        string mode = Str(a, "renderMode", "SDFAA");
        var atlas = a.ContainsKey("atlas") ? (List<object>)a["atlas"] : new List<object> { 1024.0, 1024.0 };
        int w = (int)(double)atlas[0], h = (int)(double)atlas[1];
        string pop = Str(a, "population", "Dynamic");
        key = string.Format("{0}_{1}_{2}_{3}_{4}x{5}", Path.GetFileNameWithoutExtension(font), pt, pad, mode, w, h);
        if (assets.TryGetValue(key, out var fa)) return fa;
        var f = AssetDatabase.LoadAssetAtPath<Font>("Assets/Fonts/" + font);
        if (f == null) throw new Exception("font not found: Assets/Fonts/" + font);
        var rm = (GlyphRenderMode)Enum.Parse(typeof(GlyphRenderMode), mode);
        var apm = (AtlasPopulationMode)Enum.Parse(typeof(AtlasPopulationMode), pop);
        fa = TMP_FontAsset.CreateFontAsset(f, pt, pad, rm, w, h, apm, true);
        fa.name = key;
        if (a.ContainsKey("characters")) fa.TryAddCharacters((string)a["characters"], out string _, Bool(a, "includeFontFeatures", true));
        assets[key] = fa;
        return fa;
    }

    static void DumpAsset(string key, TMP_FontAsset fa, string outDir)
    {
        var d = new Dictionary<string, object>();
        d["faceInfo"] = Fields(fa.faceInfo);
        d["atlasWidth"] = fa.atlasWidth;
        d["atlasHeight"] = fa.atlasHeight;
        d["atlasPadding"] = fa.atlasPadding;
        d["atlasRenderMode"] = fa.atlasRenderMode.ToString();
        d["atlasCount"] = fa.atlasTextures != null ? fa.atlasTextures.Length : 0;
        var glyphs = new List<object>();
        foreach (var g in fa.glyphTable)
        {
            glyphs.Add(new Dictionary<string, object>
            {
                { "index", g.index },
                { "width", g.metrics.width }, { "height", g.metrics.height },
                { "bearingX", g.metrics.horizontalBearingX }, { "bearingY", g.metrics.horizontalBearingY },
                { "advance", g.metrics.horizontalAdvance },
                { "rect", new List<object> { g.glyphRect.x, g.glyphRect.y, g.glyphRect.width, g.glyphRect.height } },
                { "scale", g.scale }, { "atlasIndex", g.atlasIndex },
            });
        }
        d["glyphs"] = glyphs;
        var chars = new List<object>();
        foreach (var ch in fa.characterTable)
            chars.Add(new List<object> { ch.unicode, ch.glyphIndex });
        d["characters"] = chars;
        d["fontFeatureTable"] = Json.Parse(JsonUtility.ToJson(fa.fontFeatureTable));
        d["materialProperties"] = MaterialFloats(fa.material);
        File.WriteAllText(Path.Combine(outDir, "atlases", key + ".json"), Json.Write(d));
        if (fa.atlasTextures != null)
        {
            for (int i = 0; i < fa.atlasTextures.Length; i++)
            {
                var t = fa.atlasTextures[i];
                if (t == null || t.width <= 1) continue;
                var px = t.GetPixels32();
                var rgba = new Texture2D(t.width, t.height, TextureFormat.RGBA32, false);
                // Alpha8 atlases: expand alpha into RGB so the PNG is viewable; alpha kept.
                if (t.format == TextureFormat.Alpha8)
                    for (int k = 0; k < px.Length; k++) px[k] = new Color32(px[k].a, px[k].a, px[k].a, px[k].a);
                rgba.SetPixels32(px);
                rgba.Apply();
                File.WriteAllBytes(Path.Combine(outDir, "atlases", key + "_" + i + ".png"), rgba.EncodeToPNG());
            }
        }
    }

    static Dictionary<string, object> MaterialFloats(Material m)
    {
        var d = new Dictionary<string, object>();
        if (m == null) return d;
        foreach (var f in typeof(ShaderUtilities).GetFields(BindingFlags.Public | BindingFlags.Static))
        {
            if (!f.Name.StartsWith("ID_") || f.FieldType != typeof(int)) continue;
            int id = (int)f.GetValue(null);
            if (!m.HasProperty(id)) continue;
            try { d[f.Name] = m.GetFloat(id); } catch { }
        }
        return d;
    }

    // ---------------------------------------------------------------- cases

    static Dictionary<string, object> RunCase(string name, Dictionary<string, object> c, string outDir, bool render)
    {
        var fa = GetAsset(c, out string assetKey);
        string comp = Str(c, "component", "ugui");
        var rect = c.ContainsKey("rect") ? (List<object>)c["rect"] : new List<object> { 400.0, 200.0 };
        var pivot = c.ContainsKey("pivot") ? (List<object>)c["pivot"] : new List<object> { 0.5, 0.5 };
        var rnd = c.ContainsKey("render") ? (Dictionary<string, object>)c["render"] : new Dictionary<string, object>();
        var size = rnd.ContainsKey("size") ? (List<object>)rnd["size"] : new List<object> { 512.0, 256.0 };
        int rw = (int)(double)size[0], rh = (int)(double)size[1];
        float ppu = (float)Num(rnd, "pixelsPerUnit", 10);

        var root = new GameObject("case_" + name);
        var camGo = new GameObject("cam");
        camGo.transform.SetParent(root.transform);
        var cam = camGo.AddComponent<Camera>();
        cam.orthographic = true;
        cam.clearFlags = CameraClearFlags.SolidColor;
        cam.backgroundColor = ParseColor(Str(rnd, "background", "#000000FF"));
        cam.nearClipPlane = 0.01f;
        cam.farClipPlane = 100f;
        camGo.transform.position = new Vector3(0, 0, -10);
        var rt = new RenderTexture(rw, rh, 24, RenderTextureFormat.ARGB32);
        cam.targetTexture = rt;
        cam.aspect = (float)rw / rh;

        TMP_Text tmp;
        if (comp == "3d")
        {
            cam.orthographicSize = rh / ppu / 2f;
            var go = new GameObject("text");
            go.transform.SetParent(root.transform);
            var t3 = go.AddComponent<TextMeshPro>();
            tmp = t3;
        }
        else
        {
            cam.orthographicSize = rh / 2f;
            var canvasGo = new GameObject("canvas");
            canvasGo.transform.SetParent(root.transform);
            var canvas = canvasGo.AddComponent<Canvas>();
            canvas.renderMode = RenderMode.ScreenSpaceCamera;
            canvas.worldCamera = cam;
            canvas.planeDistance = 5;
            canvas.pixelPerfect = false;
            var go = new GameObject("text");
            go.transform.SetParent(canvasGo.transform, false);
            tmp = go.AddComponent<TextMeshProUGUI>();
        }
        var rtf = tmp.rectTransform;
        rtf.anchorMin = rtf.anchorMax = new Vector2(0.5f, 0.5f);
        rtf.pivot = new Vector2((float)(double)pivot[0], (float)(double)pivot[1]);
        rtf.sizeDelta = new Vector2((float)(double)rect[0], (float)(double)rect[1]);
        rtf.anchoredPosition = Vector2.zero;
        tmp.font = fa;

        var props = c.ContainsKey("props") ? (Dictionary<string, object>)c["props"] : new Dictionary<string, object>();
        foreach (var kv in props) SetProp(tmp, kv.Key, kv.Value);

        if (c.ContainsKey("material") || c.ContainsKey("keywords"))
        {
            var mat = new Material(tmp.fontSharedMaterial);
            if (c.ContainsKey("keywords"))
                foreach (var k in (List<object>)c["keywords"])
                {
                    var f = typeof(ShaderUtilities).GetField((string)k, BindingFlags.Public | BindingFlags.Static);
                    mat.EnableKeyword(f != null ? (string)f.GetValue(null) : (string)k);
                }
            if (c.ContainsKey("material"))
                foreach (var kv in (Dictionary<string, object>)c["material"])
                {
                    var f = typeof(ShaderUtilities).GetField(kv.Key, BindingFlags.Public | BindingFlags.Static);
                    int id = f != null ? (int)f.GetValue(null) : Shader.PropertyToID(kv.Key);
                    if (kv.Value is string s) mat.SetColor(id, ParseColor(s));
                    else mat.SetFloat(id, (float)(double)kv.Value);
                }
            tmp.fontSharedMaterial = mat;
        }

        tmp.text = Str(c, "text", "");
        Canvas.ForceUpdateCanvases();
        tmp.ForceMeshUpdate(true, true);

        var r = new Dictionary<string, object>();
        r["asset"] = assetKey;
        r["fontSize"] = tmp.fontSize;
        r["preferredWidth"] = tmp.preferredWidth;
        r["preferredHeight"] = tmp.preferredHeight;
        r["renderedWidth"] = tmp.renderedWidth;
        r["renderedHeight"] = tmp.renderedHeight;
        r["getPreferredValues"] = V(tmp.GetPreferredValues());
        if (c.ContainsKey("preferred"))
        {
            var q = new List<object>();
            foreach (List<object> wh in (List<object>)c["preferred"])
                q.Add(new List<object> { wh[0], wh[1], V(tmp.GetPreferredValues((float)(double)wh[0], (float)(double)wh[1])) });
            r["preferredQueries"] = q;
        }
        r["getRenderedValues"] = V(tmp.GetRenderedValues(false));
        r["getRenderedValuesVisible"] = V(tmp.GetRenderedValues(true));
        r["isTextOverflowing"] = tmp.isTextOverflowing;
        r["firstOverflowCharacterIndex"] = tmp.firstOverflowCharacterIndex;
        r["textBounds"] = BoundsD(tmp.textBounds);
        r["bounds"] = BoundsD(tmp.bounds);
        r["rect"] = new List<object> { rtf.rect.x, rtf.rect.y, rtf.rect.width, rtf.rect.height };
        var ti = tmp.textInfo;
        r["characterCount"] = ti.characterCount;
        r["spriteCount"] = ti.spriteCount;
        r["spaceCount"] = ti.spaceCount;
        r["wordCount"] = ti.wordCount;
        r["linkCount"] = ti.linkCount;
        r["lineCount"] = ti.lineCount;
        r["pageCount"] = ti.pageCount;
        r["materialCount"] = ti.materialCount;
        r["characters"] = ti.characterInfo.Take(ti.characterCount).Select(x => (object)Fields(x)).ToList();
        r["words"] = ti.wordInfo.Take(ti.wordCount).Select(x => (object)Fields(x)).ToList();
        r["lines"] = ti.lineInfo.Take(ti.lineCount).Select(x => (object)Fields(x)).ToList();
        r["links"] = ti.linkInfo.Take(ti.linkCount).Select(x => (object)Fields(x)).ToList();
        r["pages"] = ti.pageInfo.Take(ti.pageCount).Select(x => (object)Fields(x)).ToList();
        var meshes = new List<object>();
        for (int i = 0; i < ti.materialCount; i++)
        {
            var mi = ti.meshInfo[i];
            int n = mi.vertexCount;
            meshes.Add(new Dictionary<string, object>
            {
                { "vertexCount", n },
                { "vertices", mi.vertices.Take(n).Select(v => (object)V(v)).ToList() },
                { "uvs0", mi.uvs0.Take(n).Select(v => (object)V(v)).ToList() },
                { "uvs2", mi.uvs2.Take(n).Select(v => (object)V(v)).ToList() },
                { "colors32", mi.colors32.Take(n).Select(v => (object)new List<object> { (int)v.r, (int)v.g, (int)v.b, (int)v.a }).ToList() },
                { "material", mi.material != null ? mi.material.name : null },
            });
        }
        r["meshes"] = meshes;
        r["material"] = MaterialFloats(tmp.fontSharedMaterial);

        if (render)
        {
            Canvas.ForceUpdateCanvases();
            cam.Render();
            var prev = RenderTexture.active;
            RenderTexture.active = rt;
            var tex = new Texture2D(rw, rh, TextureFormat.RGBA32, false);
            tex.ReadPixels(new Rect(0, 0, rw, rh), 0, 0);
            tex.Apply();
            RenderTexture.active = prev;
            File.WriteAllBytes(Path.Combine(outDir, name + ".png"), tex.EncodeToPNG());
            UnityEngine.Object.DestroyImmediate(tex);
        }
        cam.targetTexture = null;
        rt.Release();
        UnityEngine.Object.DestroyImmediate(root);
        return r;
    }

    // ---------------------------------------------------------------- helpers

    static void SetProp(TMP_Text t, string name, object value)
    {
        var p = t.GetType().GetProperty(name, BindingFlags.Public | BindingFlags.Instance);
        if (p == null || !p.CanWrite) throw new Exception("no writable TMP_Text property '" + name + "'");
        p.SetValue(t, Convert(value, p.PropertyType));
    }

    static object Convert(object v, Type t)
    {
        if (t == typeof(string)) return v as string;
        if (t == typeof(bool)) return (bool)v;
        if (t == typeof(float)) return (float)(double)v;
        if (t == typeof(int)) return (int)(double)v;
        if (t.IsEnum) return v is string s ? Enum.Parse(t, s) : Enum.ToObject(t, (int)(double)v);
        if (t == typeof(Color)) return ParseColor((string)v);
        if (t == typeof(Color32)) return (Color32)ParseColor((string)v);
        if (t == typeof(Vector4))
        {
            var l = (List<object>)v;
            return new Vector4((float)(double)l[0], (float)(double)l[1], (float)(double)l[2], (float)(double)l[3]);
        }
        if (t == typeof(Vector2))
        {
            var l = (List<object>)v;
            return new Vector2((float)(double)l[0], (float)(double)l[1]);
        }
        throw new Exception("unsupported property type " + t);
    }

    static Color ParseColor(string s)
    {
        if (ColorUtility.TryParseHtmlString(s, out var c)) return c;
        throw new Exception("bad colour " + s);
    }

    static string Str(Dictionary<string, object> d, string k, string def) => d.ContainsKey(k) ? (string)d[k] : def;
    static double Num(Dictionary<string, object> d, string k, double def) => d.ContainsKey(k) ? (double)d[k] : def;
    static bool Bool(Dictionary<string, object> d, string k, bool def) => d.ContainsKey(k) ? (bool)d[k] : def;

    static List<object> V(Vector2 v) => new List<object> { v.x, v.y };
    static List<object> V(Vector3 v) => new List<object> { v.x, v.y, v.z };
    static List<object> V(Vector4 v) => new List<object> { v.x, v.y, v.z, v.w };
    static Dictionary<string, object> BoundsD(Bounds b) => new Dictionary<string, object> { { "center", V(b.center) }, { "size", V(b.size) } };

    /// Every public instance field, recursively for nested structs; object
    /// references become their name.
    static object Fields(object o, int depth = 0)
    {
        if (o == null) return null;
        var t = o.GetType();
        if (o is string || o is bool) return o;
        if (o is char ch) return ((int)ch).ToString();
        if (t.IsPrimitive) return System.Convert.ToDouble(o, CultureInfo.InvariantCulture);
        if (t.IsEnum) return o.ToString();
        if (o is Vector2 v2) return V(v2);
        if (o is Vector3 v3) return V(v3);
        if (o is Vector4 v4) return V(v4);
        if (o is Color col) return new List<object> { col.r, col.g, col.b, col.a };
        if (o is Color32 c32) return new List<object> { (int)c32.r, (int)c32.g, (int)c32.b, (int)c32.a };
        if (o is UnityEngine.Object uo) return uo != null ? uo.name : null;
        if (o is IList list)
        {
            if (depth > 2) return null;
            var l = new List<object>();
            foreach (var x in list) l.Add(Fields(x, depth + 1));
            return l;
        }
        if (depth > 3) return t.Name;
        var d = new SortedDictionary<string, object>();
        foreach (var f in t.GetFields(BindingFlags.Public | BindingFlags.Instance))
        {
            if (typeof(Delegate).IsAssignableFrom(f.FieldType)) continue;
            try { d[f.Name] = Fields(f.GetValue(o), depth + 1); } catch { }
        }
        foreach (var p in t.GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            if (!p.CanRead || p.GetIndexParameters().Length > 0 || d.ContainsKey(p.Name)) continue;
            if (t.IsValueType == false) continue; // only struct properties (FaceInfo etc.)
            try { d[p.Name] = Fields(p.GetValue(o), depth + 1); } catch { }
        }
        return d;
    }

    static Dictionary<string, object> DumpEnums()
    {
        var d = new Dictionary<string, object>();
        var types = new List<Type>
        {
            typeof(GlyphRenderMode), typeof(TextAlignmentOptions), typeof(HorizontalAlignmentOptions),
            typeof(VerticalAlignmentOptions), typeof(TextOverflowModes), typeof(FontStyles), typeof(FontWeight),
            typeof(TextureMappingOptions), typeof(AtlasPopulationMode), typeof(TMP_TextElementType),
            typeof(TextRenderFlags), typeof(TMP_VertexDataUpdateFlags), typeof(VertexSortingOrder),
            typeof(MaskingTypes), typeof(TagUnitType), typeof(TagValueType), typeof(ColorMode),
        };
        var wrap = Type.GetType("TMPro.TextWrappingModes, Unity.TextMeshPro");
        if (wrap != null) types.Add(wrap);
        foreach (var t in types)
        {
            var e = new Dictionary<string, object>();
            foreach (var n in Enum.GetNames(t)) e[n] = System.Convert.ToInt64(Enum.Parse(t, n));
            d[t.Name] = e;
        }
        return d;
    }
}
