//! Layout parity against Unity ground truth (bench/unity/*.json).
//!
//!     cargo run --release --example parity [-- <case-substring> [--verbose] [--all-notes]]
//!
//! Uses Typesetter's own font loading and glyph metrics (core::font) and layout
//! (core::layout). Face metrics (ascent line, cap line, underline offset, ...)
//! come from the dumped font asset's faceInfo, so this measures layout rather
//! than face-metric derivation. Compares, per case: every character (code,
//! visibility, line/page, origin, advance, baseline, ascender/descender, point
//! size, style flags, colour, quad corners), every line (indices, counts,
//! metrics, extents), words, links, pages, preferred and rendered values,
//! overflow, and the linked component when present. Prints one row per case.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use typesetter_native::core::font::Face;
use typesetter_native::core::layout::{self, style, FaceMetrics, FontProvider, GlyphInfo, HAlign, Layout, Overflow, Settings, SpriteInfo, VAlign, Wrap};

const TOL: f32 = 0.02;

struct FontEntry {
    face: Face,
    pt: f32,
    hinted: bool,
    fm: FaceMetrics,
    cache: HashMap<u32, Option<GlyphInfo>>,
    kern: HashMap<(u32, u32), [f32; 8]>,
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

fn face_metrics(face: &Face, pt: f32, atlas: Option<&Value>) -> FaceMetrics {
    let m = face.metrics();
    let s = pt as f64 / m.units_per_em;
    let space = face.glyph_id(32).map(|g| face.pixel_metrics(g, pt as f64).advance).unwrap_or(0.0);
    let mut fm = FaceMetrics {
        point_size: pt,
        ascent: (m.ascender * s) as f32,
        descent: (m.descender * s) as f32,
        line_height: ((m.ascender - m.descender + m.line_gap) * s) as f32,
        cap_line: (m.cap_height * s).ceil() as f32,
        mean_line: (m.x_height * s).ceil() as f32,
        superscript_offset: (m.ascender * s) as f32,
        superscript_size: 0.5,
        subscript_offset: (m.descender * s) as f32,
        subscript_size: 0.5,
        underline_offset: (m.underline_offset * s) as f32,
        underline_thickness: (m.underline_thickness * s) as f32,
        strikethrough_offset: (m.strikethrough_offset * s) as f32,
        strikethrough_thickness: (m.underline_thickness * s) as f32,
        tab_width: space.floor() as f32,
        bold_spacing: 7.0,
        normal_spacing_offset: 0.0,
        italic_style: 35.0,
        tab_size: 10.0,
        bold_style: 0.75,
        atlas_padding: pt / 10.0,
    };
    if let Some(a) = atlas {
        let fi = &a["faceInfo"];
        fm.point_size = f(&fi["pointSize"]);
        fm.ascent = f(&fi["ascentLine"]);
        fm.descent = f(&fi["descentLine"]);
        fm.line_height = f(&fi["lineHeight"]);
        fm.cap_line = f(&fi["capLine"]);
        fm.mean_line = f(&fi["meanLine"]);
        fm.superscript_offset = f(&fi["superscriptOffset"]);
        fm.superscript_size = f(&fi["superscriptSize"]);
        fm.subscript_offset = f(&fi["subscriptOffset"]);
        fm.subscript_size = f(&fi["subscriptSize"]);
        fm.underline_offset = f(&fi["underlineOffset"]);
        fm.underline_thickness = f(&fi["underlineThickness"]);
        fm.strikethrough_offset = f(&fi["strikethroughOffset"]);
        fm.strikethrough_thickness = f(&fi["strikethroughThickness"]);
        fm.tab_width = f(&fi["tabWidth"]);
        if let Some(st) = a.get("style") {
            fm.bold_spacing = f(&st["boldSpacing"]);
            fm.normal_spacing_offset = f(&st["normalSpacingOffset"]);
            fm.italic_style = f(&st["italicStyle"]);
            fm.tab_size = f(&st["tabSize"]);
            fm.bold_style = f(&st["boldStyle"]);
        }
        if let Some(p) = a.get("atlasPadding") {
            fm.atlas_padding = f(p);
        }
    }
    fm
}

fn kern_table(atlas: Option<&Value>) -> HashMap<(u32, u32), [f32; 8]> {
    let mut m = HashMap::new();
    let Some(recs) = atlas.and_then(|a| a["fontFeatureTable"]["m_GlyphPairAdjustmentRecords"].as_array()) else { return m };
    for r in recs {
        let (a, b) = (&r["m_FirstAdjustmentRecord"], &r["m_SecondAdjustmentRecord"]);
        let v = |x: &Value| [f(&x["m_XPlacement"]), f(&x["m_YPlacement"]), f(&x["m_XAdvance"]), f(&x["m_YAdvance"])];
        let (va, vb) = (v(&a["m_GlyphValueRecord"]), v(&b["m_GlyphValueRecord"]));
        m.insert((f(&a["m_GlyphIndex"]) as u32, f(&b["m_GlyphIndex"]) as u32), [va[0], va[1], va[2], va[3], vb[0], vb[1], vb[2], vb[3]]);
    }
    m
}

struct Prov {
    fonts: Vec<FontEntry>,
    names: HashMap<String, u32>,
    fallbacks: Vec<u32>,
    styles: HashMap<String, (String, String)>,
    sprites: Vec<(String, SpriteInfo)>,
    sprite_asset: String,
    gradients: HashMap<String, [[f32; 4]; 4]>,
    breaking: Option<(String, String)>,
}

impl Prov {
    fn add_font(&mut self, root: &Path, file: &str, pt: f32, atlas: Option<&Value>) -> u32 {
        let data = std::fs::read(root.join("tests/fonts").join(file)).unwrap();
        let face = Face::new(data, 0).unwrap();
        let fm = face_metrics(&face, pt, atlas);
        self.fonts.push(FontEntry { face, pt, hinted: false, fm, cache: HashMap::new(), kern: kern_table(atlas) });
        (self.fonts.len() - 1) as u32
    }

