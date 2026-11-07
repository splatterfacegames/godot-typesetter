//! `TypesetterTextCore`: the engine behind every Typesetter text node (the
//! shared part of TMP_Text). Holds text and settings, runs the layout, exposes
//! textInfo and fills an ArrayMesh with one surface per atlas page.
//!
//! Coordinates: 2D cores use Godot's y-down Control space with the origin at the
//! rect's top-left; 3D cores use y-up local space centred on the pivot.

use std::collections::HashMap;

use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, Material, Resource, Texture2D};
use godot::prelude::*;

use super::font_asset::TypesetterFontAsset;
use super::info::{make_text_info, TypesetterTextInfo};
use crate::core::layout::{self, FaceMetrics, FontProvider, GlyphInfo, HAlign, Layout, Overflow, Settings, SpriteInfo, VAlign, Wrap};

// ------------------------------------------------------------------ font provider

pub struct GdFonts {
    pub assets: Vec<Gd<TypesetterFontAsset>>,
    pub ids: HashMap<InstanceId, u32>,
    pub global_fallbacks: Vec<Gd<TypesetterFontAsset>>,
    pub by_name: HashMap<String, Gd<TypesetterFontAsset>>,
    pub materials: Vec<Gd<Material>>,
    pub material_names: HashMap<String, u32>,
    pub styles: HashMap<String, (String, String)>,
    pub gradients: HashMap<String, [[f32; 4]; 4]>,
    pub sprites: Vec<Gd<Resource>>,
    pub allow_dynamic: bool,
    /// TMP_Settings.missingGlyphCharacter (0: none).
    pub missing_glyph: u32,
    /// Code points not found in any font asset.
    pub missing: Vec<u32>,
}

impl GdFonts {
    pub fn new(primary: Gd<TypesetterFontAsset>) -> Self {
        let mut f = GdFonts {
            assets: Vec::new(),
            ids: HashMap::new(),
            global_fallbacks: Vec::new(),
            by_name: HashMap::new(),
            materials: Vec::new(),
            material_names: HashMap::new(),
            styles: HashMap::new(),
            gradients: HashMap::new(),
            sprites: Vec::new(),
            allow_dynamic: true,
            missing_glyph: 0,
            missing: Vec::new(),
        };
        f.register(primary);
        f
    }

    pub fn register(&mut self, a: Gd<TypesetterFontAsset>) -> u32 {
        let id = a.instance_id();
        if let Some(&i) = self.ids.get(&id) {
            return i;
        }
        let i = self.assets.len() as u32;
        self.ids.insert(id, i);
        self.assets.push(a);
        i
    }

    fn lookup_in(&mut self, a: &Gd<TypesetterFontAsset>, cp: u32, add: bool) -> Option<GlyphInfo> {
        let mut a = a.clone();
        let mut b = a.bind_mut();
        b.ensure_runtime();
        let mut gid = b.rt.chars.get(&cp).copied();
        if gid.is_none() && add && b.atlas_population_mode != TypesetterFontAsset::STATIC {
            if b.add_codepoints(&[cp], false).is_empty() {
                gid = b.rt.chars.get(&cp).copied();
            }
        }
        let gid = gid?;
        let g = *b.rt.glyphs.get(&gid)?;
        let (aw, ah) = (b.atlas_width as f32, b.atlas_height as f32);
        Some(GlyphInfo {
            index: g.index,
            width: g.width,
            height: g.height,
            bearing_x: g.bearing_x,
            bearing_y: g.bearing_y,
            advance: g.advance,
            rect: [g.rect[0] as f32, g.rect[1] as f32, g.rect[2] as f32, g.rect[3] as f32],
            atlas_index: g.atlas_index,
            atlas_size: [aw, ah],
        })
    }

    fn search(&mut self, a: Gd<TypesetterFontAsset>, cp: u32, seen: &mut Vec<InstanceId>) -> Option<(Gd<TypesetterFontAsset>, GlyphInfo)> {
        if seen.contains(&a.instance_id()) {
            return None;
        }
        seen.push(a.instance_id());
        let allow = self.allow_dynamic;
        if let Some(g) = self.lookup_in(&a, cp, allow) {
            return Some((a, g));
        }
        let fallbacks: Vec<Gd<TypesetterFontAsset>> = a.bind().fallback_font_asset_table.iter_shared().collect();
        for f in fallbacks {
            if let Some(r) = self.search(f, cp, seen) {
                return Some(r);
            }
        }
        None
    }
}

