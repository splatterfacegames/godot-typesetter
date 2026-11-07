//! Layout parity against Unity ground truth (bench/unity/*.json).
//!
//!     cargo run --release --example parity [-- <case-substring> [--verbose]]
//!
//! Uses Typesetter's own font loading and metrics (core::font) and layout
//! (core::layout); compares per-character origins/advances/baselines, line
//! breaks, preferred and rendered values. Prints one row per case.
use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;
use typesetter_native::core::font::Face;
use typesetter_native::core::layout::{self, FaceMetrics, FontProvider, GlyphInfo, HAlign, Overflow, Settings, VAlign, Wrap};

struct Prov {
    face: Face,
    pt: f32,
    hinted: bool,
    fm: FaceMetrics,
    cache: HashMap<u32, Option<GlyphInfo>>,
}

impl Prov {
    fn new(face: Face, pt: f32) -> Self {
        let m = face.metrics();
        let s = pt as f64 / m.units_per_em;
        let space = face.glyph_id(32).map(|g| face.pixel_metrics(g, pt as f64).advance).unwrap_or(0.0);
        let fm = FaceMetrics {
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
            tab_width: space.floor() as f32,
            bold_spacing: 7.0,
            normal_spacing_offset: 0.0,
            italic_style: 35.0,
            tab_size: 10.0,
        };
        Prov { face, pt, fm, cache: HashMap::new(), hinted: false }
    }
}

