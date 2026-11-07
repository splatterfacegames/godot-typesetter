//! Text layout with TextMesh Pro semantics, engine-independent.
//!
//! Coordinates follow TextMesh Pro's convention so results can be compared with
//! Unity directly: x right, y up, in the text container's local space (the rect
//! is given explicitly). The Godot bindings convert to y-down where needed.
//!
//! Font size units: a font size of N maps the asset's sampling point size P to
//! N units (scale = N / P), times 0.1 for world-space (3D) text.

use std::collections::HashMap;

use super::richtext::{self, Length, Tag, Token};

// ------------------------------------------------------------------ inputs

/// Face metrics of a font asset, in pixels at its sampling point size.
#[derive(Clone, Copy, Debug, Default)]
pub struct FaceMetrics {
    pub point_size: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
    pub cap_line: f32,
    pub mean_line: f32,
    pub superscript_offset: f32,
    pub superscript_size: f32,
    pub subscript_offset: f32,
    pub subscript_size: f32,
    pub underline_offset: f32,
    pub underline_thickness: f32,
    pub strikethrough_offset: f32,
    pub strikethrough_thickness: f32,
    pub tab_width: f32,
    /// Style values (TMP font asset fields).
    pub bold_spacing: f32,
    pub normal_spacing_offset: f32,
    pub italic_style: f32,
    pub tab_size: f32,
    /// Atlas padding in pixels at the sampling size (TMP `atlasPadding`).
    pub atlas_padding: f32,
    /// Material weight added for bold (TMP font asset `boldStyle`).
    pub bold_style: f32,
}

/// A glyph in an atlas, metrics in pixels at the asset's sampling point size.
#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphInfo {
    pub index: u32,
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    /// Atlas rect (x, y-from-top, w, h) in texels.
    pub rect: [f32; 4],
    pub atlas_index: i32,
    pub atlas_size: [f32; 2],
}

/// Resolves fonts and glyphs for the layout (implemented by the bindings).
pub trait FontProvider {
    fn face(&self, font: u32) -> FaceMetrics;
    /// Finds `cp` in `font` or its fallbacks (adding it to a dynamic atlas if
    /// needed). Returns the font that supplied the glyph.
    fn glyph(&mut self, font: u32, cp: u32) -> Option<(u32, GlyphInfo)>;
    /// Pair adjustment (first glyph xp, yp, xa, ya, second glyph xp, yp, xa, ya).
    fn kerning(&self, font: u32, left: u32, right: u32) -> Option<[f32; 8]>;
    /// Looks up a font by name (`<font="name">`).
    fn font_by_name(&mut self, _name: &str) -> Option<u32> {
        None
    }
    /// Looks up a material preset by name for `<material="name">`.
    fn material_by_name(&mut self, _font: u32, _name: &str) -> Option<u32> {
        None
    }
    /// Sprite lookup (`<sprite=N>` / `<sprite name="x">`): returns sprite id.
    fn sprite(&mut self, _asset: Option<&str>, _index: Option<i32>, _name: Option<&str>) -> Option<SpriteInfo> {
        None
    }
    /// Sprite whose unicode matches `cp` (used for characters missing from
    /// every font, like TMP's emoji / sprite-asset fallback).
    fn sprite_by_unicode(&mut self, _cp: u32) -> Option<SpriteInfo> {
        None
    }
    /// Line-breaking character sets for CJK text: (characters that may not end
    /// a line, characters that may not start one). `None` uses the defaults.
    fn line_breaking_rules(&mut self) -> Option<(String, String)> {
        None
    }
    /// Named style from the style sheet: (opening markup, closing markup).
    fn style(&mut self, _name: &str) -> Option<(String, String)> {
        None
    }
    /// Named colour gradient preset for `<gradient="name">`.
    fn gradient(&mut self, _name: &str) -> Option<[[f32; 4]; 4]> {
        None
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SpriteInfo {
    pub asset: u32,
    pub index: u32,
    pub unicode: u32,
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub scale: f32,
    pub rect: [f32; 4],
    pub atlas_size: [f32; 2],
    /// Sprite asset face metrics (point size, ascent, descent).
    pub point_size: f32,
    pub ascent: f32,
    pub descent: f32,
    pub tint: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
    Justified,
    Flush,
    Geometry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
    Baseline,
    Geometry,
    Capline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    NoWrap,
    Normal,
    PreserveWhitespace,
    PreserveWhitespaceNoWrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    Overflow,
    Ellipsis,
    Masking,
    Truncate,
    ScrollRect,
    Page,
    Linked,
}

pub mod style {
    pub const BOLD: u32 = 1;
    pub const ITALIC: u32 = 2;
    pub const UNDERLINE: u32 = 4;
    pub const LOWERCASE: u32 = 8;
    pub const UPPERCASE: u32 = 16;
    pub const SMALLCAPS: u32 = 32;
    pub const STRIKETHROUGH: u32 = 64;
    pub const SUPERSCRIPT: u32 = 128;
    pub const SUBSCRIPT: u32 = 256;
    pub const HIGHLIGHT: u32 = 512;
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub rect: [f32; 4], // x_min, y_min, width, height
    pub margin: [f32; 4], // left, top, right, bottom
    pub font: u32,
    pub font_size: f32,
    pub font_style: u32,
    pub font_weight: i32,
    pub color: [f32; 4],
    pub auto_size: bool,
    pub font_size_min: f32,
    pub font_size_max: f32,
    pub char_width_max_adj: f32,
    pub line_spacing_max_adj: f32,
    pub halign: HAlign,
    pub valign: VAlign,
    pub wrap: Wrap,
    pub overflow: Overflow,
    pub character_spacing: f32,
    pub word_spacing: f32,
    pub line_spacing: f32,
    pub paragraph_spacing: f32,
    pub word_wrapping_ratios: f32,
    pub rich_text: bool,
    pub parse_ctrl_characters: bool,
    pub max_visible_characters: i32,
    pub max_visible_words: i32,
    pub max_visible_lines: i32,
    pub first_visible_character: i32,
    pub page_to_display: i32,
    pub world_space: bool,
    pub right_to_left: bool,
    pub extra_padding: bool,
    /// Quad padding around glyphs in sampling-size pixels (material dependent).
    pub padding: f32,
    pub kerning: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            rect: [0.0, 0.0, 200.0, 50.0],
            margin: [0.0; 4],
            font: 0,
            font_size: 36.0,
            font_style: 0,
            font_weight: 400,
            color: [1.0; 4],
            auto_size: false,
            font_size_min: 18.0,
            font_size_max: 72.0,
            char_width_max_adj: 0.0,
            line_spacing_max_adj: 0.0,
            halign: HAlign::Left,
            valign: VAlign::Top,
            wrap: Wrap::Normal,
            overflow: Overflow::Overflow,
            character_spacing: 0.0,
            word_spacing: 0.0,
            line_spacing: 0.0,
            paragraph_spacing: 0.0,
            word_wrapping_ratios: 0.4,
            rich_text: true,
            parse_ctrl_characters: true,
            max_visible_characters: 99999,
            max_visible_words: 99999,
            max_visible_lines: 99999,
            first_visible_character: 0,
            page_to_display: 1,
            world_space: false,
            right_to_left: false,
            extra_padding: false,
            padding: 1.25,
            kerning: true,
        }
    }
}

// ------------------------------------------------------------------ outputs

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

