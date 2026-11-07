//! Glyph outlines: contours of line / quadratic / cubic segments, flattening
//! and transforms. Coordinates are y-up font space until `to_pixels` maps them
//! into a y-down bitmap frame.

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

impl P {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        P { x, y }
    }
    #[inline]
    pub fn lerp(self, o: P, t: f64) -> P {
        P::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line(P, P),
    Quad(P, P, P),
    Cubic(P, P, P, P),
}

impl Seg {
    pub fn map(&self, f: &impl Fn(P) -> P) -> Seg {
        match *self {
            Seg::Line(a, b) => Seg::Line(f(a), f(b)),
            Seg::Quad(a, b, c) => Seg::Quad(f(a), f(b), f(c)),
            Seg::Cubic(a, b, c, d) => Seg::Cubic(f(a), f(b), f(c), f(d)),
        }
    }

    /// Appends a polyline approximation (excluding the start point) whose
    /// maximum deviation from the curve is about `tol`.
    pub fn flatten_into(&self, tol: f64, out: &mut Vec<P>) {
        match *self {
            Seg::Line(_, b) => out.push(b),
            Seg::Quad(a, b, c) => {
                // Deviation of a quadratic from its chord is |a - 2b + c| / 4.
                let dd = ((a.x - 2.0 * b.x + c.x).powi(2) + (a.y - 2.0 * b.y + c.y).powi(2)).sqrt();
                let n = ((dd / (4.0 * tol)).sqrt().ceil() as usize).clamp(1, 256);
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let mt = 1.0 - t;
                    out.push(P::new(
                        mt * mt * a.x + 2.0 * mt * t * b.x + t * t * c.x,
                        mt * mt * a.y + 2.0 * mt * t * b.y + t * t * c.y,
                    ));
                }
            }
            Seg::Cubic(a, b, c, d) => {
                let d1 = ((a.x - 2.0 * b.x + c.x).powi(2) + (a.y - 2.0 * b.y + c.y).powi(2)).sqrt();
                let d2 = ((b.x - 2.0 * c.x + d.x).powi(2) + (b.y - 2.0 * c.y + d.y).powi(2)).sqrt();
                let dd = d1.max(d2);
                let n = ((dd * 0.75 / tol).sqrt().ceil() as usize).clamp(1, 512);
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let mt = 1.0 - t;
                    let (w0, w1, w2, w3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
                    out.push(P::new(
                        w0 * a.x + w1 * b.x + w2 * c.x + w3 * d.x,
                        w0 * a.y + w1 * b.y + w2 * c.y + w3 * d.y,
                    ));
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Outline {
    pub contours: Vec<Vec<Seg>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Bounds {
    pub fn empty() -> Self {
        Bounds { x0: f64::INFINITY, y0: f64::INFINITY, x1: f64::NEG_INFINITY, y1: f64::NEG_INFINITY }
    }
    pub fn is_empty(&self) -> bool {
        !(self.x1 >= self.x0 && self.y1 >= self.y0)
    }
    fn add(&mut self, p: P) {
        self.x0 = self.x0.min(p.x);
        self.y0 = self.y0.min(p.y);
        self.x1 = self.x1.max(p.x);
        self.y1 = self.y1.max(p.y);
    }
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        self.contours.iter().all(|c| c.is_empty())
    }

    pub fn map(&self, f: impl Fn(P) -> P) -> Outline {
        Outline { contours: self.contours.iter().map(|c| c.iter().map(|s| s.map(&f)).collect()).collect() }
    }

    /// Scales font units by `scale` and moves into a y-down frame in which
    /// font-space point (ox, oy) lands on pixel coordinate (0, 0):
    /// px = (x - ox) * scale, py = (oy - y) * scale.
    pub fn to_pixels(&self, scale: f64, ox: f64, oy: f64) -> Outline {
        self.map(|p| P::new((p.x - ox) * scale, (oy - p.y) * scale))
    }

    /// Tight bounds of the flattened outline (exact enough for glyph boxes).
    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for poly in self.flatten(0.05) {
            for p in poly {
                b.add(p);
            }
        }
        b
    }

    /// Closed polylines, one per contour, start point included.
    pub fn flatten(&self, tol: f64) -> Vec<Vec<P>> {
        let mut out = Vec::with_capacity(self.contours.len());
        for c in &self.contours {
            if c.is_empty() {
                continue;
            }
            let start = match c[0] {
                Seg::Line(a, _) | Seg::Quad(a, _, _) | Seg::Cubic(a, _, _, _) => a,
            };
            let mut poly = vec![start];
            for s in c {
                s.flatten_into(tol, &mut poly);
            }
            // Ensure closure.
            if let (Some(f), Some(l)) = (poly.first().copied(), poly.last().copied()) {
                if (f.x - l.x).abs() > 1e-12 || (f.y - l.y).abs() > 1e-12 {
                    poly.push(f);
                }
            }
            out.push(poly);
        }
        out
    }
}

/// Builds an `Outline` from move/line/quad/cubic/close calls (skrifa's pen).
#[derive(Default)]
pub struct OutlineBuilder {
    pub outline: Outline,
    start: Option<P>,
    last: Option<P>,
}

impl OutlineBuilder {
    fn close_current(&mut self) {
        if let (Some(s), Some(l)) = (self.start, self.last) {
            if s != l {
                if let Some(c) = self.outline.contours.last_mut() {
                    c.push(Seg::Line(l, s));
                }
            }
        }
        self.start = None;
        self.last = None;
    }
    pub fn finish(mut self) -> Outline {
        self.close_current();
        self.outline.contours.retain(|c| !c.is_empty());
        self.outline
    }
}

impl skrifa::outline::OutlinePen for OutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_current();
        let p = P::new(x as f64, y as f64);
        self.start = Some(p);
        self.last = Some(p);
        self.outline.contours.push(Vec::new());
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = P::new(x as f64, y as f64);
        if let (Some(l), Some(c)) = (self.last, self.outline.contours.last_mut()) {
            if l != p {
                c.push(Seg::Line(l, p));
            }
        }
        self.last = Some(p);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let p = P::new(x as f64, y as f64);
        if let (Some(l), Some(c)) = (self.last, self.outline.contours.last_mut()) {
            c.push(Seg::Quad(l, P::new(cx0 as f64, cy0 as f64), p));
        }
        self.last = Some(p);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let p = P::new(x as f64, y as f64);
        if let (Some(l), Some(c)) = (self.last, self.outline.contours.last_mut()) {
            c.push(Seg::Cubic(l, P::new(cx0 as f64, cy0 as f64), P::new(cx1 as f64, cy1 as f64), p));
        }
        self.last = Some(p);
    }
    fn close(&mut self) {
        self.close_current();
    }
}

#[cfg(test)]
pub fn rect_outline(x0: f64, y0: f64, x1: f64, y1: f64) -> Outline {
    let (a, b, c, d) = (P::new(x0, y0), P::new(x1, y0), P::new(x1, y1), P::new(x0, y1));
    Outline { contours: vec![vec![Seg::Line(a, b), Seg::Line(b, c), Seg::Line(c, d), Seg::Line(d, a)]] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_flatten_stays_within_tolerance() {
        let s = Seg::Quad(P::new(0.0, 0.0), P::new(50.0, 100.0), P::new(100.0, 0.0));
        let mut pts = vec![P::new(0.0, 0.0)];
        s.flatten_into(0.1, &mut pts);
        assert!(pts.len() > 4);
        // Every point of the true curve lies within ~tol of the polyline.
        let seg_dist = |q: P, a: P, b: P| {
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let t = (((q.x - a.x) * dx + (q.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            ((q.x - a.x - dx * t).powi(2) + (q.y - a.y - dy * t).powi(2)).sqrt()
        };
        for i in 0..=1000 {
            let t = i as f64 / 1000.0;
            let mt = 1.0 - t;
            let q = P::new(2.0 * mt * t * 50.0 + t * t * 100.0, 2.0 * mt * t * 100.0);
            let d = pts.windows(2).map(|w| seg_dist(q, w[0], w[1])).fold(f64::MAX, f64::min);
            assert!(d < 0.15, "deviation {d} at t={t}");
        }
    }

    #[test]
    fn bounds_of_rect() {
        let o = rect_outline(1.0, 2.0, 3.0, 5.0);
        let b = o.bounds();
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (1.0, 2.0, 3.0, 5.0));
    }
}
