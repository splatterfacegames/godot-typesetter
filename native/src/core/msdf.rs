//! MSDF / MTSDF generation through fdsm (a pure-Rust msdfgen port): simple edge
//! colouring with msdfgen's default 3-radian corner threshold, artefact
//! correction, then sign correction with the non-zero fill rule.

use fdsm::bezier::scanline::FillRule;
use fdsm::bezier::{Point, Segment};
use fdsm::correct_error::{correct_error_msdf, correct_error_mtsdf, ErrorCorrectionConfig};
use fdsm::generate::{generate_msdf, generate_mtsdf};
use fdsm::render::{correct_sign_msdf, correct_sign_mtsdf};
use fdsm::shape::{Contour, Shape};
use image::{ImageBuffer, Rgb, Rgba};

use super::outline::{Outline, Seg};

fn to_shape(o: &Outline) -> Shape<Contour> {
    let pt = |p: super::outline::P| Point::new(p.x, p.y);
    let mut shape = Shape::default();
    for c in &o.contours {
        let mut contour = Contour::default();
        for s in c {
            contour.segments.push(match *s {
                Seg::Line(a, b) => Segment::line(pt(a), pt(b)),
                Seg::Quad(a, b, c) => Segment::quad(pt(a), pt(b), pt(c)),
                Seg::Cubic(a, b, c, d) => Segment::cubic(pt(a), pt(b), pt(c), pt(d)),
            });
        }
        if !contour.segments.is_empty() {
            shape.contours.push(contour);
        }
    }
    shape
}

/// Renders a w×h RGBA8 multi-channel field of `o` (pixel space, y-down).
/// `gradient_scale` matches the single-channel encoding: the full range is
/// 2 × gradient_scale texels. MSDF leaves alpha at 255.
pub fn render(o: &Outline, w: usize, h: usize, gradient_scale: f64, true_sdf_alpha: bool) -> Vec<u8> {
    let range = 2.0 * gradient_scale;
    let shape = to_shape(o);
    let colored = Shape::edge_coloring_simple(shape, 3.0f64.sin(), 0);
    let prepared = colored.clone().prepare();
    let cfg = ErrorCorrectionConfig::default();
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if true_sdf_alpha {
        let mut img: ImageBuffer<Rgba<f32>, Vec<f32>> = ImageBuffer::new(w as u32, h as u32);
        generate_mtsdf(&prepared, range, &mut img);
        correct_error_mtsdf(&mut img, &colored, &prepared, range, &cfg);
        correct_sign_mtsdf(&mut img, &prepared, FillRule::Nonzero);
        img.into_raw().into_iter().map(to8).collect()
    } else {
        let mut img: ImageBuffer<Rgb<f32>, Vec<f32>> = ImageBuffer::new(w as u32, h as u32);
        generate_msdf(&prepared, range, &mut img);
        correct_error_msdf(&mut img, &colored, &prepared, range, &cfg);
        correct_sign_msdf(&mut img, &prepared, FillRule::Nonzero);
        img.pixels().flat_map(|p| [to8(p.0[0]), to8(p.0[1]), to8(p.0[2]), 255]).collect()
    }
}
