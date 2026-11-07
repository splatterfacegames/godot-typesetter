//! Rectangle packing for glyph atlases (Jylänki, "A Thousand Ways to Pack the
//! Bin", 2010): MaxRects with the five classic placement heuristics for offline
//! packing, Skyline bottom-left for incremental (dynamic) packing.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.x + o.w <= self.x + self.w && o.y + o.h <= self.y + self.h
    }
    fn intersects(&self, o: &Rect) -> bool {
        o.x < self.x + self.w && o.x + o.w > self.x && o.y < self.y + self.h && o.y + o.h > self.y
    }
}

/// Mirrors the five MaxRects heuristics exposed by TextCore's public
/// `GlyphPackingMode` enum, in the same order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heuristic {
    BestShortSideFit = 0,
    BestLongSideFit = 1,
    BestAreaFit = 2,
    BottomLeftRule = 3,
    ContactPointRule = 4,
}

impl Heuristic {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Heuristic::BestLongSideFit,
            2 => Heuristic::BestAreaFit,
            3 => Heuristic::BottomLeftRule,
            4 => Heuristic::ContactPointRule,
            _ => Heuristic::BestShortSideFit,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MaxRects {
    pub w: i32,
    pub h: i32,
    free: Vec<Rect>,
    used: Vec<Rect>,
}

impl MaxRects {
    pub fn new(w: i32, h: i32) -> Self {
        MaxRects { w, h, free: vec![Rect::new(0, 0, w, h)], used: Vec::new() }
    }

    pub fn used(&self) -> &[Rect] {
        &self.used
    }

    /// Restores state from rectangles already in the atlas (dynamic assets).
    pub fn with_used(w: i32, h: i32, used: &[Rect]) -> Self {
        let mut m = MaxRects::new(w, h);
        for r in used {
            m.place(*r);
        }
        m
    }

    pub fn occupancy(&self) -> f64 {
        let a: i64 = self.used.iter().map(|r| r.w as i64 * r.h as i64).sum();
        a as f64 / (self.w as i64 * self.h as i64).max(1) as f64
    }

    fn score(&self, fr: &Rect, w: i32, h: i32, heur: Heuristic) -> (i64, i64) {
        let lw = (fr.w - w) as i64;
        let lh = (fr.h - h) as i64;
        match heur {
            Heuristic::BestShortSideFit => (lw.min(lh), lw.max(lh)),
            Heuristic::BestLongSideFit => (lw.max(lh), lw.min(lh)),
            Heuristic::BestAreaFit => (fr.w as i64 * fr.h as i64 - w as i64 * h as i64, lw.min(lh)),
            Heuristic::BottomLeftRule => ((fr.y + h) as i64, fr.x as i64),
            Heuristic::ContactPointRule => (-(self.contact(fr.x, fr.y, w, h)), 0),
        }
    }

    fn contact(&self, x: i32, y: i32, w: i32, h: i32) -> i64 {
        let overlap = |a0: i32, a1: i32, b0: i32, b1: i32| -> i64 { (a1.min(b1) - a0.max(b0)).max(0) as i64 };
        let mut s = 0i64;
        if x == 0 || x + w == self.w {
            s += h as i64;
        }
        if y == 0 || y + h == self.h {
            s += w as i64;
        }
        for u in &self.used {
            if u.x == x + w || u.x + u.w == x {
                s += overlap(u.y, u.y + u.h, y, y + h);
            }
            if u.y == y + h || u.y + u.h == y {
                s += overlap(u.x, u.x + u.w, x, x + w);
            }
        }
        s
    }

    /// Finds a position for a w×h rectangle without inserting it.
    pub fn find(&self, w: i32, h: i32, heur: Heuristic) -> Option<Rect> {
        let mut best: Option<(Rect, (i64, i64))> = None;
        for fr in &self.free {
            if fr.w >= w && fr.h >= h {
                let s = self.score(fr, w, h, heur);
                if best.as_ref().is_none_or(|b| s < b.1) {
                    best = Some((Rect::new(fr.x, fr.y, w, h), s));
                }
            }
        }
        best.map(|b| b.0)
    }

    pub fn insert(&mut self, w: i32, h: i32, heur: Heuristic) -> Option<Rect> {
        let r = self.find(w, h, heur)?;
        self.place(r);
        Some(r)
    }

