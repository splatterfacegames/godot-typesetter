//! `TypesetterFontAsset` (TMP_FontAsset) and its satellite types.
//!
//! Serialized state lives in packed arrays (compact, fast to load); a runtime
//! index (character → glyph, glyph records, pair adjustments, ligatures and the
//! atlas pages' pixels) is rebuilt lazily from them. Dynamic assets keep their
//! source `FontFile` and add glyphs on demand.

use std::collections::HashMap;

use godot::classes::image::Format;
use godot::classes::{FontFile, IResource, Image, ImageTexture, Resource, Texture2D};
use godot::prelude::*;

use crate::core::atlas::{self, AtlasPage, GenSettings, RenderMode};
use crate::core::font::Face;
use crate::core::pack::{Heuristic, MaxRects, Rect};

pub const GLYPH_STRIDE: usize = 12; // index, w, h, bx, by, adv, rx, ry, rw, rh, scale, atlas
pub const PAIR_STRIDE: usize = 10; // first, second, f.xp, f.yp, f.xa, f.ya, s.xp, s.yp, s.xa, s.ya

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphRec {
    pub index: u32,
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub rect: [i32; 4],
    pub scale: f32,
    pub atlas_index: i32,
}

#[derive(Default)]
pub struct Runtime {
    pub built: bool,
    pub chars: HashMap<u32, u32>,
    pub glyphs: HashMap<u32, GlyphRec>,
    pub pairs: HashMap<(u32, u32), [f32; 8]>,
    /// first component → [(remaining components, ligature glyph)]
    pub ligatures: HashMap<u32, Vec<(Vec<u32>, u32)>>,
    pub face: Option<Face>,
    pub face_failed: bool,
    pub pages: Vec<AtlasPage>,
    pub pages_loaded: bool,
}

// ------------------------------------------------------------------ FaceInfo

/// Face-wide metrics at the asset's sampling point size (TMP's `FaceInfo`).
/// Distances are in pixels at `point_size`, y-up from the baseline.
#[derive(GodotClass)]
#[class(tool, init, base = Resource)]
pub struct TypesetterFaceInfo {
    base: Base<Resource>,
    #[export]
    pub face_index: i32,
    #[export]
    pub family_name: GString,
    #[export]
    pub style_name: GString,
    #[export]
    pub point_size: f32,
    #[export]
    #[init(val = 1.0)]
    pub scale: f32,
    #[export]
    pub units_per_em: i32,
    #[export]
    pub line_height: f32,
    #[export]
    pub ascent_line: f32,
    #[export]
    pub cap_line: f32,
    #[export]
    pub mean_line: f32,
    #[export]
    pub baseline: f32,
    #[export]
    pub descent_line: f32,
    #[export]
    pub superscript_offset: f32,
    #[export]
    pub superscript_size: f32,
    #[export]
    pub subscript_offset: f32,
    #[export]
    pub subscript_size: f32,
    #[export]
    pub underline_offset: f32,
    #[export]
    pub underline_thickness: f32,
    #[export]
    pub strikethrough_offset: f32,
    #[export]
    pub strikethrough_thickness: f32,
    #[export]
    pub tab_width: f32,
}

#[godot_api]
impl TypesetterFaceInfo {}

impl TypesetterFaceInfo {
    pub fn fill_from(&mut self, face: &Face, point_size: f32) {
        let m = face.metrics();
        let s = point_size as f64 / m.units_per_em.max(1.0);
        self.family_name = m.family_name.as_str().into();
        self.style_name = m.style_name.as_str().into();
        self.point_size = point_size;
        self.scale = 1.0;
        self.units_per_em = m.units_per_em as i32;
        self.ascent_line = (m.ascender * s) as f32;
        self.descent_line = (m.descender * s) as f32;
        self.line_height = ((m.ascender - m.descender + m.line_gap) * s) as f32;
        self.cap_line = (m.cap_height * s) as f32;
        self.mean_line = (m.x_height * s) as f32;
        self.baseline = 0.0;
        self.superscript_offset = (m.superscript_offset * s) as f32;
        self.superscript_size = (m.superscript_size / m.units_per_em.max(1.0)) as f32;
        self.subscript_offset = (m.subscript_offset * s) as f32;
        self.subscript_size = (m.subscript_size / m.units_per_em.max(1.0)) as f32;
        self.underline_offset = (m.underline_offset * s) as f32;
        self.underline_thickness = (m.underline_thickness * s) as f32;
        self.strikethrough_offset = (m.strikethrough_offset * s) as f32;
        self.strikethrough_thickness = (m.strikethrough_thickness * s) as f32;
        self.tab_width = (m.tab_width / 10.0 * s).floor() as f32; // whole units, as TMP reports
    }
}

