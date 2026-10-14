//! EMF+ (MS-EMFPLUS): GDI+ drawing records carried in EMF comment records.
//!
//! Most EMF+ files are *dual*: the same picture is also drawn with plain EMF records, which
//! [`crate::emf`] already plays, so this reader ignores their EMF+ part. An *EMF+ only* file has
//! nothing else to draw, so it used to show as an empty picture; for those this reader plays the
//! vector records: solid brushes and pens, paths, rectangles, polygons, lines, ellipses and
//! Béziers, under the world and page transforms, save/restore and containers. Images, text,
//! gradient brushes, clipping and arcs are not drawn yet.

use crate::bytes::Bytes;
use crate::canvas::{BS_NULL, BS_SOLID, Brush, Canvas, PS_NULL, Pen, Xform, mul};
use crate::geom::{Op, P};

/// "EMF+" as the comment identifier of an EMR_COMMENT holding EMF+ records.
pub(crate) const EMF_PLUS: u32 = 0x2B46_4D45;
const IDENTITY: Xform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
/// GDI+ allows 64 objects; ids are a byte.
const MAX_OBJECTS: usize = 64;
const MAX_STACK: usize = 64;
/// Points per path or point list read, as the plain EMF reader caps paths.
const MAX_POINTS: usize = 100_000;

#[derive(Clone, Debug)]
enum Obj {
    Brush([u8; 4]),
    Pen { color: [u8; 4], width: f64 },
    Path(Vec<Op>),
}

pub(crate) struct Plus {
    /// An EMF+ header said the file is EMF+ only, so these records are the picture.
    active: bool,
    dpi: f64,
    world: Xform,
    page: f64,
    objects: Vec<Option<Obj>>,
    stack: Vec<(u32, Xform, f64)>,
}

impl Plus {
    pub fn new() -> Self {
        Plus { active: false, dpi: 96.0, world: IDENTITY, page: 1.0, objects: vec![None; MAX_OBJECTS], stack: Vec::new() }
    }

    /// Plays the EMF+ records of one EMR_COMMENT payload (after the identifier).
    pub fn comment(&mut self, cv: &mut Canvas, data: Bytes) {
        let mut pos = 0usize;
        while !cv.full() {
            let (Some(typ), Some(flags), Some(size), Some(dsize)) = (data.u16(pos), data.u16(pos + 2), data.u32(pos + 4), data.u32(pos + 8)) else {
                break;
            };
            let (size, dsize) = (size as usize, dsize as usize);
            if size < 12 || dsize > size - 12 {
                break;
            }
            let Some(rec) = data.slice(pos + 12, dsize) else { break };
            let _ = self.record(cv, typ, flags, Bytes(rec));
            let Some(next) = pos.checked_add(size) else { break };
            pos = next;
        }
    }