impl V2 {
    pub const fn new(x: f32, y: f32) -> Self {
        V2 { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ElementType {
    #[default]
    Character,
    Sprite,
}

#[derive(Clone, Debug, Default)]
pub struct CharInfo {
    pub character: u32,
    pub index: i32,
    pub string_length: i32,
    pub element_type: ElementType,
    pub font: u32,
    pub glyph: u32,
    pub sprite: Option<SpriteInfo>,
    pub material_reference_index: i32,
    pub point_size: f32,
    pub line_number: i32,
    pub page_number: i32,
    pub vertex_index: i32,
    pub top_left: V2,
    pub bottom_left: V2,
    pub top_right: V2,
    pub bottom_right: V2,
    pub origin: f32,
    pub x_advance: f32,
    pub ascender: f32,
    pub base_line: f32,
    pub descender: f32,
    pub aspect_ratio: f32,
    pub scale: f32,
    pub color: [f32; 4],
    pub underline_color: [f32; 4],
    pub strikethrough_color: [f32; 4],
    pub highlight_color: [f32; 4],
    pub style: u32,
    pub is_visible: bool,
    pub link: i32,
    pub gradient: Option<[[f32; 4]; 4]>,
    pub rotation: f32,
    pub material: u32,
    // internal
    pub uv_rect: [f32; 4],
    pub atlas_index: i32,
    pub atlas_size: [f32; 2],
    pub glyph_metrics: [f32; 5], // w, h, bx, by, adv in pt pixels
    pub elem_ascender: f32,
    pub elem_descender: f32,
    pub is_space: bool,
    pub is_newline: bool,
    pub break_after: bool,
    pub no_break: bool,
    pub voffset: f32,
    /// Element is an overflow placeholder (ellipsis slot / cut point): never part of a word.
    pub no_word: bool,
    /// The zero-width element after an ellipsis.
    pub ellipsis_tail: bool,
    /// Ascender and descender are reported at the baseline.
    pub baseline_metrics: bool,
    /// TMP reports this element's xAdvance relative to the line's left edge
    /// (the character before a `</cspace>`).
    pub xadv_from_line_left: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LineInfo {
    pub character_count: i32,
    pub visible_character_count: i32,
    pub space_count: i32,
    pub visible_space_count: i32,
    pub word_count: i32,
    pub first_character_index: i32,
    pub first_visible_character_index: i32,
    pub last_character_index: i32,
    pub last_visible_character_index: i32,
    pub length: f32,
    pub line_height: f32,
    pub ascender: f32,
    pub baseline: f32,
    pub descender: f32,
    pub max_advance: f32,
    pub width: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub alignment: HAlign,
    pub extents_min: V2,
    pub extents_max: V2,
}

impl Default for HAlign {
    fn default() -> Self {
        HAlign::Left
    }
}

#[derive(Clone, Debug, Default)]
pub struct WordInfo {
    pub first_character_index: i32,
    pub last_character_index: i32,
    pub character_count: i32,
}

#[derive(Clone, Debug, Default)]
pub struct LinkInfo {
    pub link_id: String,
    pub link_text_first_character_index: i32,
    pub link_text_length: i32,
    pub hash_code: i32,
}

#[derive(Clone, Debug, Default)]
pub struct PageInfo {
    pub first_character_index: i32,
    pub last_character_index: i32,
    pub ascender: f32,
    pub base_line: f32,
    pub descender: f32,
}

/// One quad's worth of decoration (underline / strikethrough / mark).
#[derive(Clone, Debug, Default)]
pub struct Decoration {
    pub kind: u8, // 0 underline, 1 strikethrough, 2 highlight
    pub font: u32,
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
    pub color: [f32; 4],
    pub page: i32,
    pub scale: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub characters: Vec<CharInfo>,
    pub lines: Vec<LineInfo>,
    pub words: Vec<WordInfo>,
    pub links: Vec<LinkInfo>,
    pub pages: Vec<PageInfo>,
    pub decorations: Vec<Decoration>,
    pub font_size: f32,
    pub preferred_width: f32,
    pub preferred_height: f32,
    pub rendered_min: V2,
    pub rendered_max: V2,
    pub is_text_overflowing: bool,
    pub first_overflow_character_index: i32,
    pub space_count: i32,
    pub sprite_count: i32,
    pub text_bounds_min: V2,
    pub text_bounds_max: V2,
    /// Whether pages were laid out (overflow mode Page): `pages` is meaningful.
    pub overflow_mode_page: bool,
    /// An unwrapped line runs past the right edge (auto-size fit test).
    pub h_overflow: bool,
}

impl Layout {
    pub fn rendered_size(&self) -> V2 {
        if self.characters.iter().any(|c| c.is_visible) {
            V2::new(self.rendered_max.x - self.rendered_min.x, self.rendered_max.y - self.rendered_min.y)
        } else {
            V2::default()
        }
    }
}

// ------------------------------------------------------------------ elements

#[derive(Clone, Debug)]
struct StyleState {
    font: u32,
    size: f32,
    style: u32,
    color: [f32; 4],
    alpha: Option<f32>,
    underline_color: Option<[f32; 4]>,
    strike_color: Option<[f32; 4]>,
    mark: Option<[f32; 4]>,
    cspace: f32,
    mspace: Option<f32>,
    voffset: f32,
    nobr: bool,
    link: i32,
    width_scale: Option<f32>,
    rotate: f32,
    gradient: Option<[[f32; 4]; 4]>,
    material: u32,
    halign: Option<HAlign>,
    indent: f32,
    line_indent: f32,
    margin_left: f32,
    margin_right: f32,
    line_height: Option<f32>,
    sup_sub_scale: f32,
    sup_sub_offset: f32,
    weight: i32,
}

#[derive(Clone, Debug)]
enum Cmd {
    Pos(f32),
    Space(f32),
    PageBreak,
}

#[derive(Clone, Debug)]
struct Elem {
    cp: u32,
    src: usize,
    /// Source chars this element stands for (TMP stringLength).
    len: usize,
    /// A `</cspace>` follows this element.
    close_cspace: bool,
    style: StyleState,
    cmd_before: Vec<Cmd>,
    sprite: Option<SpriteInfo>,
}

fn base_style(s: &Settings) -> StyleState {
    StyleState {
        font: s.font,
        size: s.font_size,
        style: s.font_style,
        color: s.color,
        alpha: None,
        underline_color: None,
        strike_color: None,
        mark: None,
        cspace: 0.0,
        mspace: None,
        voffset: 0.0,
        nobr: false,
        link: -1,
        width_scale: None,
        rotate: 0.0,
        gradient: None,
        material: 0,
        halign: None,
        indent: 0.0,
        line_indent: 0.0,
        margin_left: 0.0,
        margin_right: 0.0,
        line_height: None,
        sup_sub_scale: 1.0,
        sup_sub_offset: 0.0,
        weight: s.font_weight,
    }
}

/// Resolves a tag length to absolute units at the current size. `em_base` is
/// the font size; `pct_base` is what 100% means for this tag.
fn resolve_len(v: &str, font_size: f32, pct_base: f32, unit_scale: f32) -> Option<f32> {
    Some(match richtext::parse_length(v)? {
        Length::Pixels(p) => p * unit_scale,
        Length::Em(e) => e * font_size * unit_scale,
        Length::Percent(p) => p / 100.0 * pct_base,
    })
}

struct Parsed {
    elems: Vec<Elem>,
    links: Vec<LinkInfo>,
}

/// Rich-text interpreter state: walks tokens, keeps the style stacks and
/// produces elements. Tags whose value does not parse (or whose font / style /
/// sprite / gradient cannot be found) are emitted as literal text.
struct ParseState<'a> {
    s: &'a Settings,
    src: Vec<char>,
    base: StyleState,
    st: StyleState,
    /// Font size the `%` / `em` units of `<size>` refer to (the text's size).
    size: f32,
    unit: f32,
    stacks: HashMap<&'static str, Vec<StyleState>>,
    elems: Vec<Elem>,
    pending: Vec<Cmd>,
    links: Vec<LinkInfo>,
    open_link: Option<usize>,
    style_stack: Vec<String>,
}

/// TMP's case-insensitive name hash (djb2 with xor over upper-cased chars).
pub fn tag_hash(s: &str) -> i32 {
    s.chars().fold(0i32, |h, c| (h.wrapping_shl(5).wrapping_add(h)) ^ (c.to_ascii_uppercase() as i32))
}

impl<'a> ParseState<'a> {
    fn push_char(&mut self, cp: u32, index: usize, len: usize) {
        self.elems.push(Elem { cp, src: index, len, style: self.st.clone(), cmd_before: std::mem::take(&mut self.pending), sprite: None, close_cspace: false });
    }

    /// Emits the source text of `span` as literal characters.
    fn literal(&mut self, span: (usize, usize), ovr: Option<(usize, usize)>) {
        for k in span.0..span.1.min(self.src.len()) {
            let c = self.src[k] as u32;
            match ovr {
                Some((i, l)) => self.push_char(c, i, l),
                None => self.push_char(c, k, 1),
            }
        }
    }

    fn save(&mut self, k: &'static str) {
        self.stacks.entry(k).or_default().push(self.st.clone());
    }

    fn pop(&mut self, k: &'static str) -> Option<StyleState> {
        self.stacks.get_mut(k).and_then(|x| x.pop())
    }

    fn len(&self, v: &str, pct_base: f32) -> Option<f32> {
        resolve_len(v, self.st.size, pct_base, self.unit)
    }

    fn line_width(&self) -> f32 {
        self.s.rect[2] - self.s.margin[0] - self.s.margin[2]
    }

    fn feed<F: FontProvider>(&mut self, tok: Token, ovr: Option<(usize, usize)>, fonts: &mut F, depth: usize) {
        match tok {
            Token::Char(c, i, l) => {
                let (i, l) = ovr.unwrap_or((i, l));
                self.push_char(c as u32, i, l);
            }
            Token::Tag(t) => {
                if !self.tag(&t, ovr, fonts, depth) {
                    self.literal(t.span, ovr);
                }
            }
        }
    }

    /// Applies a tag; returns false when TMP would show it as text.
    fn tag<F: FontProvider>(&mut self, t: &Tag, ovr: Option<(usize, usize)>, fonts: &mut F, depth: usize) -> bool {
        let name = t.name.as_str();
        let v = t.value.clone().unwrap_or_default();
        let has_v = t.value.as_deref().is_some_and(|x| !x.is_empty());
        let (ti, tl) = ovr.unwrap_or((t.span.0, t.span.1 - t.span.0));
        macro_rules! restore {
            ($k:expr, $field:ident) => {{
                if let Some(prev) = self.pop($k) {
                    self.st.$field = prev.$field;
                } else {
                    self.st.$field = self.base.$field.clone();
                }
            }};
        }
        if t.closing {
            match name {
                "b" => self.st.style = (self.st.style & !style::BOLD) | (self.base.style & style::BOLD),
                "i" => self.st.style = (self.st.style & !style::ITALIC) | (self.base.style & style::ITALIC),
                "u" => {
                    self.st.style = (self.st.style & !style::UNDERLINE) | (self.base.style & style::UNDERLINE);
                    restore!("u", underline_color)
                }
                "s" => {
                    self.st.style = (self.st.style & !style::STRIKETHROUGH) | (self.base.style & style::STRIKETHROUGH);
                    restore!("s", strike_color)
                }
                "color" => {
                    restore!("color", color);
                    self.st.alpha = None;
                }
                "size" => restore!("size", size),
                "font" => {
                    if let Some(prev) = self.pop("font") {
                        self.st.font = prev.font;
                        self.st.material = prev.material;
                    } else {
                        self.st.font = self.base.font;
                        self.st.material = self.base.material;
                    }
                }
                "material" => restore!("material", material),
                "mark" => {
                    self.st.style &= !style::HIGHLIGHT;
                    restore!("mark", mark)
                }
                "sup" | "sub" => {
                    // Nested <sup>/<sub> unwind one level at a time.
                    if let Some(prev) = self.pop("supsub") {
                        self.st.sup_sub_scale = prev.sup_sub_scale;
                        self.st.sup_sub_offset = prev.sup_sub_offset;
                        let keep = prev.style & (style::SUPERSCRIPT | style::SUBSCRIPT);
                        self.st.style = (self.st.style & !(style::SUPERSCRIPT | style::SUBSCRIPT)) | keep;
                    } else {
                        self.st.style &= !(style::SUPERSCRIPT | style::SUBSCRIPT);
                        self.st.sup_sub_scale = 1.0;
                        self.st.sup_sub_offset = 0.0;
                    }
                }
                "cspace" => {
                    // Closing <cspace> takes the spacing back from the last
                    // character (and TMP then reports that character's
                    // xAdvance relative to the line's left edge).
                    if let Some(e) = self.elems.last_mut() {
                        e.close_cspace = true;
                    }
                    self.st.cspace = 0.0
                }
                "mspace" => self.st.mspace = None,
                "voffset" => self.st.voffset = 0.0,
                "indent" => self.st.indent = 0.0,
                "line-height" => self.st.line_height = None,
                "line-indent" => self.st.line_indent = 0.0,
                "margin" => {
                    self.st.margin_left = 0.0;
                    self.st.margin_right = 0.0
                }
                "align" => self.st.halign = None,
                "allcaps" | "uppercase" => self.st.style &= !style::UPPERCASE,
                "lowercase" => self.st.style &= !style::LOWERCASE,
                "smallcaps" => self.st.style &= !style::SMALLCAPS,
                "nobr" => self.st.nobr = false,
                "rotate" => self.st.rotate = 0.0,
                "width" => self.st.width_scale = None,
                "gradient" => self.st.gradient = None,
                "font-weight" => restore!("font-weight", weight),
                "link" | "a" => {
                    if name == "a" {
                        if let Some(prev) = self.pop("a") {
                            self.st.color = prev.color;
                            self.st.style = (self.st.style & !style::UNDERLINE) | (prev.style & style::UNDERLINE);
                        }
                    }
                    if let Some(li) = self.open_link.take() {
                        let first = self.links[li].link_text_first_character_index as usize;
                        self.links[li].link_text_length = (self.elems.len() - first) as i32;
                    }
                    self.st.link = -1;
                }
                "style" => {
                    if let Some(name) = self.style_stack.pop() {
                        if let Some((_, close)) = fonts.style(&name) {
                            self.expand(&close, (ti, tl), fonts, depth);
                        }
                    }
                }
                _ => {}
            }
            return true;
        }
        let pct_w = self.line_width();
        match name {
            "b" => self.st.style |= style::BOLD,
            "i" => self.st.style |= style::ITALIC,
            "u" => {
                self.save("u");
                self.st.style |= style::UNDERLINE;
                if let Some(c) = t.attrs.iter().find(|(k, _)| k == "color").and_then(|(_, c)| richtext::parse_color(c)) {
                    self.st.underline_color = Some(c);
                }
            }
            "s" => {
                self.save("s");
                self.st.style |= style::STRIKETHROUGH;
                if let Some(c) = t.attrs.iter().find(|(k, _)| k == "color").and_then(|(_, c)| richtext::parse_color(c)) {
                    self.st.strike_color = Some(c);
                }
            }
            "color" => {
                let Some(c) = richtext::parse_color(&v) else { return false };
                self.save("color");
                self.st.color = c;
                self.st.alpha = None;
            }
            "alpha" => {
                let Some(a) = richtext::parse_alpha(&v) else { return false };
                self.st.alpha = Some(a);
            }
            "size" => {
                if !has_v {
                    return false;
                }
                let cur = self.st.size;
                let relative = matches!(richtext::parse_length(&v), Some(Length::Pixels(_)));
                let new = if let (Some(rest), true) = (v.strip_prefix('+'), relative) {
                    resolve_len(rest, cur, cur, 1.0).map(|d| cur + d)
                } else if v.starts_with('-') && relative {
                    resolve_len(&v[1..], cur, cur, 1.0).map(|d| cur - d)
                } else {
                    match richtext::parse_length(&v) {
                        Some(Length::Percent(p)) => Some(self.size * p / 100.0),
                        Some(Length::Em(e)) => Some(self.size * e),
                        Some(Length::Pixels(p)) => Some(p),
                        None => None,
                    }
                };
                let Some(n) = new else { return false };
                self.save("size");
                self.st.size = n;
            }
            "font" => {
                if !has_v {
                    return false;
                }
                let Some(f) = fonts.font_by_name(&v) else { return false };
                self.save("font");
                self.st.font = f;
                self.st.material = match t.attrs.iter().find(|(k, _)| k == "material") {
                    Some((_, m)) => fonts.material_by_name(f, m).unwrap_or(0),
                    None => 0,
                };
            }
            "material" => {
                let Some(m) = fonts.material_by_name(self.st.font, &v) else { return false };
                self.save("material");
                self.st.material = m;
            }
            "mark" => {
                self.save("mark");
                self.st.style |= style::HIGHLIGHT;
                self.st.mark = Some(richtext::parse_color(&v).unwrap_or(DEFAULT_MARK));
            }
            "sup" | "sub" => {
                self.save("supsub");
                let f = fonts.face(self.st.font);
                let sup = name == "sup";
                self.st.style &= !(style::SUPERSCRIPT | style::SUBSCRIPT);
                self.st.style |= if sup { style::SUPERSCRIPT } else { style::SUBSCRIPT };
                let sz = if sup { f.superscript_size } else { f.subscript_size };
                let sz = if sz > 0.0 { sz } else { 0.5 };
                // The offset is the face's (scaled to the current, possibly
                // already reduced, size) times the reduction.
                let off = if sup { f.superscript_offset } else { f.subscript_offset };
                let cur = self.st.size * self.st.sup_sub_scale / f.point_size.max(1.0) * self.unit;
                self.st.sup_sub_offset += off * cur * sz;
                self.st.sup_sub_scale *= sz;
            }
            "cspace" | "mspace" | "voffset" | "indent" | "line-height" | "line-indent" | "rotate" | "width" | "pos"
            | "margin-left" | "margin-right" | "font-weight" | "space" => {
                if !has_v || (name == "space" && v.trim_end().ends_with('%')) {
                    return false;
                }
                match name {
                    "cspace" => self.st.cspace = self.len(&v, self.st.size).unwrap_or(0.0),
                    "mspace" => self.st.mspace = self.len(&v, self.st.size),
                    "voffset" => self.st.voffset = self.len(&v, self.st.size).unwrap_or(0.0),
                    "indent" => {
                        // Also moves the pen to the indent at once (even backwards).
                        let x = self.len(&v, pct_w).unwrap_or(0.0);
                        self.st.indent = x;
                        self.pending.push(Cmd::Pos(x));
                    }
                    "line-indent" => self.st.line_indent = self.len(&v, pct_w).unwrap_or(0.0),
                    "line-height" => {
                        // % is relative to the font's line height at the current size.
                        let f = fonts.face(self.st.font);
                        let natural = f.line_height * self.st.size / f.point_size.max(1.0) * self.unit;
                        self.st.line_height = self.len(&v, natural);
                    }
                    "margin-left" => self.st.margin_left = self.len(&v, pct_w).unwrap_or(0.0),
                    "margin-right" => self.st.margin_right = self.len(&v, pct_w).unwrap_or(0.0),
                    "rotate" => self.st.rotate = richtext::parse_length(&v).map(|l| match l {
                        Length::Pixels(x) | Length::Em(x) | Length::Percent(x) => x,
                    }).unwrap_or(0.0),
                    "width" => self.st.width_scale = self.len(&v, pct_w),
                    "font-weight" => {
                        self.save("font-weight");
                        self.st.weight = match richtext::parse_length(&v) {
                            Some(Length::Pixels(x)) => x as i32,
                            _ => 0,
                        };
                    }
                    "pos" => {
                        if let Some(x) = self.len(&v, pct_w) {
                            self.pending.push(Cmd::Pos(x));
                        }
                    }
                    "space" => {
                        if let Some(x) = self.len(&v, self.st.size) {
                            self.pending.push(Cmd::Space(x));
                        }
                    }
                    _ => {}
                }
            }
            "margin" => {
                let m = if has_v { self.len(&v, pct_w).unwrap_or(0.0) } else { 0.0 };
                self.st.margin_left = m;
                self.st.margin_right = m;
            }
            "align" => {
                self.st.halign = Some(match v.to_ascii_lowercase().as_str() {
                    "left" => HAlign::Left,
                    "center" => HAlign::Center,
                    "right" => HAlign::Right,
                    "justified" => HAlign::Justified,
                    "flush" => HAlign::Flush,
                    _ => return false,
                })
            }
            "allcaps" | "uppercase" => self.st.style |= style::UPPERCASE,
            "lowercase" => self.st.style |= style::LOWERCASE,
            "smallcaps" => self.st.style |= style::SMALLCAPS,
            "nobr" => self.st.nobr = true,
            // <page> only acts in Page overflow mode (elsewhere it is consumed).
            "page" => {
                if self.s.overflow == Overflow::Page {
                    self.pending.push(Cmd::PageBreak)
                }
            }
            "gradient" => {
                let Some(g) = fonts.gradient(&v) else { return false };
                self.st.gradient = Some(g);
            }
            "br" => {
                if t.value.is_some() {
                    // `<br=…>`: TMP takes "<br=" as the tag and leaves the rest as text.
                    self.push_char('\n' as u32, ti, 4);
                    self.literal((t.span.0 + 4, t.span.1), ovr);
                } else {
                    self.push_char('\n' as u32, ti, tl);
                }
            }
            "zwsp" => self.push_char(0x200B, ti, tl),
            "nbsp" => self.push_char(0xA0, ti, tl),
            "shy" => self.push_char(0xAD, ti, tl),
            "zwj" => self.push_char(0x200D, ti, tl),
            "link" | "a" => {
                // `<a href="…">` takes its link id from the attribute name; a
                // link tag without any id is consumed but records no link.
                let id = if name == "a" && t.value.is_none() { t.attrs.first().map(|x| x.0.clone()).unwrap_or_default() } else { v.clone() };
                if id.is_empty() {
                    return true;
                }
                let hash = tag_hash(&id);
                self.links.push(LinkInfo { link_id: id, link_text_first_character_index: self.elems.len() as i32, link_text_length: 0, hash_code: hash });
                if name == "a" && t.attrs.iter().any(|(k, _)| k == "href") {
                    // <a href> also styles its text as a link: underlined, #40A0FF.
                    self.save("a");
                    self.st.style |= style::UNDERLINE;
                    self.st.color = [64.0 / 255.0, 160.0 / 255.0, 1.0, 1.0];
                }
                self.open_link = Some(self.links.len() - 1);
                self.st.link = (self.links.len() - 1) as i32;
            }
            "style" => {
                if !has_v || depth > 8 {
                    return false;
                }
                let Some((open, _)) = fonts.style(&v) else { return false };
                self.style_stack.push(v.clone());
                self.expand(&open, (ti, tl), fonts, depth);
            }
            "sprite" => {
                let asset = t.attrs.iter().find(|(k, _)| k == "asset").map(|x| x.1.clone());
                let idx = t.attrs.iter().find(|(k, _)| k == "index").and_then(|x| x.1.parse().ok()).or_else(|| v.parse().ok());
                let nm = t.attrs.iter().find(|(k, _)| k == "name").map(|x| x.1.clone());
                let asset_name = if t.value.is_some() && v.parse::<i32>().is_err() { Some(v.clone()) } else { asset };
                if idx.is_none() && nm.is_none() {
                    return false;
                }
                let Some(mut sp) = fonts.sprite(asset_name.as_deref(), idx, nm.as_deref()) else { return false };
                if let Some((_, tv)) = t.attrs.iter().find(|(k, _)| k == "tint") {
                    sp.tint = tv != "0";
                }
                let mut est = self.st.clone();
                if let Some((_, c)) = t.attrs.iter().find(|(k, _)| k == "color") {
                    if let Some(c) = richtext::parse_color(c) {
                        est.color = c;
                        sp.tint = true;
                    }
                }
                self.elems.push(Elem {
                    cp: if sp.unicode != 0 { sp.unicode } else { 0xFFFE },
                    src: ti,
                    len: tl,
                    style: est,
                    cmd_before: std::mem::take(&mut self.pending),
                    sprite: Some(sp),
                    close_cspace: false,
                });
            }
            "action" => {}
            _ => return false,
        }
        true
    }

    /// Feeds a style definition's markup; its characters take the index and
    /// length of the `<style>` tag that inserted them.
    fn expand<F: FontProvider>(&mut self, markup: &str, at: (usize, usize), fonts: &mut F, depth: usize) {
        let saved = std::mem::replace(&mut self.src, markup.chars().collect());
        for tok in richtext::tokenize_with(markup, true, false) {
            self.feed(tok, Some(at), fonts, depth + 1);
        }
        self.src = saved;
    }
}

const DEFAULT_MARK: [f32; 4] = [1.0, 1.0, 0.0, 64.0 / 255.0];

fn parse<F: FontProvider>(text: &str, s: &Settings, fonts: &mut F, size: f32) -> Parsed {
    let base = base_style(s);
    let mut st = base.clone();
    st.size = size;
    let mut p = ParseState {
        s,
        src: text.chars().collect(),
        base,
        st,
        size,
        unit: if s.world_space { 0.1 } else { 1.0 },
        stacks: HashMap::new(),
        elems: Vec::new(),
        pending: Vec::new(),
        links: Vec::new(),
        open_link: None,
        style_stack: Vec::new(),
    };
    p.base.size = size;
    for tok in richtext::tokenize_with(text, s.rich_text, s.parse_ctrl_characters) {
        p.feed(tok, None, fonts, 0);
    }
    if let Some(li) = p.open_link {
        let first = p.links[li].link_text_first_character_index as usize;
        p.links[li].link_text_length = (p.elems.len() - first) as i32;
    }
    Parsed { elems: p.elems, links: p.links }
}

// ------------------------------------------------------------------ layout

fn is_space(cp: u32) -> bool {
    matches!(cp, 0x20 | 0x09 | 0xA0 | 0x2000..=0x200A | 0x200B | 0x3000 | 0x2007 | 0x202F)
}

fn is_breaking_space(cp: u32) -> bool {
    matches!(cp, 0x20 | 0x09 | 0x2000..=0x2006 | 0x2008..=0x200B | 0x202F | 0x3000)
}

/// White space as TMP counts it (spaceCount): .NET `char.IsWhiteSpace`.
fn is_whitespace(cp: u32) -> bool {
    // (carriage return excepted)
    matches!(cp, 0x09..=0x0C | 0x20 | 0x85 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000)
}

/// Unicode punctuation (general category P*), as far as text layout needs it.
fn is_punctuation(cp: u32) -> bool {
    matches!(cp,
        0x21..=0x23 | 0x25..=0x2A | 0x2C..=0x2F | 0x3A | 0x3B | 0x3F | 0x40 | 0x5B..=0x5D | 0x5F | 0x7B | 0x7D
        | 0xA1 | 0xA7 | 0xAB | 0xB6 | 0xB7 | 0xBB | 0xBF
        | 0x2010..=0x2027 | 0x2030..=0x2043 | 0x2045..=0x2051 | 0x2053..=0x205E
        | 0x3001..=0x3003 | 0x3008..=0x3011 | 0x3014..=0x301F | 0x3030 | 0x303D | 0x30A0 | 0x30FB
        | 0xFF01..=0xFF03 | 0xFF05..=0xFF0A | 0xFF0C..=0xFF0F | 0xFF1A | 0xFF1B | 0xFF1F | 0xFF20 | 0xFF3B..=0xFF3D
        | 0xFF3F | 0xFF5B | 0xFF5D | 0xFF5F..=0xFF65)
}

/// Characters that belong to words (TMP wordInfo): letters, digits, hyphens
/// and the soft hyphen; sprites count as word characters.
fn is_word_char(c: &CharInfo) -> bool {
    if c.element_type == ElementType::Sprite {
        return true;
    }
    matches!(c.character, 0x2D | 0xAD | 0x2010) || char::from_u32(c.character).is_some_and(|ch| ch.is_alphanumeric())
}

fn is_cjk(cp: u32) -> bool {
    matches!(cp, 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

fn apply_case(cp: u32, st: u32) -> (u32, bool) {
    let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
    if st & (style::UPPERCASE) != 0 {
        let u = c.to_uppercase().next().unwrap_or(c);
        return (u as u32, false);
    }
    if st & style::LOWERCASE != 0 {
        let l = c.to_lowercase().next().unwrap_or(c);
        return (l as u32, false);
    }
    if st & style::SMALLCAPS != 0 && c.is_lowercase() {
        let u = c.to_uppercase().next().unwrap_or(c);
        return (u as u32, true);
    }
    (cp, false)
}

/// Shaped element: everything needed for line breaking.
#[derive(Clone, Debug)]
struct Shaped {
    info: CharInfo,
    advance: f32,       // pen advance including spacing
    extent_right: f32,  // ink right edge relative to origin (for fit tests)
    cmds: Vec<Cmd>,
    halign: Option<HAlign>,
    indent: f32,
    line_indent: f32,
    margin_l: f32,
    margin_r: f32,
    line_height: Option<f32>,
    width_limit: Option<f32>,
    face: FaceMetrics,
    scale: f32,
    word_spacing_extra: f32,
    /// Character spacing included in `advance` (character spacing + cspace),
    /// excluded from fit tests and from a line's last character.
    char_spacing: f32,
    /// Bold spacing included in `advance` (excluded from maxAdvance).
    style_spacing: f32,
    /// Glyph origin offset from the pen position (monospace centring).
    origin_offset: f32,
    /// Advance of the hyphen a soft hyphen shows when a line breaks there.
    shy_width: f32,
    /// Horizontal squeeze (auto-size character width adjustment).
    xk: f32,
}

fn shape<F: FontProvider>(p: &Parsed, s: &Settings, fonts: &mut F, text_size: f32) -> Vec<Shaped> {
    let unit = if s.world_space { 0.1 } else { 1.0 };
    let mut out: Vec<Shaped> = Vec::with_capacity(p.elems.len());
    let mut prev: Option<(u32, u32)> = None; // (font, glyph)
    for e in &p.elems {
        let st = &e.style;
        let (cp, small) = apply_case(e.cp, st.style);
        let mut size = st.size * st.sup_sub_scale;
        if small {
            size *= 0.8;
        }
        // Small caps keep the full-size element ascender / descender.
        let metric_size = if small { size / 0.8 } else { size };
        let mut info = CharInfo {
            // TMP reports the case-converted character and the tag size
            // (not reduced by <sup>/<sub> or small caps).
            character: cp,
            index: e.src as i32,
            string_length: e.len as i32,
            point_size: st.size,
            style: st.style,
            color: {
                let mut c = st.color;
                if let Some(a) = st.alpha {
                    c[3] = a;
                }
                c
            },
            underline_color: st.underline_color.unwrap_or(st.color),
            strikethrough_color: st.strike_color.unwrap_or(st.color),
            highlight_color: st.mark.unwrap_or([0.0; 4]),
            link: st.link,
            gradient: st.gradient,
            rotation: st.rotate,
            material: st.material,
            no_break: st.nobr,
            voffset: st.voffset + st.sup_sub_offset,
            ..Default::default()
        };
        // Line feed, vertical tab and the Unicode line / paragraph separators end lines.
        let is_nl = matches!(cp, 0x0A | 0x0B | 0x2028 | 0x2029);
        let face0 = fonts.face(st.font);
        let mut advance;
        let mut extent_right = 0.0;
        let mut style_spacing = 0.0;
        let mut origin_offset = 0.0;
        let mut scale;
        let mut face = face0;
        // Glyph lookup (with the provider's fallback chain). A character found
        // nowhere becomes a sprite with that unicode if one exists (emoji), or
        // else the missing-glyph character U+25A1 (then '?'), which TMP then
        // reports as the element's character.
        let invisible_ctrl = is_nl || matches!(cp, 0x200B | 0xAD | 0x2060 | 0x200C | 0x200D);
        let mut found = if e.sprite.is_some() || invisible_ctrl { None } else { fonts.glyph(st.font, cp) };
        let mut esprite = e.sprite;
        if e.sprite.is_none() && !invisible_ctrl && found.is_none() && !is_space(cp) && cp >= 0x20 {
            if let Some(sp) = fonts.sprite_by_unicode(cp) {
                esprite = Some(sp);
            } else if let Some(x) = fonts.glyph(st.font, 0x25A1) {
                found = Some(x);
                info.character = 0x25A1;
            } else if let Some(x) = fonts.glyph(st.font, '?' as u32) {
                found = Some(x);
                info.character = '?' as u32;
            }
        }
        if let Some(sp) = esprite {
            info.element_type = ElementType::Sprite;
            info.sprite = Some(sp);
            // Sprites scale to the current font's ascent line (TMP behaviour: sprite
            // height relative to the font).
            let fs = size / face0.point_size.max(1.0) * unit;
            let sprite_scale = if sp.point_size > 0.0 { face0.ascent / sp.ascent.max(1e-3) * fs * sp.scale } else { fs * sp.scale };
            scale = sprite_scale;
            info.scale = sprite_scale;
            info.glyph_metrics = [sp.width, sp.height, sp.bearing_x, sp.bearing_y, sp.advance];
            info.uv_rect = sp.rect;
            info.atlas_size = sp.atlas_size;
            info.font = sp.asset;
            info.glyph = sp.index;
            info.elem_ascender = face0.ascent * fs;
            info.elem_descender = face0.descent * fs;
            info.is_visible = true;
            advance = sp.advance * sprite_scale;
            extent_right = (sp.bearing_x + sp.width) * sprite_scale;
            prev = None;
        } else {
            let (font_used, g) = found.unwrap_or((st.font, GlyphInfo::default()));
            face = fonts.face(font_used);
            scale = size / face.point_size.max(1.0) * unit;
            // Elements from a fallback font are scaled so the size matches the
            // primary font's point size convention (same N units).
            info.font = font_used;
            info.glyph = g.index;
            info.scale = scale;
            info.glyph_metrics = [g.width, g.height, g.bearing_x, g.bearing_y, g.advance];
            info.uv_rect = g.rect;
            info.atlas_index = g.atlas_index;
            info.atlas_size = g.atlas_size;
            let metric_scale = metric_size / face.point_size.max(1.0) * unit;
            info.elem_ascender = face.ascent * metric_scale;
            info.elem_descender = face.descent * metric_scale;
            let mut adv = g.advance;
            if st.style & style::BOLD != 0 {
                // Bold spacing scales with the text's size, not the tag's.
                let base_scale = text_size / face.point_size.max(1.0) * unit;
                style_spacing = face.bold_spacing * face.point_size / 100.0 * base_scale;
                adv += style_spacing / scale.max(1e-6);
            } else {
                adv += face.normal_spacing_offset * face.point_size / 100.0;
            }
            if cp == '\t' as u32 {
                adv = 0.0; // handled at line level
            }
            if is_nl || cp == 0x200B || cp == 0xAD {
                adv = 0.0;
            }
            advance = adv * scale;
            if let Some(m) = st.mspace {
                // Monospace: the pen moves by the cell width and the glyph is
                // centred in its cell.
                origin_offset = m * 0.5 - (g.bearing_x + g.width * 0.5) * scale;
                advance = m;
            }
            extent_right = (g.bearing_x + g.width) * scale;
            info.is_visible = !is_space(cp) && !is_nl && g.width > 0.0 && cp != 0x200B && cp != 0xAD;
            // Kerning (pair adjustment) with the previous glyph in the same font.
            if s.kerning {
                if let Some((pf, pg)) = prev {
                    if pf == font_used {
                        if let Some(k) = fonts.kerning(font_used, pg, g.index) {
                            // First glyph's advance adjustment applies to the previous element.
                            if let Some(last) = out.last_mut() {
                                last.advance += k[2] * last.scale;
                            }
                            // Second glyph placement shifts this glyph.
                            info.voffset += k[5] * scale;
                            advance += k[6] * scale;
                        }
                    }
                }
            }
            prev = if is_nl { None } else { Some((font_used, g.index)) };
        }
        let em = size * unit / 100.0;
        // Character spacing applies to every element; word spacing to spaces.
        let mut spacing = (s.character_spacing) * em + st.cspace;
        if e.close_cspace {
            spacing -= st.cspace;
            info.xadv_from_line_left = true;
        }
        let char_spacing = if is_nl { 0.0 } else { spacing };
        let mut word_extra = 0.0;
        if is_space(cp) && cp != 0xA0 {
            word_extra = s.word_spacing * em;
            spacing += word_extra;
        }
        if is_nl {
            spacing = 0.0;
        }
        advance += spacing;
        if let Some(wl) = st.width_scale {
            let _ = wl;
        }
        let shy_width = if cp == 0xAD { fonts.glyph(st.font, '-' as u32).map(|(_, g)| g.advance * scale).unwrap_or(0.0) } else { 0.0 };
        info.is_space = is_space(cp);
        info.is_newline = is_nl;
        info.break_after = is_breaking_space(cp) || cp == '-' as u32 || cp == 0x2010 || cp == 0xAD || cp == 0x200B;
        out.push(Shaped {
            info,
            advance,
            extent_right,
            cmds: e.cmd_before.clone(),
            halign: st.halign,
            indent: st.indent,
            line_indent: st.line_indent,
            margin_l: st.margin_left,
            margin_r: st.margin_right,
            line_height: st.line_height,
            width_limit: st.width_scale,
            face,
            scale,
            word_spacing_extra: word_extra,
            char_spacing,
            style_spacing,
            origin_offset,
            shy_width,
            xk: 1.0,
        });
        let _ = &mut scale;
    }
    // CJK: a line may break between two characters when either is CJK, unless
    // the first may not end a line (leading set: opening brackets…) or the
    // second may not start one (following set: closing punctuation, small kana…).
    let (leading, following) = fonts.line_breaking_rules().unwrap_or_else(|| (DEFAULT_LEADING.to_string(), DEFAULT_FOLLOWING.to_string()));
    let lead: Vec<u32> = leading.chars().map(|c| c as u32).collect();
    let follow: Vec<u32> = following.chars().map(|c| c as u32).collect();
    for i in 0..out.len().saturating_sub(1) {
        let (a, b) = (out[i].info.character, out[i + 1].info.character);
        if (is_cjk(a) || is_cjk(b)) && !lead.contains(&a) && !follow.contains(&b) {
            out[i].info.break_after = true;
        }
    }
    out
}

/// Default kinsoku sets (JIS X 4051 classes): characters that may not end a
/// line (opening brackets and quotes) …
pub const DEFAULT_LEADING: &str = "([{\u{FF5B}\u{3014}\u{3008}\u{300A}\u{300C}\u{300E}\u{3010}\u{3018}\u{3016}\u{301D}\u{2018}\u{201C}\u{FF5F}\u{AB}\u{FF3B}\u{FF08}";
/// … and characters that may not start one (closing brackets, punctuation,
/// small kana, iteration and prolonged-sound marks).
pub const DEFAULT_FOLLOWING: &str = ")]}\u{FF5D}\u{3015}\u{3009}\u{300B}\u{300D}\u{300F}\u{3011}\u{3019}\u{3017}\u{301F}\u{2019}\u{201D}\u{FF60}\u{BB}\u{30FD}\u{30FE}\u{30FC}\u{30A1}\u{30A3}\u{30A5}\u{30A7}\u{30A9}\u{30C3}\u{30E3}\u{30E5}\u{30E7}\u{30EE}\u{30F5}\u{30F6}\u{3041}\u{3043}\u{3045}\u{3047}\u{3049}\u{3063}\u{3083}\u{3085}\u{3087}\u{308E}\u{3095}\u{3096}\u{3005}\u{303B}\u{2010}\u{30A0}\u{2013}\u{301C}?!\u{203C}\u{2047}\u{2048}\u{2049}\u{30FB}\u{3001}%,.:;\u{3002}\u{FF01}\u{FF1F}\u{FF3D}\u{FF09}\u{FF1A}\u{FF1B}\u{FF1D}\u{FF0C}\u{FF0E}";

struct LineSpan {
    start: usize,
    end: usize, // exclusive
    ends_paragraph: bool,
    page_break_after: bool,
}

/// Greedy line breaking. Returns spans plus each element's x origin relative
/// to the line start (including indents, tab stops and <pos>).
fn break_lines(sh: &mut [Shaped], s: &Settings, avail: f32) -> (Vec<LineSpan>, Vec<f32>) {
    let mut lines = Vec::new();
    let mut xs = vec![0.0f32; sh.len()];
    let wrap = matches!(s.wrap, Wrap::Normal | Wrap::PreserveWhitespace);
    let mut start = 0usize;
    let mut first_line_of_para = true;
    while start < sh.len() {
        let mut x = if first_line_of_para { sh[start].line_indent } else { 0.0 } + sh[start].indent;
        let line_x0 = x;
        let mut last_break: Option<usize> = None; // index after which we may break
        let mut i = start;
        let mut end = sh.len();
        let mut ends_para = false;
        let mut page_after = false;
        let tab_w = |sh: &Shaped| sh.face.tab_width * sh.face.tab_size.max(1.0) * sh.scale;
        while i < sh.len() {
            for c in &sh[i].cmds {
                match c {
                    Cmd::Pos(p) => x = *p,
                    Cmd::Space(d) => x += *d,
                    Cmd::PageBreak => {}
                }
            }
            if i > start && sh[i].cmds.iter().any(|c| matches!(c, Cmd::PageBreak)) {
                end = i;
                page_after = true;
                break;
            }
            let e = &sh[i];
            if e.info.character == '\t' as u32 {
                let tw = tab_w(e).max(1e-3);
                let nx = ((x / tw).floor() + 1.0) * tw;
                sh[i].advance = nx - x;
            }
            let e = &sh[i];
            let width_here = e.width_limit.unwrap_or(avail).min(avail) - e.margin_l - e.margin_r;
            // Justified and flush lines may run 5% over before wrapping (they
            // are then compressed).
            let tolerance = if matches!(e.halign.unwrap_or(s.halign), HAlign::Justified | HAlign::Flush) { 1.05 } else { 1.0 };
            let fits = x + (e.advance - e.char_spacing) <= width_here * tolerance + 1e-3 || !e.info.is_visible;
            if wrap && !fits && i > start && !e.info.is_newline {
                // Break at the last opportunity, or mid-word if there is none.
                let at = match last_break {
                    Some(b) if b + 1 > start => b + 1,
                    _ => i,
                };
                end = at;
                break;
            }
            xs[i] = x;
            x += e.advance;
            if e.info.character == 0x0D {
                // Carriage return: back to the line's start, same line.
                x = line_x0;
            }
            if e.info.is_newline {
                end = i + 1;
                ends_para = true;
                break;
            }
            if e.info.break_after && !e.info.no_break {
                // A soft hyphen is a break opportunity only if its hyphen fits.
                if e.info.character != 0xAD || x + e.shy_width <= width_here * tolerance + 1e-3 {
                    last_break = Some(i);
                }
            }
            i += 1;
        }
        if end <= start {
            end = start + 1;
        }
        lines.push(LineSpan { start, end, ends_paragraph: ends_para, page_break_after: page_after });
        first_line_of_para = ends_para;
        start = end;
    }
    if lines.is_empty() {
        lines.push(LineSpan { start: 0, end: 0, ends_paragraph: false, page_break_after: false });
    }
    // Recompute xs per line from scratch so a line that starts after a break
    // begins at its indent.
    for (li, ln) in lines.iter().enumerate() {
        let para_start = li == 0 || lines[li - 1].ends_paragraph;
        let mut x = if ln.start < sh.len() { (if para_start { sh[ln.start].line_indent } else { 0.0 }) + sh[ln.start].indent } else { 0.0 };
        let line_x0 = x;
        for i in ln.start..ln.end {
            for c in &sh[i].cmds {
                match c {
                    Cmd::Pos(p) => x = *p,
                    Cmd::Space(d) => x += *d,
                    Cmd::PageBreak => {}
                }
            }
            if sh[i].info.character == '\t' as u32 {
                let e = &sh[i];
                let tw = e.face.tab_width * e.face.tab_size.max(1.0) * e.scale;
                let tw = tw.max(1e-3);
                let nx = ((x / tw).floor() + 1.0) * tw;
                sh[i].advance = nx - x;
            }
            xs[i] = x;
            x += sh[i].advance;
            if sh[i].info.character == 0x0D {
                x = line_x0;
            }
        }
    }
    (lines, xs)
}

fn trailing_visible_end(sh: &[Shaped], ln: &LineSpan, xs: &[f32]) -> f32 {
    let mut j = ln.end;
    while j > ln.start {
        let e = &sh[j - 1];
        if !(e.info.is_space || e.info.is_newline) {
            return xs[j - 1] + e.advance - e.char_spacing;
        }
        j -= 1;
    }
    if ln.end > ln.start { xs[ln.start] } else { 0.0 }
}

pub fn layout<F: FontProvider>(text: &str, s: &Settings, fonts: &mut F) -> Layout {
    if !s.auto_size {
        return layout_at(text, s, fonts, s.font_size);
    }
    // Auto size: largest size in [min, max] whose layout fits the rect.
    // Preferred values are reported at the maximum size.
    let mut best = auto_size_search(text, s, fonts);
    let hi = s.font_size_max.max(s.font_size_min);
    let mut at_hi = layout_at(text, s, fonts, hi);
    if at_hi.is_text_overflowing || at_hi.h_overflow {
        // When squeezing was needed, the preferred height includes the line
        // spacing adjustment (the width does not include the character squeeze).
        at_hi = layout_adj(text, s, fonts, hi, Adjust { width: 1.0, line_spacing: s.line_spacing_max_adj.min(0.0) });
    }
    best.preferred_width = at_hi.preferred_width;
    best.preferred_height = at_hi.preferred_height;
    best
}

fn auto_size_search<F: FontProvider>(text: &str, s: &Settings, fonts: &mut F) -> Layout {
    let fits = |l: &Layout| !l.is_text_overflowing && !l.h_overflow;
    let (lo, hi) = (s.font_size_min.min(s.font_size_max), s.font_size_max.max(s.font_size_min));
    let at_hi = layout_at(text, s, fonts, hi);
    if fits(&at_hi) {
        return at_hi;
    }
    // Text that does not fit at the maximum size is squeezed by the full
    // character-width and line-spacing adjustments while the size is searched.
    let adj = Adjust { width: 1.0 - s.char_width_max_adj.clamp(0.0, 100.0) / 100.0, line_spacing: s.line_spacing_max_adj.min(0.0) };
    let mut a = lo;
    let mut b = hi;
    let mut best = layout_adj(text, s, fonts, lo, adj);
    if !fits(&best) {
        return best;
    }
    for _ in 0..20 {
        if b - a < 0.05 {
            break;
        }
        let mid = ((a + b) * 0.5 * 20.0).round() / 20.0;
        let l = layout_adj(text, s, fonts, mid, adj);
        if fits(&l) {
            a = mid;
            best = l;
        } else {
            b = mid;
        }
    }
    best
}

/// TMP reports preferred sizes rounded up to hundredths.
fn ceil_hundredth(v: f32) -> f32 {
    ((v as f64 * 100.0 - 1e-3).ceil() / 100.0) as f32
}

fn overflows_height(s: &Settings) -> bool {
    !matches!(s.overflow, Overflow::Overflow | Overflow::ScrollRect)
}

/// Auto-size squeezing: horizontal scale of glyphs and advances, and line
/// spacing added (in sampling-size pixels, scaled like glyphs).
#[derive(Clone, Copy, Debug)]
struct Adjust {
    width: f32,
    line_spacing: f32,
}

const NO_ADJUST: Adjust = Adjust { width: 1.0, line_spacing: 0.0 };

pub fn layout_at<F: FontProvider>(text: &str, s: &Settings, fonts: &mut F, size: f32) -> Layout {
    layout_adj(text, s, fonts, size, NO_ADJUST)
}

fn layout_adj<F: FontProvider>(text: &str, s: &Settings, fonts: &mut F, size: f32, adj: Adjust) -> Layout {
    let unit = if s.world_space { 0.1 } else { 1.0 };
    let parsed = parse(text, s, fonts, size);
    let mut sh = shape(&parsed, s, fonts, size);
    if adj.width != 1.0 {
        for e in sh.iter_mut() {
            e.advance *= adj.width;
            e.origin_offset *= adj.width;
            e.xk = adj.width;
        }
    }
    // Characters before firstVisibleCharacter take no part in layout: TMP lays
    // out the rest from the top and reports these as empty elements.
    let skipped = (s.first_visible_character.max(0) as usize).min(sh.len());
    let skipped_elems: Vec<Shaped> = sh.drain(..skipped).collect();
    let [rx, ry, rw, rh] = s.rect;
    let (ml, mt, mr, mb) = (s.margin[0], s.margin[1], s.margin[2], s.margin[3]);
    let avail_w = (rw - ml - mr).max(0.0);
    let avail_h = (rh - mt - mb).max(0.0);
    let top = ry + rh - mt;
    let bottom = ry + mb;
    let left = rx + ml;

    let (mut lines, mut xs) = break_lines(&mut sh, s, avail_w);
    // A soft hyphen where a line breaks shows as a hyphen; elsewhere it is an
    // empty element sitting on the baseline.
    for li in 0..lines.len() {
        let ln = &lines[li];
        for i in ln.start..ln.end {
            if sh[i].info.character != 0xAD {
                continue;
            }
            let at_break = i + 1 == ln.end && li + 1 < lines.len() && !ln.ends_paragraph;
            if at_break {
                if let Some((fu, g)) = fonts.glyph(sh[i].info.font, '-' as u32) {
                    let e = &mut sh[i];
                    let sc = e.scale;
                    e.info.font = fu;
                    e.info.glyph = g.index;
                    e.info.glyph_metrics = [g.width, g.height, g.bearing_x, g.bearing_y, g.advance];
                    e.info.uv_rect = g.rect;
                    e.info.atlas_index = g.atlas_index;
                    e.info.atlas_size = g.atlas_size;
                    e.info.is_visible = true;
                    e.advance = g.advance * sc;
                }
            } else {
                sh[i].info.baseline_metrics = true;
            }
        }
    }

    // Vertical metrics per line.
    struct VLine {
        asc: f32,
        desc: f32,
        gap: f32,
        baseline: f32,
    }
    let base_face = fonts.face(s.font);
    let base_scale = size / base_face.point_size.max(1.0) * unit;
    let line_metrics = |sh: &[Shaped], ln: &LineSpan| -> (f32, f32, f32, Option<f32>) {
        let mut asc = f32::NEG_INFINITY;
        let mut desc = f32::INFINITY;
        let mut gap = 0.0f32;
        let mut lh = None;
        for e in &sh[ln.start..ln.end] {
            asc = asc.max(e.info.elem_ascender + e.info.voffset);
            desc = desc.min(e.info.elem_descender + e.info.voffset);
            gap = gap.max((e.face.line_height - (e.face.ascent - e.face.descent)) * e.scale);
            if e.line_height.is_some() {
                lh = e.line_height;
            }
        }
        if ln.start >= ln.end || !asc.is_finite() {
            // Empty line: use the style at that point (or the base font).
            let e = sh.get(ln.start.min(sh.len().saturating_sub(1)));
            let (a, d, sc, f) = match e {
                Some(e) => (e.info.elem_ascender, e.info.elem_descender, e.scale, e.face),
                None => (base_face.ascent * base_scale, base_face.descent * base_scale, base_scale, base_face),
            };
            asc = a;
            desc = d;
            gap = (f.line_height - (f.ascent - f.descent)) * sc;
        }
        (asc, desc, gap, lh)
    };
    let mut vl: Vec<VLine> = Vec::with_capacity(lines.len());
    let em = size * unit / 100.0;
    for (i, ln) in lines.iter().enumerate() {
        let (asc, desc, gap, lh) = line_metrics(&sh, ln);
        let baseline = if i == 0 {
            top - asc
        } else {
            let p = &vl[i - 1];
            let prev_ln = &lines[i - 1];
            // <line-height> in effect at the end of the previous line sets the pitch.
            let lh = if prev_ln.end > prev_ln.start { sh[prev_ln.end - 1].line_height } else { lh };
            let mut pitch = match lh {
                Some(h) => h,
                None => -p.desc + asc + gap.max(p.gap),
            };
            pitch += s.line_spacing * em + adj.line_spacing * base_scale;
            if prev_ln.ends_paragraph {
                pitch += s.paragraph_spacing * em;
            }
            p.baseline - pitch
        };
        vl.push(VLine { asc, desc, gap, baseline });
    }

    // Preferred values come from the complete text, before overflow handling.
    let mut pref_h_full = match (vl.first(), vl.last()) {
        (Some(f), Some(l)) => (f.baseline + f.asc) - (l.baseline + l.desc) + mt + mb,
        _ => 0.0,
    };
    if lines.iter().any(|l| l.page_break_after) {
        // Explicit <page> breaks restart at the top: the preferred height is
        // the tallest run of lines between them.
        let mut best = 0.0f32;
        let mut i = 0;
        while i < vl.len() {
            let mut j = i;
            while j + 1 < vl.len() && !lines[j].page_break_after {
                j += 1;
            }
            best = best.max((vl[i].baseline + vl[i].asc) - (vl[j].baseline + vl[j].desc));
            i = j + 1;
        }
        pref_h_full = best + mt + mb;
    }
    // Preferred width: the widest line when only line feeds break lines
    // (indents, <pos>, <space>, tabs and tag margins included).
    let pref_w_full = {
        let mut ush = sh.clone();
        for e in ush.iter_mut().filter(|e| e.info.character == 0xAD) {
            // Soft hyphens only show where a line breaks.
            e.advance = 0.0;
            e.info.is_visible = false;
            e.info.is_space = true;
        }
        let nowrap = Settings { wrap: Wrap::NoWrap, ..s.clone() };
        let (ul, uxs) = break_lines(&mut ush, &nowrap, f32::INFINITY);
        let mut best = 0.0f32;
        for ln in &ul {
            let mut end = 0.0f32;
            for i in (ln.start..ln.end).rev() {
                let e = &ush[i];
                if !(e.info.is_space || e.info.is_newline) {
                    end = if e.origin_offset != 0.0 {
                        // Monospaced: the glyph's own advance from its centred origin.
                        uxs[i] + e.origin_offset + e.info.glyph_metrics[4] * e.scale
                    } else {
                        uxs[i] + e.advance - e.char_spacing - e.style_spacing
                    };
                    break;
                }
            }
            let (lm, rm) = ush.get(ln.start).map(|e| (e.margin_l, e.margin_r)).unwrap_or((0.0, 0.0));
            best = best.max(end + lm + rm);
        }
        best + ml + mr
    };

    // Pages and vertical overflow.
    let mut is_overflowing = false;
    let mut first_overflow = -1i32;
    let mut page_of_line = vec![0i32; lines.len()];
    let mut keep_lines = lines.len();
    let mut ellipsis_line: Option<usize> = None;
    if !lines.is_empty() {
        let mut page = 0i32;
        let mut page_top_baseline_shift = 0.0f32;
        for i in 0..lines.len() {
            let desc_y = vl[i].baseline + page_top_baseline_shift + vl[i].desc;
            let explicit_break = i > 0 && lines[i - 1].page_break_after;
            if i == 0 && desc_y < bottom - 1e-3 {
                is_overflowing = true;
                if first_overflow < 0 {
                    first_overflow = sh.get(lines[0].start).map(|e| e.info.index).unwrap_or(-1);
                }
            }
            let exceeds = desc_y < bottom - 1e-3 && i > 0;
            if s.overflow == Overflow::Page && (exceeds || explicit_break) {
                page += 1;
                page_top_baseline_shift = (top - vl[i].asc) - vl[i].baseline;
                if exceeds {
                    is_overflowing = true;
                    if first_overflow < 0 {
                        first_overflow = sh.get(lines[i].start).map(|e| e.info.index).unwrap_or(-1);
                    }
                }
            } else if exceeds && overflows_height(s) && s.overflow != Overflow::Page {
                is_overflowing = true;
                if first_overflow < 0 {
                    first_overflow = sh.get(lines[i].start).map(|e| e.info.index).unwrap_or(-1);
                }
                if matches!(s.overflow, Overflow::Ellipsis | Overflow::Truncate | Overflow::Linked) {
                    keep_lines = i;
                    if s.overflow == Overflow::Ellipsis {
                        ellipsis_line = Some(i - 1);
                    }
                    break;
                }
            } else if exceeds {
                is_overflowing = true;
                if first_overflow < 0 {
                    first_overflow = sh.get(lines[i].start).map(|e| e.info.index).unwrap_or(-1);
                }
            }
            page_of_line[i] = page;
            vl[i].baseline += page_top_baseline_shift;
        }
    }
    // Horizontal overflow of non-wrapping lines: the first element whose pen
    // position passes the right edge.
    let nowrap = !matches!(s.wrap, Wrap::Normal | Wrap::PreserveWhitespace);
    let mut h_overflow: Option<usize> = None;
    if nowrap {
        'outer: for ln in &lines {
            for i in ln.start..ln.end {
                let e = &sh[i];
                if e.info.is_visible && xs[i] + e.advance - e.char_spacing > avail_w - e.margin_r - e.margin_l + 1e-3 {
                    h_overflow = Some(i);
                    break 'outer;
                }
            }
        }
        // (Auto-size treats a too-long unwrapped line as not fitting, but TMP
        // does not report it as overflowing.)
    }
    // Truncate / Ellipsis: processing stops at the overflowing element `o`, which
    // TMP keeps in the character list as an invisible zero-width element.
    let mut cut_at = sh.len();
    let lines_before = lines.len();
    let mut ellipsis_vertical = false;
    let stop: Option<(usize, usize)> = if keep_lines < lines.len() && keep_lines > 0 {
        Some((keep_lines - 1, lines[keep_lines].start))
    } else if matches!(s.overflow, Overflow::Ellipsis | Overflow::Truncate | Overflow::Linked) {
        h_overflow.map(|o| (lines.iter().position(|l| o >= l.start && o < l.end).unwrap_or(0), o))
    } else {
        None
    };
    let _ = ellipsis_line;
    if let Some((li, o)) = stop {
        let vertical = keep_lines < lines.len();
        lines.truncate(li + 1);
        vl.truncate(li + 1);
        let ln_start = lines[li].start;
        let line_end_x = |sh: &[Shaped], xs: &[f32], upto: usize| -> f32 {
            if upto > ln_start { xs[upto - 1] + sh[upto - 1].advance } else { 0.0 }
        };
        if s.overflow == Overflow::Ellipsis {
            if let Some((fu, eg)) = fonts.glyph(s.font, 0x2026) {
                let face = fonts.face(fu);
                let st_size = sh.get(o.saturating_sub(1)).map(|e| e.info.point_size).unwrap_or(size);
                let sc = st_size / face.point_size.max(1.0) * unit;
                let e_adv = eg.advance * sc;
                let mut k = o;
                while k > ln_start && xs[k - 1] + e_adv > avail_w + 1e-3 {
                    k -= 1;
                }
                let k = k.saturating_sub(1).max(ln_start);
                let kx = xs[k];
                {
                    let e = &mut sh[k];
                    e.info.font = fu;
                    e.info.glyph = eg.index;
                    e.info.scale = sc;
                    e.info.glyph_metrics = [eg.width, eg.height, eg.bearing_x, eg.bearing_y, eg.advance];
                    e.info.uv_rect = eg.rect;
                    e.info.atlas_index = eg.atlas_index;
                    e.info.atlas_size = eg.atlas_size;
                    e.info.is_visible = true;
                    e.info.is_space = false;
                    e.info.no_word = true;
                    e.advance = e_adv;
                    e.char_spacing = 0.0;
                    e.scale = sc;
                }
                cut_at = (k + 2).min(sh.len());
                if k + 1 < sh.len() {
                    xs[k + 1] = kx + e_adv;
                    let z = &mut sh[k + 1];
                    z.advance = 0.0;
                    z.char_spacing = 0.0;
                    z.info.is_visible = false;
                    z.info.no_word = true;
                    z.info.ellipsis_tail = true;
                    ellipsis_vertical = vertical;
                }
                lines[li].end = cut_at;
            }
        } else {
            // Truncate.
            let end_x = line_end_x(&sh, &xs, o.min(lines[li].end.max(o)));
            cut_at = (o + 1).min(sh.len());
            if o < sh.len() {
                xs[o] = if vertical { line_end_x(&sh, &xs, o) } else { xs[o] };
                let z = &mut sh[o];
                z.advance = 0.0;
                z.char_spacing = 0.0;
                z.info.is_visible = false;
                z.info.no_word = true;
            }
            let _ = end_x;
            lines[li].end = cut_at;
        }
    } else if keep_lines < lines.len() {
        lines.truncate(keep_lines.max(1));
        vl.truncate(keep_lines.max(1));
        cut_at = lines.last().map(|l| l.end).unwrap_or(0);
    }
    let truncated = cut_at < sh.len() || lines.len() < lines_before;
    sh.truncate(cut_at.min(sh.len()));
    xs.truncate(sh.len());

    // Vertical alignment of the block.
    let n = lines.len();
    let page_display = (s.page_to_display - 1).max(0);
    let last_page = page_of_line.get(n.saturating_sub(1)).copied().unwrap_or(0);
    let block_lines: Vec<usize> = if s.overflow == Overflow::Page {
        (0..n).filter(|&i| page_of_line[i] == page_display.min(last_page)).collect()
    } else {
        // With maxVisibleLines the block ends at the descender TMP records at
        // the first hidden line.
        (0..n.min(s.max_visible_lines.max(0) as usize + 1)).collect()
    };
    let (blk_top, blk_bot) = if block_lines.is_empty() {
        (top, top)
    } else {
        let f = block_lines[0];
        let l = *block_lines.last().unwrap();
        (vl[f].baseline + vl[f].asc, vl[l].baseline + vl[l].desc)
    };
    let pad_q = s.padding * if s.extra_padding { 1.5 } else { 1.0 };
    let center = top - avail_h * 0.5;
    let dy = match s.valign {
        VAlign::Top => 0.0,
        VAlign::Middle => {
            let h = blk_top - blk_bot;
            (top - (avail_h - h) * 0.5) - blk_top
        }
        VAlign::Bottom => bottom - blk_bot,
        VAlign::Baseline => {
            let f = block_lines.first().copied().unwrap_or(0);
            if n > 0 { center - vl[f].baseline } else { 0.0 }
        }
        VAlign::Capline => {
            let f = block_lines.first().copied().unwrap_or(0);
            if n > 0 {
                let cap = base_face.cap_line * base_scale;
                center - (vl[f].baseline + cap * 0.5)
            } else {
                0.0
            }
        }
        VAlign::Geometry => {
            // Centre the glyph quads' vertical extent.
            let (mut qt, mut qb) = (f32::NEG_INFINITY, f32::INFINITY);
            for &li in &block_lines {
                for e in &sh[lines[li].start..lines[li].end.min(sh.len())] {
                    if e.info.is_visible {
                        let [_, gh, _, by, _] = e.info.glyph_metrics;
                        let b = vl[li].baseline + e.info.voffset;
                        qt = qt.max(b + (by + pad_q) * e.scale);
                        qb = qb.min(b + (by - gh - pad_q) * e.scale);
                    }
                }
            }
            if qt.is_finite() { center - (qt + qb) * 0.5 } else { 0.0 }
        }
    };
    for v in vl.iter_mut() {
        v.baseline += dy;
    }

    // Build character infos.
    let mut out = Layout {
        font_size: size,
        first_overflow_character_index: first_overflow,
        is_text_overflowing: is_overflowing,
        overflow_mode_page: s.overflow == Overflow::Page,
        h_overflow: h_overflow.is_some(),
        ..Default::default()
    };
    let mut chars: Vec<CharInfo> = Vec::with_capacity(sh.len() + skipped);
    for e in &skipped_elems {
        chars.push(CharInfo { character: e.info.character, index: e.info.index, string_length: e.info.string_length, ..Default::default() });
    }
    let mut rendered_min = V2::new(f32::INFINITY, f32::INFINITY);
    let mut rendered_max = V2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    let pad = s.padding * if s.extra_padding { 1.5 } else { 1.0 };
    let max_vis_lines = s.max_visible_lines.max(0) as usize;
    let mut word_count_total = 0;
    let mut max_line_width = 0.0f32;
    let mut shift_first = vec![0.0f32; lines.len()];
    let mut shift_last = vec![0.0f32; lines.len()];
    for (li, ln) in lines.iter().enumerate() {
        let v = &vl[li];
        // Line-level properties (alignment, margins, width) follow the style in
        // effect at the line's last visible character.
        let sref = (ln.start..ln.end).rev().find(|&i| sh[i].info.is_visible).or(if ln.end > ln.start { Some(ln.end - 1) } else { None });
        let se = sref.map(|i| &sh[i]);
        let halign = se.and_then(|e| e.halign).unwrap_or(s.halign);
        let ml_ = se.map(|e| e.margin_l).unwrap_or(0.0);
        let mr_ = se.map(|e| e.margin_r).unwrap_or(0.0);
        let line_w = se.and_then(|e| e.width_limit).unwrap_or(avail_w).min(avail_w) - ml_ - mr_;
        let content_end = trailing_visible_end(&sh, ln, &xs);
        let content_start = if ln.end > ln.start { xs[ln.start] } else { 0.0 };
        let used = content_end;
        let is_last_of_para = ln.ends_paragraph || li + 1 == lines.len();
        let mut justify_extra_space = 0.0;
        let mut justify_extra_char = 0.0;
        let mut dx = match halign {
            HAlign::Left => 0.0,
            HAlign::Center => (line_w - used) * 0.5,
            HAlign::Right => line_w - used,
            HAlign::Justified | HAlign::Flush => {
                // A paragraph's last line is not justified unless it overran
                // the width (within the wrap tolerance): then it is compressed.
                if halign == HAlign::Justified && is_last_of_para && used <= line_w {
                    0.0
                } else {
                    // Extra width is split by word_wrapping_ratios: a share shifts every
                    // visible character after the first, the rest shifts interior spaces.
                    let last_vis = (ln.start..ln.end).rev().find(|&i| !sh[i].info.is_space && !sh[i].info.is_newline);
                    let spaces = match last_vis {
                        Some(lv) => (ln.start..lv).filter(|&i| sh[i].info.is_space).count(),
                        None => 0,
                    };
                    let vis = (ln.start..ln.end).filter(|&i| !sh[i].info.is_space && !sh[i].info.is_newline).count();
                    // Negative when a line ran into the 5% wrap tolerance.
                    let extra = line_w - used;
                    let r = s.word_wrapping_ratios.clamp(0.0, 1.0);
                    if spaces > 0 && vis > 1 {
                        justify_extra_char = extra * r / (vis - 1) as f32;
                        justify_extra_space = extra * (1.0 - r) / spaces as f32;
                    } else if spaces > 0 {
                        justify_extra_space = extra / spaces as f32;
                    } else if vis > 1 {
                        justify_extra_char = extra / (vis - 1) as f32;
                    }
                    0.0
                }
            }
            HAlign::Geometry => {
                // Centre the ink extents rather than the advances.
                let mut a = f32::INFINITY;
                let mut b = f32::NEG_INFINITY;
                for i in ln.start..ln.end {
                    let e = &sh[i];
                    if e.info.is_visible {
                        a = a.min(xs[i] + e.info.glyph_metrics[2] * e.scale);
                        b = b.max(xs[i] + (e.info.glyph_metrics[2] + e.info.glyph_metrics[0]) * e.scale);
                    }
                }
                if a.is_finite() { (line_w - (b - a)) * 0.5 - a } else { 0.0 }
            }
        };
        let _ = content_start;
        let rtl = s.right_to_left;
        let line_left = left + ml_;
        let mut info = LineInfo {
            first_character_index: chars.len() as i32,
            ascender: v.baseline + v.asc,
            baseline: v.baseline,
            descender: v.baseline + v.desc,
            line_height: v.asc - v.desc,
            width: line_w,
            margin_left: ml_,
            margin_right: mr_,
            alignment: halign,
            first_visible_character_index: -1,
            last_visible_character_index: -1,
            ..Default::default()
        };
        let mut ext_min = V2::new(f32::INFINITY, f32::INFINITY);
        let mut ext_max = V2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut jx = 0.0f32;
        let mut vis_seen = 0;
        let mut max_adv_pre = f32::NEG_INFINITY;
        let wrapped_line = !ln.ends_paragraph && li + 1 < lines.len();
        let wrap_style_spacing = sh.get(ln.end).map(|e| e.style_spacing).unwrap_or(0.0);
        let mut run_asc = f32::NEG_INFINITY;
        let mut run_desc = f32::INFINITY;
        let mut ext_pre_max = f32::NEG_INFINITY;
        let mut ext_pre_pad = 0.0f32;
        let mut ext_pre_shy = false;
        let last_vis_idx = (ln.start..ln.end).rev().find(|&i| !sh[i].info.is_space && !sh[i].info.is_newline);
        for i in ln.start..ln.end {
            let e = &sh[i];
            let mut c = e.info.clone();
            if halign == HAlign::Justified || halign == HAlign::Flush {
                if c.is_space && last_vis_idx.is_some_and(|lv| i < lv) {
                    jx += justify_extra_space;
                } else if !c.is_space && !c.is_newline && vis_seen > 0 {
                    jx += justify_extra_char;
                }
            }
            let pen = line_left + dx + xs[i] + jx;
            // Right-to-left: the line runs leftwards from its right end; each
            // origin is the glyph's left edge and xAdvance equals the origin.
            // The pre-alignment layout starts at the right edge of the line.
            let rtl_origin = line_left + dx + used - xs[i] - e.advance;
            let rtl_shift = dx + used - line_w + ml_;
            let origin = if rtl { rtl_origin } else { pen + e.origin_offset };
            let shift = if rtl { rtl_shift } else { ml_ + dx + jx };
            if i == ln.start {
                shift_first[li] = shift;
            }
            if i + 1 == ln.end {
                shift_last[li] = shift;
            }
            if !c.is_space && !c.is_newline {
                vis_seen += 1;
            }
            let mut adv = e.advance;
            // A line ended by wrapping reports its last character without its
            // character spacing and without the bold spacing of the style in
            // effect where the line broke (the next line's first character).
            if Some(i) == last_vis_idx && wrapped_line {
                adv -= e.char_spacing + wrap_style_spacing;
            }
            let x_adv = if rtl {
                origin
            } else if c.xadv_from_line_left {
                pen + adv - line_left
            } else {
                pen + adv
            };
            c.origin = origin;
            c.x_advance = x_adv;
            c.base_line = v.baseline + c.voffset;
            c.ascender = v.baseline + c.elem_ascender + c.voffset;
            c.descender = v.baseline + c.elem_descender + c.voffset;
            if is_whitespace(c.character) && run_asc.is_finite() {
                // White space reports the line's extremes so far (before it).
                c.ascender = run_asc;
                c.descender = run_desc;
            }
            run_asc = run_asc.max(c.ascender);
            run_desc = run_desc.min(c.descender);
            if c.no_word || c.baseline_metrics {
                // Overflow placeholders (ellipsis slot, cut point) and unbroken
                // soft hyphens sit on the baseline.
                c.ascender = c.base_line;
                c.descender = c.base_line;
            }
            c.line_number = li as i32;
            c.page_number = page_of_line.get(li).copied().unwrap_or(0);
            let [gw, gh, bx, by, _] = c.glyph_metrics;
            let sc = c.scale;
            let p = if c.element_type == ElementType::Sprite { 0.0 } else { pad };
            // Bold glyphs get extra horizontal padding (the dilated outline).
            let px = if c.element_type != ElementType::Sprite && c.style & style::BOLD != 0 {
                let f = fonts.face(c.font);
                p + f.bold_style / 4.0 * f.atlas_padding
            } else {
                p
            };
            let x0 = origin + (bx - px) * sc * e.xk;
            let x1 = origin + (bx + gw + px) * sc * e.xk;
            let y0 = c.base_line + (by - gh - p) * sc;
            let y1 = c.base_line + (by + p) * sc;
            // Italic: shear about half the cap height above the baseline by
            // the font asset's italic style (35 → 0.35).
            let (sb, st) = if c.style & style::ITALIC != 0 && c.element_type != ElementType::Sprite {
                let f = fonts.face(c.font);
                let k = f.italic_style * 0.01;
                let mid = c.base_line + f.cap_line * sc * 0.5;
                // The slant is measured over the glyph box padded like the
                // quad's sides (bold pads more horizontally than vertically).
                let (sy0, sy1) = (c.base_line + (by - gh - px) * sc, c.base_line + (by + px) * sc);
                (k * (sy0 - mid), k * (sy1 - mid))
            } else {
                (0.0, 0.0)
            };
            c.bottom_left = V2::new(x0 + sb, y0);
            c.top_left = V2::new(x0 + st, y1);
            c.top_right = V2::new(x1 + st, y1);
            c.bottom_right = V2::new(x1 + sb, y0);
            if c.rotation != 0.0 {
                // <rotate>: counter-clockwise about the quad's centre.
                let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
                let (sn, cs) = c.rotation.to_radians().sin_cos();
                for q in [&mut c.bottom_left, &mut c.top_left, &mut c.top_right, &mut c.bottom_right] {
                    let (dx, dy) = (q.x - cx, q.y - cy);
                    *q = V2::new(cx + dx * cs - dy * sn, cy + dx * sn + dy * cs);
                }
            }
            c.aspect_ratio = if gh > 0.0 { (gw + 2.0 * p) / (gh + 2.0 * p) } else { 0.0 };
            if !c.is_newline && (c.is_visible || c.is_space) {
                rendered_min.x = rendered_min.x.min(origin);
                rendered_max.x = rendered_max.x.max(x_adv);
            }
            if i == ln.start && !c.is_visible && !c.is_newline {
                // A line that opens with white space starts its extents there.
                ext_min.x = c.bottom_left.x;
                rendered_min.x = rendered_min.x.min(origin);
            }
            if c.is_visible {
                // Extents run from the first visible character's quad to the
                // last one's (possibly sheared) corners — not the extremes, so
                // <pos> moving back left shortens them; maxAdvance and length
                // likewise follow the last visible character.
                // (TMP reads the bottom-left and top-right corners.)
                let qx0 = c.bottom_left.x;
                let qx1 = c.top_right.x;
                if !ext_min.x.is_finite() {
                    ext_min.x = qx0;
                }
                ext_max.x = qx1;
                ext_pre_max = if rtl { qx1 - dx - used } else { qx1 - dx - jx };
                ext_pre_pad = p * sc;
                ext_pre_shy = c.character == 0xAD;
                max_adv_pre = if rtl {
                    left + line_w - xs[i] - e.advance
                } else {
                    left + xs[i] + e.advance - e.char_spacing - if wrapped_line { wrap_style_spacing } else { e.style_spacing }
                };
                if info.first_visible_character_index < 0 {
                    info.first_visible_character_index = chars.len() as i32;
                }
                info.last_visible_character_index = chars.len() as i32;
                info.visible_character_count += 1;
            } else if c.no_word && c.ellipsis_tail {
                info.visible_character_count += 1;
            } else if is_whitespace(c.character) {
                info.space_count += 1;
                if xs[i] < content_end {
                    info.visible_space_count += 1;
                }
            }
            if c.is_space && !c.is_visible {
                // spaces still extend max advance if followed by visible content
            }
            chars.push(c);
        }
        let _ = vis_seen;
        if info.visible_character_count == 0 {
            info.first_visible_character_index = info.first_character_index;
            info.last_visible_character_index = (chars.len() as i32 - 1).max(info.first_character_index);
        }
        info.last_character_index = chars.len() as i32 - 1;
        info.character_count = info.last_character_index - info.first_character_index + 1;
        // Pre-alignment advance end; the alignment shift is added after all
        // lines are known (TMP adds the next line's first-character shift).
        info.max_advance = if max_adv_pre.is_finite() { max_adv_pre } else { left };
        // Length: right edge of the furthest glyph quad from the line's left
        // edge; lines closed by a line feed or the end of the text measure the
        // glyph without its quad padding.
        // (A line ending in a shown soft hyphen measures it without padding too.)
        let soft_end = !ln.ends_paragraph && (li + 1 < lines.len() || truncated) && !ext_pre_shy;
        info.length = if ext_pre_max.is_finite() { ext_pre_max - line_left - if soft_end { 0.0 } else { ext_pre_pad } } else { 0.0 };
        if !ext_min.x.is_finite() {
            // No visible glyph: the extents are the last character's (empty) quad.
            match chars.last() {
                Some(c) if ln.end > ln.start => {
                    ext_min.x = c.bottom_left.x;
                    ext_max.x = c.top_right.x;
                }
                _ => {
                    ext_min.x = line_left + dx;
                    ext_max.x = line_left + dx;
                }
            }
        }
        ext_min.y = info.descender;
        ext_max.y = info.ascender;
        info.extents_min = ext_min;
        info.extents_max = ext_max;
        if (ln.start..ln.end).any(|i| !sh[i].info.is_space && !sh[i].info.is_newline) {
            rendered_min.y = rendered_min.y.min(info.descender);
            rendered_max.y = rendered_max.y.max(info.ascender);
        }
        max_line_width = max_line_width.max(content_end);
        word_count_total += info.word_count;
        out.lines.push(info);
    }
    let _ = word_count_total;
    // Page mode: the part of a line after <page> continues the same line entry
    // (positioned at the top of the new page); the entry reports the last part's
    // vertical metrics and right edge.
    if lines.iter().any(|l| l.page_break_after) && out.lines.len() == lines.len() {
        let mut map = vec![0usize; out.lines.len()];
        let mut merged: Vec<LineInfo> = Vec::new();
        let (mut sf, mut sl) = (Vec::new(), Vec::new());
        for (li, info) in std::mem::take(&mut out.lines).into_iter().enumerate() {
            if li > 0 && lines[li - 1].page_break_after && !merged.is_empty() {
                let m = merged.last_mut().unwrap();
                m.last_character_index = info.last_character_index;
                if info.visible_character_count > 0 {
                    if m.visible_character_count == 0 {
                        m.first_visible_character_index = info.first_visible_character_index;
                    }
                    m.last_visible_character_index = info.last_visible_character_index;
                }
                m.character_count += info.character_count;
                m.visible_character_count += info.visible_character_count;
                m.space_count += info.space_count;
                m.visible_space_count += info.visible_space_count;
                m.word_count += info.word_count;
                m.ascender = info.ascender;
                m.baseline = info.baseline;
                m.descender = info.descender;
                m.line_height = info.line_height;
                m.length = info.length;
                m.max_advance = info.max_advance;
                m.width = info.width;
                m.margin_left = info.margin_left;
                m.margin_right = info.margin_right;
                m.alignment = info.alignment;
                m.extents_max = info.extents_max;
                m.extents_min.y = info.extents_min.y;
                *sl.last_mut().unwrap() = shift_last[li];
            } else {
                merged.push(info);
                sf.push(shift_first[li]);
                sl.push(shift_last[li]);
            }
            map[li] = merged.len() - 1;
        }
        for c in chars.iter_mut() {
            c.line_number = map[c.line_number.max(0) as usize] as i32;
        }
        out.lines = merged;
        shift_first = sf;
        shift_last = sl;
    }
    // Words: maximal runs of word characters over the whole text (a word may
    // span lines).
    {
        let first_vis = s.first_visible_character.max(0) as usize;
        let alnum = |c: &CharInfo| char::from_u32(c.character).is_some_and(|ch| ch.is_alphanumeric());
        let is_word = |i: usize, c: &CharInfo| -> bool {
            if i < first_vis || c.no_word {
                return false;
            }
            if is_word_char(c) {
                return true;
            }
            // An apostrophe between letters or digits joins them (don't, rock’n’roll).
            matches!(c.character, 0x27 | 0x2019) && i > 0 && i + 1 < chars.len() && alnum(&chars[i - 1]) && alnum(&chars[i + 1])
        };
        if let Some(c0) = chars.first() {
            // A text that opens with a character that is neither part of a word
            // nor punctuation (white space, a symbol, a hidden character)
            // records an empty word at index 0 (first 0, last -1).
            if !is_word(0, c0) && (first_vis > 0 || !is_punctuation(c0.character)) {
                out.words.push(WordInfo { first_character_index: 0, last_character_index: -1, character_count: 0 });
                if let Some(l) = out.lines.first_mut() {
                    l.word_count += 1;
                }
            }
        }
        let mut i = 0;
        while i < chars.len() {
            if !is_word(i, &chars[i]) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j + 1 < chars.len() && is_word(j + 1, &chars[j + 1]) {
                j += 1;
            }
            out.words.push(WordInfo { first_character_index: i as i32, last_character_index: j as i32, character_count: (j - i + 1) as i32 });
            // A word is counted on the line of the character that closes it.
            let ln = chars.get(j + 1).unwrap_or(&chars[j]).line_number.max(0) as usize;
            if let Some(l) = out.lines.get_mut(ln) {
                l.word_count += 1;
            }
            i = j + 1;
        }
    }
    // TMP reports maxAdvance shifted by the alignment offset of the following
    // line's first character (the last line uses its own last character).
    for li in 0..out.lines.len() {
        let d = if li + 1 < out.lines.len() { shift_first[li + 1] } else { shift_last[li] };
        out.lines[li].max_advance += d;
    }
    // Visibility limits.
    let first_vis = s.first_visible_character.max(0);
    // maxVisibleWords hides everything after the last allowed word's end.
    let max_words = s.max_visible_words.max(0) as usize;
    let word_limit: i32 = if max_words >= out.words.len() {
        i32::MAX
    } else if max_words == 0 {
        -1
    } else {
        out.words[max_words - 1].last_character_index
    };
    for (i, c) in chars.iter_mut().enumerate() {
        if !c.is_visible {
            continue;
        }
        let visible_by_count = (i as i32) >= first_vis && (i as i32) < s.max_visible_characters;
        let visible_by_line = (c.line_number as usize) < max_vis_lines;
        let visible_by_word = (i as i32) <= word_limit;
        let visible_by_page = s.overflow != Overflow::Page || c.page_number == page_display;
        if !(visible_by_count && visible_by_line && visible_by_word && visible_by_page) {
            c.is_visible = false;
        }
    }
    // Space count and decorations.
    out.space_count = chars.iter().skip(skipped).filter(|c| is_whitespace(c.character)).count() as i32;
    if ellipsis_vertical {
        // A vertical-overflow ellipsis also counts its zero-width tail as a space.
        out.space_count += 1;
    }
    out.sprite_count = chars.iter().filter(|c| c.element_type == ElementType::Sprite).count() as i32;
    build_decorations(&chars, fonts, &mut out, unit);
    // Pages.
    if !chars.is_empty() {
        let mut pi = -1;
        for (i, c) in chars.iter().enumerate() {
            if c.page_number != pi {
                pi = c.page_number;
                out.pages.push(PageInfo { first_character_index: i as i32, last_character_index: i as i32, ascender: c.ascender, base_line: c.base_line, descender: c.descender });
            }
            let p = out.pages.last_mut().unwrap();
            p.last_character_index = i as i32;
            p.descender = p.descender.min(c.descender);
            p.ascender = p.ascender.max(c.ascender);
        }
    } else {
        out.pages.push(PageInfo::default());
    }
    // Links: convert element indices into character indices (they coincide).
    out.links = parsed.links.into_iter().filter(|l| (l.link_text_first_character_index as usize) < chars.len()).collect();
    for l in out.links.iter_mut() {
        let max_len = chars.len() as i32 - l.link_text_first_character_index;
        l.link_text_length = l.link_text_length.min(max_len);
    }
    if skipped > 0 {
        // Skipped elements are empty, at the start of the first line. (TMP
        // leaves them unprocessed: whatever an earlier layout pass left there,
        // offset again by each pass, so their values are not comparable.)
        let (lx, by) = match out.lines.first() {
            Some(l) => (left + l.margin_left, l.baseline),
            None => (left, top),
        };
        for c in chars.iter_mut().take(skipped) {
            let (x, y) = (lx, by);
            c.origin = x;
            c.x_advance = x;
            c.base_line = y;
            c.ascender = y;
            c.descender = y;
            c.bottom_left = V2::new(x, y);
            c.top_left = V2::new(x, y);
            c.top_right = V2::new(x, y);
            c.bottom_right = V2::new(x, y);
            c.color = [0.0; 4];
        }
    }
    // Rendered bounds.
    if rendered_min.x.is_finite() {
        out.rendered_min = rendered_min;
        out.rendered_max = rendered_max;
    }
    out.preferred_width = ceil_hundredth(pref_w_full);
    out.preferred_height = ceil_hundredth(pref_h_full);
    let _ = max_line_width;
    out.text_bounds_min = out.rendered_min;
    out.text_bounds_max = out.rendered_max;
    out.characters = chars;
    out
}

fn build_decorations<F: FontProvider>(chars: &[CharInfo], fonts: &mut F, out: &mut Layout, _unit: f32) {
    // Runs of consecutive characters on one line sharing a decoration flag and colour.
    for (kind, flag) in [(2u8, style::HIGHLIGHT), (0u8, style::UNDERLINE), (1u8, style::STRIKETHROUGH)] {
        let mut i = 0;
        while i < chars.len() {
            let c = &chars[i];
            if c.style & flag == 0 || c.is_newline {
                i += 1;
                continue;
            }
            let color = match kind {
                0 => c.underline_color,
                1 => c.strikethrough_color,
                _ => c.highlight_color,
            };
            let mut j = i;
            while j + 1 < chars.len()
                && chars[j + 1].style & flag != 0
                && chars[j + 1].line_number == c.line_number
                && !chars[j + 1].is_newline
                && (match kind {
                    0 => chars[j + 1].underline_color,
                    1 => chars[j + 1].strikethrough_color,
                    _ => chars[j + 1].highlight_color,
                }) == color
            {
                j += 1;
            }
            // Trim trailing spaces.
            let mut k = j;
            while k > i && chars[k].is_space {
                k -= 1;
            }
            let f = fonts.face(c.font);
            let sc = c.scale;
            let (y0, y1) = match kind {
                0 => {
                    let y = c.base_line - c.voffset + f.underline_offset * sc;
                    (y - f.underline_thickness * sc * 0.5, y + f.underline_thickness * sc * 0.5)
                }
                1 => {
                    let y = c.base_line + f.strikethrough_offset * sc;
                    (y - f.underline_thickness * sc * 0.5, y + f.underline_thickness * sc * 0.5)
                }
                _ => (c.descender, c.ascender),
            };
            out.decorations.push(Decoration { kind, font: c.font, x0: c.origin, x1: chars[k].x_advance, y0, y1, color, page: c.page_number, scale: sc });
            i = j + 1;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A fixed-advance test font: every glyph 0.6 em wide, ascent 0.8, descent -0.2.
    pub struct MonoFont;
    impl FontProvider for MonoFont {
        fn face(&self, _f: u32) -> FaceMetrics {
            FaceMetrics { point_size: 100.0, ascent: 80.0, descent: -20.0, line_height: 100.0, cap_line: 70.0, mean_line: 50.0, superscript_size: 0.5, subscript_size: 0.5, superscript_offset: 80.0, subscript_offset: -20.0, underline_offset: -10.0, underline_thickness: 5.0, strikethrough_offset: 25.0, tab_width: 60.0, tab_size: 10.0, italic_style: 35.0, bold_spacing: 7.0, ..Default::default() }
        }
        fn glyph(&mut self, f: u32, cp: u32) -> Option<(u32, GlyphInfo)> {
            let w = if cp == ' ' as u32 { 0.0 } else { 50.0 };
            Some((f, GlyphInfo { index: cp, width: w, height: 70.0, bearing_x: 5.0, bearing_y: 70.0, advance: 60.0, rect: [0.0, 0.0, w, 70.0], atlas_index: 0, atlas_size: [512.0, 512.0] }))
        }
        fn kerning(&self, _f: u32, _l: u32, _r: u32) -> Option<[f32; 8]> {
            None
        }
    }

    fn settings(w: f32, h: f32) -> Settings {
        Settings { rect: [0.0, 0.0, w, h], font_size: 10.0, ..Default::default() }
    }

    #[test]
    fn single_line_positions() {
        let s = settings(100.0, 50.0);
        let l = layout("abc", &s, &mut MonoFont);
        assert_eq!(l.characters.len(), 3);
        assert!((l.characters[1].origin - 6.0).abs() < 1e-4);
        assert!((l.lines[0].baseline - 42.0).abs() < 1e-4);
        assert!((l.preferred_width - 18.0).abs() < 1e-4);
        assert!((l.preferred_height - 10.0).abs() < 1e-4);
    }

    #[test]
    fn wraps_at_spaces_and_mid_word() {
        let s = settings(20.0, 100.0);
        let l = layout("aa bb ccccc", &s, &mut MonoFont);
        let lines: Vec<(i32, i32)> = l.lines.iter().map(|x| (x.first_character_index, x.last_character_index)).collect();
        assert_eq!(lines[0], (0, 2)); // "aa "
        assert_eq!(lines[1], (3, 5)); // "bb "
        // "ccccc" is 30 wide: split mid-word.
        assert_eq!(lines[2].0, 6);
        assert!(l.lines.len() == 4);
    }

    #[test]
    fn rich_text_size_and_color() {
        let s = settings(200.0, 50.0);
        let l = layout("a<size=20>b</size><color=#ff0000>c", &s, &mut MonoFont);
        assert_eq!(l.characters.len(), 3);
        assert!((l.characters[1].point_size - 20.0).abs() < 1e-4);
        assert!((l.characters[2].origin - 18.0).abs() < 1e-4);
        assert_eq!(l.characters[2].color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn alignment_center_right() {
        let mut s = settings(100.0, 50.0);
        s.halign = HAlign::Center;
        let l = layout("ab", &s, &mut MonoFont);
        assert!((l.characters[0].origin - 44.0).abs() < 1e-4);
        s.halign = HAlign::Right;
        let l = layout("ab ", &s, &mut MonoFont);
        assert!((l.characters[0].origin - 88.0).abs() < 1e-4, "{}", l.characters[0].origin);
    }

    #[test]
    fn ellipsis_and_truncate() {
        let mut s = settings(30.0, 15.0);
        s.overflow = Overflow::Truncate;
        let l = layout("aaaa bbbb cccc", &s, &mut MonoFont);
        assert!(l.is_text_overflowing);
        assert_eq!(l.lines.len(), 1);
        s.overflow = Overflow::Ellipsis;
        let l = layout("aaaa bbbb cccc", &s, &mut MonoFont);
        // The ellipsis takes over a character slot (TMP keeps that slot's code
        // point) and draws the U+2026 glyph.
        assert!(l.characters.iter().any(|c| c.glyph == 0x2026 && c.is_visible));
    }

    #[test]
    fn auto_size_fits() {
        let mut s = settings(60.0, 12.0);
        s.auto_size = true;
        s.font_size_min = 1.0;
        s.font_size_max = 100.0;
        let l = layout("aaaaa", &s, &mut MonoFont);
        assert!(!l.is_text_overflowing);
        assert!(l.font_size > 9.0 && l.font_size <= 12.0, "{}", l.font_size);
    }
}