// ------------------------------------------------------------------ Glyph / Character

/// A glyph record (TextCore's `Glyph`): metrics at the sampling point size and
/// its rectangle in an atlas page (top-left origin). A snapshot: edits do not
/// write back to the font asset.
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct TypesetterGlyph {
    base: Base<RefCounted>,
    #[var]
    pub index: i64,
    /// width, height, horizontal_bearing_x, horizontal_bearing_y, horizontal_advance
    #[var]
    pub metrics: VarDictionary,
    #[var]
    pub glyph_rect: Rect2i,
    #[var]
    pub scale: f32,
    #[var]
    pub atlas_index: i32,
}

#[godot_api]
impl TypesetterGlyph {}

/// A character → glyph mapping (TMP_Character).
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct TypesetterCharacter {
    base: Base<RefCounted>,
    #[var]
    pub unicode: i64,
    #[var]
    pub glyph_index: i64,
    #[var]
    pub scale: f32,
    #[var]
    pub element_type: i32,
}

#[godot_api]
impl TypesetterCharacter {}

// ------------------------------------------------------------------ FontAsset

/// A signed-distance-field (or bitmap) font asset: face metrics, glyph and
/// character tables, OpenType pair adjustments and ligatures, and one or more
/// atlas textures. TextMesh Pro equivalent: `TMP_FontAsset`.
#[derive(GodotClass)]
#[class(tool, base = Resource)]
pub struct TypesetterFontAsset {
    base: Base<Resource>,

    /// Font the atlas is generated from. Dynamic assets need it at run time.
    #[export]
    pub source_font_file: Option<Gd<FontFile>>,
    #[export]
    pub face_info: Option<Gd<TypesetterFaceInfo>>,
    /// STATIC = 0, DYNAMIC = 1, DYNAMIC_OS = 2.
    #[export(enum = (Static = 0, Dynamic = 1, DynamicOS = 2))]
    pub atlas_population_mode: i32,
    /// GlyphRenderMode (see `TypesetterFontAsset.GlyphRenderMode` in GDScript docs).
    #[export(enum = (Smooth = 0, SmoothHinted = 1, Raster = 2, RasterHinted = 3, SDF = 4, SDF8 = 5, SDF16 = 6, SDF32 = 7, SDFAA = 8, SDFAAHinted = 9, MSDF = 10, MTSDF = 11))]
    pub atlas_render_mode: i32,
    #[export]
    pub atlas_width: i32,
    #[export]
    pub atlas_height: i32,
    #[export]
    pub atlas_padding: i32,
    /// GlyphPackingMode used for new glyphs (BestShortSideFit = 0 … ContactPointRule = 4).
    #[export]
    pub packing_mode: i32,
    #[export]
    pub is_multi_atlas_textures_enabled: bool,
    #[export]
    pub clear_dynamic_data_on_build: bool,
    #[export]
    pub atlas_textures: Array<Gd<Texture2D>>,
    #[export]
    pub fallback_font_asset_table: Array<Gd<TypesetterFontAsset>>,
    /// Ten entries (weights 100..900 in steps of 100, index 0 unused), each
    /// `[regular: TypesetterFontAsset, italic: TypesetterFontAsset]`.
    #[export]
    pub font_weight_table: VarArray,
    #[export]
    pub normal_style: f32,
    #[export]
    pub normal_spacing_offset: f32,
    #[export]
    pub bold_style: f32,
    #[export]
    pub bold_spacing: f32,
    #[export]
    pub italic_style: f32,
    #[export]
    pub tab_size: f32,
    /// Default material (a ShaderMaterial using a Typesetter distance-field shader).
    #[export]
    pub material: Option<Gd<godot::classes::Material>>,
    #[export]
    pub creation_settings: VarDictionary,

    #[export]
    #[var(usage_flags = [STORAGE])]
    pub glyph_data: PackedFloat32Array,
    #[export]
    #[var(usage_flags = [STORAGE])]
    pub character_data: PackedInt32Array,
    #[export]
    #[var(usage_flags = [STORAGE])]
    pub pair_adjustment_data: PackedFloat32Array,
    /// True once `update_font_feature_table` ran: later glyph additions read
    /// GPOS/GSUB instead of only the legacy kern table.
    #[export]
    pub full_font_features: bool,
    /// Ligatures as runs: [n, component_1 .. component_n, ligature, ...].
    #[export]
    #[var(usage_flags = [STORAGE])]
    pub ligature_data: PackedInt32Array,

    pub rt: Runtime,
}

