//! Font atlas generation: render glyphs in a chosen render mode at a sampling
//! point size, pack them (with padding) into one or more atlas pages, and
//! report per-glyph metrics in the conventions a font asset stores.
//!
//! Conventions (all at the sampling point size, 1 point = 1 pixel):
//! * glyph box: the ink bounds rounded outwards to whole pixels;
//!   `width`/`height` are its size, `bearing_x` its left edge relative to the
//!   pen position, `bearing_y` its top edge above the baseline, `advance` the
//!   unrounded horizontal advance.
//! * `rect`: the glyph box inside its atlas page (top-left origin, y down),
//!   excluding padding. The padded rectangle `rect` grown by `padding` on every
//!   side is reserved for the glyph, so quads may sample it.
//! * distance encoding: texel = 0.5 + d / (2 * gradient_scale), d in texels,
//!   positive inside, gradient_scale = padding + 1.

use rayon::prelude::*;

use super::font::Face;
use super::outline::{Outline, P};
use super::pack::{Heuristic, MaxRects, Rect};
use super::{raster, sdf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Smooth,
    SmoothHinted,
    Raster,
    RasterHinted,
    Sdf,
    Sdf8,
    Sdf16,
    Sdf32,
    Sdfaa,
    SdfaaHinted,
    Msdf,
    Mtsdf,
}