    fn record(&mut self, cv: &mut Canvas, typ: u16, flags: u16, r: Bytes) -> Option<()> {
        if typ == 0x4001 {
            // Header: dual files also draw with EMF records, which win.
            self.active = flags & 1 == 0;
            if let Some(d) = r.u32(8).map(f64::from).filter(|d| *d > 0.0 && *d < 100_000.0) {
                self.dpi = d;
            }
            return Some(());
        }
        if !self.active {
            return None;
        }
        let compressed = flags & 0x4000 != 0;
        let id = usize::from(flags & 0xFF);
        match typ {
            0x4008 => self.object(flags, r),
            0x400A => self.fill_rects(cv, flags, r),
            0x400B => self.with_pen(id, |p| p.draw_rects(cv, compressed, r)),
            0x400C => self.fill_polygon(cv, flags, r),
            0x400D => self.with_pen(id, |p| {
                let pts = points(r, 4, r.u32(0)? as usize, compressed)?;
                p.shape(cv, poly_ops(&pts, flags & 0x2000 != 0), false, true);
                Some(())
            }),
            0x400E => self.fill_ellipse(cv, flags, r),
            0x400F => self.with_pen(id, |p| {
                let rect = rects(r, 0, 1, compressed)?;
                p.shape(cv, ellipse_ops(*rect.first()?), false, true);
                Some(())
            }),
            0x4014 => self.fill_path(cv, flags, r),
            0x4015 => {
                let path = self.path(id);
                r.u32(0).and_then(|pen| {
                    self.with_pen(pen as usize, |p| {
                        p.shape(cv, path?, false, true);
                        Some(())
                    })
                })
            }
            0x4019 => self.with_pen(id, |p| {
                let pts = points(r, 4, r.u32(0)? as usize, compressed)?;
                let (first, rest) = pts.split_first()?;
                let mut ops = vec![Op::M(*first)];
                ops.extend(rest.as_chunks::<3>().0.iter().map(|c| Op::C(c[0], c[1], c[2])));
                p.shape(cv, ops, false, true);
                Some(())
            }),
            0x4025 | 0x4028 => r.u32(0).map(|i| self.push(i)),
            0x4027 => r.u32(24).map(|i| self.push(i)),
            0x4026 | 0x4029 => r.u32(0).map(|i| self.pop(i)),
            0x402A => matrix(r, 0).map(|m| self.world = m),
            0x402B => {
                self.world = IDENTITY;
                Some(())
            }
            0x402C => matrix(r, 0).and_then(|m| self.apply(m, flags)),
            0x402D => self.apply([1.0, 0.0, 0.0, 1.0, f64::from(r.f32(0)?), f64::from(r.f32(4)?)], flags),
            0x402E => self.apply([f64::from(r.f32(0)?), 0.0, 0.0, f64::from(r.f32(4)?), 0.0, 0.0], flags),
            0x402F => {
                let a = f64::from(r.f32(0)?).to_radians();
                self.apply([a.cos(), a.sin(), -a.sin(), a.cos(), 0.0, 0.0], flags)
            }
            0x4030 => {
                let scale = f64::from(r.f32(0)?);
                let unit = match flags & 0xFF {
                    3 => self.dpi / 72.0,
                    4 => self.dpi,
                    5 => self.dpi / 300.0,
                    6 => self.dpi / 25.4,
                    _ => 1.0,
                };
                let page = scale * unit;
                if page.is_finite() && page > 0.0 && page < 1.0e6 {
                    self.page = page;
                }
                Some(())
            }
            _ => None,
        }
    }

    /// Without the `A` flag a transform is applied before the world transform (prepended).
    fn apply(&mut self, m: Xform, flags: u16) -> Option<()> {
        self.world = if flags & 0x2000 != 0 { mul(self.world, m) } else { mul(m, self.world) };
        Some(())
    }

    fn push(&mut self, id: u32) {
        if self.stack.len() < MAX_STACK {
            self.stack.push((id, self.world, self.page));
        }
    }

    fn pop(&mut self, id: u32) {
        if let Some(i) = self.stack.iter().rposition(|s| s.0 == id) {
            let (_, w, p) = self.stack[i];
            self.stack.truncate(i);
            self.world = w;
            self.page = p;
        }
    }

    fn object(&mut self, flags: u16, r: Bytes) -> Option<()> {
        if flags & 0x8000 != 0 {
            // Continued (split) objects are images and large paths; not read yet.
            return None;
        }
        let id = usize::from(flags & 0xFF);
        let obj = match (flags >> 8) & 0x7F {
            1 => brush(r, 0).map(|(c, _)| Obj::Brush(c)),
            2 => pen(r),
            3 => path(r).map(Obj::Path),
            _ => None,
        };
        if let Some(slot) = self.objects.get_mut(id) {
            *slot = obj;
        }
        Some(())
    }

    fn path(&self, id: usize) -> Option<Vec<Op>> {
        match self.objects.get(id)? {
            Some(Obj::Path(ops)) => Some(ops.clone()),
            _ => None,
        }
    }

    /// The brush colour of a fill record: an ARGB value with the `S` flag, else a brush object.
    fn brush_of(&self, flags: u16, v: u32) -> Option<[u8; 4]> {
        if flags & 0x8000 != 0 {
            return Some(argb(v));
        }
        match self.objects.get(v as usize)? {
            Some(Obj::Brush(c)) => Some(*c),
            _ => None,
        }
    }

    fn with_pen<T>(&mut self, id: usize, f: impl FnOnce(&mut Painter) -> Option<T>) -> Option<T> {
        let Some(Some(Obj::Pen { color, width })) = self.objects.get(id).cloned() else { return None };
        let mut p = Painter { xf: mul(self.world, [self.page, 0.0, 0.0, self.page, 0.0, 0.0]), pen: Some((color, width)), fill: None };
        f(&mut p)
    }

