//! Writes binary k× masks of glyph boxes for distance-field calibration.
//! cargo run --release --example sdf_probe -- <font> <point> <padding> <k> <out_dir>
//! Each glyph: <out>/<gid>.bin (w*k × h*k bytes, 1 = inside) and index.json
//! entries [gid, box_x0, box_y1, w, h] (w, h include the padding).
use typesetter_native::core::{font::Face, outline::P, raster};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let face = Face::new(std::fs::read(&a[1]).unwrap(), 0).unwrap();
    let pt: f64 = a[2].parse().unwrap();
    let pad: i64 = a[3].parse().unwrap();
    let k: usize = a[4].parse().unwrap();
    let out = std::path::Path::new(&a[5]);
    std::fs::create_dir_all(out).unwrap();
    let mut idx = Vec::new();
    for cp in 32u32..127 {
        let Some(gid) = face.glyph_id(cp) else { continue };
        let m = face.pixel_metrics(gid, pt);
        let s16 = typesetter_native::core::font::ft_scale(pt, face.metrics().units_per_em);
        let q = |v: f64| typesetter_native::core::font::ft_mul(v, s16) as f64 / 64.0;
        let Some(o) = face.outline(gid).filter(|o| !o.is_empty()) else { continue };
        let o = o.map(|p| P::new(q(p.x), q(p.y)));
        let (w, h) = ((m.box_w + 2 * pad) as usize, (m.box_h + 2 * pad) as usize);
        let px = if std::env::var("PROBE_HIRES_Q").is_ok() {
            // Outline quantized on the k× grid (rendered at k × ppem).
            let s16k = typesetter_native::core::font::ft_scale(pt * k as f64, face.metrics().units_per_em);
            let qk = |v: f64| typesetter_native::core::font::ft_mul(v, s16k) as f64 / 64.0;
            face.outline(gid).unwrap().map(|p| P::new(qk(p.x), qk(p.y))).to_pixels(1.0, ((m.box_x0 - pad) * k as i64) as f64, ((m.box_y1 + pad) * k as i64) as f64)
        } else {
            o.to_pixels(k as f64, (m.box_x0 - pad) as f64, (m.box_y1 + pad) as f64)
        };
        let polys = px.flatten(0.01);
        let mask = if std::env::var("PROBE_EXCL").is_ok() { raster::inside_mask(&polys, w * k, h * k) } else { raster::inside_mask_dropout(&polys, w * k, h * k) };
        std::fs::write(out.join(format!("{gid}.bin")), mask.iter().map(|&b| b as u8).collect::<Vec<u8>>()).unwrap();
        let cov = raster::coverage(&polys, w * k, h * k);
        std::fs::write(out.join(format!("{gid}.cov")), cov.iter().map(|&c| (c * 255.0).round() as u8).collect::<Vec<u8>>()).unwrap();
        let s16k = typesetter_native::core::font::ft_scale(pt * k as f64, face.metrics().units_per_em);
        let qk = |v: f64| typesetter_native::core::font::ft_mul(v, s16k) as f64 / 64.0;
        let bk = face.outline(gid).unwrap().map(|p| P::new(qk(p.x), qk(p.y))).bounds();
        idx.push(format!("[{gid},{},{},{w},{h},{},{},{},{}]", m.box_x0, m.box_y1, bk.x0, bk.y0, bk.x1, bk.y1));
    }
    std::fs::write(out.join("index.json"), format!("[{}]", idx.join(","))).unwrap();
}