impl FontProvider for GdFonts {
    fn face(&self, font: u32) -> FaceMetrics {
        let Some(a) = self.assets.get(font as usize) else { return FaceMetrics::default() };
        let b = a.bind();
        let mut fm = FaceMetrics { point_size: 90.0, tab_size: b.tab_size, bold_spacing: b.bold_spacing, normal_spacing_offset: b.normal_spacing_offset, italic_style: b.italic_style, ..Default::default() };
        if let Some(fi) = &b.face_info {
            let f = fi.bind();
            fm.point_size = f.point_size;
            fm.ascent = f.ascent_line;
            fm.descent = f.descent_line;
            fm.line_height = f.line_height;
            fm.cap_line = f.cap_line;
            fm.mean_line = f.mean_line;
            fm.superscript_offset = f.superscript_offset;
            fm.superscript_size = f.superscript_size;
            fm.subscript_offset = f.subscript_offset;
            fm.subscript_size = f.subscript_size;
            fm.underline_offset = f.underline_offset;
            fm.underline_thickness = f.underline_thickness;
            fm.strikethrough_offset = f.strikethrough_offset;
            fm.tab_width = f.tab_width;
        }
        fm
    }

    fn glyph(&mut self, font: u32, cp: u32) -> Option<(u32, GlyphInfo)> {
        let primary = self.assets.get(font as usize)?.clone();
        let mut seen = Vec::new();
        if let Some((a, g)) = self.search(primary, cp, &mut seen) {
            let id = self.register(a);
            return Some((id, g));
        }
        let globals = self.global_fallbacks.clone();
        for f in globals {
            if let Some((a, g)) = self.search(f, cp, &mut seen) {
                let id = self.register(a);
                return Some((id, g));
            }
        }
        if cp != 0x25A1 && cp != '?' as u32 && cp != self.missing_glyph {
            if !self.missing.contains(&cp) {
                self.missing.push(cp);
            }
            if self.missing_glyph != 0 {
                return self.glyph(font, self.missing_glyph);
            }
        }
        None
    }

    fn kerning(&self, font: u32, left: u32, right: u32) -> Option<[f32; 8]> {
        let a = self.assets.get(font as usize)?;
        let b = a.bind();
        b.rt.pairs.get(&(left, right)).copied()
    }

    fn font_by_name(&mut self, name: &str) -> Option<u32> {
        if let Some(a) = self.by_name.get(name).cloned() {
            return Some(self.register(a));
        }
        // Registered assets by resource name or file name.
        for (i, a) in self.assets.iter().enumerate() {
            let rn = a.get_name().to_string();
            let path = a.get_path().to_string();
            let stem = path.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
            if rn == name || stem == name {
                return Some(i as u32);
            }
        }
        None
    }

    fn material_by_name(&mut self, _font: u32, name: &str) -> Option<u32> {
        self.material_names.get(name).copied()
    }

    fn style(&mut self, name: &str) -> Option<(String, String)> {
        self.styles.get(&name.to_ascii_lowercase()).cloned().or_else(|| self.styles.get(name).cloned())
    }

    fn gradient(&mut self, name: &str) -> Option<[[f32; 4]; 4]> {
        self.gradients.get(name).copied()
    }

    fn sprite(&mut self, asset: Option<&str>, index: Option<i32>, name: Option<&str>) -> Option<SpriteInfo> {
        // Sprite assets are GDScript resources (TypesetterSpriteAsset); query them
        // through their `find_sprite` method: returns a Dictionary or empty.
        for (ai, res) in self.sprites.clone().into_iter().enumerate() {
            let mut res = res;
            let rn = res.get_name().to_string();
            if let Some(a) = asset {
                let path = res.get_path().to_string();
                let stem = path.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
                if a != rn && a != stem {
                    continue;
                }
            }
            let d: Variant = res.call("find_sprite", &[index.unwrap_or(-1).to_variant(), name.unwrap_or("").to_variant()]);
            let Ok(d) = d.try_to::<VarDictionary>() else { continue };
            if d.is_empty() {
                continue;
            }
            // The sprite may come from a fallback sprite asset: register it so the
            // surface samples the right sheet.
            let mut ai = ai;
            if let Some(found) = d.get("asset").and_then(|v| v.try_to::<Gd<Resource>>().ok()) {
                if found.instance_id() != res.instance_id() {
                    ai = match self.sprites.iter().position(|s| s.instance_id() == found.instance_id()) {
                        Some(i) => i,
                        None => {
                            self.sprites.push(found);
                            self.sprites.len() - 1
                        }
                    };
                }
            }
            let g = |k: &str| d.get(k).and_then(|v| v.try_to::<f32>().ok()).unwrap_or(0.0);
            let rect = d.get("rect").and_then(|v| v.try_to::<Rect2>().ok()).unwrap_or_default();
            let tex = d.get("atlas_size").and_then(|v| v.try_to::<Vector2>().ok()).unwrap_or(Vector2::new(1.0, 1.0));
            return Some(SpriteInfo {
                asset: 1_000_000 + ai as u32,
                index: g("index") as u32,
                unicode: g("unicode") as u32,
                width: g("width"),
                height: g("height"),
                bearing_x: g("bearing_x"),
                bearing_y: g("bearing_y"),
                advance: g("advance"),
                scale: g("scale").max(1e-4),
                rect: [rect.position.x, rect.position.y, rect.size.x, rect.size.y],
                atlas_size: [tex.x, tex.y],
                point_size: g("point_size"),
                ascent: g("ascent"),
                descent: g("descent"),
                tint: d.get("tint").and_then(|v| v.try_to::<bool>().ok()).unwrap_or(false),
            });
        }
        None
    }
}