#[godot_api]
impl IResource for TypesetterFontAsset {
    fn init(base: Base<Resource>) -> Self {
        let mut weights = VarArray::new();
        for _ in 0..10 {
            weights.push(&VarArray::new().to_variant());
        }
        TypesetterFontAsset {
            base,
            source_font_file: None,
            face_info: None,
            atlas_population_mode: 1,
            atlas_render_mode: 8,
            atlas_width: 1024,
            atlas_height: 1024,
            atlas_padding: 9,
            packing_mode: 0,
            is_multi_atlas_textures_enabled: true,
            clear_dynamic_data_on_build: false,
            atlas_textures: Array::new(),
            fallback_font_asset_table: Array::new(),
            font_weight_table: weights,
            normal_style: 0.0,
            normal_spacing_offset: 0.0,
            bold_style: 0.75,
            bold_spacing: 7.0,
            italic_style: 35.0,
            tab_size: 10.0,
            material: None,
            creation_settings: VarDictionary::new(),
            glyph_data: PackedFloat32Array::new(),
            character_data: PackedInt32Array::new(),
            pair_adjustment_data: PackedFloat32Array::new(),
            full_font_features: false,
            ligature_data: PackedInt32Array::new(),
            rt: Runtime::default(),
        }
    }
}

#[godot_api]
impl TypesetterFontAsset {
    #[constant]
    pub const STATIC: i32 = 0;
    #[constant]
    pub const DYNAMIC: i32 = 1;
    #[constant]
    pub const DYNAMIC_OS: i32 = 2;

    #[constant]
    pub const SMOOTH: i32 = 0;
    #[constant]
    pub const SMOOTH_HINTED: i32 = 1;
    #[constant]
    pub const RASTER: i32 = 2;
    #[constant]
    pub const RASTER_HINTED: i32 = 3;
    #[constant]
    pub const SDF: i32 = 4;
    #[constant]
    pub const SDF8: i32 = 5;
    #[constant]
    pub const SDF16: i32 = 6;
    #[constant]
    pub const SDF32: i32 = 7;
    #[constant]
    pub const SDFAA: i32 = 8;
    #[constant]
    pub const SDFAA_HINTED: i32 = 9;
    #[constant]
    pub const MSDF: i32 = 10;
    #[constant]
    pub const MTSDF: i32 = 11;

    /// Emitted after glyphs are added or the atlas changes.
    #[signal]
    fn atlas_updated();

    /// Creates a font asset from a `FontFile` (TMP_FontAsset.CreateFontAsset).
    #[func]
    pub fn create_font_asset(
        font: Gd<FontFile>,
        #[opt(default = 90)] sampling_point_size: i32,
        #[opt(default = 9)] atlas_padding: i32,
        #[opt(default = 8)] render_mode: i32,
        #[opt(default = 1024)] atlas_width: i32,
        #[opt(default = 1024)] atlas_height: i32,
        #[opt(default = 1)] atlas_population_mode: i32,
        #[opt(default = true)] enable_multi_atlas_support: bool,
    ) -> Option<Gd<TypesetterFontAsset>> {
        let data = font.get_data();
        let face = match Face::new(data.to_vec(), 0) {
            Ok(f) => f,
            Err(e) => {
                godot_error!("TypesetterFontAsset.create_font_asset: {e}");
                return None;
            }
        };
        let mut asset = TypesetterFontAsset::new_gd();
        {
            let mut a = asset.bind_mut();
            a.source_font_file = Some(font.clone());
            a.atlas_render_mode = render_mode;
            a.atlas_width = atlas_width.max(16);
            a.atlas_height = atlas_height.max(16);
            a.atlas_padding = atlas_padding.max(0);
            a.atlas_population_mode = atlas_population_mode;
            a.is_multi_atlas_textures_enabled = enable_multi_atlas_support;
            let mut fi = TypesetterFaceInfo::new_gd();
            fi.bind_mut().fill_from(&face, sampling_point_size as f32);
            a.face_info = Some(fi);
            let mut cs = VarDictionary::new();
            cs.set("point_size", sampling_point_size);
            cs.set("padding", atlas_padding);
            cs.set("render_mode", render_mode);
            cs.set("atlas_width", atlas_width);
            cs.set("atlas_height", atlas_height);
            a.creation_settings = cs;
            let name = face.metrics().family_name;
            let style = face.metrics().style_name;
            a.rt.face = Some(face);
            drop(a);
            asset.set_name(&format!("{name} {style} SDF"));
        }
        Some(asset)
    }