    pub fn place(&mut self, r: Rect) {
        let mut i = 0;
        let mut added: Vec<Rect> = Vec::new();
        while i < self.free.len() {
            let fr = self.free[i];
            if fr.intersects(&r) {
                self.free.swap_remove(i);
                if r.x > fr.x {
                    added.push(Rect::new(fr.x, fr.y, r.x - fr.x, fr.h));
                }
                if r.x + r.w < fr.x + fr.w {
                    added.push(Rect::new(r.x + r.w, fr.y, fr.x + fr.w - (r.x + r.w), fr.h));
                }
                if r.y > fr.y {
                    added.push(Rect::new(fr.x, fr.y, fr.w, r.y - fr.y));
                }
                if r.y + r.h < fr.y + fr.h {
                    added.push(Rect::new(fr.x, r.y + r.h, fr.w, fr.y + fr.h - (r.y + r.h)));
                }
            } else {
                i += 1;
            }
        }
        self.free.extend(added);
        // Prune rectangles contained in others.
        let mut j = 0;
        while j < self.free.len() {
            let mut removed = false;
            let mut k = j + 1;
            while k < self.free.len() {
                if self.free[k].contains(&self.free[j]) {
                    self.free.swap_remove(j);
                    removed = true;
                    break;
                }
                if self.free[j].contains(&self.free[k]) {
                    self.free.swap_remove(k);
                } else {
                    k += 1;
                }
            }
            if !removed {
                j += 1;
            }
        }
        self.used.push(r);
    }
}

/// Skyline bottom-left packer: fast online insertion for dynamic atlases.
#[derive(Clone, Debug)]
pub struct Skyline {
    pub w: i32,
    pub h: i32,
    // (x, y, width) segments covering [0, w).
    nodes: Vec<(i32, i32, i32)>,
}

impl Skyline {
    pub fn new(w: i32, h: i32) -> Self {
        Skyline { w, h, nodes: vec![(0, 0, w)] }
    }

    fn fit(&self, i: usize, w: i32, h: i32) -> Option<i32> {
        let x = self.nodes[i].0;
        if x + w > self.w {
            return None;
        }
        let mut left = w;
        let mut y = 0;
        let mut j = i;
        while left > 0 {
            if j >= self.nodes.len() {
                return None;
            }
            y = y.max(self.nodes[j].1);
            if y + h > self.h {
                return None;
            }
            left -= self.nodes[j].2;
            j += 1;
        }
        Some(y)
    }

    pub fn insert(&mut self, w: i32, h: i32) -> Option<Rect> {
        let mut best: Option<(usize, i32, i32)> = None; // (node, y, width-of-node)
        for i in 0..self.nodes.len() {
            if let Some(y) = self.fit(i, w, h) {
                let better = match best {
                    None => true,
                    Some((_, by, bw)) => y + h < by + h || (y == by && self.nodes[i].2 < bw),
                };
                if better {
                    best = Some((i, y, self.nodes[i].2));
                }
            }
        }
        let (i, y, _) = best?;
        let x = self.nodes[i].0;
        self.nodes.insert(i, (x, y + h, w));
        let k = i + 1;
        while k < self.nodes.len() {
            let prev_end = self.nodes[k - 1].0 + self.nodes[k - 1].2;
            let (nx, ny, nw) = self.nodes[k];
            if nx < prev_end {
                let shrink = prev_end - nx;
                if nw - shrink <= 0 {
                    self.nodes.remove(k);
                    continue;
                }
                self.nodes[k] = (nx + shrink, ny, nw - shrink);
            }
            break;
        }
        // Merge equal heights.
        let mut m = 0;
        while m + 1 < self.nodes.len() {
            if self.nodes[m].1 == self.nodes[m + 1].1 {
                self.nodes[m].2 += self.nodes[m + 1].2;
                self.nodes.remove(m + 1);
            } else {
                m += 1;
            }
        }
        Some(Rect::new(x, y, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_overlap(rs: &[Rect], w: i32, h: i32) {
        for (i, a) in rs.iter().enumerate() {
            assert!(a.x >= 0 && a.y >= 0 && a.x + a.w <= w && a.y + a.h <= h, "{a:?} out of bounds");
            for b in &rs[i + 1..] {
                assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn maxrects_all_heuristics_pack_without_overlap() {
        for hv in 0..5 {
            let heur = Heuristic::from_i32(hv);
            let mut m = MaxRects::new(256, 256);
            let mut placed = Vec::new();
            for i in 0..200 {
                let (w, h) = (5 + (i * 7) % 23, 6 + (i * 11) % 19);
                if let Some(r) = m.insert(w, h, heur) {
                    placed.push(r);
                }
            }
            assert!(placed.len() > 100, "heuristic {hv} placed {}", placed.len());
            no_overlap(&placed, 256, 256);
        }
    }

    #[test]
    fn skyline_packs_without_overlap() {
        let mut s = Skyline::new(128, 128);
        let mut placed = Vec::new();
        for i in 0..100 {
            if let Some(r) = s.insert(4 + (i * 5) % 13, 4 + (i * 3) % 17) {
                placed.push(r);
            }
        }
        assert!(placed.len() > 50);
        no_overlap(&placed, 128, 128);
    }
}