// ------------------------------------------------------------------ enums

/// TMP `TextAlignmentOptions` values (horizontal flag | vertical flag).
pub fn split_alignment(v: i32) -> (HAlign, VAlign) {
    let h = match v & 0xFF {
        0x1 => HAlign::Left,
        0x2 => HAlign::Center,
        0x4 => HAlign::Right,
        0x8 => HAlign::Justified,
        0x10 => HAlign::Flush,
        0x20 => HAlign::Geometry,
        _ => HAlign::Left,
    };
    let vv = match v & 0xFF00 {
        0x100 => VAlign::Top,
        0x200 => VAlign::Middle,
        0x400 => VAlign::Bottom,
        0x800 => VAlign::Baseline,
        0x1000 => VAlign::Geometry,
        0x2000 => VAlign::Capline,
        _ => VAlign::Top,
    };
    (h, vv)
}

// ------------------------------------------------------------------ core

/// Shared text engine for `TypesetterLabel`, `TypesetterText3D` and godot-uxml
/// (TMP_Text's behaviour without the node). Set properties, call `update()`,
/// then read sizes, `text_info` and `build_mesh()`.
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct TypesetterTextCore {
    base: Base<RefCounted>,
    #[var]
    pub text: GString,
    #[var]
    pub font: Option<Gd<TypesetterFontAsset>>,
    #[var]
    pub font_size: f32,
    #[var]
    pub font_style: i32,
    #[var]
    pub font_weight: i32,
    #[var]
    pub color: Color,
    /// TextAlignmentOptions (horizontal | vertical bit flags), default TopLeft.
    #[var]
    pub alignment: i32,
    /// TextWrappingModes: NoWrap 0, Normal 1, PreserveWhitespace 2, PreserveWhitespaceNoWrap 3.
    #[var]
    pub text_wrapping_mode: i32,
    /// TextOverflowModes: Overflow 0, Ellipsis 1, Masking 2, Truncate 3, ScrollRect 4, Page 5, Linked 6.
    #[var]
    pub overflow_mode: i32,
    #[var]
    pub character_spacing: f32,
    #[var]
    pub word_spacing: f32,
    #[var]
    pub line_spacing: f32,
    #[var]
    pub paragraph_spacing: f32,
    #[var]
    pub line_spacing_adjustment: f32,
    #[var]
    pub character_width_adjustment: f32,
    #[var]
    pub word_wrapping_ratios: f32,
    /// Left, top, right, bottom.
    #[var]
    pub margin: Vector4,
    #[var]
    pub enable_auto_sizing: bool,
    #[var]
    pub font_size_min: f32,
    #[var]
    pub font_size_max: f32,
    #[var]
    pub rich_text: bool,
    #[var]
    pub parse_ctrl_characters: bool,
    #[var]
    pub max_visible_characters: i32,
    #[var]
    pub max_visible_words: i32,
    #[var]
    pub max_visible_lines: i32,
    #[var]
    pub first_visible_character: i32,
    #[var]
    pub page_to_display: i32,
    #[var]
    pub is_right_to_left_text: bool,
    #[var]
    pub extra_padding: bool,
    #[var]
    pub enable_kerning: bool,
    /// Size of the text container (Control size, or RectTransform size in 3D units).
    #[var]
    pub rect_size: Vector2,
    /// Pivot for 3D text (0..1), ignored in 2D.
    #[var]
    pub pivot: Vector2,
    /// True for world-space (TextMeshPro) text: scale × 0.1, y-up.
    #[var]
    pub world_space: bool,
    /// Quad padding in sampling-size pixels (1.25 for SDF, 1.0 bitmap) plus material extra.
    #[var]
    pub mesh_padding: f32,
    #[var]
    pub fallback_font_assets: Array<Gd<TypesetterFontAsset>>,
    /// Name → TypesetterFontAsset for `<font="name">`.
    #[var]
    pub font_lookup: VarDictionary,
    /// Name → Material for `<material="name">`.
    #[var]
    pub material_lookup: VarDictionary,
    /// Name → [opening, closing] markup for `<style="name">`.
    #[var]
    pub style_lookup: VarDictionary,
    /// Name → [top_left, top_right, bottom_left, bottom_right] colours for `<gradient>`.
    #[var]
    pub gradient_lookup: VarDictionary,
    /// Sprite assets (TypesetterSpriteAsset) for `<sprite>`.
    #[var]
    pub sprite_assets: Array<Gd<Resource>>,
    /// Vertex colour gradient applied to every character (TMP colorGradient), or empty.
    #[var]
    pub color_gradient: PackedColorArray,
    /// TextureMappingOptions for the face texture: Character 0, Line 1, Paragraph 2, MatchAspect 3.
    #[var]
    pub horizontal_mapping: i32,
    #[var]
    pub vertical_mapping: i32,
    /// Extra horizontal offset for line/paragraph face mapping (TMP mappingUvLineOffset).
    #[var]
    pub mapping_uv_line_offset: f32,
    /// Markup wrapped around the text (TMP textStyle); character indices stay
    /// relative to `text`.
    #[var]
    pub text_style_opening: GString,
    #[var]
    pub text_style_closing: GString,
    /// Substitute for characters missing from every font asset (0: U+25A1, then '?').
    #[var]
    pub missing_glyph_character: i64,
    /// CJK line-breaking rules: characters that may not start / end a line.
    #[var]
    pub leading_characters: GString,
    #[var]
    pub following_characters: GString,
    /// Ignore `<color>` tags (TMP overrideColorTags).
    #[var]
    pub override_color_tags: bool,
    /// Multiply sprites by the text colour (TMP tintAllSprites).
    #[var]
    pub tint_all_sprites: bool,
    /// With max_visible_*, align vertically on the visible lines (TMP useMaxVisibleDescender).
    #[var]
    pub use_max_visible_descender: bool,
    /// OpenType features: 1 kern, 2 liga, 4 mark, 8 mkmk.
    #[var]
    pub font_features: i32,
    /// VertexSortingOrder: 0 normal, 1 reverse.
    #[var]
    pub geometry_sorting_order: i32,

    pub layout: Option<Layout>,
    pub missing: Vec<(u32, i32)>,
    pub fonts: Option<GdFonts>,
    pub info_cache: Option<Gd<TypesetterTextInfo>>,
    pub dirty: bool,
}