    /// Loads a font from a file path and creates an asset (TMP's path overload).
    #[func]
    pub fn create_font_asset_from_path(
        path: GString,
        #[opt(default = 0)] _face_index: i32,
        #[opt(default = 90)] sampling_point_size: i32,
        #[opt(default = 9)] atlas_padding: i32,
        #[opt(default = 8)] render_mode: i32,
        #[opt(default = 1024)] atlas_width: i32,
        #[opt(default = 1024)] atlas_height: i32,
    ) -> Option<Gd<TypesetterFontAsset>> {
        let mut ff = FontFile::new_gd();
        if ff.load_dynamic_font(&path) != godot::global::Error::OK {
            godot_error!("TypesetterFontAsset: cannot load font {path}");
            return None;
        }
        Self::create_font_asset(ff, sampling_point_size, atlas_padding, render_mode, atlas_width, atlas_height, 1, true)
    }

    /// Largest sampling point size at which all `characters` fit in one atlas
    /// of this asset's size, padding and packing (the Font Asset Creator's
    /// "Auto Sizing"). `fast` uses a coarser search (TMP's Fast packing).
    #[func]
    pub fn find_auto_point_size(&mut self, characters: GString, #[opt(default = false)] fast: bool) -> i32 {
        self.ensure_runtime();
        let Some(face) = self.rt.face.clone() else { return 0 };
        let mut gids: Vec<u32> = characters.to_string().chars().filter_map(|c| face.glyph_id(c as u32)).collect();
        gids.sort();
        gids.dedup();
        let s = self.settings();
        atlas::auto_point_size(&face, &gids, &s, fast)
    }

    /// Sets the sampling point size (and face info) of an empty asset before
    /// glyphs are added.
    #[func]
    pub fn set_sampling_point_size(&mut self, point_size: i32) {
        self.ensure_runtime();
        let Some(face) = self.rt.face.clone() else { return };
        let mut fi = TypesetterFaceInfo::new_gd();
        fi.bind_mut().fill_from(&face, point_size as f32);
        self.face_info = Some(fi);
        self.creation_settings.set("point_size", point_size);
    }

    /// Reads the font's OpenType features (GPOS pair adjustments, GSUB
    /// ligatures) for the glyphs in the asset into the feature table, as the
    /// Font Asset Creator's "Get Font Features" does. Later additions keep
    /// using the full tables.
    #[func]
    pub fn update_font_feature_table(&mut self) {
        self.ensure_runtime();
        self.full_font_features = true;
        let Some(face) = self.rt.face.clone() else { return };
        // Pull in ligature glyphs for sequences the asset already covers.
        let mut all: Vec<u32> = self.rt.glyphs.keys().copied().collect();
        all.sort();
        let ligs: Vec<u32> = face.ligatures_from(&all).iter().map(|l| l.ligature).filter(|g| !self.rt.glyphs.contains_key(g)).collect();
        if !ligs.is_empty() {
            self.load_pages();
            let s = self.settings();
            let mut pages = std::mem::take(&mut self.rt.pages);
            let res = atlas::add_glyphs(&face, &ligs, &s, &mut pages);
            self.rt.pages = pages;
            for g in &res.glyphs {
                self.rt.glyphs.insert(g.index, GlyphRec { index: g.index, width: g.width, height: g.height, bearing_x: g.bearing_x, bearing_y: g.bearing_y, advance: g.advance, rect: [g.rect.x, g.rect.y, g.rect.w, g.rect.h], scale: 1.0, atlas_index: g.atlas_index });
            }
            self.upload_pages();
        }
        self.refresh_features(&face, true);
        self.store_tables();
        self.base_mut().emit_changed();
    }

    /// The first atlas texture (TMP `atlasTexture`).
    #[func]
    pub fn get_atlas_texture(&self) -> Option<Gd<Texture2D>> {
        self.atlas_textures.get(0)
    }

    #[func]
    pub fn get_atlas_texture_count(&self) -> i32 {
        self.atlas_textures.len() as i32
    }

    /// `gradient_scale` used by the distance-field shaders: padding + 1.
    #[func]
    pub fn get_gradient_scale(&self) -> f32 {
        self.atlas_padding as f32 + 1.0
    }

    #[func]
    pub fn get_glyph_table(&mut self) -> Array<Gd<TypesetterGlyph>> {
        self.ensure_runtime();
        let mut v: Vec<&GlyphRec> = self.rt.glyphs.values().collect();
        v.sort_by_key(|g| g.index);
        let mut out = Array::new();
        for g in v {
            let mut o = TypesetterGlyph::new_gd();
            {
                let mut b = o.bind_mut();
                b.index = g.index as i64;
                let mut m = VarDictionary::new();
                m.set("width", g.width);
                m.set("height", g.height);
                m.set("horizontal_bearing_x", g.bearing_x);
                m.set("horizontal_bearing_y", g.bearing_y);
                m.set("horizontal_advance", g.advance);
                b.metrics = m;
                b.glyph_rect = Rect2i::new(Vector2i::new(g.rect[0], g.rect[1]), Vector2i::new(g.rect[2], g.rect[3]));
                b.scale = g.scale;
                b.atlas_index = g.atlas_index;
            }
            out.push(&o);
        }
        out
    }

