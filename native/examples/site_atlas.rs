//! Writes the atlas behind the product site's live hero.
//! cargo run --release --example site_atlas -- <font.ttf> <out.pgm> <out.json>
use typesetter_native::core::{atlas, font::Face, pack::Heuristic};

const POINT_SIZE: f64 = 96.0;
const PADDING: i32 = 12;
const SIZE: i32 = 1024;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let face = Face::new(std::fs::read(&a[1]).unwrap(), 0).unwrap();
    let s = atlas::GenSettings {
        point_size: POINT_SIZE,
        padding: PADDING,
        mode: atlas::RenderMode::Sdf16,
        width: SIZE,
        height: SIZE,
        heuristic: Heuristic::BestShortSideFit,
        multi_atlas: false,
    };
    let chars: Vec<u32> = (32u32..127).filter(|&c| face.glyph_id(c).is_some()).collect();
    let gids: Vec<u32> = chars.iter().filter_map(|&c| face.glyph_id(c)).collect();
    let mut pages = Vec::new();
    let r = atlas::add_glyphs(&face, &gids, &s, &mut pages);
    assert!(r.missing.is_empty(), "missing {:?}", r.missing);
    let p = &pages[0];
    let mut out = format!("P5\n{} {}\n255\n", p.width, p.height).into_bytes();
    out.extend(&p.pixels);
    std::fs::write(&a[2], out).unwrap();
    let mut items = Vec::new();
    for &c in &chars {
        let gid = face.glyph_id(c).unwrap();
        let g = r.glyphs.iter().find(|g| g.index == gid).unwrap();
        items.push(format!(
            "\"{}\":[{},{},{},{},{},{},{},{},{}]",
            c, g.rect.x, g.rect.y, g.rect.w, g.rect.h, g.width, g.height, g.bearing_x, g.bearing_y, g.advance
        ));
    }
    let json = format!(
        "{{\"pointSize\":{},\"padding\":{},\"size\":{},\"gradientScale\":{},\"glyphs\":{{{}}}}}\n",
        POINT_SIZE,
        PADDING,
        SIZE,
        s.gradient_scale(),
        items.join(",")
    );
    std::fs::write(&a[3], json).unwrap();
}