    fn lookup(&mut self, f: u32, cp: u32) -> Option<GlyphInfo> {
        let e = &mut self.fonts[f as usize];
        let pt = e.pt as f64;
        let hinted = e.hinted;
        let face = &e.face;
        *e.cache.entry(cp).or_insert_with(|| {
            let gid = face.glyph_id(cp)?;
            let mut m = face.pixel_metrics(gid, pt);
            if hinted {
                // Hinted assets grid-fit metrics: box edges out to whole pixels, advance rounded.
                let (x0, x1) = (m.bearing_x.floor(), (m.bearing_x + m.width).ceil());
                let (y1, y0) = (m.bearing_y.ceil(), (m.bearing_y - m.height).floor());
                m.bearing_x = x0;
                m.bearing_y = y1;
                m.width = x1 - x0;
                m.height = y1 - y0;
                m.advance = m.advance.round();
            }
            Some(GlyphInfo {
                index: gid,
                width: m.width as f32,
                height: m.height as f32,
                bearing_x: m.bearing_x as f32,
                bearing_y: m.bearing_y as f32,
                advance: m.advance as f32,
                rect: [0.0, 0.0, m.box_w as f32, m.box_h as f32],
                atlas_index: 0,
                atlas_size: [1024.0, 1024.0],
            })
        })
    }
}

impl FontProvider for Prov {
    fn face(&self, f: u32) -> FaceMetrics {
        self.fonts[f as usize].fm
    }
    fn glyph(&mut self, f: u32, cp: u32) -> Option<(u32, GlyphInfo)> {
        if let Some(g) = self.lookup(f, cp) {
            return Some((f, g));
        }
        if f == 0 {
            for fb in self.fallbacks.clone() {
                if let Some(g) = self.lookup(fb, cp) {
                    return Some((fb, g));
                }
            }
        }
        None
    }
    fn kerning(&self, f: u32, l: u32, r: u32) -> Option<[f32; 8]> {
        // Pair adjustments come from the dumped asset's font feature table (the
        // records the font asset holds), like the face metrics.
        self.fonts[f as usize].kern.get(&(l, r)).copied()
    }
    fn line_breaking_rules(&mut self) -> Option<(String, String)> {
        self.breaking.clone()
    }
    fn font_by_name(&mut self, name: &str) -> Option<u32> {
        self.names.get(name).copied()
    }
    fn style(&mut self, name: &str) -> Option<(String, String)> {
        let key = name.to_ascii_lowercase();
        self.styles.iter().find(|(k, _)| k.to_ascii_lowercase() == key).map(|(_, v)| v.clone())
    }
    fn gradient(&mut self, name: &str) -> Option<[[f32; 4]; 4]> {
        self.gradients.get(name).copied()
    }
    fn sprite(&mut self, asset: Option<&str>, index: Option<i32>, name: Option<&str>) -> Option<SpriteInfo> {
        if asset.is_some_and(|a| a != self.sprite_asset) {
            return None;
        }
        if let Some(n) = name {
            return self.sprites.iter().find(|(sn, _)| sn == n).map(|x| x.1);
        }
        let i = index?;
        self.sprites.get(i as usize).map(|x| x.1)
    }
    fn sprite_by_unicode(&mut self, cp: u32) -> Option<SpriteInfo> {
        self.sprites.iter().find(|(_, s)| s.unicode == cp && cp != 0xFFFE).map(|x| x.1)
    }
}

fn load_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn make_provider(root: &Path, case: &Value, truth: &Value) -> Prov {
    let mut prov = Prov { fonts: vec![], names: HashMap::new(), fallbacks: vec![], styles: HashMap::new(), sprites: vec![], sprite_asset: String::new(), gradients: HashMap::new(), breaking: None };
    let atlases = root.join("bench/unity/atlases");
    let pt = case["asset"]["pointSize"].as_f64().unwrap_or(90.0) as f32;
    let asset_key = truth["asset"].as_str().unwrap_or("");
    let main_atlas = load_json(&atlases.join(format!("{asset_key}.json")));
    prov.add_font(root, case["font"].as_str().unwrap_or("Inter-Regular.ttf"), pt, main_atlas.as_ref());
    prov.fonts[0].hinted = case["asset"]["renderMode"].as_str().is_some_and(|m| m.ends_with("_HINTED"));
    if let Some(fonts) = case.get("fonts").and_then(|v| v.as_object()) {
        for (alias, spec) in fonts {
            let a = load_json(&atlases.join(format!("{alias}.json")));
            let pt = spec["asset"]["pointSize"].as_f64().unwrap_or(90.0) as f32;
            let id = prov.add_font(root, spec["font"].as_str().unwrap_or("Inter-Regular.ttf"), pt, a.as_ref());
            prov.names.insert(alias.clone(), id);
        }
    }
    if let Some(fb) = case.get("fallbacks").and_then(|v| v.as_array()) {
        prov.fallbacks = fb.iter().filter_map(|n| n.as_str().and_then(|n| prov.names.get(n).copied())).collect();
    }
    if let Some(settings) = load_json(&root.join("bench/unity/_settings.json")) {
        if let (Some(l), Some(fo)) = (settings["leadingCharacters"].as_str(), settings["followingCharacters"].as_str()) {
            prov.breaking = Some((l.to_string(), fo.to_string()));
        }
        for st in settings["styles"].as_array().into_iter().flatten() {
            prov.styles.insert(st["name"].as_str().unwrap_or("").to_string(), (st["open"].as_str().unwrap_or("").to_string(), st["close"].as_str().unwrap_or("").to_string()));
        }
    }
    if let Some(g) = case.get("gradients").and_then(|v| v.as_object()) {
        for (name, cols) in g {
            let c: Vec<[f32; 4]> = cols.as_array().unwrap().iter().map(|c| typesetter_native::core::richtext::parse_color(c.as_str().unwrap()).unwrap()).collect();
            prov.gradients.insert(name.clone(), [c[0], c[1], c[2], c[3]]);
        }
    }
    if let Some(sa) = case.get("spriteAsset") {
        prov.sprite_asset = sa["name"].as_str().unwrap_or("").to_string();
        let fi = &sa["faceInfo"];
        let dumped = &truth["spriteAsset"]["characters"];
        let (tw, th) = (f(&sa["width"]), f(&sa["height"]));
        for (i, sp) in sa["sprites"].as_array().unwrap().iter().enumerate() {
            let m = &sp["metrics"];
            let r = &sp["rect"];
            let scale = dumped.get(i).map(|d| f(&d["scale"])).unwrap_or(1.0);
            let unicode = dumped.get(i).map(|d| f(&d["unicode"]) as u32).unwrap_or(0xFFFE);
            prov.sprites.push((
                sp["name"].as_str().unwrap_or("").to_string(),
                SpriteInfo {
                    asset: 1000,
                    index: i as u32,
                    unicode,
                    width: f(&m[0]),
                    height: f(&m[1]),
                    bearing_x: f(&m[2]),
                    bearing_y: f(&m[3]),
                    advance: f(&m[4]),
                    scale,
                    rect: [f(&r[0]), th - f(&r[1]) - f(&r[3]), f(&r[2]), f(&r[3])],
                    atlas_size: [tw, th],
                    point_size: f(&fi["pointSize"]),
                    ascent: f(&fi["ascentLine"]),
                    descent: f(&fi["descentLine"]),
                    tint: false,
                },
            ));
        }
    }
    prov
}

fn align(name: &str) -> (HAlign, VAlign) {
    let (v, rest) = ["Top", "Bottom", "Baseline", "Midline", "Capline"]
        .iter()
        .find_map(|p| name.strip_prefix(p).map(|r| (*p, r)))
        .unwrap_or(("", name));
    let va = match v {
        "Top" => VAlign::Top,
        "Bottom" => VAlign::Bottom,
        "Baseline" => VAlign::Baseline,
        "Midline" => VAlign::Geometry,
        "Capline" => VAlign::Capline,
        _ => VAlign::Middle,
    };
    let ha = match rest {
        "Left" => HAlign::Left,
        "Right" => HAlign::Right,
        "Justified" => HAlign::Justified,
        "Flush" => HAlign::Flush,
        "GeoAligned" => HAlign::Geometry,
        _ => HAlign::Center,
    };
    (ha, va)
}

fn style_bits(s: &str) -> u32 {
    let mut b = 0;
    for part in s.split(',').map(|x| x.trim()) {
        b |= match part {
            "Bold" => style::BOLD,
            "Italic" => style::ITALIC,
            "Underline" => style::UNDERLINE,
            "LowerCase" => style::LOWERCASE,
            "UpperCase" => style::UPPERCASE,
            "SmallCaps" => style::SMALLCAPS,
            "Strikethrough" => style::STRIKETHROUGH,
            "Superscript" => style::SUPERSCRIPT,
            "Subscript" => style::SUBSCRIPT,
            "Highlight" => style::HIGHLIGHT,
            _ => 0,
        };
    }
    b
}

pub fn settings_from_case(case: &Value) -> Settings {
    let props = &case["props"];
    let rect = &case["rect"];
    let (w, h) = (f(&rect[0]), f(&rect[1]));
    let pv = case.get("pivot").cloned().unwrap_or(serde_json::json!([0.5, 0.5]));
    let mut s = Settings { rect: [-w * f(&pv[0]), -h * f(&pv[1]), w, h], world_space: case["component"] == "3d", kerning: true, ..Default::default() };
    let num = |k: &str| props.get(k).map(f);
    let flag = |k: &str| props.get(k).and_then(|v| v.as_bool());
    if let Some(v) = num("fontSize") {
        s.font_size = v;
    }
    if let Some(v) = props.get("alignment").and_then(|v| v.as_str()) {
        (s.halign, s.valign) = align(v);
    }
    if let Some(v) = props.get("textWrappingMode").and_then(|v| v.as_str()) {
        s.wrap = match v {
            "NoWrap" => Wrap::NoWrap,
            "PreserveWhitespace" => Wrap::PreserveWhitespace,
            "PreserveWhitespaceNoWrap" => Wrap::PreserveWhitespaceNoWrap,
            _ => Wrap::Normal,
        };
    }
    if let Some(v) = props.get("overflowMode").and_then(|v| v.as_str()) {
        s.overflow = match v {
            "Ellipsis" => Overflow::Ellipsis,
            "Masking" => Overflow::Masking,
            "Truncate" => Overflow::Truncate,
            "ScrollRect" => Overflow::ScrollRect,
            "Page" => Overflow::Page,
            "Linked" => Overflow::Linked,
            _ => Overflow::Overflow,
        };
    }
    if let Some(v) = props.get("fontStyle").and_then(|v| v.as_str()) {
        s.font_style = style_bits(v);
    }
    if let Some(v) = props.get("fontWeight") {
        s.font_weight = match v.as_str() {
            Some("Thin") => 100,
            Some("ExtraLight") => 200,
            Some("Light") => 300,
            Some("Regular") => 400,
            Some("Medium") => 500,
            Some("SemiBold") => 600,
            Some("Bold") => 700,
            Some("Heavy") => 800,
            Some("Black") => 900,
            _ => f(v) as i32,
        };
    }
    macro_rules! set {
        ($k:expr, $field:ident) => {
            if let Some(v) = num($k) {
                s.$field = v;
            }
        };
    }
    set!("characterSpacing", character_spacing);
    set!("wordSpacing", word_spacing);
    set!("lineSpacing", line_spacing);
    set!("paragraphSpacing", paragraph_spacing);
    set!("fontSizeMin", font_size_min);
    set!("fontSizeMax", font_size_max);
    set!("wordWrappingRatios", word_wrapping_ratios);
    set!("characterWidthAdjustment", char_width_max_adj);
    set!("lineSpacingAdjustment", line_spacing_max_adj);
    if let Some(v) = props.get("margin") {
        s.margin = [f(&v[0]), f(&v[1]), f(&v[2]), f(&v[3])];
    }
    if let Some(v) = flag("enableAutoSizing") {
        s.auto_size = v;
    }
    if let Some(v) = flag("isRightToLeftText") {
        s.right_to_left = v;
    }
    if let Some(v) = flag("richText") {
        s.rich_text = v;
    }
    if let Some(v) = flag("parseCtrlCharacters") {
        s.parse_ctrl_characters = v;
    }
    if let Some(v) = num("maxVisibleCharacters") {
        s.max_visible_characters = v as i32;
    }
    if let Some(v) = num("maxVisibleWords") {
        s.max_visible_words = v as i32;
    }
    if let Some(v) = num("maxVisibleLines") {
        s.max_visible_lines = v as i32;
    }
    if let Some(v) = num("firstVisibleCharacter") {
        s.first_visible_character = v as i32;
    }
    if let Some(v) = num("pageToDisplay") {
        s.page_to_display = v as i32;
    }
    s
}

/// Collects mismatches: numeric ones beyond tolerance and exact ones.
#[derive(Default)]
struct Cmp {
    notes: Vec<String>,
    seen: HashMap<String, usize>,
    pos_err: f32,
    base_err: f32,
}

impl Cmp {
    fn note(&mut self, key: &str, msg: String) {
        let n = self.seen.entry(key.to_string()).or_insert(0);
        *n += 1;
        if *n == 1 {
            self.notes.push(msg);
        }
    }
    fn num(&mut self, key: &str, at: &str, ours: f32, theirs: f32) {
        if (ours - theirs).abs() > TOL && !(ours.is_nan() && theirs.is_nan()) {
            self.note(key, format!("{key}{at} {ours:.3} vs {theirs:.3}"));
        }
    }
    fn eq<T: PartialEq + std::fmt::Debug>(&mut self, key: &str, at: &str, ours: T, theirs: T) {
        if ours != theirs {
            self.note(key, format!("{key}{at} {ours:?} vs {theirs:?}"));
        }
    }
}

fn halign_name(h: HAlign) -> &'static str {
    match h {
        HAlign::Left => "Left",
        HAlign::Center => "Center",
        HAlign::Right => "Right",
        HAlign::Justified => "Justified",
        HAlign::Flush => "Flush",
        HAlign::Geometry => "Geometry",
    }
}