    #[func]
    pub fn get_character_table(&mut self) -> Array<Gd<TypesetterCharacter>> {
        self.ensure_runtime();
        let mut v: Vec<(&u32, &u32)> = self.rt.chars.iter().collect();
        v.sort();
        let mut out = Array::new();
        for (&u, &g) in v {
            let mut o = TypesetterCharacter::new_gd();
            {
                let mut b = o.bind_mut();
                b.unicode = u as i64;
                b.glyph_index = g as i64;
                b.scale = 1.0;
                b.element_type = 0;
            }
            out.push(&o);
        }
        out
    }

    /// Pair adjustments and ligatures (TMP `fontFeatureTable`) as plain data:
    /// `{ "glyph_pair_adjustment_records": [{first_glyph, second_glyph,
    /// first: {x_placement, y_placement, x_advance, y_advance}, second: {...}}],
    /// "ligature_records": [{component_glyph_ids, ligature_glyph_id}] }`.
    #[func]
    pub fn get_font_feature_table(&mut self) -> VarDictionary {
        self.ensure_runtime();
        let vr = |a: &[f32]| {
            let mut d = VarDictionary::new();
            d.set("x_placement", a[0]);
            d.set("y_placement", a[1]);
            d.set("x_advance", a[2]);
            d.set("y_advance", a[3]);
            d
        };
        let mut pairs: Vec<(&(u32, u32), &[f32; 8])> = self.rt.pairs.iter().collect();
        pairs.sort_by_key(|p| *p.0);
        let mut arr = VarArray::new();
        for ((f, s), v) in pairs {
            let mut d = VarDictionary::new();
            d.set("first_glyph", *f as i64);
            d.set("second_glyph", *s as i64);
            d.set("first", &vr(&v[0..4]));
            d.set("second", &vr(&v[4..8]));
            arr.push(&d.to_variant());
        }
        let mut ligs = VarArray::new();
        for (first, list) in &self.rt.ligatures {
            for (rest, lig) in list {
                let mut comps = PackedInt32Array::new();
                comps.push(*first as i32);
                for c in rest {
                    comps.push(*c as i32);
                }
                let mut d = VarDictionary::new();
                d.set("component_glyph_ids", &comps);
                d.set("ligature_glyph_id", *lig as i64);
                ligs.push(&d.to_variant());
            }
        }
        let mut out = VarDictionary::new();
        out.set("glyph_pair_adjustment_records", &arr);
        out.set("ligature_records", &ligs);
        out
    }

    /// Adds the characters of `characters` (TMP TryAddCharacters). Returns true
    /// when every character is now present.
    #[func]
    pub fn try_add_characters(&mut self, characters: GString, #[opt(default = false)] include_font_features: bool) -> bool {
        let cps: Vec<u32> = characters.to_string().chars().map(|c| c as u32).collect();
        self.add_codepoints(&cps, include_font_features).is_empty()
    }

    /// Like `try_add_characters` but returns the characters that could not be added.
    #[func]
    pub fn try_add_characters_get_missing(&mut self, characters: GString, #[opt(default = false)] include_font_features: bool) -> GString {
        let cps: Vec<u32> = characters.to_string().chars().map(|c| c as u32).collect();
        let missing = self.add_codepoints(&cps, include_font_features);
        missing.iter().filter_map(|&c| char::from_u32(c)).collect::<String>().as_str().into()
    }

    /// Unicode code point overload of `try_add_characters`.
    #[func]
    pub fn try_add_unicodes(&mut self, unicodes: PackedInt32Array, #[opt(default = false)] include_font_features: bool) -> bool {
        let cps: Vec<u32> = unicodes.as_slice().iter().map(|&c| c as u32).collect();
        self.add_codepoints(&cps, include_font_features).is_empty()
    }

