//! Glyph quads → ArrayMesh surface arrays.
//!
//! Per-vertex data read by the Typesetter shaders:
//! * VERTEX  – position (node space)
//! * UV      – atlas texture coordinate (top-left origin)
//! * COLOR   – vertex colour (character colour × gradient)
//! * CUSTOM0 – (face_u, face_v, scale, flags) where flags: 1 bold, 2 sprite,
//!             4 solid (highlight / no texture), 8 decoration
//! * CUSTOM1 – (character index, line index, word index, 0)

use std::collections::BTreeMap;

use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
use godot::classes::{rendering_server, ArrayMesh, RenderingServer};
use godot::prelude::*;

use super::text::TypesetterTextCore;
use crate::core::layout::{CharInfo, ElementType, Layout};

pub const FLAG_BOLD: f32 = 1.0;
pub const FLAG_SPRITE: f32 = 2.0;
pub const FLAG_SOLID: f32 = 4.0;
pub const FLAG_DECORATION: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceKey {
    /// Draw order: highlights first, then glyphs, then decorations.
    pub kind: u8,
    pub sprite: bool,
    pub font: u32,
    pub atlas: i32,
    pub material: u32,
}

#[derive(Default, Clone)]
pub struct Quads {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub custom0: Vec<[f32; 4]>,
    pub custom1: Vec<[f32; 4]>,
    /// Character index for each quad (−1 for decorations).
    pub owners: Vec<i32>,
}

impl Quads {
    #[allow(clippy::too_many_arguments)]
    fn push(&mut self, corners: [[f32; 2]; 4], uv: [[f32; 2]; 4], col: [[f32; 4]; 4], face: [[f32; 2]; 4], scale: f32, flags: f32, ids: [f32; 3], owner: i32, flip_y: bool) {
        for k in 0..4 {
            let [x, y] = corners[k];
            self.positions.push([x, if flip_y { -y } else { y }, 0.0]);
            self.uvs.push(uv[k]);
            self.colors.push(col[k]);
            self.custom0.push([face[k][0], face[k][1], scale, flags]);
            self.custom1.push([ids[0], ids[1], ids[2], 0.0]);
        }
        self.owners.push(owner);
    }
}

fn glyph_uv(c: &CharInfo, pad: f32) -> [[f32; 2]; 4] {
    let [rx, ry, rw, rh] = c.uv_rect;
    let [aw, ah] = c.atlas_size;
    let (aw, ah) = (aw.max(1.0), ah.max(1.0));
    let u0 = (rx - pad) / aw;
    let u1 = (rx + rw + pad) / aw;
    let v_top = (ry - pad) / ah;
    let v_bot = (ry + rh + pad) / ah;
    // BL, TL, TR, BR
    [[u0, v_bot], [u0, v_top], [u1, v_top], [u1, v_bot]]
}

fn corner_colors(c: &CharInfo, global: Option<[[f32; 4]; 4]>) -> [[f32; 4]; 4] {
    // Gradient order: top-left, top-right, bottom-left, bottom-right → BL, TL, TR, BR.
    let g = c.gradient.as_deref().copied().or(global);
    match g {
        Some([tl, tr, bl, br]) => {
            let m = |a: [f32; 4]| [a[0] * c.color[0], a[1] * c.color[1], a[2] * c.color[2], a[3] * c.color[3]];
            [m(bl), m(tl), m(tr), m(br)]
        }
        None => [c.color; 4],
    }
}