#[godot_api]
impl IRefCounted for TypesetterTextCore {
    fn init(base: Base<RefCounted>) -> Self {
        TypesetterTextCore {
            base,
            text: GString::new(),
            font: None,
            font_size: 36.0,
            font_style: 0,
            font_weight: 400,
            color: Color::WHITE,
            alignment: 0x101,
            text_wrapping_mode: 1,
            overflow_mode: 0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            line_spacing: 0.0,
            paragraph_spacing: 0.0,
            line_spacing_adjustment: 0.0,
            character_width_adjustment: 0.0,
            word_wrapping_ratios: 0.4,
            margin: Vector4::ZERO,
            enable_auto_sizing: false,
            font_size_min: 18.0,
            font_size_max: 72.0,
            rich_text: true,
            parse_ctrl_characters: true,
            max_visible_characters: 99999,
            max_visible_words: 99999,
            max_visible_lines: 99999,
            first_visible_character: 0,
            page_to_display: 1,
            is_right_to_left_text: false,
            extra_padding: false,
            enable_kerning: true,
            rect_size: Vector2::new(200.0, 50.0),
            pivot: Vector2::new(0.5, 0.5),
            world_space: false,
            mesh_padding: 1.25,
            fallback_font_assets: Array::new(),
            font_lookup: VarDictionary::new(),
            material_lookup: VarDictionary::new(),
            style_lookup: VarDictionary::new(),
            gradient_lookup: VarDictionary::new(),
            sprite_assets: Array::new(),
            color_gradient: PackedColorArray::new(),
            horizontal_mapping: 0,
            vertical_mapping: 0,
            mapping_uv_line_offset: 0.0,
            text_style_opening: GString::new(),
            text_style_closing: GString::new(),
            missing_glyph_character: 0,
            leading_characters: GString::new(),
            following_characters: GString::new(),
            override_color_tags: false,
            tint_all_sprites: false,
            use_max_visible_descender: true,
            font_features: 3,
            geometry_sorting_order: 0,
            layout: None,
            missing: Vec::new(),
            fonts: None,
            info_cache: None,
            dirty: true,
        }
    }
}