    /// TMP HasCharacter: `character` is a code point. With `try_add_character`
    /// a dynamic asset generates the glyph if the source font has it.
    #[func]
    pub fn has_character(&mut self, character: i64, #[opt(default = false)] search_fallbacks: bool, #[opt(default = false)] try_add_character: bool) -> bool {
        let cp = character as u32;
        self.ensure_runtime();
        if self.rt.chars.contains_key(&cp) {
            return true;
        }
        if try_add_character && self.atlas_population_mode != Self::STATIC && self.add_codepoints(&[cp], true).is_empty() {
            return true;
        }
        if search_fallbacks {
            let fallbacks: Vec<Gd<TypesetterFontAsset>> = self.fallback_font_asset_table.iter_shared().collect();
            for mut f in fallbacks {
                if f.bind_mut().has_character(character, true, try_add_character) {
                    return true;
                }
            }
        }
        false
    }

    /// True when every character of `text` is in the asset.
    #[func]
    pub fn has_characters(&mut self, text: GString) -> bool {
        self.ensure_runtime();
        text.to_string().chars().all(|c| self.rt.chars.contains_key(&(c as u32)))
    }

    /// Code points of all characters in the asset, as a string.
    #[func]
    pub fn get_characters(&mut self) -> GString {
        self.ensure_runtime();
        let mut v: Vec<u32> = self.rt.chars.keys().copied().collect();
        v.sort();
        v.iter().filter_map(|&c| char::from_u32(c)).collect::<String>().as_str().into()
    }

    /// Removes all glyphs, characters, features and atlas content (TMP ClearFontAssetData).
    #[func]
    pub fn clear_font_asset_data(&mut self, #[opt(default = false)] set_atlas_size_to_zero: bool) {
        self.glyph_data = PackedFloat32Array::new();
        self.character_data = PackedInt32Array::new();
        self.pair_adjustment_data = PackedFloat32Array::new();
        self.ligature_data = PackedInt32Array::new();
        self.atlas_textures = Array::new();
        if set_atlas_size_to_zero {
            self.atlas_width = 0;
            self.atlas_height = 0;
        }
        let face = self.rt.face.take();
        self.rt = Runtime { face, ..Default::default() };
        self.base_mut().emit_changed();
    }

    /// Re-reads the packed tables after they were edited directly.
    #[func]
    pub fn read_font_asset_definition(&mut self) {
        let face = self.rt.face.take();
        self.rt = Runtime { face, ..Default::default() };
        self.ensure_runtime();
    }

    /// Point size the atlas was sampled at.
    #[func]
    pub fn get_sampling_point_size(&self) -> f32 {
        self.face_info.as_ref().map(|f| f.bind().point_size).unwrap_or(90.0)
    }

    /// Glyph index for a code point in this asset only (0 when absent).
    #[func]
    pub fn get_glyph_index(&mut self, unicode: i64) -> i64 {
        self.ensure_runtime();
        self.rt.chars.get(&(unicode as u32)).copied().unwrap_or(0) as i64
    }
}

impl TypesetterFontAsset {
    pub fn settings(&self) -> GenSettings {
        GenSettings {
            point_size: self.get_sampling_point_size() as f64,
            padding: self.atlas_padding,
            mode: RenderMode::from_i32(self.atlas_render_mode).unwrap_or(RenderMode::Sdfaa),
            width: self.atlas_width.max(16),
            height: self.atlas_height.max(16),
            heuristic: Heuristic::from_i32(self.packing_mode),
            multi_atlas: self.is_multi_atlas_textures_enabled,
        }
    }

    pub fn ensure_runtime(&mut self) {
        if self.rt.built {
            return;
        }
        self.rt.built = true;
        let g = self.glyph_data.as_slice();
        for rec in g.chunks_exact(GLYPH_STRIDE) {
            let gr = GlyphRec {
                index: rec[0] as u32,
                width: rec[1],
                height: rec[2],
                bearing_x: rec[3],
                bearing_y: rec[4],
                advance: rec[5],
                rect: [rec[6] as i32, rec[7] as i32, rec[8] as i32, rec[9] as i32],
                scale: rec[10],
                atlas_index: rec[11] as i32,
            };
            self.rt.glyphs.insert(gr.index, gr);
        }
        for c in self.character_data.as_slice().chunks_exact(2) {
            self.rt.chars.insert(c[0] as u32, c[1] as u32);
        }
        for p in self.pair_adjustment_data.as_slice().chunks_exact(PAIR_STRIDE) {
            let mut v = [0f32; 8];
            v.copy_from_slice(&p[2..10]);
            self.rt.pairs.insert((p[0] as u32, p[1] as u32), v);
        }
        let l = self.ligature_data.as_slice();
        let mut i = 0;
        while i < l.len() {
            let n = l[i].max(0) as usize;
            if i + 1 + n >= l.len() + 1 || n == 0 {
                break;
            }
            let comps: Vec<u32> = l[i + 1..i + 1 + n].iter().map(|&c| c as u32).collect();
            let lig = l.get(i + 1 + n).copied().unwrap_or(0) as u32;
            self.rt.ligatures.entry(comps[0]).or_default().push((comps[1..].to_vec(), lig));
            i += n + 2;
        }
    }