/// Builds quads for every visible element of the layout, grouped by surface.
pub fn build_quads(l: &Layout, core: &TypesetterTextCore) -> BTreeMap<SurfaceKey, Quads> {
    let mut out: BTreeMap<SurfaceKey, Quads> = BTreeMap::new();
    let flip = !core.world_space;
    let pad = core.mesh_padding * if core.extra_padding { 1.5 } else { 1.0 };
    let global_grad = if core.color_gradient.len() >= 4 {
        let g = core.color_gradient.as_slice();
        let f = |c: Color| [c.r, c.g, c.b, c.a];
        Some([f(g[0]), f(g[1]), f(g[2]), f(g[3])])
    } else {
        None
    };
    let rect_w = core.rect_size.x.max(1e-3);
    let rect_h = core.rect_size.y.max(1e-3);
    // Word index per character for CUSTOM1.
    let mut word_of = vec![-1.0f32; l.characters.len()];
    for (wi, w) in l.words.iter().enumerate() {
        for k in w.first_character_index..=w.last_character_index {
            if let Some(x) = word_of.get_mut(k as usize) {
                *x = wi as f32;
            }
        }
    }
    // Highlights (drawn first).
    for d in l.decorations.iter().filter(|d| d.kind == 2) {
        let key = SurfaceKey { kind: 0, sprite: false, font: d.font, atlas: 0, material: 0 };
        let q = out.entry(key).or_default();
        let corners = [[d.x0, d.y0], [d.x0, d.y1], [d.x1, d.y1], [d.x1, d.y0]];
        q.push(corners, [[0.0; 2]; 4], [d.color; 4], [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], d.scale, FLAG_SOLID, [-1.0, 0.0, -1.0], -1, flip);
    }
    for (i, c) in l.characters.iter().enumerate() {
        if !c.is_visible {
            continue;
        }
        let sprite = c.element_type == ElementType::Sprite;
        let key = SurfaceKey { kind: 1, sprite, font: c.font, atlas: if sprite { 0 } else { c.atlas_index }, material: c.material };
        let q = out.entry(key).or_default();
        let (bl, tl, tr, br) = (c.bottom_left, c.top_left, c.top_right, c.bottom_right);
        // Layout has already applied italic shear and <rotate> to these corners.
        let corners = [[bl.x, bl.y], [tl.x, tl.y], [tr.x, tr.y], [br.x, br.y]];
        // Bold glyphs sample further into the atlas padding, on both axes (measured
        // from TextMesh Pro's vertices).
        let uv_pad = if c.style & crate::core::layout::style::BOLD != 0 {
            core.font.as_ref().map(|f| {
                let f = f.bind();
                pad + f.bold_style / 4.0 * f.atlas_padding as f32
            }).unwrap_or(pad)
        } else {
            pad
        };
        let uv = if sprite { glyph_uv(c, 0.0) } else { glyph_uv(c, uv_pad) };
        let mut col = corner_colors(c, global_grad);
        if sprite && !c.sprite.as_ref().is_some_and(|s| s.tint) {
            col = [[1.0, 1.0, 1.0, c.color[3]]; 4];
        }
        if let Some(tint) = c.sprite.as_ref().and_then(|s| s.color) {
            col = [tint; 4];
        }
        let face = face_uvs(core, l, c, rect_w, rect_h);
        let mut flags = 0.0;
        if c.style & crate::core::layout::style::BOLD != 0 {
            flags += FLAG_BOLD;
        }
        if sprite {
            flags += FLAG_SPRITE;
        }
        q.push(corners, uv, col, face, c.scale, flags, [i as f32, c.line_number as f32, word_of[i]], i as i32, flip);
    }
    // Underline / strikethrough after glyphs.
    for d in l.decorations.iter().filter(|d| d.kind != 2) {
        let key = SurfaceKey { kind: 2, sprite: false, font: d.font, atlas: 0, material: 0 };
        let q = out.entry(key).or_default();
        let corners = [[d.x0, d.y0], [d.x0, d.y1], [d.x1, d.y1], [d.x1, d.y0]];
        q.push(corners, [[0.0; 2]; 4], [d.color; 4], [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], d.scale, FLAG_SOLID + FLAG_DECORATION, [-1.0, 0.0, -1.0], -1, flip);
    }
    out
}

