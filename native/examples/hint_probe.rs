//! Prints hinted glyph boxes for several hinting configurations, for
//! comparison with TextMesh Pro's hinted atlases (tools: compare by hand).
//! cargo run --release --example hint_probe -- <font> <ppem> > out.json
use skrifa::outline::{Engine, HintingOptions, SmoothMode, Target};
use typesetter_native::core::font::Face;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let face = Face::new(std::fs::read(&a[1]).unwrap(), 0).unwrap();
    let ppem: f32 = a[2].parse().unwrap();
    let mut configs: Vec<(String, HintingOptions)> = Vec::new();
    for (en, e) in [("auto", Engine::Auto(None)), ("fallback", Engine::AutoFallback)] {
        configs.push((format!("{en}-mono"), HintingOptions { engine: e.clone(), target: Target::Mono }));
        for (mn, m) in [("normal", SmoothMode::Normal), ("light", SmoothMode::Light), ("lcd", SmoothMode::Lcd)] {
            for sym in [true, false] {
                for lin in [false, true] {
                    configs.push((format!("{en}-{mn}-sym{}-lin{}", sym as u8, lin as u8), HintingOptions { engine: e.clone(), target: Target::Smooth { mode: m, symmetric_rendering: sym, preserve_linear_metrics: lin } }));
                }
            }
        }
    }
    let mut out = String::from("{");
    for (ci, (name, opts)) in configs.iter().enumerate() {
        if ci > 0 { out.push(','); }
        out.push_str(&format!("\"{name}\":{{"));
        let mut first = true;
        for cp in 32u32..127 {
            let Some(gid) = face.glyph_id(cp) else { continue };
            let Some(o) = face.outline_hinted_with(gid, ppem, opts.clone()) else { continue };
            let b = o.bounds();
            if b.is_empty() { continue; }
            if !first { out.push(','); }
            first = false;
            out.push_str(&format!("\"{gid}\":[{},{},{},{}]", b.x0, b.y0, b.x1, b.y1));
        }
        out.push('}');
    }
    out.push('}');
    println!("{out}");
}
