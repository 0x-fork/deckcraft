//! Geometric clipping of drawn output to a device rectangle (EMF+ clip rectangles, paths and regions are
//! reduced to their bounding rectangle). Shapes wholly inside pass unchanged; shapes crossing the
//! edge are flattened and cut: fills with Sutherland–Hodgman per subpath, outlines with
//! Liang–Barsky per segment. Bitmaps are cropped and text gets a clip rectangle.

use crate::canvas::Raw;
use crate::dib::Rgba;
use crate::geom::{Op, P};

/// Line segments per Bézier when a crossing shape is flattened.
const CURVE_STEPS: usize = 16;

/// The bounding box `[l, t, r, b]` of the points in `ops` (control points included).
pub(crate) fn ops_bbox(ops: &[Op]) -> Option<[f64; 4]> {
    let mut b: Option<[f64; 4]> = None;
    let mut add = |p: P| {
        let v = b.get_or_insert([p.0, p.1, p.0, p.1]);
        *v = [v[0].min(p.0), v[1].min(p.1), v[2].max(p.0), v[3].max(p.1)];
    };
    for op in ops {
        match *op {
            Op::M(p) | Op::L(p) => add(p),
            Op::C(a, c, d) => {
                add(a);
                add(c);
                add(d);
            }
            Op::Z => {}
        }
    }
    b
}

pub(crate) fn intersect(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    // An empty intersection stays empty (zero size) so it clips everything away.
    if r[2] < r[0] || r[3] < r[1] { [r[0], r[1], r[0], r[1]] } else { r }
}

fn inside(b: [f64; 4], c: [f64; 4]) -> bool {
    b[0] >= c[0] && b[1] >= c[1] && b[2] <= c[2] && b[3] <= c[3]
}

fn disjoint(b: [f64; 4], c: [f64; 4]) -> bool {
    b[2] < c[0] || b[0] > c[2] || b[3] < c[1] || b[1] > c[3] || c[2] <= c[0] || c[3] <= c[1]
}

/// Clips one drawn item; a shape that is both filled and stroked comes back as two items.
pub(crate) fn clip_raw(raw: Raw, c: [f64; 4]) -> Vec<Raw> {
    match raw {
        Raw::Path { ops, fill, stroke, even_odd } => {
            let Some(b) = ops_bbox(&ops) else { return Vec::new() };
            if inside(b, c) {
                return vec![Raw::Path { ops, fill, stroke, even_odd }];
            }
            if disjoint(b, c) {
                return Vec::new();
            }
            let subs = flatten(&ops);
            let mut out = Vec::new();
            if fill.is_some() {
                let f = clip_fill(&subs, c);
                if !f.is_empty() {
                    out.push(Raw::Path { ops: f, fill, stroke: None, even_odd });
                }
            }
            if stroke.is_some() {
                let s = clip_stroke(&subs, c);
                if !s.is_empty() {
                    out.push(Raw::Path { ops: s, fill: None, stroke, even_odd });
                }
            }
            out
        }
        Raw::Pattern { ops, even_odd, tile } => {
            let Some(b) = ops_bbox(&ops) else { return Vec::new() };
            if inside(b, c) {
                return vec![Raw::Pattern { ops, even_odd, tile }];
            }
            if disjoint(b, c) {
                return Vec::new();
            }
            let f = clip_fill(&flatten(&ops), c);
            if f.is_empty() { Vec::new() } else { vec![Raw::Pattern { ops: f, even_odd, tile }] }
        }
        Raw::Bitmap { rect, img } => crop(rect, img, c).map(|(rect, img)| vec![Raw::Bitmap { rect, img }]).unwrap_or_default(),
        Raw::Text(mut t) => {
            let cur = t.clip.map_or(c, |[a, b, d, e]| intersect(c, [f64::from(a), f64::from(b), f64::from(d), f64::from(e)]));
            t.clip = Some(cur.map(|v| v as f32));
            vec![Raw::Text(t)]
        }
    }
}

/// Subpaths as point lists with their closed flag.
fn flatten(ops: &[Op]) -> Vec<(Vec<P>, bool)> {
    let mut subs: Vec<(Vec<P>, bool)> = Vec::new();
    for op in ops {
        match *op {
            Op::M(p) => subs.push((vec![p], false)),
            Op::L(p) => match subs.last_mut() {
                Some(s) => s.0.push(p),
                None => subs.push((vec![p], false)),
            },
            Op::C(a, b, d) => {
                let Some(s) = subs.last_mut() else { continue };
                let Some(&p0) = s.0.last() else { continue };
                for i in 1..=CURVE_STEPS {
                    let t = i as f64 / CURVE_STEPS as f64;
                    let u = 1.0 - t;
                    let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    s.0.push((k[0] * p0.0 + k[1] * a.0 + k[2] * b.0 + k[3] * d.0, k[0] * p0.1 + k[1] * a.1 + k[2] * b.1 + k[3] * d.1));
                }
            }
            Op::Z => {
                if let Some(s) = subs.last_mut() {
                    s.1 = true;
                    // Drawing after a close starts from the subpath's first point.
                    let first = s.0.first().copied();
                    if let Some(f) = first {
                        subs.push((vec![f], false));
                    }
                }
            }
        }
    }
    subs.retain(|s| s.0.len() > 1);
    subs
}

