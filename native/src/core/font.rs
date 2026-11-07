//! Font file access through skrifa / read-fonts: face metrics, character map,
//! glyph metrics, outlines (unhinted in font units, or hinted at a pixel size),
//! and the OpenType layout data a font asset stores (GPOS pair adjustments
//! from the `kern` feature, GSUB ligatures from `liga`).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, HintingInstance, HintingOptions};
use skrifa::raw::tables::gpos::{PairPos, PositionSubtables};
use skrifa::raw::tables::gsub::SubstitutionSubtables;
use skrifa::raw::types::{GlyphId16, Tag};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use super::outline::{Outline, OutlineBuilder};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceMetrics {
    pub family_name: String,
    pub style_name: String,
    pub units_per_em: f64,
    pub ascender: f64,
    pub descender: f64,
    pub line_gap: f64,
    pub cap_height: f64,
    pub x_height: f64,
    pub underline_offset: f64,
    pub underline_thickness: f64,
    pub strikethrough_offset: f64,
    pub strikethrough_thickness: f64,
    pub superscript_offset: f64,
    pub superscript_size: f64,
    pub subscript_offset: f64,
    pub subscript_size: f64,
    pub tab_width: f64,
    pub glyph_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphMetrics {
    /// Ink bounds in font units (y-up). All zero for glyphs without ink.
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
    pub advance: f64,
}

/// One glyph's adjustment in a pair (OpenType ValueRecord, font units).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ValueAdj {
    pub x_placement: f64,
    pub y_placement: f64,
    pub x_advance: f64,
    pub y_advance: f64,
}

