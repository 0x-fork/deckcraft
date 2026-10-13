//! 3-D extrusion (`a:sp3d extrusionH`, `a:extrusionClr`) seen through a turned camera, drawn in
//! 2-D: the shape's outline is swept from its back face to its front face along the camera's view
//! of the depth axis, in the side colour, and the face is drawn on top as usual.
//!
//! The sides are one shaded colour (the extrusion colour, or the fill's, darkened as a matte side
//! facing away from the three-point rig's key light); PowerPoint shades each side by its own
//! normal. Without a turned camera an extrusion points at the viewer and nothing of it shows.

use deckcraft_color::{Rgba, SchemeSlot};
use deckcraft_model::ColorRef;
use deckcraft_model::resolve::Ctx;
use kurbo::{Affine, BezPath, Vec2};
use vello_cpu::RenderContext;

use crate::camera::{self, Camera};

/// How much darker the sides are than the face colour.
const SIDE_SHADE: f64 = 0.62;
/// Most sweep steps (one per output pixel of depth up to this).
const MAX_STEPS: usize = 96;

/// An extrusion: its depth in points and its own colour, if set.
#[derive(Clone, Debug, PartialEq)]
pub struct Extrusion {
    pub depth: f64,
    pub color: Option<ColorRef>,
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let s = tag.find(&key)? + key.len();
    let rest = tag.get(s..)?;
    rest.get(..rest.find('"')?)
}

/// The extrusion in 3-D XML kept from the file (`a:sp3d extrusionH="…"` with an `a:scene3d`).
pub fn from_raw(raw: &str) -> Option<Extrusion> {
    if !raw.contains("scene3d") {
        return None;
    }
    let i = raw.find("sp3d ")?;
    let tag = raw.get(i..)?;
    let tag = tag.get(..tag.find('>')?)?;
    let depth = attr(tag, "extrusionH")?.parse::<f64>().ok()? / 12700.0;
    if !(depth.is_finite() && depth > 0.01) {
        return None;
    }
    let color = raw.find("extrusionClr").and_then(|j| {
        let body = raw.get(j..)?;
        let body = body.get(..body.find("/a:extrusionClr").or_else(|| body.find("/extrusionClr")).unwrap_or(body.len()))?;
        if let Some(k) = body.find("srgbClr ") {
            let v = attr(body.get(k..)?, "val")?;
            let n = u32::from_str_radix(v, 16).ok()?;
            Some(ColorRef::rgb(Rgba::rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)))
        } else if let Some(k) = body.find("schemeClr ") {
            SchemeSlot::from_xml(attr(body.get(k..)?, "val")?).map(ColorRef::scheme)
        } else {
            None
        }
    });
    Some(Extrusion { depth: depth.min(4000.0), color })
}

/// The camera's view of the depth axis: where the back face sits relative to the front face, in
/// the turned shape's space (the space `camera::affine` maps into).
pub fn back_offset(c: &Camera, depth: f64, w: f64, h: f64) -> Vec2 {
    let size = w.max(h);
    camera::project3(c, 0.0, 0.0, -depth, size) - camera::project3(c, 0.0, 0.0, 0.0, size)
}

/// The side colour: the extrusion colour, or the face's, shaded.
pub fn side_color(rctx: &Ctx, e: &Extrusion, face: Option<Rgba>) -> Rgba {
    let base = e.color.as_ref().map(|c| rctx.color(c, face)).or(face).unwrap_or(Rgba::rgb(0x80, 0x80, 0x80));
    let ch = |v: u8| (f64::from(v) * SIDE_SHADE).round().clamp(0.0, 255.0) as u8;
    Rgba { r: ch(base.r), g: ch(base.g), b: ch(base.b), a: base.a }
}

/// Sweep `outline` (shape space) from its back face to its front face. `pre` maps the turned
/// shape's space to the output and `turn` is the camera's turn, so the shape is drawn under
/// `pre * turn` and its back face under `pre * translate(offset) * turn`.
pub fn draw(ctx: &mut RenderContext, outline: &BezPath, pre: Affine, turn: Affine, offset: Vec2, side: Rgba) {
    let px = (pre * offset.to_point() - pre * kurbo::Point::ZERO).hypot();
    if !px.is_finite() || px < 0.25 {
        return;
    }
    let steps = (px.ceil() as usize).clamp(1, MAX_STEPS);
    ctx.set_paint(crate::color(side, 1.0));
    for i in (0..steps).rev() {
        let t = (i + 1) as f64 / steps as f64;
        ctx.set_transform(pre * Affine::translate(offset * t) * turn);
        ctx.fill_path(outline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_depth_and_colour() {
        let raw = r#"<a:scene3d><a:camera prst="isometricOffAxis2Left"/><a:lightRig rig="threePt" dir="t"/></a:scene3d><a:sp3d extrusionH="381000"><a:extrusionClr><a:srgbClr val="C00000"/></a:extrusionClr></a:sp3d>"#;
        let e = from_raw(raw).unwrap();
        assert!((e.depth - 30.0).abs() < 1e-9);
        assert_eq!(e.color, Some(ColorRef::rgb(Rgba::rgb(0xC0, 0, 0))));
        assert!(from_raw(r#"<a:sp3d extrusionH="381000"/>"#).is_none(), "no scene: drawn flat");
        assert!(from_raw(r#"<a:scene3d/><a:sp3d/>"#).is_none());
    }

    #[test]
    fn the_back_face_goes_away_from_the_viewer() {
        // Turned left about the vertical axis, the back face shows to the right... of the turned
        // face for a positive longitude, and nothing for a front camera.
        let c = Camera { lat: 0.0, lon: 30.0, rev: 0.0, dist: 0.0 };
        let o = back_offset(&c, 20.0, 100.0, 50.0);
        assert!((o.x.abs() - 10.0).abs() < 1e-9 && o.y.abs() < 1e-9, "{o:?}");
        let front = Camera { lat: 0.0, lon: 0.0, rev: 0.0, dist: 0.0 };
        assert!(back_offset(&front, 20.0, 100.0, 50.0).hypot() < 1e-9);
    }
}
