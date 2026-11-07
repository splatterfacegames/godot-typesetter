//! cargo run --release --example dump_atlas -- <font.ttf> <mode 0-11> <point> <padding> <out.pgm|ppm>
use typesetter_native::core::{atlas, font::Face, pack::Heuristic};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let face = Face::new(std::fs::read(&a[1]).unwrap(), 0).unwrap();
    let mode = atlas::RenderMode::from_i32(a[2].parse().unwrap()).unwrap();
    let s = atlas::GenSettings { point_size: a[3].parse().unwrap(), padding: a[4].parse().unwrap(), mode, width: 512, height: 512, heuristic: Heuristic::BestShortSideFit, multi_atlas: false };
    let gids: Vec<u32> = (32u32..127).filter_map(|c| face.glyph_id(c)).collect();
    let t = std::time::Instant::now();
    let mut pages = Vec::new();
    let r = atlas::add_glyphs(&face, &gids, &s, &mut pages);
    eprintln!("{:?}: {} glyphs, {} missing, {:.1} ms", mode, r.glyphs.len(), r.missing.len(), t.elapsed().as_secs_f64() * 1e3);
    let p = &pages[0];
    let mut out = Vec::new();
    if p.channels == 1 {
        out.extend(format!("P5\n{} {}\n255\n", p.width, p.height).bytes());
        out.extend(&p.pixels);
    } else {
        out.extend(format!("P6\n{} {}\n255\n", p.width, p.height).bytes());
        for px in p.pixels.chunks(4) { out.extend(&px[..3]); }
    }
    std::fs::write(&a[5], out).unwrap();
    // Glyph rectangles (top-left origin) next to the image.
    let mut j = String::from("[");
    for (i, g) in r.glyphs.iter().enumerate() {
        if i > 0 { j.push(','); }
        j.push_str(&format!("[{},{},{},{},{}]", g.index, g.rect.x, g.rect.y, g.rect.w, g.rect.h));
    }
    j.push(']');
    std::fs::write(format!("{}.json", &a[5]), j).unwrap();
}