impl ValueAdj {
    pub fn is_zero(&self) -> bool {
        self.x_placement == 0.0 && self.y_placement == 0.0 && self.x_advance == 0.0 && self.y_advance == 0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PairAdjustment {
    pub first: u32,
    pub second: u32,
    pub first_adj: ValueAdj,
    pub second_adj: ValueAdj,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ligature {
    pub components: Vec<u32>,
    pub ligature: u32,
}

#[derive(Clone)]
pub struct Face {
    data: Arc<Vec<u8>>,
    index: u32,
}

#[derive(Debug)]
pub struct FontError(pub String);

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// FreeType-style 16.16 scale for `ppem` pixels per em (FT_DivFix(ppem*64, upem)).
pub fn ft_scale(ppem: f64, upem: f64) -> i64 {
    (((ppem * 64.0) * 65536.0) / upem.max(1.0)).round() as i64
}

/// FT_MulFix: v (font units) × 16.16 scale → 26.6 pixels, rounded to nearest.
pub fn ft_mul(v: f64, scale16: i64) -> i64 {
    let a = v.round() as i64;
    let p = a * scale16;
    if p >= 0 { (p + 0x8000) >> 16 } else { -((-p + 0x8000) >> 16) }
}

/// Glyph metrics at a sampling size in the conventions font assets store:
/// exact outline control box (26.6-quantized) and the pixel-aligned bitmap box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PixelGlyphMetrics {
    pub width: f64,
    pub height: f64,
    pub bearing_x: f64,
    pub bearing_y: f64,
    pub advance: f64,
    /// Bitmap box in whole pixels: x0 (left), y1 (top), width, height.
    pub box_x0: i64,
    pub box_y1: i64,
    pub box_w: i64,
    pub box_h: i64,
}

impl Face {
    /// Metrics of `gid` at `ppem`, using the unhinted outline scaled as FreeType does.
    pub fn pixel_metrics(&self, gid: u32, ppem: f64) -> PixelGlyphMetrics {
        let m = self.metrics();
        let s16 = ft_scale(ppem, m.units_per_em);
        let gm = self.glyph_metrics(gid);
        let advance = ft_mul(gm.advance, s16) as f64 / 64.0;
        let Some(o) = self.outline(gid).filter(|o| !o.is_empty()) else {
            return PixelGlyphMetrics { advance, ..Default::default() };
        };
        let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        let mut add = |p: super::outline::P| {
            let x = ft_mul(p.x, s16);
            let y = ft_mul(p.y, s16);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        };
        for c in &o.contours {
            for seg in c {
                match *seg {
                    super::outline::Seg::Line(a, b) => {
                        add(a);
                        add(b)
                    }
                    super::outline::Seg::Quad(a, b, c) => {
                        add(a);
                        add(b);
                        add(c)
                    }
                    super::outline::Seg::Cubic(a, b, c, d) => {
                        add(a);
                        add(b);
                        add(c);
                        add(d)
                    }
                }
            }
        }
        let fl = |v: i64| v.div_euclid(64);
        let ce = |v: i64| (v + 63).div_euclid(64);
        PixelGlyphMetrics {
            width: (x1 - x0) as f64 / 64.0,
            height: (y1 - y0) as f64 / 64.0,
            bearing_x: x0 as f64 / 64.0,
            bearing_y: y1 as f64 / 64.0,
            advance,
            box_x0: fl(x0),
            box_y1: ce(y1),
            box_w: ce(x1) - fl(x0),
            box_h: ce(y1) - fl(y0),
        }
    }

    pub fn new(data: Vec<u8>, index: u32) -> Result<Face, FontError> {
        let face = Face { data: Arc::new(data), index };
        face.font()?;
        Ok(face)
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn font(&self) -> Result<FontRef<'_>, FontError> {
        FontRef::from_index(&self.data, self.index).map_err(|e| FontError(format!("cannot read font: {e}")))
    }

    fn name(font: &FontRef, id: StringId) -> String {
        font.localized_strings(id).english_or_first().map(|s| s.to_string()).unwrap_or_default()
    }

    pub fn metrics(&self) -> FaceMetrics {
        let Ok(font) = self.font() else { return FaceMetrics::default() };
        let m = font.metrics(Size::unscaled(), LocationRef::default());
        let mut fm = FaceMetrics {
            family_name: {
                let typo = Self::name(&font, StringId::TYPOGRAPHIC_FAMILY_NAME);
                if typo.is_empty() { Self::name(&font, StringId::FAMILY_NAME) } else { typo }
            },
            style_name: {
                let typo = Self::name(&font, StringId::TYPOGRAPHIC_SUBFAMILY_NAME);
                if typo.is_empty() { Self::name(&font, StringId::SUBFAMILY_NAME) } else { typo }
            },
            units_per_em: m.units_per_em as f64,
            ascender: m.ascent as f64,
            descender: m.descent as f64,
            line_gap: m.leading as f64,
            cap_height: m.cap_height.unwrap_or(m.ascent * 0.7) as f64,
            x_height: m.x_height.unwrap_or(m.ascent * 0.5) as f64,
            glyph_count: m.glyph_count as u32,
            ..Default::default()
        };
        if let Some(u) = m.underline {
            fm.underline_offset = u.offset as f64;
            fm.underline_thickness = u.thickness as f64;
        }
        if let Some(s) = m.strikeout {
            fm.strikethrough_offset = s.offset as f64;
            fm.strikethrough_thickness = s.thickness as f64;
        } else {
            fm.strikethrough_offset = fm.x_height * 0.5;
            fm.strikethrough_thickness = fm.underline_thickness;
        }
        if let Ok(os2) = font.os2() {
            fm.superscript_offset = os2.y_superscript_y_offset() as f64;
            fm.superscript_size = os2.y_superscript_y_size() as f64;
            fm.subscript_offset = -(os2.y_subscript_y_offset() as f64);
            fm.subscript_size = os2.y_subscript_y_size() as f64;
        }
        if fm.superscript_size <= 0.0 {
            fm.superscript_size = fm.units_per_em * 0.5;
            fm.superscript_offset = fm.ascender * 0.5;
        }
        if fm.subscript_size <= 0.0 {
            fm.subscript_size = fm.units_per_em * 0.5;
            fm.subscript_offset = fm.descender * 0.5;
        }
        // Tab stops default to the width of ten spaces (assets may override).
        let space = self.glyph_id(' ' as u32).map(|g| self.glyph_metrics(g).advance).unwrap_or(fm.units_per_em * 0.25);
        fm.tab_width = space * 10.0;
        fm
    }

    pub fn glyph_id(&self, codepoint: u32) -> Option<u32> {
        let font = self.font().ok()?;
        let gid = font.charmap().map(codepoint)?;
        let v = gid.to_u32();
        if v == 0 { None } else { Some(v) }
    }

    /// All (codepoint, glyph) pairs the character map defines.
    pub fn mappings(&self) -> Vec<(u32, u32)> {
        let Ok(font) = self.font() else { return Vec::new() };
        font.charmap().mappings().map(|(c, g)| (c, g.to_u32())).filter(|&(_, g)| g != 0).collect()
    }

    pub fn glyph_metrics(&self, gid: u32) -> GlyphMetrics {
        let Ok(font) = self.font() else { return GlyphMetrics::default() };
        let gm = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        let g = GlyphId::new(gid);
        let advance = gm.advance_width(g).unwrap_or(0.0) as f64;
        // Ink bounds from the outline itself (glyf bboxes can be stale; CFF has none).
        let b = self.outline(gid).map(|o| o.bounds());
        match b {
            Some(b) if !b.is_empty() => GlyphMetrics { x_min: b.x0, y_min: b.y0, x_max: b.x1, y_max: b.y1, advance },
            _ => GlyphMetrics { advance, ..Default::default() },
        }
    }

    /// Unhinted outline in font units (y-up).
    pub fn outline(&self, gid: u32) -> Option<Outline> {
        let font = self.font().ok()?;
        let glyphs = font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::new(gid))?;
        let mut pen = OutlineBuilder::default();
        glyph.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen).ok()?;
        Some(pen.finish())
    }