fn c4(c: Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

#[godot_api]
impl TypesetterTextCore {
    /// Runs layout with the current settings (TMP ForceMeshUpdate's layout half).
    #[func]
    pub fn update(&mut self) {
        self.info_cache = None;
        let Some(font) = self.font.clone() else {
            self.layout = None;
            return;
        };
        let mut fonts = GdFonts::new(font);
        fonts.global_fallbacks = self.fallback_font_assets.iter_shared().collect();
        for (k, v) in self.font_lookup.iter_shared() {
            if let Ok(a) = v.try_to::<Gd<TypesetterFontAsset>>() {
                fonts.by_name.insert(k.to_string(), a);
            }
        }
        for (k, v) in self.material_lookup.iter_shared() {
            if let Ok(m) = v.try_to::<Gd<Material>>() {
                fonts.material_names.insert(k.to_string(), fonts.materials.len() as u32 + 1);
                fonts.materials.push(m);
            }
        }
        for (k, v) in self.style_lookup.iter_shared() {
            if let Ok(a) = v.try_to::<PackedStringArray>() {
                let o = a.get(0).map(|s| s.to_string()).unwrap_or_default();
                let c = a.get(1).map(|s| s.to_string()).unwrap_or_default();
                fonts.styles.insert(k.to_string().to_ascii_lowercase(), (o, c));
            }
        }
        for (k, v) in self.gradient_lookup.iter_shared() {
            if let Ok(a) = v.try_to::<PackedColorArray>() {
                if a.len() >= 4 {
                    fonts.gradients.insert(k.to_string(), [c4(a[0]), c4(a[1]), c4(a[2]), c4(a[3])]);
                }
            }
        }
        fonts.sprites = self.sprite_assets.iter_shared().collect();
        fonts.missing_glyph = self.missing_glyph_character.max(0) as u32;
        let s = self.settings();
        let opening = self.text_style_opening.to_string();
        let closing = self.text_style_closing.to_string();
        let src = self.text.to_string();
        let text = if opening.is_empty() && closing.is_empty() { src.clone() } else { format!("{opening}{src}{closing}") };
        let mut l = layout::layout(&text, &s, &mut fonts);
        // Character indices refer to `text`, not to the style markup around it.
        if !opening.is_empty() {
            let off = opening.chars().count() as i32;
            for c in l.characters.iter_mut() {
                c.index = (c.index - off).max(0);
            }
        }
        if self.override_color_tags {
            let col = c4(self.color);
            for c in l.characters.iter_mut() {
                c.color = [col[0], col[1], col[2], c.color[3]];
            }
        }
        if self.tint_all_sprites {
            for c in l.characters.iter_mut() {
                if let Some(sp) = c.sprite.as_mut() {
                    sp.tint = true;
                }
            }
        }
        self.missing = fonts
            .missing
            .iter()
            .map(|&cp| {
                let idx = src.chars().position(|ch| ch as u32 == cp).map(|i| i as i32).unwrap_or(-1);
                (cp, idx)
            })
            .collect();
        self.layout = Some(l);
        self.fonts = Some(fonts);
        self.dirty = false;
    }

    /// Characters missing from every font asset in the last layout, as
    /// [unicode, string index, unicode, string index, …].
    #[func]
    pub fn get_missing_characters(&self) -> PackedInt32Array {
        self.missing.iter().flat_map(|&(cp, i)| [cp as i32, i]).collect()
    }

    #[func]
    pub fn get_preferred_width(&self) -> f32 {
        self.layout.as_ref().map(|l| l.preferred_width).unwrap_or(0.0)
    }

    #[func]
    pub fn get_preferred_height(&self) -> f32 {
        self.layout.as_ref().map(|l| l.preferred_height).unwrap_or(0.0)
    }

    /// Preferred size for a container of `width` × `height` (TMP GetPreferredValues(w, h)).
    /// Use 0 or INF for an unconstrained axis.
    #[func]
    pub fn get_preferred_values(&mut self, width: f32, height: f32) -> Vector2 {
        let saved = self.rect_size;
        let w = if width <= 0.0 || !width.is_finite() { 1e7 } else { width };
        let h = if height <= 0.0 || !height.is_finite() { 1e7 } else { height };
        self.rect_size = Vector2::new(w, h);
        self.update();
        let pw = self.get_preferred_width();
        let ph = self.get_preferred_height();
        self.rect_size = saved;
        self.update();
        Vector2::new(pw, ph)
    }

    /// Size of the laid-out text (TMP GetRenderedValues). With `only_visible`,
    /// characters hidden by max_visible_* are excluded.
    #[func]
    pub fn get_rendered_values(&self, #[opt(default = true)] only_visible: bool) -> Vector2 {
        let Some(l) = &self.layout else { return Vector2::ZERO };
        if !only_visible {
            let v = l.rendered_size();
            return Vector2::new(v.x, v.y);
        }
        let mut a = Vector2::new(f32::INFINITY, f32::INFINITY);
        let mut b = Vector2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for c in &l.characters {
            if c.is_visible {
                a.x = a.x.min(c.origin);
                b.x = b.x.max(c.x_advance);
                a.y = a.y.min(c.descender);
                b.y = b.y.max(c.ascender);
            }
        }
        if a.x.is_finite() { b - a } else { Vector2::ZERO }
    }

    #[func]
    pub fn is_text_overflowing(&self) -> bool {
        self.layout.as_ref().is_some_and(|l| l.is_text_overflowing)
    }

    #[func]
    pub fn get_first_overflow_character_index(&self) -> i32 {
        self.layout.as_ref().map(|l| l.first_overflow_character_index).unwrap_or(-1)
    }

    /// Font size after auto-sizing.
    #[func]
    pub fn get_resolved_font_size(&self) -> f32 {
        self.layout.as_ref().map(|l| l.font_size).unwrap_or(self.font_size)
    }

    /// Bounds of the text (rendered area) in node space.
    #[func]
    pub fn get_text_bounds(&self) -> Rect2 {
        let Some(l) = &self.layout else { return Rect2::default() };
        let (a, b) = (l.rendered_min, l.rendered_max);
        if !a.x.is_finite() || a.x > b.x {
            return Rect2::default();
        }
        let p0 = self.to_node(a.x, b.y);
        let p1 = self.to_node(b.x, a.y);
        Rect2::new(Vector2::new(p0.x.min(p1.x), p0.y.min(p1.y)), Vector2::new((p1.x - p0.x).abs(), (p1.y - p0.y).abs()))
    }

    #[func]
    pub fn get_text_info(&mut self) -> Gd<TypesetterTextInfo> {
        if let Some(i) = &self.info_cache {
            return i.clone();
        }
        let info = make_text_info(self);
        self.info_cache = Some(info.clone());
        info
    }

    #[func]
    pub fn get_page_count(&self) -> i32 {
        self.layout.as_ref().map(|l| l.pages.len() as i32).unwrap_or(0)
    }

    /// Index of the character at `point` (node space) or -1 (TMP_TextUtilities.FindIntersectingCharacter).
    #[func]
    pub fn find_intersecting_character(&self, point: Vector2, #[opt(default = true)] visible_only: bool) -> i32 {
        let Some(l) = &self.layout else { return -1 };
        for (i, c) in l.characters.iter().enumerate() {
            if visible_only && !c.is_visible {
                continue;
            }
            let p0 = self.to_node(c.origin, c.ascender);
            let p1 = self.to_node(c.x_advance, c.descender);
            let r = Rect2::new(Vector2::new(p0.x.min(p1.x), p0.y.min(p1.y)), Vector2::new((p1.x - p0.x).abs(), (p1.y - p0.y).abs()));
            if r.contains_point(point) {
                return i as i32;
            }
        }
        -1
    }

    /// Nearest character to `point` (TMP_TextUtilities.FindNearestCharacter).
    #[func]
    pub fn find_nearest_character(&self, point: Vector2, #[opt(default = true)] visible_only: bool) -> i32 {
        let Some(l) = &self.layout else { return -1 };
        let mut best = (-1i32, f32::INFINITY);
        for (i, c) in l.characters.iter().enumerate() {
            if visible_only && !c.is_visible {
                continue;
            }
            let p0 = self.to_node(c.origin, c.ascender);
            let p1 = self.to_node(c.x_advance, c.descender);
            let cx = point.x.clamp(p0.x.min(p1.x), p0.x.max(p1.x));
            let cy = point.y.clamp(p0.y.min(p1.y), p0.y.max(p1.y));
            let d = (cx - point.x).powi(2) + (cy - point.y).powi(2);
            if d < best.1 {
                best = (i as i32, d);
            }
        }
        best.0
    }

    /// Link index under `point` or -1 (TMP_TextUtilities.FindIntersectingLink).
    #[func]
    pub fn find_intersecting_link(&self, point: Vector2) -> i32 {
        let Some(l) = &self.layout else { return -1 };
        let ci = self.find_intersecting_character(point, false);
        if ci < 0 {
            return -1;
        }
        let link = l.characters[ci as usize].link;
        if link >= 0 { link } else { -1 }
    }

    /// Line index nearest to `point.y` (TMP_TextUtilities.FindNearestLine).
    #[func]
    pub fn find_nearest_line(&self, point: Vector2) -> i32 {
        let Some(l) = &self.layout else { return -1 };
        let mut best = (-1i32, f32::INFINITY);
        for (i, ln) in l.lines.iter().enumerate() {
            let a = self.to_node(0.0, ln.ascender).y;
            let b = self.to_node(0.0, ln.descender).y;
            let (lo, hi) = (a.min(b), a.max(b));
            let d = if point.y < lo { lo - point.y } else if point.y > hi { point.y - hi } else { 0.0 };
            if d < best.1 {
                best = (i as i32, d);
            }
        }
        best.0
    }

    /// Builds one single-surface ArrayMesh per (font asset, atlas page, material).
    /// Returns `[{ "mesh": ArrayMesh, "texture": Texture2D, "font": TypesetterFontAsset
    /// (or sprite asset), "material": Material|null, "sprite": bool, "kind": int }]`
    /// in draw order. `into` holds meshes from a previous call to reuse (may be empty).
    #[func]
    pub fn build_meshes(&mut self, into: Array<Gd<ArrayMesh>>) -> Array<VarDictionary> {
        let mut out: Array<VarDictionary> = Array::new();
        let Some(l) = self.layout.as_ref() else { return out };
        let Some(fonts) = self.fonts.as_ref() else { return out };
        let mut quads = super::mesh::build_quads(l, self);
        if self.geometry_sorting_order == 1 {
            for q in quads.values_mut() {
                reverse_quads(q);
            }
        }
        for (si, (key, q)) in quads.into_iter().enumerate() {
            let arrays = super::mesh::to_arrays(&q, !self.world_space);
            let fmt = super::mesh::format(!self.world_space);
            let mut mesh = into.get(si).unwrap_or_else(ArrayMesh::new_gd);
            mesh.clear_surfaces();
            mesh.add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays).flags(fmt).done();
            let mut d = VarDictionary::new();
            d.set("mesh", &mesh.to_variant());
            if key.sprite {
                let ai = (key.font - 1_000_000) as usize;
                if let Some(res) = fonts.sprites.get(ai) {
                    let tex: Variant = res.get("sprite_sheet");
                    d.set("texture", &tex);
                    d.set("font", &res.to_variant());
                }
                d.set("sprite", true);
            } else if let Some(a) = fonts.assets.get(key.font as usize) {
                let b = a.bind();
                let tex: Option<Gd<Texture2D>> = b.atlas_textures.get(key.atlas as usize);
                d.set("texture", &tex.map(|t| t.to_variant()).unwrap_or_default());
                d.set("font", &a.to_variant());
                d.set("sprite", false);
            }
            if key.material > 0 {
                if let Some(m) = fonts.materials.get(key.material as usize - 1) {
                    d.set("material", &m.to_variant());
                }
            }
            d.set("vertex_count", q.positions.len() as i64);
            d.set("kind", key.kind as i64);
            out.push(&d);
        }
        out
    }

    /// Builds every surface into one ArrayMesh (3D text: one mesh, one material
    /// per surface). Returns the same per-surface dictionaries as `build_meshes`
    /// with "surface" set to the surface index instead of "mesh".
    #[func]
    pub fn build_mesh(&mut self, mut mesh: Gd<ArrayMesh>) -> Array<VarDictionary> {
        mesh.clear_surfaces();
        let dicts = self.build_meshes(Array::new());
        let mut out: Array<VarDictionary> = Array::new();
        for mut d in dicts.iter_shared() {
            let Some(m) = d.get("mesh").and_then(|v| v.try_to::<Gd<ArrayMesh>>().ok()) else { continue };
            if m.get_surface_count() == 0 {
                continue;
            }
            let arrays = m.surface_get_arrays(0);
            let fmt = m.surface_get_format(0);
            let si = mesh.get_surface_count();
            mesh.add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays).flags(fmt).done();
            d.remove("mesh");
            d.set("surface", si as i64);
            out.push(&d);
        }
        out
    }

    /// Rebuilds meshes from (possibly edited) `text_info.mesh_info` without a new
    /// layout (TMP UpdateVertexData). `meshes` are the ones returned by `build_meshes`.
    #[func]
    pub fn update_vertex_data(&mut self, meshes: Array<Gd<ArrayMesh>>) {
        let Some(info) = self.info_cache.clone() else { return };
        let infos: Vec<Gd<super::info::TypesetterMeshInfo>> = info.bind().mesh_info.iter_shared().collect();
        let is_2d = !self.world_space;
        for (i, mi) in infos.iter().enumerate() {
            let Some(mut mesh) = meshes.get(i) else { continue };
            let b = mi.bind();
            let n = b.vertices.len();
            let mut q = super::mesh::Quads::default();
            for k in 0..n {
                let v = b.vertices[k];
                q.positions.push([v.x, v.y, v.z]);
                let u = b.uvs0.get(k).unwrap_or_default();
                q.uvs.push([u.x, u.y]);
                let c = b.colors32.get(k).unwrap_or(Color::WHITE);
                q.colors.push([c.r, c.g, c.b, c.a]);
                let f = b.uvs2.get(k).unwrap_or_default();
                q.custom0.push([f.x, f.y, u.w, u.z]);
                let c1 = b.custom1.get(k).unwrap_or_default();
                q.custom1.push([c1.x, c1.y, c1.z, c1.w]);
            }
            let arrays = super::mesh::to_arrays(&q, is_2d);
            mesh.clear_surfaces();
            mesh.add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays).flags(super::mesh::format(is_2d)).done();
        }
    }
}