    pub fn face(&mut self) -> Option<&Face> {
        if self.rt.face.is_none() && !self.rt.face_failed {
            if let Some(ff) = &self.source_font_file {
                let data = ff.get_data();
                match Face::new(data.to_vec(), 0) {
                    Ok(f) => self.rt.face = Some(f),
                    Err(e) => {
                        self.rt.face_failed = true;
                        godot_warn!("TypesetterFontAsset: {e}");
                    }
                }
            } else {
                self.rt.face_failed = true;
            }
        }
        self.rt.face.as_ref()
    }

    fn load_pages(&mut self) {
        if self.rt.pages_loaded {
            return;
        }
        self.rt.pages_loaded = true;
        let s = self.settings();
        let ch = s.mode.channels();
        let pad = s.padding.max(0);
        let mut used: Vec<Vec<Rect>> = vec![Vec::new(); self.atlas_textures.len()];
        for g in self.rt.glyphs.values() {
            if g.rect[2] > 0 && (g.atlas_index as usize) < used.len() {
                used[g.atlas_index as usize].push(Rect::new(g.rect[0] - pad - 1, g.rect[1] - pad - 1, g.rect[2] + 2 * pad + 1, g.rect[3] + 2 * pad + 1));
            }
        }
        let mut pages = Vec::new();
        for (i, tex) in self.atlas_textures.iter_shared().enumerate() {
            let Some(img) = tex.get_image() else { continue };
            let (w, h) = (img.get_width(), img.get_height());
            let mut img = img;
            let want = if ch == 4 { Format::RGBA8 } else { Format::R8 };
            if img.get_format() != want {
                img.convert(want);
            }
            let data = img.get_data();
            pages.push(AtlasPage { width: w, height: h, channels: ch, pixels: data.to_vec(), packer: MaxRects::with_used(w, h, &used[i]) });
        }
        self.rt.pages = pages;
    }

    /// Generates glyphs for `codepoints` not yet present. Returns the code points
    /// that could not be added (no glyph in the font, static asset, or atlas full).
    pub fn add_codepoints(&mut self, codepoints: &[u32], include_font_features: bool) -> Vec<u32> {
        self.ensure_runtime();
        let mut wanted: Vec<u32> = codepoints.iter().copied().filter(|c| !self.rt.chars.contains_key(c)).collect();
        wanted.sort();
        wanted.dedup();
        if wanted.is_empty() {
            return Vec::new();
        }
        if self.face().is_none() {
            return wanted;
        }
        let face = self.rt.face.clone().unwrap();
        let mut missing = Vec::new();
        let mut new_chars: Vec<(u32, u32)> = Vec::new();
        let mut new_gids: Vec<u32> = Vec::new();
        for &c in &wanted {
            match face.glyph_id(c) {
                Some(g) => {
                    new_chars.push((c, g));
                    if !self.rt.glyphs.contains_key(&g) && !new_gids.contains(&g) {
                        new_gids.push(g);
                    }
                }
                None => missing.push(c),
            }
        }
        // Ligature glyphs for liga sequences fully covered by the asset's characters
        // (only when the full OpenType feature table is in use).
        if include_font_features && self.full_font_features {
            let mut all: Vec<u32> = self.rt.glyphs.keys().copied().chain(new_gids.iter().copied()).collect();
            all.sort();
            for lig in face.ligatures_from(&all) {
                if !self.rt.glyphs.contains_key(&lig.ligature) && !new_gids.contains(&lig.ligature) {
                    new_gids.push(lig.ligature);
                }
            }
        }
        if !new_gids.is_empty() {
            self.load_pages();
            let s = self.settings();
            let mut pages = std::mem::take(&mut self.rt.pages);
            let res = atlas::add_glyphs(&face, &new_gids, &s, &mut pages);
            self.rt.pages = pages;
            for g in &res.glyphs {
                self.rt.glyphs.insert(
                    g.index,
                    GlyphRec {
                        index: g.index,
                        width: g.width,
                        height: g.height,
                        bearing_x: g.bearing_x,
                        bearing_y: g.bearing_y,
                        advance: g.advance,
                        rect: [g.rect.x, g.rect.y, g.rect.w, g.rect.h],
                        scale: 1.0,
                        atlas_index: g.atlas_index,
                    },
                );
            }
            for &(c, g) in &new_chars {
                if res.missing.contains(&g) {
                    missing.push(c);
                }
            }
            new_chars.retain(|(_, g)| !res.missing.contains(g));
            self.upload_pages();
        }
        for (c, g) in new_chars {
            self.rt.chars.insert(c, g);
        }
        if include_font_features {
            let full = self.full_font_features;
            self.refresh_features(&face, full);
        }
        self.store_tables();
        self.base_mut().emit_changed();
        self.base_mut().emit_signal("atlas_updated", &[]);
        missing.sort();
        missing
    }

