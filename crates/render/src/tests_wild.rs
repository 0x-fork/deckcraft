//! Render tests for shape, line, fill and effect cases checked against PowerPoint's own exports
//! of synthetic "wild" slides (observed black-box; see `plan/powerpoint/tools/gen_wild.py`).

use super::*;
use deckcraft_color::Rgba;
use deckcraft_model::style::ColorRef;
use deckcraft_model::{CustomPath, Geom, Presentation, Shape, ShapeId, Xfrm};

fn deck_with(shapes: Vec<Shape>) -> Presentation {
    let mut p = Presentation::default();
    let s = std::sync::Arc::make_mut(&mut p.slides[0]);
    s.shapes = shapes;
    p
}

fn render(shapes: Vec<Shape>) -> Image {
    render_slide(&deck_with(shapes), 0, &RenderOpts { scale: 1.0, ..Default::default() })
}

fn solid(c: Rgba) -> Option<Fill> {
    Some(Fill::solid(ColorRef::rgb(c)))
}

fn no_line() -> Option<Line> {
    Some(Line { fill: Some(Fill::None), ..Default::default() })
}

fn close(px: [u8; 4], c: Rgba, tol: i32) -> bool {
    (px[0] as i32 - c.r as i32).abs() <= tol && (px[1] as i32 - c.g as i32).abs() <= tol && (px[2] as i32 - c.b as i32).abs() <= tol
}

const WHITE: Rgba = Rgba::WHITE;
const RED: Rgba = Rgba::rgb(255, 0, 0);

#[test]
fn custom_geometry_sub_paths_cut_holes_whichever_way_they_wind() {
    // Outer square clockwise, inner square also clockwise: PowerPoint still shows a hole.
    let d = "M 0 0 L 100 0 L 100 100 L 0 100 Z M 25 25 L 75 25 L 75 75 L 25 75 Z";
    let sh = Shape {
        id: ShapeId(10),
        xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 200.0)),
        geom: Geom::Custom { paths: vec![CustomPath { w: 100.0, h: 100.0, d: d.into(), ..Default::default() }] },
        fill: solid(RED),
        line: no_line(),
        ..Default::default()
    };
    let img = render(vec![sh]);
    assert!(close(img.pixel(120, 120), RED, 2), "outer ring filled");
    assert!(close(img.pixel(200, 200), WHITE, 2), "inner square is a hole: {:?}", img.pixel(200, 200));
}