impl RenderMode {
    /// Stable integer ids used across the GDScript API (`GlyphRenderMode`).
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => RenderMode::Smooth,
            1 => RenderMode::SmoothHinted,
            2 => RenderMode::Raster,
            3 => RenderMode::RasterHinted,
            4 => RenderMode::Sdf,
            5 => RenderMode::Sdf8,
            6 => RenderMode::Sdf16,
            7 => RenderMode::Sdf32,
            8 => RenderMode::Sdfaa,
            9 => RenderMode::SdfaaHinted,
            10 => RenderMode::Msdf,
            11 => RenderMode::Mtsdf,
            _ => return None,
        })
    }
    pub fn is_hinted(self) -> bool {
        matches!(self, RenderMode::SmoothHinted | RenderMode::RasterHinted | RenderMode::SdfaaHinted)
    }
    pub fn is_distance_field(self) -> bool {
        !matches!(self, RenderMode::Smooth | RenderMode::SmoothHinted | RenderMode::Raster | RenderMode::RasterHinted)
    }
    /// 1 (alpha / distance) or 4 (RGBA multi-channel) bytes per texel.
    pub fn channels(self) -> usize {
        match self {
            RenderMode::Msdf | RenderMode::Mtsdf => 4,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GenSettings {
    pub point_size: f64,
    pub padding: i32,
    pub mode: RenderMode,
    pub width: i32,
    pub height: i32,
    pub heuristic: Heuristic,
    pub multi_atlas: bool,
}

impl GenSettings {
    pub fn gradient_scale(&self) -> f64 {
        self.padding as f64 + 1.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphRecord {
    pub index: u32,
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub rect: Rect,
    pub atlas_index: i32,
}

#[derive(Clone, Debug)]
pub struct AtlasPage {
    pub width: i32,
    pub height: i32,
    pub channels: usize,
    pub pixels: Vec<u8>,
    pub packer: MaxRects,
}

impl AtlasPage {
    pub fn new(w: i32, h: i32, channels: usize) -> Self {
        let fill = if channels == 4 { 0 } else { 0 };
        AtlasPage { width: w, height: h, channels, pixels: vec![fill; (w * h) as usize * channels], packer: MaxRects::new(w, h) }
    }
}

/// A rendered glyph bitmap including its padding.
pub struct GlyphBitmap {
    pub record: GlyphRecord,
    pub w: i32,
    pub h: i32,
    pub data: Vec<u8>, // w*h*channels
}

fn encode(d: f32, gs: f32) -> u8 {
    ((0.5 + d / (2.0 * gs)).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Renders one glyph (metrics + padded bitmap) without placing it.
pub fn render_glyph(face: &Face, gid: u32, s: &GenSettings) -> Option<GlyphBitmap> {
    let m = face.metrics();
    let s16 = super::font::ft_scale(s.point_size, m.units_per_em);
    let pm = face.pixel_metrics(gid, s.point_size);
    let advance = pm.advance as f32;
    // Outline in pixel units (y-up, pen origin), points scaled and rounded to
    // 1/64 px exactly as the metrics are.
    let (px_outline, metrics) = if s.mode.is_hinted() {
        let o = face.outline_hinted(gid, s.point_size as f32).filter(|o| !o.is_empty());
        let b = o.as_ref().map(|o| o.bounds());
        let pmh = match b {
            Some(b) if !b.is_empty() => super::font::PixelGlyphMetrics {
                width: ((b.x1 - b.x0) * 64.0).round() / 64.0,
                height: ((b.y1 - b.y0) * 64.0).round() / 64.0,
                bearing_x: (b.x0 * 64.0).round() / 64.0,
                bearing_y: (b.y1 * 64.0).round() / 64.0,
                advance: pm.advance.round(),
                box_x0: b.x0.floor() as i64,
                box_y1: b.y1.ceil() as i64,
                box_w: (b.x1.ceil() - b.x0.floor()) as i64,
                box_h: (b.y1.ceil() - b.y0.floor()) as i64,
            },
            _ => pm,
        };
        (o, pmh)
    } else {
        let q = |v: f64| super::font::ft_mul(v, s16) as f64 / 64.0;
        (face.outline(gid).filter(|o| !o.is_empty()).map(|o| o.map(|p| P::new(q(p.x), q(p.y)))), pm)
    };
    let Some(o) = px_outline else {
        // No ink: metrics only.
        return Some(GlyphBitmap {
            record: GlyphRecord { index: gid, advance: metrics.advance as f32, ..Default::default() },
            w: 0,
            h: 0,
            data: Vec::new(),
        });
    };
    let (gw, gh) = (metrics.box_w as i32, metrics.box_h as i32);
    let pad = s.padding.max(0);
    let (w, h) = (gw + 2 * pad, gh + 2 * pad);
    // Bitmap frame: y-down, glyph box starts at (pad, pad).
    let bitmap_outline = o.to_pixels(1.0, metrics.box_x0 as f64 - pad as f64, metrics.box_y1 as f64 + pad as f64);
    let (uw, uh) = (w as usize, h as usize);
    let gs = s.gradient_scale() as f32;
    let data: Vec<u8> = match s.mode {
        RenderMode::Smooth | RenderMode::SmoothHinted => {
            let polys = bitmap_outline.flatten(0.02);
            raster::coverage(&polys, uw, uh).iter().map(|&c| (c * 255.0).round() as u8).collect()
        }
        RenderMode::Raster | RenderMode::RasterHinted => {
            let polys = bitmap_outline.flatten(0.02);
            raster::inside_mask(&polys, uw, uh).iter().map(|&i| if i { 255 } else { 0 }).collect()
        }
        RenderMode::Sdf => {
            let polys = bitmap_outline.flatten(0.01);
            sdf::analytic(&polys, uw, uh, gs as f64 + 1.0).iter().map(|&d| encode(d, gs)).collect()
        }
        RenderMode::Sdf8 | RenderMode::Sdf16 | RenderMode::Sdf32 => {
            let k = match s.mode {
                RenderMode::Sdf8 => 8,
                RenderMode::Sdf16 => 16,
                _ => 32,
            };
            let polys = bitmap_outline.flatten(0.02 / k as f64);
            sdf::edt_oversampled(&polys, uw, uh, k).iter().map(|&d| encode(d, gs)).collect()
        }
        RenderMode::Sdfaa | RenderMode::SdfaaHinted => {
            let polys = bitmap_outline.flatten(0.02);
            sdf::edt_antialiased(&polys, uw, uh).iter().map(|&d| encode(d, gs)).collect()
        }
        RenderMode::Msdf | RenderMode::Mtsdf => super::msdf::render(&bitmap_outline, uw, uh, gs as f64, s.mode == RenderMode::Mtsdf),
    };
    let _ = advance;
    Some(GlyphBitmap {
        record: GlyphRecord {
            index: gid,
            width: metrics.width as f32,
            height: metrics.height as f32,
            bearing_x: metrics.bearing_x as f32,
            bearing_y: metrics.bearing_y as f32,
            advance: metrics.advance as f32,
            rect: Rect::default(),
            atlas_index: 0,
        },
        w,
        h,
        data,
    })
}

fn blit(page: &mut AtlasPage, at: Rect, bm: &GlyphBitmap) {
    let ch = page.channels;
    for y in 0..bm.h {
        let src = (y * bm.w) as usize * ch;
        let dst = (((at.y + y) * page.width + at.x) as usize) * ch;
        page.pixels[dst..dst + bm.w as usize * ch].copy_from_slice(&bm.data[src..src + bm.w as usize * ch]);
    }
}

pub struct GenResult {
    pub glyphs: Vec<GlyphRecord>,
    pub missing: Vec<u32>,
}

/// Renders `gids` and packs them into `pages` (creating the first page, and more
/// when `multi_atlas`, as needed). Glyphs that fit nowhere are reported missing.
/// Larger glyphs are placed first for better packing.
pub fn add_glyphs(face: &Face, gids: &[u32], s: &GenSettings, pages: &mut Vec<AtlasPage>) -> GenResult {
    let mut bitmaps: Vec<GlyphBitmap> = gids.par_iter().filter_map(|&g| render_glyph(face, g, s)).collect();
    bitmaps.sort_by(|a, b| (b.h, b.w, a.record.index).cmp(&(a.h, a.w, b.record.index)));
    let mut glyphs = Vec::with_capacity(bitmaps.len());
    let mut missing = Vec::new();
    let ch = s.mode.channels();
    if pages.is_empty() {
        pages.push(AtlasPage::new(s.width, s.height, ch));
    }
    for bm in bitmaps {
        let mut rec = bm.record;
        if bm.w == 0 || bm.h == 0 {
            rec.atlas_index = 0;
            glyphs.push(rec);
            continue;
        }
        let mut placed = None;
        for (pi, page) in pages.iter_mut().enumerate() {
            if let Some(r) = page.packer.insert(bm.w + 1, bm.h + 1, s.heuristic) {
                placed = Some((pi, Rect::new(r.x + 1, r.y + 1, bm.w, bm.h)));
                break;
            }
        }
        if placed.is_none() && s.multi_atlas && bm.w < s.width && bm.h < s.height {
            let mut page = AtlasPage::new(s.width, s.height, ch);
            let r = page.packer.insert(bm.w + 1, bm.h + 1, s.heuristic);
            pages.push(page);
            placed = r.map(|r| (pages.len() - 1, Rect::new(r.x + 1, r.y + 1, bm.w, bm.h)));
        }
        match placed {
            Some((pi, r)) => {
                blit(&mut pages[pi], r, &bm);
                let pad = s.padding.max(0);
                rec.rect = Rect::new(r.x + pad, r.y + pad, bm.w - 2 * pad, bm.h - 2 * pad);
                rec.atlas_index = pi as i32;
                glyphs.push(rec);
            }
            None => missing.push(rec.index),
        }
    }
    glyphs.sort_by_key(|g| g.index);
    GenResult { glyphs, missing }
}

/// Largest integer point size at which every glyph fits on one page
/// ("Auto Sizing" in the font asset creator). `fast` uses coarser steps.
pub fn auto_point_size(face: &Face, gids: &[u32], s: &GenSettings, fast: bool) -> i32 {
    let fits = |pt: i32| -> bool {
        let mut t = *s;
        t.point_size = pt as f64;
        t.multi_atlas = false;
        // Pack boxes only (cheap: metrics, no distance fields).
        let m = face.metrics();
        let scale = pt as f64 / m.units_per_em.max(1.0);
        let mut sizes: Vec<(i32, i32)> = gids
            .iter()
            .filter_map(|&g| {
                let gm = face.glyph_metrics(g);
                if gm.x_max <= gm.x_min {
                    return None;
                }
                let w = ((gm.x_max * scale).ceil() - (gm.x_min * scale).floor()) as i32 + 2 * t.padding + 1;
                let h = ((gm.y_max * scale).ceil() - (gm.y_min * scale).floor()) as i32 + 2 * t.padding + 1;
                Some((w, h))
            })
            .collect();
        sizes.sort_by(|a, b| (b.1, b.0).cmp(&(a.1, a.0)));
        let mut mr = MaxRects::new(t.width, t.height);
        sizes.iter().all(|&(w, h)| mr.insert(w, h, t.heuristic).is_some())
    };
    let (mut lo, mut hi) = (4, 1024);
    if !fits(lo) {
        return lo;
    }
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
        if fast && hi - lo <= 2 {
            break;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::font::inter;

    fn settings(mode: RenderMode) -> GenSettings {
        GenSettings { point_size: 48.0, padding: 5, mode, width: 512, height: 512, heuristic: Heuristic::BestShortSideFit, multi_atlas: false }
    }

    #[test]
    fn ascii_sdf_atlas() {
        let f = inter();
        let gids: Vec<u32> = (32u32..127).filter_map(|c| f.glyph_id(c)).collect();
        for mode in [RenderMode::Sdf, RenderMode::Sdfaa, RenderMode::Sdf8, RenderMode::Smooth, RenderMode::Mtsdf] {
            let s = settings(mode);
            let mut pages = Vec::new();
            let r = add_glyphs(&f, &gids, &s, &mut pages);
            assert!(r.missing.is_empty(), "{mode:?} missing {:?}", r.missing);
            assert_eq!(r.glyphs.len(), gids.len());
            let a = r.glyphs.iter().find(|g| g.index == f.glyph_id('A' as u32).unwrap()).unwrap();
            // Inter 'A' at 48 px: about 34 px tall cap height, positive advance.
            assert!(a.height >= 33.0 && a.height <= 36.0, "{a:?}");
            assert!(a.advance > 25.0 && a.bearing_y >= 33.0);
            assert_eq!(a.rect.w as f32, (a.bearing_x + a.width).ceil() - a.bearing_x.floor());
            let p = &pages[0];
            if mode.is_distance_field() && mode.channels() == 1 {
                // Centre of the glyph box row through the crossbar is inside → > 0.5.
                let cx = a.rect.x + a.rect.w / 2;
                let cy = a.rect.y + a.rect.h - 2;
                let v = p.pixels[(cy * p.width + cx) as usize];
                assert!(v < 128, "{mode:?}: gap between A's legs should be outside, got {v}");
                let edge = p.pixels[((a.rect.y - 5) * p.width + a.rect.x) as usize];
                assert_eq!(edge, 0, "{mode:?}: far outside saturates");
            }
        }
    }

    #[test]
    fn auto_size_fills_page() {
        let f = inter();
        let gids: Vec<u32> = (32u32..127).filter_map(|c| f.glyph_id(c)).collect();
        let s = settings(RenderMode::Sdfaa);
        let pt = auto_point_size(&f, &gids, &s, false);
        assert!(pt > 40 && pt < 120, "auto size {pt}");
        let mut s2 = s;
        s2.point_size = pt as f64;
        let mut pages = Vec::new();
        assert!(add_glyphs(&f, &gids, &s2, &mut pages).missing.is_empty());
    }
}
