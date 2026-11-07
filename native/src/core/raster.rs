//! Exact-area coverage rasteriser and non-zero winding inside tests.
//!
//! The coverage rasteriser is the signed-area accumulation technique popularised by
//! Raph Levien's font-rs (each line segment deposits the signed area it sweeps into
//! an accumulation buffer; a running sum then yields coverage). Overlapping contours
//! of the same direction sum above 1 and are clamped, which matches the non-zero fill
//! rule for the outlines fonts contain.

use super::outline::P;

pub struct Coverage {
    pub w: usize,
    pub h: usize,
    acc: Vec<f32>,
}

impl Coverage {
    pub fn new(w: usize, h: usize) -> Self {
        Coverage { w, h, acc: vec![0.0; w * h + 4] }
    }

    fn line(&mut self, p0: P, p1: P) {
        if (p0.y - p1.y).abs() < 1e-12 {
            return;
        }
        let (dir, p0, p1) = if p0.y < p1.y { (1.0f32, p0, p1) } else { (-1.0f32, p1, p0) };
        let w = self.w as i64;
        let h = self.h as f64;
        let dxdy = (p1.x - p0.x) / (p1.y - p0.y);
        let mut x = p0.x;
        let ystart = p0.y.max(0.0);
        if p0.y < 0.0 {
            x -= p0.y * dxdy;
        }
        let yend = p1.y.min(h);
        if ystart >= yend {
            return;
        }
        let y0 = ystart.floor() as i64;
        let y1 = yend.ceil() as i64;
        for y in y0..y1 {
            let linestart = y * w;
            let dy = ((y + 1) as f64).min(p1.y) - (y as f64).max(p0.y);
            let xnext = x + dxdy * dy;
            let d = (dy as f32) * dir;
            let (x0, x1) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let x0floor = x0.floor();
            let x0i = x0floor as i64;
            let x1ceil = x1.ceil();
            let x1i = x1ceil as i64;
            let mut add = |xi: i64, v: f32| {
                let xi = xi.clamp(0, w);
                let idx = (linestart + xi) as usize;
                if idx < self.acc.len() {
                    self.acc[idx] += v;
                }
            };
            if x1i <= x0i + 1 {
                let xmf = (0.5 * (x + xnext) - x0floor) as f32;
                add(x0i, d - d * xmf);
                add(x0i + 1, d * xmf);
            } else {
                let s = (1.0 / (x1 - x0)) as f32;
                let x0f = (x0 - x0floor) as f32;
                let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
                let x1f = (x1 - x1ceil + 1.0) as f32;
                let am = 0.5 * s * x1f * x1f;
                add(x0i, d * a0);
                if x1i == x0i + 2 {
                    add(x0i + 1, d * (1.0 - a0 - am));
                } else {
                    let a1 = s * (1.5 - x0f);
                    add(x0i + 1, d * (a1 - a0));
                    for xi in (x0i + 2)..(x1i - 1) {
                        add(xi, d * s);
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    add(x1i - 1, d * (1.0 - a2 - am));
                }
                add(x1i, d * am);
            }
            x = xnext;
        }
    }

    pub fn fill_polys(&mut self, polys: &[Vec<P>]) {
        for poly in polys {
            for win in poly.windows(2) {
                self.line(win[0], win[1]);
            }
        }
    }

    /// Coverage in [0, 1], row-major, y-down.
    pub fn finish(self) -> Vec<f32> {
        let n = self.w * self.h;
        let mut out = vec![0.0f32; n];
        let mut acc = 0.0f32;
        for (i, o) in out.iter_mut().enumerate() {
            acc += self.acc[i];
            // Snap float residue from the running sum so "untouched" stays exactly 0.
            let a = acc.abs().min(1.0);
            *o = if a < 1e-4 { 0.0 } else if a > 1.0 - 1e-4 { 1.0 } else { a };
        }
        out
    }
}

/// Rasterises closed polylines (pixel coordinates, y-down) to coverage.
pub fn coverage(polys: &[Vec<P>], w: usize, h: usize) -> Vec<f32> {
    let mut c = Coverage::new(w, h);
    c.fill_polys(polys);
    c.finish()
}

/// Non-zero winding inside test at every texel centre (x + 0.5, y + 0.5).
pub fn inside_mask(polys: &[Vec<P>], w: usize, h: usize) -> Vec<bool> {
    let mut out = vec![false; w * h];
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for y in 0..h {
        let sy = y as f64 + 0.5;
        xs.clear();
        for poly in polys {
            for win in poly.windows(2) {
                let (a, b) = (win[0], win[1]);
                if (a.y <= sy && b.y > sy) || (b.y <= sy && a.y > sy) {
                    let t = (sy - a.y) / (b.y - a.y);
                    let x = a.x + (b.x - a.x) * t;
                    xs.push((x, if b.y > a.y { 1 } else { -1 }));
                }
            }
        }
        if xs.is_empty() {
            continue;
        }
        xs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
        let row = &mut out[y * w..(y + 1) * w];
        let mut wind = 0i32;
        let mut k = 0usize;
        for (x, cell) in row.iter_mut().enumerate() {
            let sx = x as f64 + 0.5;
            while k < xs.len() && xs[k].0 <= sx {
                wind += xs[k].1;
                k += 1;
            }
            *cell = wind != 0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::outline::rect_outline;

    #[test]
    fn rect_coverage_is_exact() {
        // A 2.5 x 2 rectangle at (1.25, 1) .. (3.75, 3).
        let o = rect_outline(1.25, 1.0, 3.75, 3.0);
        let polys = o.flatten(0.01);
        let c = coverage(&polys, 6, 5);
        let total: f32 = c.iter().sum();
        assert!((total - 5.0).abs() < 1e-4, "area {total}");
        assert!((c[1 * 6 + 1] - 0.75).abs() < 1e-4);
        assert!((c[1 * 6 + 2] - 1.0).abs() < 1e-4);
        assert!((c[1 * 6 + 3] - 0.75).abs() < 1e-4);
        assert_eq!(c[0], 0.0);
    }

    #[test]
    fn overlapping_contours_clamp() {
        let mut o = rect_outline(0.0, 0.0, 4.0, 4.0);
        o.contours.extend(rect_outline(2.0, 0.0, 6.0, 4.0).contours);
        let polys = o.flatten(0.01);
        let c = coverage(&polys, 8, 4);
        assert!((c[3] - 1.0).abs() < 1e-5);
        let m = inside_mask(&polys, 8, 4);
        assert!(m[3] && m[5] && !m[6]);
    }
}