    fn filler(&self, color: [u8; 4]) -> Painter {
        Painter { xf: mul(self.world, [self.page, 0.0, 0.0, self.page, 0.0, 0.0]), pen: None, fill: Some(color) }
    }

    fn fill_rects(&mut self, cv: &mut Canvas, flags: u16, r: Bytes) -> Option<()> {
        let c = self.brush_of(flags, r.u32(0)?)?;
        let rs = rects(r, 8, r.u32(4)? as usize, flags & 0x4000 != 0)?;
        let p = self.filler(c);
        for rc in rs {
            p.shape(cv, rect_ops(rc), true, false);
        }
        Some(())
    }

    fn fill_polygon(&mut self, cv: &mut Canvas, flags: u16, r: Bytes) -> Option<()> {
        let c = self.brush_of(flags, r.u32(0)?)?;
        let pts = points(r, 8, r.u32(4)? as usize, flags & 0x4000 != 0)?;
        self.filler(c).shape(cv, poly_ops(&pts, true), true, false);
        Some(())
    }

    fn fill_ellipse(&mut self, cv: &mut Canvas, flags: u16, r: Bytes) -> Option<()> {
        let c = self.brush_of(flags, r.u32(0)?)?;
        let rect = *rects(r, 4, 1, flags & 0x4000 != 0)?.first()?;
        self.filler(c).shape(cv, ellipse_ops(rect), true, false);
        Some(())
    }

    fn fill_path(&mut self, cv: &mut Canvas, flags: u16, r: Bytes) -> Option<()> {
        let c = self.brush_of(flags, r.u32(0)?)?;
        let ops = self.path(usize::from(flags & 0xFF))?;
        self.filler(c).shape(cv, ops, true, false);
        Some(())
    }
}

/// Draws through the canvas with EMF+'s transform and its own pen or brush, then puts the
/// canvas's GDI state back.
struct Painter {
    xf: Xform,
    pen: Option<([u8; 4], f64)>,
    fill: Option<[u8; 4]>,
}

impl Painter {
    fn shape(&self, cv: &mut Canvas, ops: Vec<Op>, fill: bool, stroke: bool) {
        let world = cv.world();
        cv.save_dc();
        cv.modify_world(mul(self.xf, world), 4);
        let (pc, pw) = self.pen.unwrap_or(([0, 0, 0, 0], 0.0));
        cv.set_pen(Pen { style: if self.pen.is_some() { 0 } else { PS_NULL }, width: pw, color: pc });
        cv.set_brush(match self.fill {
            Some(c) => Brush::new(BS_SOLID, c),
            None => Brush::new(BS_NULL, [0, 0, 0, 0]),
        });
        cv.shape(&ops, fill, stroke);
        cv.restore_dc(-1);
    }

    fn draw_rects(&self, cv: &mut Canvas, compressed: bool, r: Bytes) -> Option<()> {
        for rc in rects(r, 4, r.u32(0)? as usize, compressed)? {
            self.shape(cv, rect_ops(rc), false, true);
        }
        Some(())
    }
}

/// ARGB (0xAARRGGBB) to RGBA.
fn argb(v: u32) -> [u8; 4] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8]
}

fn matrix(r: Bytes, off: usize) -> Option<Xform> {
    let mut m = [0.0; 6];
    for (i, v) in m.iter_mut().enumerate() {
        *v = f64::from(r.f32(off + i * 4)?);
    }
    m.iter().all(|v| v.is_finite()).then_some(m)
}

/// A brush object at `off`: its colour and byte length. Only solid brushes are read; other kinds
/// (hatch, texture, gradients) are given their first colour so shapes still show.
fn brush(r: Bytes, off: usize) -> Option<([u8; 4], usize)> {
    let kind = r.u32(off + 4)?;
    let c = argb(r.u32(off + 8)?);
    let c = if kind == 1 { argb(r.u32(off + 12)?) } else { c };
    Some((c, 12))
}