    fn refresh_features(&mut self, face: &Face, full: bool) {
        let gids: Vec<u32> = {
            let mut v: Vec<u32> = self.rt.glyphs.keys().copied().collect();
            v.sort();
            v
        };
        let s = self.get_sampling_point_size() as f64 / face.metrics().units_per_em.max(1.0);
        self.rt.pairs.clear();
        let pairs = if full { face.pair_adjustments(&gids) } else { face.legacy_pair_adjustments(&gids) };
        for p in pairs {
            let f = &p.first_adj;
            let q = &p.second_adj;
            self.rt.pairs.insert(
                (p.first, p.second),
                [f.x_placement, f.y_placement, f.x_advance, f.y_advance, q.x_placement, q.y_placement, q.x_advance, q.y_advance].map(|v| (v * s) as f32),
            );
        }
        self.rt.ligatures.clear();
        if !full {
            return;
        }
        for l in face.ligatures(&gids) {
            self.rt.ligatures.entry(l.components[0]).or_default().push((l.components[1..].to_vec(), l.ligature));
        }
    }

    fn store_tables(&mut self) {
        let mut g: Vec<&GlyphRec> = self.rt.glyphs.values().collect();
        g.sort_by_key(|r| r.index);
        let mut gd = Vec::with_capacity(g.len() * GLYPH_STRIDE);
        for r in g {
            gd.extend_from_slice(&[
                r.index as f32,
                r.width,
                r.height,
                r.bearing_x,
                r.bearing_y,
                r.advance,
                r.rect[0] as f32,
                r.rect[1] as f32,
                r.rect[2] as f32,
                r.rect[3] as f32,
                r.scale,
                r.atlas_index as f32,
            ]);
        }
        self.glyph_data = PackedFloat32Array::from(gd.as_slice());
        let mut c: Vec<(u32, u32)> = self.rt.chars.iter().map(|(a, b)| (*a, *b)).collect();
        c.sort();
        let cd: Vec<i32> = c.iter().flat_map(|(a, b)| [*a as i32, *b as i32]).collect();
        self.character_data = PackedInt32Array::from(cd.as_slice());
        let mut p: Vec<(&(u32, u32), &[f32; 8])> = self.rt.pairs.iter().collect();
        p.sort_by_key(|x| *x.0);
        let mut pd = Vec::with_capacity(p.len() * PAIR_STRIDE);
        for ((a, b), v) in p {
            pd.push(*a as f32);
            pd.push(*b as f32);
            pd.extend_from_slice(v);
        }
        self.pair_adjustment_data = PackedFloat32Array::from(pd.as_slice());
        let mut ld = Vec::new();
        for (first, list) in &self.rt.ligatures {
            for (rest, lig) in list {
                ld.push(rest.len() as i32 + 1);
                ld.push(*first as i32);
                ld.extend(rest.iter().map(|&c| c as i32));
                ld.push(*lig as i32);
            }
        }
        self.ligature_data = PackedInt32Array::from(ld.as_slice());
    }

    fn upload_pages(&mut self) {
        let ch = self.settings().mode.channels();
        let fmt = if ch == 4 { Format::RGBA8 } else { Format::R8 };
        for (i, page) in self.rt.pages.iter().enumerate() {
            let data = PackedByteArray::from(page.pixels.as_slice());
            let Some(img) = Image::create_from_data(page.width, page.height, false, fmt, &data) else { continue };
            if (i as usize) < self.atlas_textures.len() {
                if let Some(tex) = self.atlas_textures.get(i) {
                    if let Ok(mut it) = tex.try_cast::<ImageTexture>() {
                        it.update(&img);
                        continue;
                    }
                }
            }
            let tex = ImageTexture::create_from_image(&img).expect("atlas texture");
            let t2: Gd<Texture2D> = tex.upcast();
            if (i as usize) < self.atlas_textures.len() {
                self.atlas_textures.set(i, &t2);
            } else {
                self.atlas_textures.push(&t2);
            }
        }
    }

    /// Glyph lookup for layout.
    pub fn glyph(&self, gid: u32) -> Option<&GlyphRec> {
        self.rt.glyphs.get(&gid)
    }
}
