//! Single-channel signed distance fields. All functions return signed distances
//! in texels, positive inside the glyph, sampled at texel centres.
//!
//! * `analytic`  – exact distance to the (finely flattened) outline, sign from the
//!                 non-zero winding rule. Used for the `SDF` render mode.
//! * `edt_oversampled` – Felzenszwalb & Huttenlocher's exact Euclidean distance
//!                 transform of a k× supersampled binary raster, box-filtered down.
//!                 Used for `SDF8` / `SDF16` / `SDF32`.
//! * `edt_antialiased` – nearest-edge propagation seeded with sub-pixel edge points
//!                 estimated from anti-aliased coverage (Gustavson & Strand 2011's
//!                 edge-distance function). Used for `SDFAA`.

use super::outline::P;
use super::raster;

/// Exact distance field. `polys` are closed polylines in pixel space; `reach`
/// bounds how far (in texels) distances are computed precisely – beyond it the
/// magnitude is clamped to `reach`.
pub fn analytic(polys: &[Vec<P>], w: usize, h: usize, reach: f64) -> Vec<f32> {
    let inside = raster::inside_mask(polys, w, h);
    let r = reach.max(1.0);
    let mut d2 = vec![(r * r) as f32; w * h];
    for poly in polys {
        for win in poly.windows(2) {
            let (a, b) = (win[0], win[1]);
            let x0 = ((a.x.min(b.x) - r - 0.5).floor().max(0.0)) as usize;
            let x1 = ((a.x.max(b.x) + r - 0.5).ceil().max(0.0) as usize).min(w.saturating_sub(1));
            let y0 = ((a.y.min(b.y) - r - 0.5).floor().max(0.0)) as usize;
            let y1 = ((a.y.max(b.y) + r - 0.5).ceil().max(0.0) as usize).min(h.saturating_sub(1));
            if x0 > x1 || y0 > y1 || w == 0 || h == 0 {
                continue;
            }
            let (abx, aby) = (b.x - a.x, b.y - a.y);
            let ll = abx * abx + aby * aby;
            let inv = if ll > 0.0 { 1.0 / ll } else { 0.0 };
            for y in y0..=y1 {
                let py = y as f64 + 0.5;
                let row = y * w;
                for x in x0..=x1 {
                    let px = x as f64 + 0.5;
                    let t = (((px - a.x) * abx + (py - a.y) * aby) * inv).clamp(0.0, 1.0);
                    let dx = px - (a.x + abx * t);
                    let dy = py - (a.y + aby * t);
                    let dd = (dx * dx + dy * dy) as f32;
                    let cell = &mut d2[row + x];
                    if dd < *cell {
                        *cell = dd;
                    }
                }
            }
        }
    }
    d2.iter().zip(inside.iter()).map(|(&dd, &ins)| if ins { dd.sqrt() } else { -dd.sqrt() }).collect()
}

const INF: f64 = 1e20;

/// One-dimensional squared-distance transform of a sampled function
/// (Felzenszwalb & Huttenlocher 2012, lower envelope of parabolas).
fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
            if s <= z[k] {
                if k == 0 {
                    // Cannot happen with finite seeds; guard anyway.
                    v[0] = q;
                    z[0] = -INF;
                    z[1] = INF;
                    break;
                }
                k -= 1;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = INF;
            break;
        }
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dx = q as f64 - p as f64;
        *dq = dx * dx + f[p];
    }
}

/// Squared Euclidean distance (in samples) from every sample to the nearest seed.
pub fn edt_2d(seed: &[bool], w: usize, h: usize) -> Vec<f64> {
    let mut grid: Vec<f64> = seed.iter().map(|&s| if s { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let mut f = vec![0.0; n];
    let mut d = vec![0.0; n];
    let mut v = vec![0usize; n];
    let mut z = vec![0.0; n + 1];
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        edt_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    grid
}

/// Signed distance (in samples) of a binary mask, positive inside. The edge is
/// taken to lie half a sample beyond the last inside sample centre.
pub fn edt_signed(mask: &[bool], w: usize, h: usize) -> Vec<f64> {
    let outside: Vec<bool> = mask.iter().map(|&m| !m).collect();
    let to_inside = edt_2d(mask, w, h);
    let to_outside = edt_2d(&outside, w, h);
    mask.iter()
        .enumerate()
        .map(|(i, &m)| {
            if m {
                to_outside[i].sqrt() - 0.5
            } else {
                -(to_inside[i].sqrt() - 0.5)
            }
        })
        .collect()
}

/// `SDF8/16/32`: binary raster at `k`× resolution, exact EDT, then each output
/// texel is the mean of its k×k block of signed distances, scaled back to texels.
pub fn edt_oversampled(polys: &[Vec<P>], w: usize, h: usize, k: usize) -> Vec<f32> {
    let k = k.max(1);
    let (hw, hh) = (w * k, h * k);
    let scaled: Vec<Vec<P>> =
        polys.iter().map(|p| p.iter().map(|q| P::new(q.x * k as f64, q.y * k as f64)).collect()).collect();
    let mask = raster::inside_mask(&scaled, hw, hh);
    let sd = edt_signed(&mask, hw, hh);
    let mut out = vec![0.0f32; w * h];
    let norm = 1.0 / ((k * k) as f64 * k as f64);
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for yy in 0..k {
                let row = (y * k + yy) * hw + x * k;
                for xx in 0..k {
                    s += sd[row + xx];
                }
            }
            out[y * w + x] = (s * norm) as f32;
        }
    }
    out
}