/// A pen object: version, type, then pen data (flags, unit, width, optional fields), then a brush.
fn pen(r: Bytes) -> Option<Obj> {
    let pflags = r.u32(8)?;
    let width = f64::from(r.f32(16)?);
    let mut off = 20usize;
    for (bit, len) in [(0x1, 24), (0x2, 4), (0x4, 4), (0x8, 4), (0x10, 4), (0x20, 4), (0x40, 4), (0x80, 4)] {
        if pflags & bit != 0 {
            off += len;
        }
    }
    for bit in [0x100, 0x200, 0x400] {
        if pflags & bit != 0 {
            off += if bit == 0x200 { 4 } else { 4 + 4 * (r.u32(off)? as usize).min(MAX_POINTS) };
        }
    }
    for bit in [0x800, 0x1000] {
        if pflags & bit != 0 {
            off += 4 + (r.u32(off)? as usize).min(1 << 20);
        }
    }
    let (color, _) = brush(r, off)?;
    Some(Obj::Pen { color, width: if width.is_finite() { width.clamp(0.0, 1.0e5) } else { 0.0 } })
}

/// A path object: points, then one type byte per point (start, line, Bézier; 0x80 closes).
fn path(r: Bytes) -> Option<Vec<Op>> {
    let n = (r.u32(4)? as usize).min(MAX_POINTS);
    let pflags = r.u32(8)?;
    if pflags & 0x1800 != 0 {
        // Relative (0x0800) and run-length encoded (0x1000) points: not read yet.
        return None;
    }
    let compressed = pflags & 0x4000 != 0;
    let pts = points(r, 12, n, compressed)?;
    let toff = 12 + n * if compressed { 4 } else { 8 };
    let types = r.slice(toff, n)?;
    let mut ops = Vec::with_capacity(n + 4);
    let mut i = 0;
    while i < n {
        let (t, p) = (types[i], pts[i]);
        match t & 0x07 {
            0 => ops.push(Op::M(p)),
            3 if i + 2 < n => {
                ops.push(Op::C(p, pts[i + 1], pts[i + 2]));
                let last = types[i + 2];
                i += 2;
                if last & 0x80 != 0 {
                    ops.push(Op::Z);
                }
                i += 1;
                continue;
            }
            _ => ops.push(Op::L(p)),
        }
        if t & 0x80 != 0 {
            ops.push(Op::Z);
        }
        i += 1;
    }
    Some(ops)
}

/// `n` points at `off`: 16-bit integers when compressed, else 32-bit floats.
fn points(r: Bytes, off: usize, n: usize, compressed: bool) -> Option<Vec<P>> {
    let n = n.min(MAX_POINTS);
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        v.push(if compressed {
            (f64::from(r.i16(off + i * 4)?), f64::from(r.i16(off + i * 4 + 2)?))
        } else {
            let (x, y) = (f64::from(r.f32(off + i * 8)?), f64::from(r.f32(off + i * 8 + 4)?));
            if !(x.is_finite() && y.is_finite()) {
                return None;
            }
            (x, y)
        });
    }
    Some(v)
}

/// `n` rectangles (x, y, w, h) at `off`, as `[l, t, r, b]`.
fn rects(r: Bytes, off: usize, n: usize, compressed: bool) -> Option<Vec<[f64; 4]>> {
    let pts = points(r, off, n.min(MAX_POINTS / 2).checked_mul(2)?, compressed)?;
    Some(pts.as_chunks::<2>().0.iter().map(|c| [c[0].0, c[0].1, c[0].0 + c[1].0, c[0].1 + c[1].1]).collect())
}

fn rect_ops([l, t, r, b]: [f64; 4]) -> Vec<Op> {
    crate::geom::rect(l, t, r, b)
}

fn poly_ops(pts: &[P], close: bool) -> Vec<Op> {
    let mut ops: Vec<Op> = pts.iter().enumerate().map(|(i, p)| if i == 0 { Op::M(*p) } else { Op::L(*p) }).collect();
    if close {
        ops.push(Op::Z);
    }
    ops
}

/// An ellipse in its bounding box, as four cubic arcs.
fn ellipse_ops([l, t, r, b]: [f64; 4]) -> Vec<Op> {
    let (cx, cy, rx, ry) = ((l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0);
    let k = 0.552_284_75;
    let (ox, oy) = (rx * k, ry * k);
    vec![
        Op::M((cx + rx, cy)),
        Op::C((cx + rx, cy + oy), (cx + ox, cy + ry), (cx, cy + ry)),
        Op::C((cx - ox, cy + ry), (cx - rx, cy + oy), (cx - rx, cy)),
        Op::C((cx - rx, cy - oy), (cx - ox, cy - ry), (cx, cy - ry)),
        Op::C((cx + ox, cy - ry), (cx + rx, cy - oy), (cx + rx, cy)),
        Op::Z,
    ]
}