fn face_uvs(core: &TypesetterTextCore, l: &Layout, c: &CharInfo, rect_w: f32, rect_h: f32) -> [[f32; 2]; 4] {
    let (bl, tr) = (c.bottom_left, c.top_right);
    let u = |x: f32| -> f32 {
        match core.horizontal_mapping {
            1 => {
                let ln = &l.lines[c.line_number.max(0) as usize];
                let w = (ln.extents_max.x - ln.extents_min.x).max(1e-3);
                (x - ln.extents_min.x) / w + core.mapping_uv_line_offset * c.line_number as f32
            }
            2 | 3 => (x - core.settings().rect[0]) / rect_w,
            _ => 0.0,
        }
    };
    let v = |y: f32| -> f32 {
        match core.vertical_mapping {
            1 => {
                let ln = &l.lines[c.line_number.max(0) as usize];
                (y - ln.descender) / (ln.ascender - ln.descender).max(1e-3)
            }
            2 | 3 => (y - core.settings().rect[1]) / rect_h,
            _ => 0.0,
        }
    };
    let (u0, u1) = if core.horizontal_mapping == 0 { (0.0, 1.0) } else { (u(bl.x), u(tr.x)) };
    let (v0, v1) = if core.vertical_mapping == 0 { (0.0, 1.0) } else { (v(bl.y), v(tr.y)) };
    [[u0, v0], [u0, v1], [u1, v1], [u1, v0]]
}

pub fn format(is_2d: bool) -> ArrayFormat {
    // ARRAY_CUSTOM_RGBA_FLOAT (7) in CUSTOM0 (shift 13) and CUSTOM1 (shift 16).
    let custom = (7u64 << 13) | (7u64 << 16);
    let mut bits = custom;
    if is_2d {
        bits |= ArrayFormat::FLAG_USE_2D_VERTICES.ord();
    }
    ArrayFormat::from_ord(bits)
}

pub fn to_arrays(q: &Quads, is_2d: bool) -> VarArray {
    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    let n = q.positions.len();
    if is_2d {
        let v: PackedVector2Array = q.positions.iter().map(|p| Vector2::new(p[0], p[1])).collect();
        arrays.set(ArrayType::VERTEX.ord() as usize, &v.to_variant());
    } else {
        let v: PackedVector3Array = q.positions.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect();
        arrays.set(ArrayType::VERTEX.ord() as usize, &v.to_variant());
        let nrm: PackedVector3Array = (0..n).map(|_| Vector3::new(0.0, 0.0, 1.0)).collect();
        arrays.set(ArrayType::NORMAL.ord() as usize, &nrm.to_variant());
        let tan: PackedFloat32Array = (0..n).flat_map(|_| [1.0f32, 0.0, 0.0, 1.0]).collect();
        arrays.set(ArrayType::TANGENT.ord() as usize, &tan.to_variant());
    }
    let uv: PackedVector2Array = q.uvs.iter().map(|u| Vector2::new(u[0], u[1])).collect();
    arrays.set(ArrayType::TEX_UV.ord() as usize, &uv.to_variant());
    let col: PackedColorArray = q.colors.iter().map(|c| Color::from_rgba(c[0], c[1], c[2], c[3])).collect();
    arrays.set(ArrayType::COLOR.ord() as usize, &col.to_variant());
    let c0: PackedFloat32Array = q.custom0.iter().flat_map(|c| c.iter().copied()).collect();
    arrays.set(ArrayType::CUSTOM0.ord() as usize, &c0.to_variant());
    let c1: PackedFloat32Array = q.custom1.iter().flat_map(|c| c.iter().copied()).collect();
    arrays.set(ArrayType::CUSTOM1.ord() as usize, &c1.to_variant());
    let mut idx = PackedInt32Array::new();
    for k in 0..(n / 4) as i32 {
        let b = k * 4;
        // BL, TL, TR is clockwise seen from +Z: Godot's front face in 3D.
        idx.extend_array(&PackedInt32Array::from(&[b, b + 1, b + 2, b + 2, b + 3, b][..]));
    }
    arrays.set(ArrayType::INDEX.ord() as usize, &idx.to_variant());
    arrays
}