/// Distance from a texel centre to the edge inside that texel, given its
/// coverage `a` and the coverage gradient (Gustavson & Strand 2011). Positive
/// when the centre lies outside the shape.
fn edge_distance(gx: f64, gy: f64, a: f64) -> f64 {
    if (gx == 0.0 && gy == 0.0) || a <= 0.0 || a >= 1.0 {
        return 0.5 - a;
    }
    let gl = (gx * gx + gy * gy).sqrt();
    let (mut x, mut y) = ((gx / gl).abs(), (gy / gl).abs());
    if x < y {
        std::mem::swap(&mut x, &mut y);
    }
    if y < 1e-9 {
        return 0.5 - a;
    }
    let a1 = 0.5 * y / x;
    if a < a1 {
        0.5 * (x + y) - (2.0 * x * y * a).sqrt()
    } else if a < 1.0 - a1 {
        (0.5 - a) * x
    } else {
        -0.5 * (x + y) + (2.0 * x * y * (1.0 - a)).sqrt()
    }
}

/// `SDFAA`: anti-aliased coverage at 1×, sub-pixel edge points from the
/// coverage gradient, then two raster passes propagating the nearest edge point.
pub fn edt_antialiased(polys: &[Vec<P>], w: usize, h: usize) -> Vec<f32> {
    let cov = raster::coverage(polys, w, h);
    let at = |x: isize, y: isize| -> f64 {
        if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
            0.0
        } else {
            cov[y as usize * w + x as usize] as f64
        }
    };
    // Nearest edge point per texel (NaN = none yet).
    let mut ex = vec![f64::NAN; w * h];
    let mut ey = vec![f64::NAN; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let a = at(x, y);
            let is_edge = (a > 0.0 && a < 1.0)
                || (a >= 1.0 && (at(x - 1, y) < 1.0 || at(x + 1, y) < 1.0 || at(x, y - 1) < 1.0 || at(x, y + 1) < 1.0) && {
                    // Fully covered texel touching an uncovered one: edge on its border.
                    at(x - 1, y) <= 0.0 || at(x + 1, y) <= 0.0 || at(x, y - 1) <= 0.0 || at(x, y + 1) <= 0.0
                });
            if !is_edge {
                continue;
            }
            // Sobel gradient of coverage (points towards increasing coverage = inside).
            let s2 = std::f64::consts::SQRT_2;
            let gx = -at(x - 1, y - 1) - s2 * at(x - 1, y) - at(x - 1, y + 1) + at(x + 1, y - 1) + s2 * at(x + 1, y) + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - s2 * at(x, y - 1) - at(x + 1, y - 1) + at(x - 1, y + 1) + s2 * at(x, y + 1) + at(x + 1, y + 1);
            let gl = (gx * gx + gy * gy).sqrt();
            let df = edge_distance(gx, gy, a.clamp(0.0, 1.0));
            let (nx, ny) = if gl > 0.0 { (gx / gl, gy / gl) } else { (0.0, 0.0) };
            let i = y as usize * w + x as usize;
            ex[i] = x as f64 + 0.5 + nx * df;
            ey[i] = y as f64 + 0.5 + ny * df;
        }
    }
    let dist2 = |i: usize, cx: f64, cy: f64, ex: &[f64], ey: &[f64]| -> f64 {
        if ex[i].is_nan() {
            f64::INFINITY
        } else {
            (cx - ex[i]).powi(2) + (cy - ey[i]).powi(2)
        }
    };
    let mut best: Vec<f64> = (0..w * h)
        .map(|i| dist2(i, (i % w) as f64 + 0.5, (i / w) as f64 + 0.5, &ex, &ey))
        .collect();
    let relax = |x: usize, y: usize, nx: isize, ny: isize, ex: &mut [f64], ey: &mut [f64], best: &mut [f64]| {
        if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
            return;
        }
        let j = ny as usize * w + nx as usize;
        if ex[j].is_nan() {
            return;
        }
        let i = y * w + x;
        let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
        let d = (cx - ex[j]).powi(2) + (cy - ey[j]).powi(2);
        if d < best[i] {
            best[i] = d;
            ex[i] = ex[j];
            ey[i] = ey[j];
        }
    };
    for _ in 0..2 {
        for y in 0..h {
            for x in 0..w {
                let (xi, yi) = (x as isize, y as isize);
                relax(x, y, xi - 1, yi, &mut ex, &mut ey, &mut best);
                relax(x, y, xi, yi - 1, &mut ex, &mut ey, &mut best);
                relax(x, y, xi - 1, yi - 1, &mut ex, &mut ey, &mut best);
                relax(x, y, xi + 1, yi - 1, &mut ex, &mut ey, &mut best);
            }
            for x in (0..w).rev() {
                relax(x, y, x as isize + 1, y as isize, &mut ex, &mut ey, &mut best);
            }
        }
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let (xi, yi) = (x as isize, y as isize);
                relax(x, y, xi + 1, yi, &mut ex, &mut ey, &mut best);
                relax(x, y, xi, yi + 1, &mut ex, &mut ey, &mut best);
                relax(x, y, xi + 1, yi + 1, &mut ex, &mut ey, &mut best);
                relax(x, y, xi - 1, yi + 1, &mut ex, &mut ey, &mut best);
            }
            for x in 0..w {
                relax(x, y, x as isize - 1, y as isize, &mut ex, &mut ey, &mut best);
            }
        }
    }
    (0..w * h)
        .map(|i| {
            let d = best[i].sqrt();
            let d = if d.is_finite() { d } else { (w + h) as f64 };
            if cov[i] >= 0.5 {
                d as f32
            } else {
                -(d as f32)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::outline::{rect_outline, Outline, Seg};

    fn circle(cx: f64, cy: f64, r: f64) -> Outline {
        // Four cubic arcs.
        let k = 0.552_284_749_8 * r;
        let p = |x: f64, y: f64| P::new(cx + x, cy + y);
        Outline {
            contours: vec![vec![
                Seg::Cubic(p(r, 0.0), p(r, k), p(k, r), p(0.0, r)),
                Seg::Cubic(p(0.0, r), p(-k, r), p(-r, k), p(-r, 0.0)),
                Seg::Cubic(p(-r, 0.0), p(-r, -k), p(-k, -r), p(0.0, -r)),
                Seg::Cubic(p(0.0, -r), p(k, -r), p(r, -k), p(r, 0.0)),
            ]],
        }
    }

    fn check_circle(f: impl Fn(&[Vec<P>], usize, usize) -> Vec<f32>, tol: f64) {
        let (w, h, r) = (48usize, 48usize, 14.0);
        let polys = circle(24.0, 24.0, r).flatten(0.01);
        let sd = f(&polys, w, h);
        let mut worst = 0.0f64;
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (x as f64 + 0.5 - 24.0, y as f64 + 0.5 - 24.0);
                let truth = r - (px * px + py * py).sqrt();
                if truth.abs() < 6.0 {
                    worst = worst.max((sd[y * w + x] as f64 - truth).abs());
                }
            }
        }
        assert!(worst < tol, "max error {worst} >= {tol}");
    }

    #[test]
    fn analytic_matches_circle() {
        check_circle(|p, w, h| analytic(p, w, h, 8.0), 0.02);
    }

    #[test]
    fn oversampled_matches_circle() {
        check_circle(|p, w, h| edt_oversampled(p, w, h, 16), 0.15);
    }

    #[test]
    fn antialiased_matches_circle() {
        check_circle(edt_antialiased, 0.35);
    }

    #[test]
    fn edt_1d_line() {
        let mask = vec![false, false, true, false, false, false];
        let d = edt_2d(&mask, 6, 1);
        assert_eq!(d, vec![4.0, 1.0, 0.0, 1.0, 4.0, 9.0]);
    }

    #[test]
    fn analytic_sign_and_reach() {
        let polys = rect_outline(4.0, 4.0, 12.0, 12.0).flatten(0.01);
        let sd = analytic(&polys, 16, 16, 3.0);
        assert!((sd[8 * 16 + 8] - 3.0).abs() < 1e-5); // clamped at reach inside
        assert!((sd[8 * 16 + 4] - 0.5).abs() < 1e-5);
        assert!((sd[8 * 16 + 3] + 0.5).abs() < 1e-5);
    }
}
