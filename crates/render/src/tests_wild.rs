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

fn dotted_line(cap: deckcraft_model::style::LineCap, len: f64) -> Shape {
    Shape {
        id: ShapeId(11),
        xfrm: Some(Xfrm::new(100.0, 100.0, len, 0.0)),
        geom: Geom::preset("line"),
        line: Some(Line {
            fill: solid(RED),
            width: Some(10.0),
            dash: Some(deckcraft_model::style::Dash::Preset { name: "sysDot".into() }),
            cap: Some(cap),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn round_dots_are_separate_circles_with_the_caps_inside_each_dash() {
    use deckcraft_model::style::LineCap;
    // PowerPoint's "Round Dot": one width on, one off, round caps. The caps sit inside the dash, so
    // the dots stay apart (they used to grow into each other and draw a solid line).
    let img = render(vec![dotted_line(LineCap::Round, 200.0)]);
    assert!(close(img.pixel(100, 100), RED, 40), "a dot centred on the start {:?}", img.pixel(100, 100));
    assert!(close(img.pixel(120, 100), RED, 40), "the next dot one period on");
    assert!(close(img.pixel(110, 100), WHITE, 10), "a gap between dots {:?}", img.pixel(110, 100));
    // Flat caps: each dash is a width long, from the start.
    let img = render(vec![dotted_line(LineCap::Flat, 200.0)]);
    assert!(close(img.pixel(105, 100), RED, 40) && close(img.pixel(115, 100), WHITE, 10));
    // Square caps: dashes end flat; only the line's own start is squared off, half a width back.
    let img = render(vec![dotted_line(LineCap::Square, 200.0)]);
    assert!(close(img.pixel(97, 100), RED, 40), "square cap before the start {:?}", img.pixel(97, 100));
    assert!(close(img.pixel(105, 100), RED, 40) && close(img.pixel(115, 100), WHITE, 10), "dashes keep their length");
    assert!(close(img.pixel(122, 100), RED, 40) && close(img.pixel(118, 100), WHITE, 10), "no cap on the next dash");
    // The end: 200 pt is ten whole periods, so the line ends in a gap and gets no cap.
    assert!(close(img.pixel(302, 100), WHITE, 10), "{:?}", img.pixel(302, 100));
    // 205 pt ends half way through a dash: squared off past the end.
    let img = render(vec![dotted_line(LineCap::Square, 205.0)]);
    assert!(close(img.pixel(308, 100), RED, 40), "{:?}", img.pixel(308, 100));
}

#[test]
fn dash_lengths_make_room_for_round_and_square_caps() {
    assert_eq!(dash_with_caps(vec![4.0, 3.0], kurbo::Cap::Round, 1.0), vec![3.0, 4.0]);
    assert_eq!(dash_with_caps(vec![1.0, 1.0], kurbo::Cap::Round, 2.0), vec![0.002, 3.0]);
    assert_eq!(dash_with_caps(vec![1.0, 1.0], kurbo::Cap::Square, 2.0), vec![1.0, 1.0]);
    assert_eq!(dash_with_caps(vec![4.0, 3.0], kurbo::Cap::Butt, 1.0), vec![4.0, 3.0]);
    assert_eq!(dash_with_caps(vec![4.0, 3.0, 1.0], kurbo::Cap::Round, 1.0), vec![4.0, 3.0, 1.0]);
}