/// Rewrites a 2D surface's vertex and attribute data in place when `mesh` already
/// holds one triangle surface with the same vertex count (the index buffer is then
/// identical). The byte layout comes from Godot's own stride and offset queries.
/// Returns false when the surface has to be rebuilt instead.
fn headless() -> bool {
    thread_local! {
        static HEADLESS: bool = godot::classes::DisplayServer::singleton().get_name().to_string() == "headless";
    }
    HEADLESS.with(|h| *h)
}

pub fn update_2d_in_place(mesh: &Gd<ArrayMesh>, q: &Quads, is_2d: bool) -> bool {
    let n = q.positions.len();
    // The headless display server's dummy renderer ignores region updates.
    if !is_2d || n == 0 || mesh.get_surface_count() != 1 || headless() {
        return false;
    }
    if mesh.surface_get_array_len(0) as usize != n || mesh.surface_get_primitive_type(0) != PrimitiveType::TRIANGLES {
        return false;
    }
    let stored = mesh.surface_get_format(0);
    if stored.ord() & ArrayFormat::FLAG_USE_2D_VERTICES.ord() == 0 {
        return false;
    }
    let (vertex, attribute) = encode_2d(q, stored.ord());
    let rid = mesh.get_rid();
    let mut rs = RenderingServer::singleton();
    rs.mesh_surface_update_vertex_region(rid, 0, 0, &vertex);
    rs.mesh_surface_update_attribute_region(rid, 0, 0, &attribute);
    let (mut lo, mut hi) = (Vector2::new(f32::MAX, f32::MAX), Vector2::new(f32::MIN, f32::MIN));
    for p in &q.positions {
        lo = Vector2::new(lo.x.min(p[0]), lo.y.min(p[1]));
        hi = Vector2::new(hi.x.max(p[0]), hi.y.max(p[1]));
    }
    rs.mesh_set_custom_aabb(rid, Aabb::new(Vector3::new(lo.x, lo.y, 0.0), Vector3::new(hi.x - lo.x, hi.y - lo.y, 0.0)));
    true
}

/// A 2D surface's vertex and attribute streams, laid out as Godot stores them for
/// `format`: 2D positions as f32 pairs; colour as RGBA8; UV as f32 pairs; CUSTOM0/1
/// as four f32 each.
pub fn encode_2d(q: &Quads, format: u64) -> (PackedByteArray, PackedByteArray) {
    let rs = RenderingServer::singleton();
    let fmt = rendering_server::ArrayFormat::from_ord(format);
    let n = q.positions.len();
    let ni = n as i32;
    let vs = rs.mesh_surface_get_format_vertex_stride(fmt, ni) as usize;
    let at = rs.mesh_surface_get_format_attribute_stride(fmt, ni) as usize;
    let off = |a: rendering_server::ArrayType| rs.mesh_surface_get_format_offset(fmt, ni, a.ord()) as usize;
    let (o_v, o_c, o_uv, o_c0, o_c1) = (
        off(rendering_server::ArrayType::VERTEX),
        off(rendering_server::ArrayType::COLOR),
        off(rendering_server::ArrayType::TEX_UV),
        off(rendering_server::ArrayType::CUSTOM0),
        off(rendering_server::ArrayType::CUSTOM1),
    );
    let mut v = vec![0u8; vs * n];
    let mut a = vec![0u8; at * n];
    let put = |buf: &mut [u8], at: usize, vals: &[f32]| {
        for (k, x) in vals.iter().enumerate() {
            buf[at + 4 * k..at + 4 * k + 4].copy_from_slice(&x.to_le_bytes());
        }
    };
    for i in 0..n {
        let p = q.positions[i];
        put(&mut v, i * vs + o_v, &[p[0], p[1]]);
        let c = q.colors[i];
        let b = i * at;
        for k in 0..4 {
            a[b + o_c + k] = (c[k].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        put(&mut a, b + o_uv, &q.uvs[i]);
        put(&mut a, b + o_c0, &q.custom0[i]);
        put(&mut a, b + o_c1, &q.custom1[i]);
    }
    (PackedByteArray::from(v.as_slice()), PackedByteArray::from(a.as_slice()))
}