impl TypesetterTextCore {
    pub fn settings(&self) -> Settings {
        let (h, v) = split_alignment(self.alignment);
        let (w, hh) = (self.rect_size.x.max(0.0), self.rect_size.y.max(0.0));
        let rect = if self.world_space { [-w * self.pivot.x, -hh * self.pivot.y, w, hh] } else { [0.0, -hh, w, hh] };
        Settings {
            rect,
            margin: [self.margin.x, self.margin.y, self.margin.z, self.margin.w],
            font: 0,
            font_size: self.font_size,
            font_style: self.font_style as u32,
            font_weight: self.font_weight,
            color: c4(self.color),
            auto_size: self.enable_auto_sizing,
            font_size_min: self.font_size_min,
            font_size_max: self.font_size_max,
            char_width_max_adj: self.character_width_adjustment,
            line_spacing_max_adj: self.line_spacing_adjustment,
            halign: h,
            valign: v,
            wrap: match self.text_wrapping_mode {
                0 => Wrap::NoWrap,
                2 => Wrap::PreserveWhitespace,
                3 => Wrap::PreserveWhitespaceNoWrap,
                _ => Wrap::Normal,
            },
            overflow: match self.overflow_mode {
                1 => Overflow::Ellipsis,
                2 => Overflow::Masking,
                3 => Overflow::Truncate,
                4 => Overflow::ScrollRect,
                5 => Overflow::Page,
                6 => Overflow::Linked,
                _ => Overflow::Overflow,
            },
            character_spacing: self.character_spacing,
            word_spacing: self.word_spacing,
            line_spacing: self.line_spacing,
            paragraph_spacing: self.paragraph_spacing,
            word_wrapping_ratios: self.word_wrapping_ratios,
            rich_text: self.rich_text,
            parse_ctrl_characters: self.parse_ctrl_characters,
            max_visible_characters: self.max_visible_characters,
            max_visible_words: self.max_visible_words,
            max_visible_lines: self.max_visible_lines,
            first_visible_character: self.first_visible_character,
            page_to_display: self.page_to_display,
            world_space: self.world_space,
            right_to_left: self.is_right_to_left_text,
            extra_padding: self.extra_padding,
            padding: self.mesh_padding,
            kerning: self.enable_kerning && (self.font_features & 1) != 0,
        }
    }

    /// Layout (TMP) space → node space.
    pub fn to_node(&self, x: f32, y: f32) -> Vector2 {
        if self.world_space { Vector2::new(x, y) } else { Vector2::new(x, -y) }
    }
}


fn reverse_quads(q: &mut super::mesh::Quads) {
    let n = q.positions.len() / 4;
    let rev4 = |v: &mut Vec<[f32; 4]>| {
        let mut out = Vec::with_capacity(v.len());
        for k in (0..n).rev() {
            out.extend_from_slice(&v[k * 4..k * 4 + 4]);
        }
        *v = out;
    };
    let mut p = Vec::with_capacity(q.positions.len());
    let mut u = Vec::with_capacity(q.uvs.len());
    for k in (0..n).rev() {
        p.extend_from_slice(&q.positions[k * 4..k * 4 + 4]);
        u.extend_from_slice(&q.uvs[k * 4..k * 4 + 4]);
    }
    q.positions = p;
    q.uvs = u;
    rev4(&mut q.colors);
    rev4(&mut q.custom0);
    rev4(&mut q.custom1);
    q.owners.reverse();
}

// Silence unused import warnings for types only referenced in docs.
#[allow(dead_code)]
fn _types(_: ArrayFormat, _: ArrayType) {}