fn c32(c: [f32; 4]) -> [i64; 4] {
    c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as i64)
}

fn compare(l: &Layout, truth: &Value, cmp: &mut Cmp, prefix: &str) {
    let tc = truth["characters"].as_array().unwrap();
    cmp.eq(&format!("{prefix}chars"), "", l.characters.len(), tc.len());
    for (i, (c, t)) in l.characters.iter().zip(tc.iter()).enumerate() {
        let at = format!("[{i}]");
        let k = |s: &str| format!("{prefix}{s}");
        let tcode: u32 = t["character"].as_str().and_then(|x| x.parse().ok()).unwrap_or(0);
        // TMP stores the character as a 16-bit char.
        cmp.eq(&k("code"), &at, c.character & 0xFFFF, tcode);
        cmp.eq(&k("strLen"), &at, c.string_length as i64, t["stringLength"].as_i64().unwrap_or(-1));
        cmp.eq(&k("visible"), &at, c.is_visible, t["isVisible"].as_bool().unwrap_or(false));
        cmp.eq(&k("line#"), &at, c.line_number as i64, t["lineNumber"].as_i64().unwrap_or(-1));
        cmp.eq(&k("page#"), &at, c.page_number as i64, t["pageNumber"].as_i64().unwrap_or(-1));
        cmp.eq(&k("index"), &at, c.index as i64, t["index"].as_i64().unwrap_or(-1));
        cmp.eq(&k("style"), &at, c.style, style_bits(t["style"].as_str().unwrap_or("")));
        cmp.eq(&k("type"), &at, c.element_type == layout::ElementType::Sprite, t["elementType"] == "Sprite");
        let e = (c.origin - f(&t["origin"])).abs().max((c.x_advance - f(&t["xAdvance"])).abs());
        cmp.pos_err = cmp.pos_err.max(e);
        cmp.base_err = cmp.base_err.max((c.base_line - f(&t["baseLine"])).abs());
        cmp.num(&k("origin"), &at, c.origin, f(&t["origin"]));
        cmp.num(&k("xAdvance"), &at, c.x_advance, f(&t["xAdvance"]));
        cmp.num(&k("baseLine"), &at, c.base_line, f(&t["baseLine"]));
        cmp.num(&k("ascender"), &at, c.ascender, f(&t["ascender"]));
        cmp.num(&k("descender"), &at, c.descender, f(&t["descender"]));
        cmp.num(&k("pointSize"), &at, c.point_size, f(&t["pointSize"]));
        let tcol: Vec<i64> = t["color"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as i64).collect();
        let oc = c32(c.color);
        if oc.iter().zip(tcol.iter()).any(|(a, b)| (a - b).abs() > 1) {
            cmp.note(&k("color"), format!("{prefix}color{at} {oc:?} vs {tcol:?}"));
        }
        if c.is_visible {
            cmp.num(&k("quadBL.x"), &at, c.bottom_left.x, f(&t["bottomLeft"][0]));
            cmp.num(&k("quadBL.y"), &at, c.bottom_left.y, f(&t["bottomLeft"][1]));
            cmp.num(&k("quadTR.x"), &at, c.top_right.x, f(&t["topRight"][0]));
            cmp.num(&k("quadTR.y"), &at, c.top_right.y, f(&t["topRight"][1]));
        }
    }
    let tl = truth["lines"].as_array().unwrap();
    cmp.eq(&format!("{prefix}lines"), "", l.lines.len(), tl.len());
    for (i, (o, t)) in l.lines.iter().zip(tl.iter()).enumerate() {
        let at = format!("[{i}]");
        let k = |s: &str| format!("{prefix}line.{s}");
        let gi = |n: &str| t[n].as_i64().unwrap_or(-99) as i32;
        cmp.eq(&k("range"), &at, (o.first_character_index, o.last_character_index), (gi("firstCharacterIndex"), gi("lastCharacterIndex")));
        cmp.eq(&k("visRange"), &at, (o.first_visible_character_index, o.last_visible_character_index), (gi("firstVisibleCharacterIndex"), gi("lastVisibleCharacterIndex")));
        cmp.eq(&k("counts"), &at, (o.character_count, o.visible_character_count, o.space_count, o.word_count), (gi("characterCount"), gi("visibleCharacterCount"), gi("spaceCount"), gi("wordCount")));
        cmp.num(&k("ascender"), &at, o.ascender, f(&t["ascender"]));
        cmp.num(&k("baseline"), &at, o.baseline, f(&t["baseline"]));
        cmp.num(&k("descender"), &at, o.descender, f(&t["descender"]));
        cmp.num(&k("lineHeight"), &at, o.line_height, f(&t["lineHeight"]));
        cmp.num(&k("length"), &at, o.length, f(&t["length"]));
        cmp.num(&k("maxAdvance"), &at, o.max_advance, f(&t["maxAdvance"]));
        cmp.num(&k("width"), &at, o.width, f(&t["width"]));
        cmp.num(&k("marginLeft"), &at, o.margin_left, f(&t["marginLeft"]));
        cmp.num(&k("marginRight"), &at, o.margin_right, f(&t["marginRight"]));
        cmp.eq(&k("alignment"), &at, halign_name(o.alignment), t["alignment"].as_str().unwrap_or(""));
        let ex = &t["lineExtents"];
        cmp.num(&k("extMin.x"), &at, o.extents_min.x, f(&ex["min"][0]));
        cmp.num(&k("extMin.y"), &at, o.extents_min.y, f(&ex["min"][1]));
        cmp.num(&k("extMax.x"), &at, o.extents_max.x, f(&ex["max"][0]));
        cmp.num(&k("extMax.y"), &at, o.extents_max.y, f(&ex["max"][1]));
    }
    let tw = truth["words"].as_array().unwrap();
    let ow: Vec<(i32, i32, i32)> = l.words.iter().map(|w| (w.first_character_index, w.last_character_index, w.character_count)).collect();
    let twv: Vec<(i32, i32, i32)> = tw.iter().map(|w| (w["firstCharacterIndex"].as_i64().unwrap() as i32, w["lastCharacterIndex"].as_i64().unwrap() as i32, w["characterCount"].as_i64().unwrap() as i32)).collect();
    if ow != twv {
        let d = ow.iter().zip(twv.iter()).position(|(a, b)| a != b).unwrap_or(ow.len().min(twv.len()));
        cmp.note(&format!("{prefix}words"), format!("{prefix}words n={}/{} first diff @{d}: {:?} vs {:?}", ow.len(), twv.len(), ow.get(d), twv.get(d)));
    }
    let tlk = truth["links"].as_array().unwrap();
    let ol: Vec<(i32, i32, i32)> = l.links.iter().map(|k| (k.link_text_first_character_index, k.link_text_length, k.hash_code)).collect();
    let tlv: Vec<(i32, i32, i32)> = tlk.iter().map(|k| (k["linkTextfirstCharacterIndex"].as_i64().unwrap() as i32, k["linkTextLength"].as_i64().unwrap() as i32, k["hashCode"].as_i64().unwrap() as i32)).collect();
    cmp.eq(&format!("{prefix}links"), "", ol, tlv);
    if l.overflow_mode_page {
        let op: Vec<(i32, i32)> = l.pages.iter().map(|p| (p.first_character_index, p.last_character_index)).collect();
        let tp: Vec<(i32, i32)> = truth["pages"].as_array().unwrap().iter().map(|p| (p["firstCharacterIndex"].as_i64().unwrap() as i32, p["lastCharacterIndex"].as_i64().unwrap() as i32)).collect();
        cmp.eq(&format!("{prefix}pages"), "", op, tp);
    }
    cmp.eq(&format!("{prefix}overflowing"), "", l.is_text_overflowing, truth["isTextOverflowing"].as_bool().unwrap_or(false));
    cmp.eq(&format!("{prefix}firstOverflow"), "", l.first_overflow_character_index as i64, truth["firstOverflowCharacterIndex"].as_i64().unwrap_or(-1));
    cmp.eq(&format!("{prefix}counts"), "", (l.space_count, l.sprite_count), (truth["spaceCount"].as_i64().unwrap_or(0) as i32, truth["spriteCount"].as_i64().unwrap_or(0) as i32));
    let rv = l.rendered_size();
    let trv = &truth["getRenderedValues"];
    cmp.num(&format!("{prefix}renderedW"), "", rv.x, f(&trv[0]));
    cmp.num(&format!("{prefix}renderedH"), "", rv.y, f(&trv[1]));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned().unwrap_or_default();
    let verbose = args.iter().any(|a| a == "--verbose");
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut names: Vec<String> = std::fs::read_dir(root.join("bench/cases"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().trim_end_matches(".json").to_string())
        .filter(|n| n.contains(&filter))
        .collect();
    names.sort();
    let (mut pass, mut total) = (0, 0);
    println!("{:<28} {:>5} {:>9} {:>9} {:>9} {:>9}  notes", "case", "lines", "pos_err", "base_err", "pref_w", "pref_h");
    for name in names {
        let case: Value = load_json(&root.join(format!("bench/cases/{name}.json"))).unwrap();
        let Some(truth) = load_json(&root.join(format!("bench/unity/{name}.json"))) else { continue };
        if truth.get("error").is_some() {
            continue;
        }
        let mut prov = make_provider(&root, &case, &truth);
        let mut s = settings_from_case(&case);
        // Bitmap (raster) materials pad quads by one pixel instead of 1.25.
        if case["asset"]["renderMode"].as_str().is_some_and(|m| m.starts_with("RASTER") || m.starts_with("SMOOTH")) {
            s.padding = 1.0;
        }
        let text = case["text"].as_str().unwrap_or("");
        let l = layout::layout(text, &s, &mut prov);
        let mut cmp = Cmp::default();
        compare(&l, &truth, &mut cmp, "");
        if let Some(lt) = truth.get("linked") {
            // The linked component shows the text from the first overflowing character.
            let mut s2 = s.clone();
            let lr = &case["linked"]["rect"];
            s2.rect = [-f(&lr[0]) * 0.5, -f(&lr[1]) * 0.5, f(&lr[0]), f(&lr[1])];
            s2.overflow = Overflow::Overflow;
            s2.first_visible_character = l.first_overflow_character_index.max(0);
            let l2 = layout::layout(text, &s2, &mut prov);
            compare(&l2, lt, &mut cmp, "linked.");
        }
        let pw = l.preferred_width - f(&truth["preferredWidth"]);
        let ph = l.preferred_height - f(&truth["preferredHeight"]);
        cmp.num("fontSize", "", l.font_size, f(&truth["fontSize"]));
        total += 1;
        let ok = cmp.notes.is_empty() && pw.abs() < TOL && ph.abs() < TOL;
        if ok {
            pass += 1;
        }
        let mut notes: Vec<String> = cmp.notes.iter().map(|n| {
            let key = n.split(|c: char| c == '[' || c == ' ').next().unwrap_or("");
            match cmp.seen.get(key) {
                Some(&k) if k > 1 => format!("{n} (+{})", k - 1),
                _ => n.clone(),
            }
        }).collect();
        if !verbose && notes.len() > 6 {
            let more = notes.len() - 6;
            notes.truncate(6);
            notes.push(format!("… {more} more"));
        }
        println!("{:<28} {:>5} {:>9.4} {:>9.4} {:>9.3} {:>9.3}  {}{}", name, l.lines.len(), cmp.pos_err, cmp.base_err, pw, ph, if ok { "ok " } else { "" }, notes.join("; "));
        if verbose {
            let tc = truth["characters"].as_array().unwrap();
            for (i, (c, t)) in l.characters.iter().zip(tc.iter()).enumerate().take(200) {
                println!(
                    "   {i:3} {:?}/{:?} origin {:.3}/{:.3} adv {:.3}/{:.3} base {:.3}/{:.3} asc {:.3}/{:.3} pt {:.2}/{:.2} vis {}/{}",
                    char::from_u32(c.character).unwrap_or('?'),
                    t["character"].as_str().and_then(|x| x.parse::<u32>().ok()).and_then(char::from_u32).unwrap_or('?'),
                    c.origin,
                    f(&t["origin"]),
                    c.x_advance,
                    f(&t["xAdvance"]),
                    c.base_line,
                    f(&t["baseLine"]),
                    c.ascender,
                    f(&t["ascender"]),
                    c.point_size,
                    f(&t["pointSize"]),
                    c.is_visible,
                    t["isVisible"]
                );
            }
        }
    }
    println!("\n{pass}/{total} cases within tolerance ({TOL} units)");
}
