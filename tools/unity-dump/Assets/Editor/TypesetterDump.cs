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
        if (Flag("-probe")) File.WriteAllText(Path.Combine(outDir, "_api.json"), Json.Write(DumpApi()));
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
        File.WriteAllText(Path.Combine(outDir, "_settings.json"), Json.Write(DumpSettings()));
        Debug.Log("[typesetter-dump] done: " + ok + " ok, " + failed + " failed");
    }

    // ---------------------------------------------------------------- assets

    static TMP_FontAsset GetAsset(Dictionary<string, object> c, out string key)
    {
        string font = Str(c, "font", "Inter-Regular.ttf");
        var a = c.ContainsKey("asset") ? (Dictionary<string, object>)c["asset"] : new Dictionary<string, object>();
        // Extra named font assets for <font="name"> (registered with
        // MaterialReferenceManager under the hash of their name).
        var aliases = new Dictionary<string, TMP_FontAsset>();
        if (c.ContainsKey("fonts"))
            foreach (var kv in (Dictionary<string, object>)c["fonts"])
            {
                var spec = (Dictionary<string, object>)kv.Value;
                var sa = spec.ContainsKey("asset") ? (Dictionary<string, object>)spec["asset"] : new Dictionary<string, object>();
                aliases[kv.Key] = MakeAsset(Str(spec, "font", "Inter-Regular.ttf"), sa, kv.Key, out string _);
            }
        var fallbacks = c.ContainsKey("fallbacks") ? ((List<object>)c["fallbacks"]).Select(x => (string)x).ToList() : new List<string>();
        var fa = MakeAsset(font, a, fallbacks.Count > 0 ? "fb_" + string.Join("_", fallbacks) : null, out key);
        if (fallbacks.Count > 0)
            fa.fallbackFontAssetTable = fallbacks.Select(n => aliases[n]).ToList();
        return fa;
    }

    static TMP_FontAsset MakeAsset(string font, Dictionary<string, object> a, string alias, out string key)
    {
        int pt = (int)Num(a, "pointSize", 90);
        int pad = (int)Num(a, "padding", 9);
        string mode = Str(a, "renderMode", "SDFAA");
        var atlas = a.ContainsKey("atlas") ? (List<object>)a["atlas"] : new List<object> { 1024.0, 1024.0 };
        int w = (int)(double)atlas[0], h = (int)(double)atlas[1];
        string pop = Str(a, "population", "Dynamic");
        key = string.Format("{0}_{1}_{2}_{3}_{4}x{5}", Path.GetFileNameWithoutExtension(font), pt, pad, mode, w, h);
        if (alias != null) key = alias.StartsWith("fb_") ? key + "_" + alias : alias;
        if (assets.TryGetValue(key, out var fa)) return fa;
        var f = AssetDatabase.LoadAssetAtPath<Font>("Assets/Fonts/" + font);
        if (f == null) throw new Exception("font not found: Assets/Fonts/" + font);
        var rm = (GlyphRenderMode)Enum.Parse(typeof(GlyphRenderMode), mode);
        var apm = (AtlasPopulationMode)Enum.Parse(typeof(AtlasPopulationMode), pop);
        fa = TMP_FontAsset.CreateFontAsset(f, pt, pad, rm, w, h, apm, true);
        fa.name = key;
        if (alias != null && !alias.StartsWith("fb_"))
        {
            // <font="name"> is looked up by the hash of the upper-cased name
            // (registering twice would also re-add the material, which throws).
            var hp = typeof(TMP_Asset).GetProperty("hashCode", BindingFlags.Public | BindingFlags.Instance);
            if (hp != null && hp.CanWrite) hp.SetValue(fa, Hashes(alias.ToUpperInvariant()).First());
            MaterialReferenceManager.AddFontAsset(fa);
        }
        if (a.ContainsKey("characters")) fa.TryAddCharacters((string)a["characters"], out string _, Bool(a, "includeFontFeatures", true));
        assets[key] = fa;
        return fa;
    }

    // ---------------------------------------------------------------- sprites

    /// Builds a sprite asset at run time from a texture drawn here (coloured
    /// discs and squares; no Unity sprite assets are used). Spec:
    /// { "width", "height", "faceInfo": {...}, "sprites": [{ "name", "unicode",
    ///   "rect": [x, y, w, h], "metrics": [w, h, bx, by, adv], "scale" }] }.
    static TMP_SpriteAsset MakeSpriteAsset(Dictionary<string, object> spec, string outDir)
    {
        int tw = (int)Num(spec, "width", 256), th = (int)Num(spec, "height", 128);
        var tex = new Texture2D(tw, th, TextureFormat.RGBA32, false);
        var px = new Color32[tw * th];
        var sprites = (List<object>)spec["sprites"];
        var palette = new[] { new Color32(255, 80, 80, 255), new Color32(80, 200, 80, 255), new Color32(80, 120, 255, 255), new Color32(255, 200, 40, 255) };
        for (int i = 0; i < sprites.Count; i++)
        {
            var r = (List<object>)((Dictionary<string, object>)sprites[i])["rect"];
            int rx = (int)(double)r[0], ry = (int)(double)r[1], rw = (int)(double)r[2], rh = (int)(double)r[3];
            for (int y = 0; y < rh; y++)
                for (int x = 0; x < rw; x++)
                {
                    float dx = (x + 0.5f) / rw - 0.5f, dy = (y + 0.5f) / rh - 0.5f;
                    bool inside = i % 2 == 0 ? dx * dx + dy * dy < 0.2f : Math.Abs(dx) < 0.4f && Math.Abs(dy) < 0.4f;
                    px[(ry + y) * tw + rx + x] = inside ? palette[i % palette.Length] : new Color32(0, 0, 0, 0);
                }
        }
        tex.SetPixels32(px);
        tex.Apply();
        tex.name = "typesetter-sprites";
        File.WriteAllBytes(Path.Combine(outDir, "atlases", "sprites.png"), tex.EncodeToPNG());
        var sa = ScriptableObject.CreateInstance<TMP_SpriteAsset>();
        sa.name = Str(spec, "name", "TypesetterSprites");
        sa.spriteSheet = tex;
        var faceProp = typeof(TMP_Asset).GetProperty("faceInfo", BindingFlags.Public | BindingFlags.Instance);
        if (spec.ContainsKey("faceInfo") && faceProp != null && faceProp.CanWrite)
        {
            object boxed = new FaceInfo();
            foreach (var kv in (Dictionary<string, object>)spec["faceInfo"])
            {
                var pp = typeof(FaceInfo).GetProperty(kv.Key, BindingFlags.Public | BindingFlags.Instance);
                if (pp != null && pp.CanWrite) pp.SetValue(boxed, pp.PropertyType == typeof(int) ? (object)(int)(double)kv.Value : (float)(double)kv.Value);
            }
            faceProp.SetValue(sa, (FaceInfo)boxed);
        }
        // A fresh asset counts as a legacy (pre-1.1) sprite asset and is rebuilt
        // from its public legacy sprite list by UpdateLookupTables, so the
        // sprites are described there (public TMP_Sprite fields, set by name).
        var legacy = new List<TMP_Sprite>();
        for (int i = 0; i < sprites.Count; i++)
        {
            var sp = (Dictionary<string, object>)sprites[i];
            var r = (List<object>)sp["rect"];
            var m = (List<object>)sp["metrics"];
            var ls = new TMP_Sprite();
            object boxed = ls;
            void Set(string f, object v)
            {
                var fi = typeof(TMP_Sprite).GetField(f, BindingFlags.Public | BindingFlags.Instance);
                if (fi == null) { Debug.LogWarning("[typesetter-dump] TMP_Sprite has no public field " + f); return; }
                fi.SetValue(boxed, System.Convert.ChangeType(v, fi.FieldType, CultureInfo.InvariantCulture));
            }
            string nm = Str(sp, "name", "sprite" + i);
            Set("id", i); Set("name", nm); Set("hashCode", TMP_TextUtilities.GetSimpleHashCode(nm));
            Set("unicode", (int)Num(sp, "unicode", 0));
            Set("x", (float)(double)r[0]); Set("y", (float)(double)r[1]); Set("width", (float)(double)m[0]); Set("height", (float)(double)m[1]);
            Set("xOffset", (float)(double)m[2]); Set("yOffset", (float)(double)m[3]); Set("xAdvance", (float)(double)m[4]);
            Set("scale", (float)Num(sp, "scale", 1));
            legacy.Add((TMP_Sprite)boxed);
        }
        sa.spriteInfoList = legacy;
        var mat = new Material(Shader.Find("TextMeshPro/Sprite"));
        mat.SetTexture(ShaderUtilities.ID_MainTex, tex);
        sa.material = mat;
        sa.UpdateLookupTables();
        return sa;
    }

    static Dictionary<string, object> SpriteAssetInfo(TMP_SpriteAsset sa)
    {
        return new Dictionary<string, object>
        {
            { "name", sa.name },
            { "faceInfo", Fields(sa.faceInfo) },
            { "glyphs", sa.spriteGlyphTable.Select(g => (object)new Dictionary<string, object> {
                { "index", g.index },
                { "metrics", new List<object> { g.metrics.width, g.metrics.height, g.metrics.horizontalBearingX, g.metrics.horizontalBearingY, g.metrics.horizontalAdvance } },
                { "rect", new List<object> { g.glyphRect.x, g.glyphRect.y, g.glyphRect.width, g.glyphRect.height } },
                { "scale", g.scale } }).ToList() },
            { "characters", sa.spriteCharacterTable.Select(ch => (object)new Dictionary<string, object> {
                { "name", ch.name }, { "unicode", ch.unicode }, { "glyphIndex", ch.glyphIndex }, { "scale", ch.scale } }).ToList() },
        };
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
        // Style values (bold spacing, italic slant, tab size, ...): every public
        // primitive field / property of the asset.
        var style = new SortedDictionary<string, object>();
        foreach (var f in typeof(TMP_FontAsset).GetFields(BindingFlags.Public | BindingFlags.Instance))
            if (f.FieldType.IsPrimitive) style[f.Name] = System.Convert.ToDouble(f.GetValue(fa), CultureInfo.InvariantCulture);
        foreach (var p in typeof(TMP_FontAsset).GetProperties(BindingFlags.Public | BindingFlags.Instance))
            if (p.PropertyType.IsPrimitive && p.CanRead && p.GetIndexParameters().Length == 0)
                try { style[p.Name] = System.Convert.ToDouble(p.GetValue(fa), CultureInfo.InvariantCulture); } catch { }
        d["style"] = style;
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
        TMP_SpriteAsset spriteAsset = null;
        if (c.ContainsKey("spriteAsset"))
        {
            spriteAsset = MakeSpriteAsset((Dictionary<string, object>)c["spriteAsset"], outDir);
            MaterialReferenceManager.AddSpriteAsset(spriteAsset);
            foreach (int h in Hashes(spriteAsset.name)) MaterialReferenceManager.AddSpriteAsset(h, spriteAsset);
            tmp.spriteAsset = spriteAsset;
        }
        if (c.ContainsKey("gradients"))
            foreach (var kv in (Dictionary<string, object>)c["gradients"])
            {
                var cols = ((List<object>)kv.Value).Select(x => ParseColor((string)x)).ToArray();
                var g = ScriptableObject.CreateInstance<TMP_ColorGradient>();
                g.name = kv.Key;
                g.colorMode = ColorMode.FourCornersGradient;
                g.topLeft = cols[0]; g.topRight = cols[1]; g.bottomLeft = cols[2]; g.bottomRight = cols[3];
                foreach (int h in Hashes(kv.Key)) MaterialReferenceManager.AddColorGradientPreset(h, g);
            }
        TMP_Text linked = null;
        if (c.ContainsKey("linked"))
        {
            var lr = (List<object>)((Dictionary<string, object>)c["linked"])["rect"];
            var lgo = new GameObject("linked");
            lgo.transform.SetParent(tmp.transform.parent, false);
            linked = comp == "3d" ? (TMP_Text)lgo.AddComponent<TextMeshPro>() : lgo.AddComponent<TextMeshProUGUI>();
            var lrt = linked.rectTransform;
            lrt.anchorMin = lrt.anchorMax = new Vector2(0.5f, 0.5f);
            lrt.pivot = new Vector2(0.5f, 0.5f);
            lrt.sizeDelta = new Vector2((float)(double)lr[0], (float)(double)lr[1]);
            lrt.anchoredPosition = Vector2.zero;
            linked.font = fa;
        }

        var props = c.ContainsKey("props") ? (Dictionary<string, object>)c["props"] : new Dictionary<string, object>();
        foreach (var kv in props) SetProp(tmp, kv.Key, kv.Value);

        if (c.ContainsKey("material") || c.ContainsKey("keywords") || c.ContainsKey("shader"))
        {
            var mat = new Material(tmp.fontSharedMaterial);
            if (c.ContainsKey("shader")) mat.shader = Shader.Find((string)c["shader"]);
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

        if (linked != null) tmp.linkedTextComponent = linked;
        tmp.text = Str(c, "text", "");
        Canvas.ForceUpdateCanvases();
        tmp.ForceMeshUpdate(true, true);
        if (linked != null) linked.ForceMeshUpdate(true, true);

        var r = TextInfoDump(tmp, c);
        r["asset"] = assetKey;
        r["material"] = MaterialFloats(tmp.fontSharedMaterial);
        if (spriteAsset != null) r["spriteAsset"] = SpriteAssetInfo(spriteAsset);
        if (linked != null) r["linked"] = TextInfoDump(linked, new Dictionary<string, object>());

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

    static Dictionary<string, object> TextInfoDump(TMP_Text tmp, Dictionary<string, object> c)
    {
        var r = new Dictionary<string, object>();
        var rtf = tmp.rectTransform;
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

    /// djb2-xor hashes of a name as written, upper-cased and lower-cased.
    static IEnumerable<int> Hashes(string s)
    {
        int H(string t) { int h = 0; foreach (char ch in t) h = ((h << 5) + h) ^ ch; return h; }
        return new[] { H(s), H(s.ToUpperInvariant()), H(s.ToLowerInvariant()) }.Distinct();
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
        if (o is IDictionary dict)
        {
            // Dictionaries (e.g. line-breaking character tables): sorted keys.
            var keys = new List<object>();
            foreach (var k in dict.Keys) keys.Add(k is char kc ? (object)(double)kc : k is IConvertible ? System.Convert.ToDouble(k, CultureInfo.InvariantCulture) : (object)k.ToString());
            return keys.OrderBy(k => k is double dk ? dk : 0).ToList();
        }
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

    /// Public static TMP_Settings values and the default style sheet's styles
    /// (configuration the layout depends on: line-breaking character sets,
    /// missing-glyph character, style definitions).
    static Dictionary<string, object> DumpSettings()
    {
        var d = new Dictionary<string, object>();
        foreach (var p in typeof(TMP_Settings).GetProperties(BindingFlags.Public | BindingFlags.Static))
        {
            if (!p.CanRead || p.GetIndexParameters().Length > 0) continue;
            try
            {
                var v = p.GetValue(null);
                if (v is TMP_Settings) continue;
                if (v is TextAsset ta) { d[p.Name] = ta.text; continue; }
                if (v is TMP_StyleSheet ss)
                {
                    d[p.Name] = ss.name;
                    var styles = new List<object>();
                    // The style list is not public: probe GetStyle(name) with common names.
                    var names = new List<string> { "Normal", "Title", "Subtitle", "Quote", "Link", "Note", "Code", "Caption", "Small", "Bold", "Italic", "Underline", "Strikethrough", "Blockquote", "Heading", "Header", "Footer", "Emphasis", "Strong", "Highlight", "Mark", "Warning", "Error", "Info", "Default" };
                    for (int h = 1; h <= 6; h++) names.Add("H" + h);
                    for (int k = 1; k <= 6; k++) names.Add("C" + k);
                    foreach (var st in names.Select(n => ss.GetStyle(n)).Where(x => x != null).Distinct())
                        styles.Add(new Dictionary<string, object>
                        {
                            { "name", st.name }, { "hashCode", st.hashCode },
                            { "open", st.styleOpeningDefinition }, { "close", st.styleClosingDefinition },
                        });
                    d["styles"] = styles;
                    continue;
                }
                d[p.Name] = Fields(v);
            }
            catch (Exception e) { d[p.Name] = "error: " + e.GetType().Name; }
        }
        return d;
    }

    static Dictionary<string, object> DumpApi()
    {
        var d = new Dictionary<string, object>();
        foreach (var t in new[] { typeof(TMP_Text), typeof(TMP_FontAsset), typeof(TMP_SpriteAsset), typeof(TMP_SpriteGlyph), typeof(TMP_SpriteCharacter), typeof(TMP_Settings), typeof(MaterialReferenceManager), typeof(TMP_StyleSheet), typeof(TMP_Style), typeof(TMP_ColorGradient) })
        {
            var l = new List<object>();
            foreach (var m in t.GetMembers(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly))
            {
                string s = m.MemberType + " " + m.Name;
                if (m is PropertyInfo pi) s += " : " + pi.PropertyType.Name + (pi.GetSetMethod() != null ? " {get;set}" : " {get}");
                if (m is FieldInfo fi) s += " : " + fi.FieldType.Name;
                if (m is MethodInfo mi) s += "(" + string.Join(", ", mi.GetParameters().Select(x => x.ParameterType.Name + " " + x.Name)) + ")";
                if (m is ConstructorInfo ci) s += "(" + string.Join(", ", ci.GetParameters().Select(x => x.ParameterType.Name + " " + x.Name)) + ")";
                l.Add(s);
            }
            d[t.Name] = l;
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