fn clip_fill(subs: &[(Vec<P>, bool)], c: [f64; 4]) -> Vec<Op> {
    let mut ops = Vec::new();
    for (pts, _) in subs {
        let mut poly = pts.clone();
        // Left, top, right, bottom edges in turn.
        for edge in 0..4 {
            let keep = |p: P| match edge {
                0 => p.0 >= c[0],
                1 => p.1 >= c[1],
                2 => p.0 <= c[2],
                _ => p.1 <= c[3],
            };
            let cut = |a: P, b: P| {
                let t = match edge {
                    0 => (c[0] - a.0) / (b.0 - a.0),
                    1 => (c[1] - a.1) / (b.1 - a.1),
                    2 => (c[2] - a.0) / (b.0 - a.0),
                    _ => (c[3] - a.1) / (b.1 - a.1),
                };
                (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
            };
            let mut next = Vec::with_capacity(poly.len() + 4);
            for i in 0..poly.len() {
                let (a, b) = (poly[(i + poly.len() - 1) % poly.len()], poly[i]);
                match (keep(a), keep(b)) {
                    (true, true) => next.push(b),
                    (true, false) => next.push(cut(a, b)),
                    (false, true) => {
                        next.push(cut(a, b));
                        next.push(b);
                    }
                    (false, false) => {}
                }
            }
            poly = next;
            if poly.is_empty() {
                break;
            }
        }
        if poly.len() >= 3 {
            ops.push(Op::M(poly[0]));
            ops.extend(poly[1..].iter().map(|p| Op::L(*p)));
            ops.push(Op::Z);
        }
    }
    ops
}

fn clip_stroke(subs: &[(Vec<P>, bool)], c: [f64; 4]) -> Vec<Op> {
    let mut ops = Vec::new();
    for (pts, closed) in subs {
        let mut pts = pts.clone();
        if *closed && let Some(&f) = pts.first() {
            pts.push(f);
        }
        let mut last: Option<P> = None;
        for w in pts.windows(2) {
            let Some((a, b)) = segment(w[0], w[1], c) else {
                last = None;
                continue;
            };
            if last != Some(a) {
                ops.push(Op::M(a));
            }
            ops.push(Op::L(b));
            last = Some(b);
        }
    }
    ops
}

/// Liang–Barsky: the part of segment `a`–`b` inside `c`.
fn segment(a: P, b: P, c: [f64; 4]) -> Option<(P, P)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.0 - c[0]), (dx, c[2] - a.0), (-dy, a.1 - c[1]), (dy, c[3] - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then_some(((a.0 + dx * t0, a.1 + dy * t0), (a.0 + dx * t1, a.1 + dy * t1)))
}

/// Crops a bitmap drawn into `rect` = `[x, y, w, h]` (device, positive extents) to `c`.
fn crop(rect: [f64; 4], img: Rgba, c: [f64; 4]) -> Option<([f64; 4], Rgba)> {
    let [x, y, w, h] = rect;
    let b = [x, y, x + w, y + h];
    if inside(b, c) {
        return Some((rect, img));
    }
    if disjoint(b, c) || w <= 0.0 || h <= 0.0 || img.w == 0 || img.h == 0 {
        return None;
    }
    let i = intersect(b, c);
    let (kx, ky) = (img.w as f64 / w, img.h as f64 / h);
    let x0 = (((i[0] - x) * kx).floor().max(0.0) as usize).min(img.w);
    let x1 = (((i[2] - x) * kx).ceil().max(0.0) as usize).min(img.w);
    let y0 = (((i[1] - y) * ky).floor().max(0.0) as usize).min(img.h);
    let y1 = (((i[3] - y) * ky).ceil().max(0.0) as usize).min(img.h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for row in y0..y1 {
        // Rows are top-down unless the image says otherwise.
        let r = if img.bottom_up { img.h - 1 - row } else { row };
        let s = (r * img.w + x0) * 4;
        px.extend_from_slice(img.px.get(s..s + (x1 - x0) * 4)?);
    }
    let out = [x + x0 as f64 / kx, y + y0 as f64 / ky, (x1 - x0) as f64 / kx, (y1 - y0) as f64 / ky];
    Some((out, Rgba { w: x1 - x0, h: y1 - y0, bottom_up: false, px }))
}
