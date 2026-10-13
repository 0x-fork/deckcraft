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

fn arrow_line(kind: &str, width: f64, c: Rgba) -> Shape {
    let end = deckcraft_model::style::LineEnd { kind: kind.into(), w: "med".into(), len: "med".into() };
    Shape {
        id: ShapeId(12),
        xfrm: Some(Xfrm::new(100.0, 100.0, 200.0, 0.0)),
        geom: Geom::preset("line"),
        line: Some(Line { fill: solid(c), width: Some(width), tail: Some(end), ..Default::default() }),
        ..Default::default()
    }
}

#[test]
fn a_wide_line_stops_inside_its_arrowhead() {
    // 10 pt line, medium triangle (30 pt long, 30 wide) whose tip is the line's end at x = 300.
    let img = render(vec![arrow_line("triangle", 10.0, RED)]);
    assert!(close(img.pixel(297, 100), RED, 40), "the tip {:?}", img.pixel(297, 100));
    // 2 pt back from the tip the triangle is 2 pt wide: the line's corner must not show there.
    assert!(close(img.pixel(298, 104), WHITE, 10), "line corner past the head {:?}", img.pixel(298, 104));
}

#[test]
fn a_see_through_line_and_its_arrowhead_are_one_shape() {
    let half_red = Rgba::rgba(255, 0, 0, 128);
    let img = render(vec![arrow_line("triangle", 10.0, half_red)]);
    let line_only = img.pixel(200, 100);
    let head_over_line = img.pixel(285, 100);
    assert!(close(line_only, Rgba::rgb(255, 127, 127), 6), "{line_only:?}");
    assert!(close(head_over_line, Rgba::rgb(line_only[0], line_only[1], line_only[2]), 3), "no darker overlap: {head_over_line:?}");
}

#[test]
fn an_open_arrowheads_point_is_on_the_lines_end() {
    let img = render(vec![arrow_line("arrow", 8.0, RED)]);
    let reach = (290..320).rev().find(|x| img.pixel(*x, 100)[1] < 128);
    assert!(reach.is_some_and(|x| x <= 300), "point reaches {reach:?}");
    assert!(reach.is_some_and(|x| x >= 297), "point reaches {reach:?}");
}

#[test]
fn arrowheads_are_sized_for_at_least_a_two_point_line() {
    let e = deckcraft_model::style::LineEnd { kind: "triangle".into(), w: "sm".into(), len: "lg".into() };
    assert_eq!(head_size(&e, 0.5), (4.0, 10.0));
    assert_eq!(head_size(&e, 4.0), (8.0, 20.0));
}

#[test]
fn trimming_a_path_leaves_its_ends_off_by_arc_length() {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((50.0, 0.0));
    p.line_to((50.0, 50.0));
    let t = trim_ends(&p, 10.0, 20.0);
    let els = t.elements();
    assert_eq!(els.first(), Some(&PathEl::MoveTo(Point::new(10.0, 0.0))));
    match els.last() {
        Some(PathEl::LineTo(q)) => assert!((q.x - 50.0).abs() < 1e-6 && (q.y - 30.0).abs() < 1e-6, "{q:?}"),
        other => panic!("{other:?}"),
    }
    // Trims longer than the path leave nothing; closed paths are untouched.
    assert!(trim_ends(&p, 60.0, 50.0).elements().is_empty());
    let r = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
    assert_eq!(trim_ends(&r, 3.0, 3.0).elements().len(), r.elements().len());
}
