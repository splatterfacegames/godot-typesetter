//! textInfo: TMP_TextInfo and its character / word / line / link / page /
//! mesh records, in node space (2D: y-down from the rect's top-left; 3D: y-up
//! about the pivot).

use std::cell::Cell;

use godot::obj::WithBaseField;
use godot::prelude::*;

use super::font_asset::TypesetterFontAsset;
use super::text::TypesetterTextCore;
use crate::core::layout::ElementType;

macro_rules! record {
    ($(#[$doc:meta])* $name:ident { $($(#[$fdoc:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$doc])*
        #[derive(GodotClass)]
        #[class(init, base = RefCounted)]
        pub struct $name {
            base: Base<RefCounted>,
            $( $(#[$fdoc])* #[var] pub $field: $ty, )*
        }
        #[godot_api]
        impl $name {}
    };
}

record!(
    /// One character or sprite (TMP_CharacterInfo).
    TypesetterCharacterInfo {
        character: GString,
        unicode: i64,
        index: i32,
        string_length: i32,
        /// 0 character, 1 sprite.
        element_type: i32,
        font_asset: Option<Gd<TypesetterFontAsset>>,
        glyph_index: i64,
        material_reference_index: i32,
        point_size: f32,
        line_number: i32,
        page_number: i32,
        vertex_index: i32,
        top_left: Vector3,
        bottom_left: Vector3,
        top_right: Vector3,
        bottom_right: Vector3,
        origin: f32,
        x_advance: f32,
        ascender: f32,
        base_line: f32,
        descender: f32,
        aspect_ratio: f32,
        scale: f32,
        color: Color,
        underline_color: Color,
        strikethrough_color: Color,
        highlight_color: Color,
        style: i32,
        is_visible: bool,
        link_index: i32,
    }
);

record!(
    /// A word (TMP_WordInfo).
    TypesetterWordInfo {
        first_character_index: i32,
        last_character_index: i32,
        character_count: i32,
        text: GString,
    }
);

record!(
    /// A line (TMP_LineInfo).
    TypesetterLineInfo {
        character_count: i32,
        visible_character_count: i32,
        space_count: i32,
        visible_space_count: i32,
        word_count: i32,
        first_character_index: i32,
        first_visible_character_index: i32,
        last_character_index: i32,
        last_visible_character_index: i32,
        length: f32,
        line_height: f32,
        ascender: f32,
        baseline: f32,
        descender: f32,
        max_advance: f32,
        width: f32,
        margin_left: f32,
        margin_right: f32,
        /// HorizontalAlignmentOptions.
        alignment: i32,
        line_extents: Rect2,
    }
);

record!(
    /// A `<link>` (TMP_LinkInfo).
    TypesetterLinkInfo {
        link_id: GString,
        link_text: GString,
        link_text_first_character_index: i32,
        link_text_length: i32,
        hash_code: i32,
    }
);

record!(
    /// A page in Page overflow mode (TMP_PageInfo).
    TypesetterPageInfo {
        first_character_index: i32,
        last_character_index: i32,
        ascender: f32,
        base_line: f32,
        descender: f32,
    }
);

record!(
    /// Vertex data of one surface (TMP_MeshInfo). Edit `vertices` / `colors32`
    /// (assign the arrays back) then call the node's `update_vertex_data()`.
    TypesetterMeshInfo {
        vertex_count: i32,
        vertices: PackedVector3Array,
        uvs0: PackedVector4Array,
        uvs2: PackedVector2Array,
        colors32: PackedColorArray,
        custom1: PackedVector4Array,
        texture: Variant,
        font_asset: Variant,
    }
);

/// Everything about the laid-out text (TMP_TextInfo).
///
/// Counts are filled at once. The character, word, line, link, page and mesh
/// arrays are built the first time they are read and then kept, so a layout
/// that nobody inspects costs nothing extra.
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct TypesetterTextInfo {
    base: Base<RefCounted>,
    #[var]
    pub character_count: i32,
    #[var]
    pub sprite_count: i32,
    #[var]
    pub space_count: i32,
    #[var]
    pub word_count: i32,
    #[var]
    pub link_count: i32,
    #[var]
    pub line_count: i32,
    #[var]
    pub page_count: i32,
    #[var(get = get_material_count, set = set_material_count)]
    pub material_count: i32,
    #[var(get = get_character_info, set = set_character_info)]
    pub character_info: Array<Gd<TypesetterCharacterInfo>>,
    #[var(get = get_word_info, set = set_word_info)]
    pub word_info: Array<Gd<TypesetterWordInfo>>,
    #[var(get = get_line_info, set = set_line_info)]
    pub line_info: Array<Gd<TypesetterLineInfo>>,
    #[var(get = get_link_info, set = set_link_info)]
    pub link_info: Array<Gd<TypesetterLinkInfo>>,
    #[var(get = get_page_info, set = set_page_info)]
    pub page_info: Array<Gd<TypesetterPageInfo>>,
    #[var(get = get_mesh_info, set = set_mesh_info)]
    pub mesh_info: Array<Gd<TypesetterMeshInfo>>,
    /// The core whose layout this describes (weak: the core caches this object).
    source: Option<InstanceId>,
    /// Sections already built (bits: SEC_*).
    built: Cell<u8>,
}

const SEC_CHARS: u8 = 1;
const SEC_WORDS: u8 = 2;
const SEC_LINES: u8 = 4;
const SEC_LINKS: u8 = 8;
const SEC_PAGES: u8 = 16;
const SEC_MESHES: u8 = 32;

#[godot_api]
impl TypesetterTextInfo {
    #[func]
    pub fn get_character_info(&self) -> Array<Gd<TypesetterCharacterInfo>> {
        self.ensure_from_source(SEC_CHARS);
        self.character_info.clone()
    }
    #[func]
    pub fn set_character_info(&mut self, value: Array<Gd<TypesetterCharacterInfo>>) {
        self.character_info = value;
        self.mark(SEC_CHARS);
    }
    #[func]
    pub fn get_word_info(&self) -> Array<Gd<TypesetterWordInfo>> {
        self.ensure_from_source(SEC_WORDS);
        self.word_info.clone()
    }
    #[func]
    pub fn set_word_info(&mut self, value: Array<Gd<TypesetterWordInfo>>) {
        self.word_info = value;
        self.mark(SEC_WORDS);
    }
    #[func]
    pub fn get_line_info(&self) -> Array<Gd<TypesetterLineInfo>> {
        self.ensure_from_source(SEC_LINES);
        self.line_info.clone()
    }
    #[func]
    pub fn set_line_info(&mut self, value: Array<Gd<TypesetterLineInfo>>) {
        self.line_info = value;
        self.mark(SEC_LINES);
    }
    #[func]
    pub fn get_link_info(&self) -> Array<Gd<TypesetterLinkInfo>> {
        self.ensure_from_source(SEC_LINKS);
        self.link_info.clone()
    }
    #[func]
    pub fn set_link_info(&mut self, value: Array<Gd<TypesetterLinkInfo>>) {
        self.link_info = value;
        self.mark(SEC_LINKS);
    }
    #[func]
    pub fn get_page_info(&self) -> Array<Gd<TypesetterPageInfo>> {
        self.ensure_from_source(SEC_PAGES);
        self.page_info.clone()
    }
    #[func]
    pub fn set_page_info(&mut self, value: Array<Gd<TypesetterPageInfo>>) {
        self.page_info = value;
        self.mark(SEC_PAGES);
    }
    #[func]
    pub fn get_mesh_info(&self) -> Array<Gd<TypesetterMeshInfo>> {
        self.ensure_from_source(SEC_MESHES);
        self.mesh_info.clone()
    }
    #[func]
    pub fn set_mesh_info(&mut self, value: Array<Gd<TypesetterMeshInfo>>) {
        self.mesh_info = value;
        self.mark(SEC_MESHES);
    }

    #[func]
    pub fn get_material_count(&self) -> i32 {
        self.ensure_from_source(SEC_MESHES);
        self.mesh_info.len() as i32
    }

    #[func]
    pub fn set_material_count(&mut self, value: i32) {
        self.material_count = value;
    }

    /// Text of word `index` (TMP_WordInfo.GetWord()).
    #[func]
    pub fn get_word(&self, index: i32) -> GString {
        self.get_word_info().get(index as usize).map(|w| w.bind().text.clone()).unwrap_or_default()
    }

    /// Restores mesh_info vertex data from the last layout (TMP's
    /// `CopyMeshInfoVertexData` / `ClearMeshInfo`-style reset for animations).
    #[func]
    pub fn get_cached_mesh_info(&self) -> Array<Gd<TypesetterMeshInfo>> {
        let mut out = Array::new();
        for m in self.get_mesh_info().iter_shared() {
            let b = m.bind();
            let mut c = TypesetterMeshInfo::new_gd();
            {
                let mut cb = c.bind_mut();
                cb.vertex_count = b.vertex_count;
                cb.vertices = b.vertices.clone();
                cb.uvs0 = b.uvs0.clone();
                cb.uvs2 = b.uvs2.clone();
                cb.colors32 = b.colors32.clone();
                cb.custom1 = b.custom1.clone();
                cb.texture = b.texture.clone();
                cb.font_asset = b.font_asset.clone();
            }
            out.push(&c);
        }
        out
    }
}

impl TypesetterTextInfo {
    pub const MESHES: u8 = SEC_MESHES;

    fn mark(&self, bits: u8) {
        self.built.set(self.built.get() | bits);
    }

    /// Builds missing sections from the source core, when it is still alive and
    /// not dropped (calls from inside the core use `ensure_with`).
    fn ensure_from_source(&self, bits: u8) {
        if self.built.get() & bits == bits {
            return;
        }
        let Some(id) = self.source else { return };
        let Ok(core) = Gd::<TypesetterTextCore>::try_from_instance_id(id) else { return };
        let b = core.bind();
        self.ensure_with(&b, bits);
    }

    /// Builds missing sections from `core`'s current layout. The arrays are
    /// shared handles, so filling a clone fills the field.
    pub fn ensure_with(&self, core: &TypesetterTextCore, bits: u8) {
        let missing = bits & !self.built.get();
        if missing == 0 {
            return;
        }
        self.mark(missing);
        let Some(l) = core.layout.as_ref() else { return };
        if missing & (SEC_CHARS | SEC_MESHES) != 0 {
            let quads = super::mesh::build_quads(l, core);
            if missing & SEC_MESHES != 0 {
                let mut out = self.mesh_info.clone();
                build_meshes(core, &quads, &mut out);
            }
            if missing & SEC_CHARS != 0 {
                let mut out = self.character_info.clone();
                build_chars(core, l, &quads, &mut out);
            }
        }
        if missing & SEC_WORDS != 0 {
            let mut out = self.word_info.clone();
            build_words(l, &mut out);
        }
        if missing & SEC_LINES != 0 {
            let mut out = self.line_info.clone();
            build_lines(core, l, &mut out);
        }
        if missing & SEC_LINKS != 0 {
            let mut out = self.link_info.clone();
            build_links(l, &mut out);
        }
        if missing & SEC_PAGES != 0 {
            let mut out = self.page_info.clone();
            build_pages(core, l, &mut out);
        }
    }
}

fn color(c: [f32; 4]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], c[3])
}

fn flip_of(core: &TypesetterTextCore) -> impl Fn(f32) -> f32 {
    let flip = !core.world_space;
    move |y: f32| if flip { -y } else { y }
}

type QuadMap = std::collections::BTreeMap<super::mesh::SurfaceKey, super::mesh::Quads>;

fn build_meshes(core: &TypesetterTextCore, quads: &QuadMap, out: &mut Array<Gd<TypesetterMeshInfo>>) {
    let fonts = core.fonts.as_ref();
    for (key, q) in quads.iter() {
        let mut m = TypesetterMeshInfo::new_gd();
        {
            let mut b = m.bind_mut();
            b.vertex_count = q.positions.len() as i32;
            b.vertices = q.positions.iter().map(|p| Vector3::new(p[0], p[1], p[2])).collect();
            b.uvs0 = q.uvs.iter().zip(q.custom0.iter()).map(|(u, c)| Vector4::new(u[0], u[1], c[3], c[2])).collect();
            b.uvs2 = q.custom0.iter().map(|c| Vector2::new(c[0], c[1])).collect();
            b.colors32 = q.colors.iter().map(|&c| color(c)).collect();
            b.custom1 = q.custom1.iter().map(|c| Vector4::new(c[0], c[1], c[2], c[3])).collect();
            if let Some(f) = fonts.and_then(|f| f.assets.get(key.font as usize)) {
                b.font_asset = f.to_variant();
                let t = f.bind().atlas_textures.get(key.atlas as usize);
                b.texture = t.map(|t| t.to_variant()).unwrap_or_default();
            }
        }
        out.push(&m);
    }
}

fn build_chars(core: &TypesetterTextCore, l: &crate::core::layout::Layout, quads: &QuadMap, out: &mut Array<Gd<TypesetterCharacterInfo>>) {
    let fonts = core.fonts.as_ref();
    let fy = flip_of(core);
    let v3 = |x: f32, y: f32| Vector3::new(x, fy(y), 0.0);
    let mut mat_of = vec![0i32; l.characters.len()];
    let mut vtx_of = vec![0i32; l.characters.len()];
    for (si, q) in quads.values().enumerate() {
        for (qi, &owner) in q.owners.iter().enumerate() {
            if owner >= 0 {
                mat_of[owner as usize] = si as i32;
                vtx_of[owner as usize] = (qi * 4) as i32;
            }
        }
    }
    for (i, c) in l.characters.iter().enumerate() {
        let mut o = TypesetterCharacterInfo::new_gd();
        {
            let mut b = o.bind_mut();
            b.character = char::from_u32(c.character).map(|ch| ch.to_string()).unwrap_or_default().as_str().into();
            b.unicode = c.character as i64;
            b.index = c.index;
            b.string_length = c.string_length;
            b.element_type = if c.element_type == ElementType::Sprite { 1 } else { 0 };
            if c.element_type == ElementType::Character {
                b.font_asset = fonts.and_then(|f| f.assets.get(c.font as usize).cloned());
            }
            b.glyph_index = c.glyph as i64;
            b.material_reference_index = mat_of[i];
            b.point_size = c.point_size;
            b.line_number = c.line_number;
            b.page_number = c.page_number;
            b.vertex_index = vtx_of[i];
            b.top_left = v3(c.top_left.x, c.top_left.y);
            b.bottom_left = v3(c.bottom_left.x, c.bottom_left.y);
            b.top_right = v3(c.top_right.x, c.top_right.y);
            b.bottom_right = v3(c.bottom_right.x, c.bottom_right.y);
            b.origin = c.origin;
            b.x_advance = c.x_advance;
            b.ascender = fy(c.ascender);
            b.base_line = fy(c.base_line);
            b.descender = fy(c.descender);
            b.aspect_ratio = c.aspect_ratio;
            b.scale = c.scale;
            b.color = color(c.color);
            b.underline_color = color(c.underline_color);
            b.strikethrough_color = color(c.strikethrough_color);
            b.highlight_color = color(c.highlight_color);
            b.style = c.style as i32;
            b.is_visible = c.is_visible;
            b.link_index = c.link;
        }
        out.push(&o);
    }
}

fn build_words(l: &crate::core::layout::Layout, out: &mut Array<Gd<TypesetterWordInfo>>) {
    for w in &l.words {
        let mut o = TypesetterWordInfo::new_gd();
        {
            let mut b = o.bind_mut();
            b.first_character_index = w.first_character_index;
            b.last_character_index = w.last_character_index;
            b.character_count = w.character_count;
            let s: String = (w.first_character_index..=w.last_character_index)
                .filter_map(|k| l.characters.get(k as usize))
                .filter_map(|c| char::from_u32(c.character))
                .collect();
            b.text = s.as_str().into();
        }
        out.push(&o);
    }
}

fn build_lines(core: &TypesetterTextCore, l: &crate::core::layout::Layout, out: &mut Array<Gd<TypesetterLineInfo>>) {
    let fy = flip_of(core);
    for ln in &l.lines {
        let mut o = TypesetterLineInfo::new_gd();
        {
            let mut b = o.bind_mut();
            b.character_count = ln.character_count;
            b.visible_character_count = ln.visible_character_count;
            b.space_count = ln.space_count;
            b.visible_space_count = ln.visible_space_count;
            b.word_count = ln.word_count;
            b.first_character_index = ln.first_character_index;
            b.first_visible_character_index = ln.first_visible_character_index;
            b.last_character_index = ln.last_character_index;
            b.last_visible_character_index = ln.last_visible_character_index;
            b.length = ln.length;
            b.line_height = ln.line_height;
            b.ascender = fy(ln.ascender);
            b.baseline = fy(ln.baseline);
            b.descender = fy(ln.descender);
            b.max_advance = ln.max_advance;
            b.width = ln.width;
            b.margin_left = ln.margin_left;
            b.margin_right = ln.margin_right;
            b.alignment = match ln.alignment {
                crate::core::layout::HAlign::Left => 1,
                crate::core::layout::HAlign::Center => 2,
                crate::core::layout::HAlign::Right => 4,
                crate::core::layout::HAlign::Justified => 8,
                crate::core::layout::HAlign::Flush => 16,
                crate::core::layout::HAlign::Geometry => 32,
            };
            let (y0, y1) = (fy(ln.extents_min.y), fy(ln.extents_max.y));
            b.line_extents = Rect2::new(Vector2::new(ln.extents_min.x, y0.min(y1)), Vector2::new(ln.extents_max.x - ln.extents_min.x, (y1 - y0).abs()));
        }
        out.push(&o);
    }
}

fn build_links(l: &crate::core::layout::Layout, out: &mut Array<Gd<TypesetterLinkInfo>>) {
    for lk in &l.links {
        let mut o = TypesetterLinkInfo::new_gd();
        {
            let mut b = o.bind_mut();
            b.link_id = lk.link_id.as_str().into();
            b.link_text_first_character_index = lk.link_text_first_character_index;
            b.link_text_length = lk.link_text_length;
            b.hash_code = lk.hash_code;
            let s: String = (lk.link_text_first_character_index..lk.link_text_first_character_index + lk.link_text_length)
                .filter_map(|k| l.characters.get(k as usize))
                .filter_map(|c| char::from_u32(c.character))
                .collect();
            b.link_text = s.as_str().into();
        }
        out.push(&o);
    }
}

fn build_pages(core: &TypesetterTextCore, l: &crate::core::layout::Layout, out: &mut Array<Gd<TypesetterPageInfo>>) {
    let fy = flip_of(core);
    for p in &l.pages {
        let mut o = TypesetterPageInfo::new_gd();
        {
            let mut b = o.bind_mut();
            b.first_character_index = p.first_character_index;
            b.last_character_index = p.last_character_index;
            b.ascender = fy(p.ascender);
            b.base_line = fy(p.base_line);
            b.descender = fy(p.descender);
        }
        out.push(&o);
    }
}

/// textInfo for `core`'s current layout: counts now, sections on first read.
pub fn make_text_info(core: &TypesetterTextCore) -> Gd<TypesetterTextInfo> {
    let mut info = TypesetterTextInfo::new_gd();
    let Some(l) = core.layout.as_ref() else { return info };
    {
        let mut b = info.bind_mut();
        b.character_count = l.characters.len() as i32;
        b.sprite_count = l.sprite_count;
        b.space_count = l.space_count;
        b.word_count = l.words.len() as i32;
        b.link_count = l.links.len() as i32;
        b.line_count = l.lines.len() as i32;
        b.page_count = l.pages.len() as i32;
        b.source = Some(core.to_gd().instance_id());
    }
    info
}