    /// Hinted outline at `ppem` pixels per em, in pixel units (y-up).
    pub fn outline_hinted(&self, gid: u32, ppem: f32) -> Option<Outline> {
        self.outline_hinted_with(gid, ppem, HintingOptions::default())
    }

    pub fn outline_hinted_with(&self, gid: u32, ppem: f32, opts: HintingOptions) -> Option<Outline> {
        let font = self.font().ok()?;
        let glyphs = font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::new(gid))?;
        let inst = HintingInstance::new(&glyphs, Size::new(ppem), LocationRef::default(), opts).ok()?;
        let mut pen = OutlineBuilder::default();
        glyph.draw(DrawSettings::hinted(&inst, false), &mut pen).ok()?;
        Some(pen.finish())
    }

    fn feature_lookups_gpos(font: &FontRef, tag: Tag) -> BTreeSet<u16> {
        let mut out = BTreeSet::new();
        if let Ok(gpos) = font.gpos() {
            if let Ok(fl) = gpos.feature_list() {
                for rec in fl.feature_records() {
                    if rec.feature_tag() == tag {
                        if let Ok(f) = rec.feature(fl.offset_data()) {
                            out.extend(f.lookup_list_indices().iter().map(|i| i.get()));
                        }
                    }
                }
            }
        }
        out
    }

    fn feature_lookups_gsub(font: &FontRef, tag: Tag) -> BTreeSet<u16> {
        let mut out = BTreeSet::new();
        if let Ok(gsub) = font.gsub() {
            if let Ok(fl) = gsub.feature_list() {
                for rec in fl.feature_records() {
                    if rec.feature_tag() == tag {
                        if let Ok(f) = rec.feature(fl.offset_data()) {
                            out.extend(f.lookup_list_indices().iter().map(|i| i.get()));
                        }
                    }
                }
            }
        }
        out
    }

    /// Pair adjustments among `glyphs` from the GPOS `kern` feature (first
    /// subtable that covers a pair wins, as OpenType specifies), falling back to
    /// the legacy `kern` table's format 0 pairs when the font has no GPOS kerning.
    pub fn pair_adjustments(&self, glyphs: &[u32]) -> Vec<PairAdjustment> {
        self.pair_adjustments_from(glyphs, true)
    }

    /// The pairs TextMesh Pro's runtime `TryAddCharacters(…, includeFontFeatures)`
    /// picks up: GPOS `kern` lookups without lookup flags (measured: Noto Sans JP's
    /// are read, Inter's IgnoreMarks lookup is not), else the legacy `kern` table.
    pub fn legacy_pair_adjustments(&self, glyphs: &[u32]) -> Vec<PairAdjustment> {
        self.pair_adjustments_from(glyphs, false)
    }

    fn pair_adjustments_from(&self, glyphs: &[u32], flagged_lookups: bool) -> Vec<PairAdjustment> {
        let Ok(font) = self.font() else { return Vec::new() };
        let set: BTreeSet<u32> = glyphs.iter().copied().collect();
        let mut found: HashMap<(u32, u32), PairAdjustment> = HashMap::new();
        let lookups = Self::feature_lookups_gpos(&font, Tag::new(b"kern"));
        if let (Ok(gpos), false) = (font.gpos(), lookups.is_empty()) {
            if let Ok(ll) = gpos.lookup_list() {
                for li in lookups {
                    let Ok(lookup) = ll.lookups().get(li as usize) else { continue };
                    if !flagged_lookups && lookup.lookup_flag().to_bits() != 0 {
                        continue;
                    }
                    let Ok(PositionSubtables::Pair(subs)) = lookup.subtables() else { continue };
                    for sub in subs.iter().flatten() {
                        Self::collect_pairs(&sub, &set, &mut found);
                    }
                }
            }
        }
        if found.is_empty() {
            if let Ok(kern) = font.kern() {
                for st in kern.subtables().flatten() {
                    if !st.is_horizontal() {
                        continue;
                    }
                    for &a in &set {
                        for &b in &set {
                            let Ok(skrifa::raw::tables::kern::SubtableKind::Format0(k0)) = st.kind() else { continue };
                            if let Some(v) = k0.kerning(GlyphId::new(a), GlyphId::new(b)) {
                                if v != 0 {
                                    found.entry((a, b)).or_insert(PairAdjustment {
                                        first: a,
                                        second: b,
                                        first_adj: ValueAdj { x_advance: v as f64, ..Default::default() },
                                        second_adj: ValueAdj::default(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut v: Vec<PairAdjustment> =
            found.into_values().filter(|p| !(p.first_adj.is_zero() && p.second_adj.is_zero())).collect();
        v.sort_by_key(|p| (p.first, p.second));
        v
    }

    fn collect_pairs(sub: &PairPos, set: &BTreeSet<u32>, found: &mut HashMap<(u32, u32), PairAdjustment>) {
        use skrifa::raw::tables::gpos::ValueRecord;
        let adj = |r: &ValueRecord| ValueAdj {
            x_placement: r.x_placement().unwrap_or(0) as f64,
            y_placement: r.y_placement().unwrap_or(0) as f64,
            x_advance: r.x_advance().unwrap_or(0) as f64,
            y_advance: r.y_advance().unwrap_or(0) as f64,
        };
        match sub {
            PairPos::Format1(f) => {
                let Ok(cov) = f.coverage() else { return };
                let sets = f.pair_sets();
                for &a in set {
                    let Some(ci) = cov.get(GlyphId::new(a)) else { continue };
                    let Ok(ps) = sets.get(ci as usize) else { continue };
                    for &b in set {
                        if found.contains_key(&(a, b)) {
                            continue;
                        }
                        let Ok(b16) = u16::try_from(b) else { continue };
                        if let Some(rec) = ps.find_pair_value_record(GlyphId16::new(b16)) {
                            let (a1, a2) = (adj(rec.value_record1()), adj(rec.value_record2()));
                            found.insert((a, b), PairAdjustment { first: a, second: b, first_adj: a1, second_adj: a2 });
                        }
                    }
                }
            }
            PairPos::Format2(f) => {
                let (Ok(cov), Ok(cd1), Ok(cd2)) = (f.coverage(), f.class_def1(), f.class_def2()) else { return };
                let c1recs = f.class1_records();
                for &a in set {
                    if cov.get(GlyphId::new(a)).is_none() {
                        continue;
                    }
                    let c1 = cd1.get(GlyphId::new(a));
                    let Ok(c1rec) = c1recs.get(c1 as usize) else { continue };
                    let c2recs = c1rec.class2_records();
                    for &b in set {
                        if found.contains_key(&(a, b)) {
                            continue;
                        }
                        let c2 = cd2.get(GlyphId::new(b));
                        let Ok(c2rec) = c2recs.get(c2 as usize) else { continue };
                        let (a1, a2) = (adj(c2rec.value_record1()), adj(c2rec.value_record2()));
                        // Format 2 assigns a record to every class pair; zero records mean
                        // "covered, no adjustment" and still stop the search.
                        found.insert((a, b), PairAdjustment { first: a, second: b, first_adj: a1, second_adj: a2 });
                    }
                }
            }
            // Formats 3/4 (24-bit glyph ids) are not used by fonts in scope yet.
            _ => {}
        }
    }

    /// Ligature substitutions from the GSUB `liga` feature whose components and
    /// result are all in `glyphs`.
    pub fn ligatures(&self, glyphs: &[u32]) -> Vec<Ligature> {
        self.ligatures_impl(glyphs, true)
    }

    /// Ligatures whose components are all in `glyphs` (the result may be new).
    pub fn ligatures_from(&self, glyphs: &[u32]) -> Vec<Ligature> {
        self.ligatures_impl(glyphs, false)
    }

    fn ligatures_impl(&self, glyphs: &[u32], require_result: bool) -> Vec<Ligature> {
        let Ok(font) = self.font() else { return Vec::new() };
        let set: BTreeSet<u32> = glyphs.iter().copied().collect();
        let mut out = Vec::new();
        let Ok(gsub) = font.gsub() else { return out };
        let Ok(ll) = gsub.lookup_list() else { return out };
        for li in Self::feature_lookups_gsub(&font, Tag::new(b"liga")) {
            let Ok(lookup) = ll.lookups().get(li as usize) else { continue };
            let Ok(SubstitutionSubtables::Ligature(subs)) = lookup.subtables() else { continue };
            for sub in subs.iter().flatten() {
                let skrifa::raw::tables::gsub::LigatureSubst::Format1(sub) = sub else { continue };
                let Ok(cov) = sub.coverage() else { continue };
                let lsets = sub.ligature_sets();
                for (ci, first) in cov.iter().enumerate() {
                    let first = first.to_u32();
                    if !set.contains(&first) {
                        continue;
                    }
                    let Ok(ls) = lsets.get(ci) else { continue };
                    for lig in ls.ligatures().iter().flatten() {
                        let mut comps = vec![first];
                        comps.extend(lig.component_glyph_ids().iter().map(|g| g.get().to_u32()));
                        let res = lig.ligature_glyph().to_u32();
                        if comps.iter().all(|c| set.contains(c)) && (!require_result || set.contains(&res)) {
                            out.push(Ligature { components: comps, ligature: res });
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
pub fn inter() -> Face {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fonts/Inter-Regular.ttf");
    Face::new(std::fs::read(p).expect("tests/fonts/Inter-Regular.ttf"), 0).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_metrics() {
        let f = inter();
        let m = f.metrics();
        assert_eq!(m.units_per_em, 2816.0);
        assert_eq!(m.family_name, "Inter");
        assert!(m.ascender > 2000.0 && m.descender < 0.0, "{m:?}");
        assert!(m.cap_height > 1500.0 && m.x_height > 1000.0);
    }

    #[test]
    fn inter_glyphs_and_outline() {
        let f = inter();
        let a = f.glyph_id('A' as u32).unwrap();
        let gm = f.glyph_metrics(a);
        assert!(gm.advance > 1000.0 && gm.x_max > gm.x_min && gm.y_min.abs() < 1.0, "{gm:?}");
        let o = f.outline(a).unwrap();
        assert!(!o.is_empty());
        let space = f.glyph_id(' ' as u32).unwrap();
        assert!(f.outline(space).unwrap().is_empty());
        assert!(f.outline_hinted(a, 12.0).is_some());
    }

    #[test]
    fn freetype_style_metrics_match_unity() {
        // Values Unity reports for Inter Regular at 90 pt (black-box dump).
        let f = inter();
        let t = f.pixel_metrics(f.glyph_id('T' as u32).unwrap(), 90.0);
        assert_eq!((t.width, t.height, t.bearing_x, t.bearing_y, t.advance), (49.09375, 65.453125, 4.34375, 65.453125, 57.78125));
        assert_eq!((t.box_w, t.box_h), (50, 66));
        let h = f.pixel_metrics(f.glyph_id('H' as u32).unwrap(), 90.0);
        assert_eq!((h.width, h.bearing_x, h.advance, h.box_w), (50.75, 7.921875, 66.609375, 52));
    }

    #[test]
    fn inter_kerning_from_gpos() {
        let f = inter();
        let g: Vec<u32> = "AVTo".chars().map(|c| f.glyph_id(c as u32).unwrap()).collect();
        let pairs = f.pair_adjustments(&g);
        let av = pairs.iter().find(|p| p.first == g[0] && p.second == g[1]).expect("A-V kerning");
        assert!(av.first_adj.x_advance < 0.0, "{av:?}");
    }
}