impl FontProvider for Prov {
    fn face(&self, _f: u32) -> FaceMetrics {
        self.fm
    }
    fn glyph(&mut self, f: u32, cp: u32) -> Option<(u32, GlyphInfo)> {
        let pt = self.pt as f64;
        let hinted = self.hinted;
        let face = &self.face;
        let g = self.cache.entry(cp).or_insert_with(|| {
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
        });
        g.map(|g| (f, g))
    }
    fn kerning(&self, _f: u32, _l: u32, _r: u32) -> Option<[f32; 8]> {
        None
    }
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

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

pub fn settings_from_case(case: &Value) -> Settings {
    let props = &case["props"];
    let rect = &case["rect"];
    let (w, h) = (f(&rect[0]), f(&rect[1]));
    let pv = case.get("pivot").cloned().unwrap_or(serde_json::json!([0.5, 0.5]));
    let mut s = Settings { rect: [-w * f(&pv[0]), -h * f(&pv[1]), w, h], world_space: case["component"] == "3d", kerning: false, ..Default::default() };
    if let Some(v) = props.get("fontSize") {
        s.font_size = f(v);
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
    if let Some(v) = props.get("characterSpacing") {
        s.character_spacing = f(v);
    }
    if let Some(v) = props.get("wordSpacing") {
        s.word_spacing = f(v);
    }
    if let Some(v) = props.get("lineSpacing") {
        s.line_spacing = f(v);
    }
    if let Some(v) = props.get("paragraphSpacing") {
        s.paragraph_spacing = f(v);
    }
    if let Some(v) = props.get("margin") {
        s.margin = [f(&v[0]), f(&v[1]), f(&v[2]), f(&v[3])];
    }
    if let Some(v) = props.get("enableAutoSizing") {
        s.auto_size = v.as_bool().unwrap_or(false);
    }
    if let Some(v) = props.get("fontSizeMin") {
        s.font_size_min = f(v);
    }
    if let Some(v) = props.get("fontSizeMax") {
        s.font_size_max = f(v);
    }
    if let Some(v) = props.get("maxVisibleCharacters") {
        s.max_visible_characters = f(v) as i32;
    }
    if let Some(v) = props.get("maxVisibleLines") {
        s.max_visible_lines = f(v) as i32;
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned().unwrap_or_default();
    let verbose = args.iter().any(|a| a == "--verbose");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let font = std::fs::read(root.join("tests/fonts/Inter-Regular.ttf")).unwrap();
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
        let case: Value = serde_json::from_str(&std::fs::read_to_string(root.join(format!("bench/cases/{name}.json"))).unwrap()).unwrap();
        let Ok(truth_s) = std::fs::read_to_string(root.join(format!("bench/unity/{name}.json"))) else { continue };
        let truth: Value = serde_json::from_str(&truth_s).unwrap();
        if truth.get("error").is_some() {
            continue;
        }
        let pt = case["asset"]["pointSize"].as_f64().unwrap_or(90.0) as f32;
        let mut prov = Prov::new(Face::new(font.clone(), 0).unwrap(), pt);
        prov.hinted = case["asset"]["renderMode"].as_str().is_some_and(|m| m.ends_with("_HINTED"));
        let s = settings_from_case(&case);
        let text = case["text"].as_str().unwrap_or("");
        let l = layout::layout(text, &s, &mut prov);

        let tc = truth["characters"].as_array().unwrap();
        let tl = truth["lines"].as_array().unwrap();
        let mut notes = Vec::new();
        if l.characters.len() != tc.len() {
            notes.push(format!("chars {} vs {}", l.characters.len(), tc.len()));
        }
        let mut pos_err = 0f32;
        let mut base_err = 0f32;
        for (c, t) in l.characters.iter().zip(tc.iter()) {
            pos_err = pos_err.max((c.origin - f(&t["origin"])).abs()).max((c.x_advance - f(&t["xAdvance"])).abs());
            base_err = base_err.max((c.base_line - f(&t["baseLine"])).abs());
        }
        let ours: Vec<(i32, i32)> = l.lines.iter().map(|x| (x.first_character_index, x.last_character_index)).collect();
        let theirs: Vec<(i32, i32)> = tl
            .iter()
            .map(|x| (x["firstCharacterIndex"].as_i64().unwrap() as i32, x["lastCharacterIndex"].as_i64().unwrap() as i32))
            .collect();
        if ours != theirs {
            notes.push(format!("breaks {:?} vs {:?}", &ours[..ours.len().min(4)], &theirs[..theirs.len().min(4)]));
        }
        let pw = l.preferred_width - f(&truth["preferredWidth"]);
        let ph = l.preferred_height - f(&truth["preferredHeight"]);
        if (l.font_size - f(&truth["fontSize"])).abs() > 0.01 {
            notes.push(format!("fontSize {} vs {}", l.font_size, f(&truth["fontSize"])));
        }
        if l.is_text_overflowing != truth["isTextOverflowing"].as_bool().unwrap_or(false) {
            notes.push(format!("overflowing {} vs {}", l.is_text_overflowing, truth["isTextOverflowing"]));
        }
        let rv = l.rendered_size();
        let trv = &truth["getRenderedValues"];
        if (rv.x - f(&trv[0])).abs() > 0.01 || (rv.y - f(&trv[1])).abs() > 0.01 {
            notes.push(format!("rendered ({:.3},{:.3}) vs ({:.3},{:.3})", rv.x, rv.y, f(&trv[0]), f(&trv[1])));
        }
        total += 1;
        let ok = notes.is_empty() && pos_err < 0.02 && base_err < 0.02 && pw.abs() < 0.02 && ph.abs() < 0.02;
        if ok {
            pass += 1;
        }
        println!(
            "{:<28} {:>5} {:>9.4} {:>9.4} {:>9.3} {:>9.3}  {}{}",
            name,
            l.lines.len(),
            pos_err,
            base_err,
            pw,
            ph,
            if ok { "ok " } else { "" },
            notes.join("; ")
        );
        if verbose {
            for (i, (c, t)) in l.characters.iter().zip(tc.iter()).enumerate().take(60) {
                println!(
                    "   {i:3} {:?} origin {:.3}/{:.3} adv {:.3}/{:.3} base {:.3}/{:.3}",
                    char::from_u32(c.character).unwrap_or('?'),
                    c.origin,
                    f(&t["origin"]),
                    c.x_advance,
                    f(&t["xAdvance"]),
                    c.base_line,
                    f(&t["baseLine"])
                );
            }
        }
    }
    println!("\n{pass}/{total} cases within tolerance (0.02 units)");
}
