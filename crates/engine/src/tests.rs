use serde_json::json;

use crate::*;

fn session() -> Session {
    Session::with_new()
}

#[test]
fn new_presentation_and_slides() {
    let mut s = session();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 1);
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
    assert_eq!(s.doc().unwrap().selection.slide, 2);
    s.execute("slide.duplicate", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 4);
    s.execute("slide.move", &json!({"from": 3, "to": 0})).unwrap();
    s.execute("slide.delete", &json!({"index": 0})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 4);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 3);
}

#[test]
fn text_transform_sets_and_clears_a_warp() {
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [80.0, 80.0, 420.0, 160.0], "text": "ARCH"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    let body = |s: &mut Session| s.execute("shape.inspect", &json!({"id": id})).unwrap()["shape"]["text"]["body"].clone();
    assert!(s.execute("format.textWarp", &json!({})).is_err());
    assert!(s.execute("format.textWarp", &json!({"preset": "notAWarp"})).is_err());
    assert!(s.execute("format.textWarp", &json!({"preset": "textArchUp", "adj": "wide"})).is_err());
    assert!(s.execute("format.textWarp", &json!({"preset": "textArchUp", "adj": [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]})).is_err());
    assert!(s.execute("format.textWarp", &json!({"preset": "textArchUp", "adj": [null]})).is_err());
    assert!(body(&mut s)["warp"].is_null());
    s.execute("format.textWarp", &json!({"preset": "textArchUp"})).unwrap();
    assert_eq!(body(&mut s)["warp"], "textArchUp");
    assert!(body(&mut s).get("warpAdj").is_none());
    s.execute("format.textWarp", &json!({"preset": "textInflate", "adj": [20_000.0]})).unwrap();
    let b = body(&mut s);
    assert_eq!(b["warp"], "textInflate");
    assert_eq!(b["warpAdj"], json!([20_000.0]));
    s.execute("format.textWarp", &json!({"preset": "none"})).unwrap();
    assert!(body(&mut s)["warp"].is_null());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(body(&mut s)["warp"], "textInflate");
    s.execute("text.edit", &json!({"id": id})).unwrap();
    s.execute("format.textWarp", &json!({"preset": "textCircle"})).unwrap();
    assert_eq!(body(&mut s)["warp"], "textCircle");
}

#[test]
fn insert_shape_and_format() {
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "roundRect", "rect": [100, 100, 200, 100], "text": "Hi"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.execute("shape.fill", &json!({"color": "#ff0000"})).unwrap();
    s.execute("shape.line", &json!({"width": 3, "color": "accent2", "dash": "dash"})).unwrap();
    s.execute("shape.rotate", &json!({"deg": 30})).unwrap();
    s.execute("shape.move", &json!({"dx": 10, "dy": -5})).unwrap();
    s.execute("format.bold", &json!({})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 110.0);
    assert_eq!(v["box"]["rot"], 30.0);
    let runs = &v["shape"]["text"]["paragraphs"][0]["runs"][0]["props"];
    assert_eq!(runs["bold"], true);
    // Undo restores step by step.
    for _ in 0..5 {
        s.execute("edit.undo", &json!({})).unwrap();
    }
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert!(v["shape"]["fill"].is_null());
}

#[test]
fn text_editing_flow() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.move", &json!({"to": "wordLeft", "extend": true})).unwrap();
    s.execute("format.italic", &json!({})).unwrap();
    s.execute("text.move", &json!({"to": "end"})).unwrap();
    s.execute("text.delete", &json!({"dir": "backward"})).unwrap();
    s.execute("text.insert", &json!({"text": "D\nSecond"})).unwrap();
    let t = s.execute("text.get", &json!({"id": title})).unwrap();
    assert_eq!(t["text"], "Hello worlD\nSecond");
    s.execute("text.exit", &json!({})).unwrap();
    assert!(s.doc().unwrap().selection.text.is_none());
    assert_eq!(s.doc().unwrap().selection.shapes.len(), 1);
}

#[test]
fn editing_text_refits_a_saved_shrink() {
    use deckcraft_model::text::AutoFit;
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id;
    s.execute("text.edit", &json!({"id": title.0})).unwrap();
    s.execute("text.insert", &json!({"text": "Hi"})).unwrap();
    // As if opened from a file PowerPoint saved shrunk.
    s.edit(|doc, sel| {
        let shapes = crate::shapes_mut(doc, sel).unwrap();
        let sh = deckcraft_model::find_shape_mut(shapes, title).unwrap();
        sh.text.as_mut().unwrap().body.autofit = Some(AutoFit::Shrink { font_scale: 0.5, line_reduction: 0.2 });
        Ok(())
    })
    .unwrap();
    s.execute("text.insert", &json!({"text": "!"})).unwrap();
    let sh = s.doc().unwrap().shape(title).cloned().unwrap();
    assert_eq!(sh.text.unwrap().body.autofit, Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 }));
}

#[test]
fn smart_quotes_and_autocorrect() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    for c in ["\"", "h", "i", "\"", " ", "t", "e", "h", " "] {
        s.execute("text.insert", &json!({"text": c})).unwrap();
    }
    let t = s.execute("text.get", &json!({"id": title})).unwrap();
    assert_eq!(t["text"], "“hi” the ");
}

#[test]
fn clipboard_copy_paste_shapes_and_slides() {
    let mut s = session();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [10, 10, 50, 50]})).unwrap();
    s.execute("edit.copy", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    s.execute("edit.paste", &json!({})).unwrap();
    let n = s.doc().unwrap().current_slide().unwrap().shapes.len();
    assert_eq!(n, 2 + 3);
    s.execute("edit.copy", &json!({"scope": "slides"})).unwrap();
    s.execute("edit.paste", &json!({"scope": "slides"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), 2);
    assert!(s.doc().unwrap().doc.validate().is_empty());
}

#[test]
fn group_ungroup_roundtrip_positions() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300, 200, 50, 50]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    let g = s.execute("arrange.group", &json!({})).unwrap()["id"].as_u64().unwrap();
    s.execute("shape.move", &json!({"id": g, "dx": 10})).unwrap();
    s.execute("arrange.ungroup", &json!({"ids": [g]})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": b})).unwrap();
    assert!((v["box"]["x"].as_f64().unwrap() - 310.0).abs() < 1e-6, "{v}");
}

#[test]
fn align_and_distribute() {
    let mut s = session();
    let mut ids = vec![];
    for (x, y) in [(10.0, 10.0), (200.0, 50.0), (500.0, 90.0)] {
        ids.push(s.execute("shape.insert", &json!({"preset": "rect", "rect": [x, y, 40, 40]})).unwrap()["id"].as_u64().unwrap());
    }
    s.execute("edit.select", &json!({"ids": ids})).unwrap();
    s.execute("arrange.align", &json!({"edge": "top"})).unwrap();
    s.execute("arrange.distribute", &json!({"dir": "horizontal"})).unwrap();
    let ys: Vec<f64> = ids.iter().map(|i| s.execute("shape.inspect", &json!({"id": i})).unwrap()["box"]["y"].as_f64().unwrap()).collect();
    assert!(ys.iter().all(|y| (*y - 10.0).abs() < 1e-6));
    let x1 = s.execute("shape.inspect", &json!({"id": ids[1]})).unwrap()["box"]["x"].as_f64().unwrap();
    assert!((x1 - 255.0).abs() < 1e-6, "{x1}");
}

#[test]
fn pointer_draw_move_resize() {
    let mut s = session();
    s.set_tool(ToolKind::Shape { preset: "rect".into() });
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down, 100.0, 100.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 200.0, 180.0)).unwrap();
    let r = s.pointer(ev(PointerKind::Up, 200.0, 180.0)).unwrap();
    let id = r["id"].as_u64().unwrap();
    assert_eq!(s.tool.kind, ToolKind::Select);
    // Move by dragging the middle.
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    s.pointer(ev(PointerKind::Down, 150.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 170.0, 150.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 170.0, 150.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 120.0);
    assert_eq!(v["box"]["y"], 110.0);
    // One undo step per gesture.
    s.execute("edit.undo", &json!({})).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["x"], 100.0);
    // Resize from the SE handle.
    s.execute("edit.select", &json!({"ids": [id]})).unwrap();
    s.pointer(ev(PointerKind::Down, 200.0, 180.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 250.0, 200.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 250.0, 200.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": id})).unwrap();
    assert_eq!(v["box"]["w"], 150.0);
}

#[test]
fn click_into_placeholder_starts_editing() {
    let mut s = session();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::Down, 480.0, 200.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 480.0, 200.0)).unwrap();
    assert!(s.doc().unwrap().selection.text.is_some());
    s.execute("text.insert", &json!({"text": "Typed"})).unwrap();
    assert_eq!(s.doc().unwrap().current_slide().unwrap().title(), "Typed");
}

#[test]
fn sample_deck_builds_and_renders() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let st = s.doc().unwrap();
    assert!(st.doc.slides.len() >= 8);
    assert!(st.doc.validate().is_empty());
    assert!(!st.is_dirty());
    for i in 0..st.doc.slides.len() {
        let img = deckcraft_render::render_slide(&st.doc, i, &deckcraft_render::RenderOpts { scale: 0.2, ..Default::default() });
        assert_eq!(img.width, 192);
    }
    let v = s.execute("document.inspect", &json!({})).unwrap();
    assert!(v["slides"].as_array().unwrap().len() >= 8);
}

#[test]
fn writable_format_for_path_rejects_formats_deckcraft_does_not_write() {
    use crate::cmd::file::writable_format_for_path as f;
    assert_eq!(f("a.deckcraft"), Some("deckcraft"));
    assert_eq!(f("dir/A.PPTX"), Some("pptx"));
    assert_eq!(f("a.pdf"), Some("pdf"));
    assert_eq!(f("a.jpg"), Some("jpeg"));
    assert_eq!(f("a.txt"), Some("outline"));
    for path in ["a.odp", "a.gif", "a.svg", "a.outline", "a", "dir.v2/a"] {
        assert_eq!(f(path), None, "{path}");
    }
}

#[test]
fn save_and_reopen_native() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    let bytes = s.execute("file.saveBytes", &json!({})).unwrap()["data"].as_str().unwrap().to_string();
    let n = s.doc().unwrap().doc.slides.len();
    s.execute("file.openBytes", &json!({"name": "x.deckcraft", "data": bytes})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slides.len(), n);
    assert_eq!(s.documents().len(), 2);
}

#[test]
fn every_command_survives_empty_and_hostile_params() {
    let mut s = Session::new();
    sample::open_sample(&mut s).unwrap();
    s.execute("edit.select", &json!({"ids": [s.doc().unwrap().current_slide().unwrap().shapes[0].id.0]})).unwrap();
    let ids: Vec<&str> = command_specs()
        .iter()
        .map(|c| c.id)
        .filter(|id| !matches!(*id, "file.close" | "file.open" | "file.save" | "file.saveAs" | "file.export" | "file.saveTemplate"))
        .collect();
    for p in [
        json!({}),
        json!(null),
        json!([1, 2]),
        json!({"index": 99999, "ids": [999999], "id": -1, "text": "x", "rect": "bad", "color": 5, "size": 1e308}),
    ] {
        for id in &ids {
            let _ = s.execute(id, &p);
        }
    }
    assert!(s.active().is_some());
    for d in s.documents() {
        assert!(d.doc.validate().is_empty(), "{:?}", d.doc.validate());
    }
}

#[test]
fn base64_roundtrip() {
    for data in [vec![], vec![0u8], vec![1, 2], vec![1, 2, 3], (0..=255u8).collect::<Vec<_>>()] {
        assert_eq!(cmd::base64_decode(&cmd::base64_encode(&data)).unwrap(), data);
    }
    assert!(cmd::base64_decode("!!").is_none());
}

#[test]
fn layout_change_keeps_text() {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "titleAndContent", "title": "T", "body": "B"})).unwrap();
    s.execute("slide.layout", &json!({"layout": "twoContent"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    let texts: Vec<String> = v["shapes"].as_array().unwrap().iter().filter_map(|x| x["text"].as_str().map(String::from)).collect();
    assert!(texts.contains(&"T".to_string()) && texts.contains(&"B".to_string()), "{v}");
    assert_eq!(v["layout"], "Two Content");
}

#[test]
fn themes_and_slide_size() {
    let mut s = session();
    s.execute("design.theme", &json!({"name": "Ember"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.masters[0].theme.name, "Ember");
    s.execute("design.slideSize", &json!({"preset": "standard"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.slide_size.width, 720.0);
    s.execute("design.slideSize", &json!({"preset": "widescreen", "orientation": "portrait"})).unwrap();
    assert_eq!((s.doc().unwrap().doc.slide_size.width, s.doc().unwrap().doc.slide_size.height), (540.0, 960.0));
    s.execute("design.slideSize", &json!({"w": 540.0, "h": 960.0, "orientation": "landscape"})).unwrap();
    assert_eq!((s.doc().unwrap().doc.slide_size.width, s.doc().unwrap().doc.slide_size.height), (960.0, 540.0));
    assert!(s.execute("design.slideSize", &json!({"preset": "standard", "orientation": "sideways"})).is_err());
    s.execute("design.headerFooter", &json!({"slideNumber": true, "footer": true, "footerText": "Hello"})).unwrap();
    let v = s.execute("slide.inspect", &json!({})).unwrap();
    assert!(v["shapes"].as_array().unwrap().iter().any(|x| x["placeholder"] == "sldNum"));
}

fn line_ends(s: &Session, id: u64) -> (deckcraft_geom::Point, deckcraft_geom::Point) {
    let st = s.doc().unwrap();
    let sh = st.shape(deckcraft_model::ShapeId(id as u32)).unwrap();
    connect::endpoints(&cmd::xfrm_of(&st.doc, &st.selection, sh))
}

#[test]
fn connectors_follow_glued_shapes() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 300, 100, 50]})).unwrap()["id"].as_u64().unwrap();
    let r = s.execute("shape.connect", &json!({"from": a, "to": b, "preset": "bentConnector3"})).unwrap();
    let c = r["id"].as_u64().unwrap();
    let (p0, p1) = line_ends(&s, c);
    // Closest pair: right side of A → left side of B... or bottom/top; either way on the boxes.
    let sites_a: Vec<Vec<f64>> = serde_json::from_value(s.execute("shape.sites", &json!({"id": a})).unwrap()).unwrap();
    assert!(sites_a.iter().any(|q| (q[0] - p0.x).abs() < 1e-6 && (q[1] - p0.y).abs() < 1e-6));
    // Move B: the connector's end follows, its start stays.
    s.execute("edit.select", &json!({"ids": [b]})).unwrap();
    s.execute("shape.move", &json!({"dx": 50, "dy": 20})).unwrap();
    let (q0, q1) = line_ends(&s, c);
    assert!((q0 - p0).hypot() < 1e-6);
    assert!(((q1 - p1) - deckcraft_geom::Vec2::new(50.0, 20.0)).hypot() < 1e-6, "{p1:?} → {q1:?}");
    // Undo puts both back in one step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((line_ends(&s, c).1 - p1).hypot() < 1e-6);
    // Deleting a glued shape unglues that end and leaves the line where it was.
    s.execute("edit.select", &json!({"ids": [a]})).unwrap();
    s.execute("edit.delete", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let sh = st.shape(deckcraft_model::ShapeId(c as u32)).unwrap();
    assert!(matches!(sh.kind, deckcraft_model::ShapeKind::Connector { start: None, end: Some(_) }));
}

#[test]
fn drawing_a_line_between_shapes_glues_it() {
    let mut s = session();
    let a = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [100, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [400, 100, 100, 100]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.deselect", &json!({})).unwrap();
    s.tool.kind = tools::ToolKind::Shape { preset: "straightConnector1".into() };
    let ev = |kind, x: f64, y: f64| tools::PointerEvent { kind, x, y, mods: Default::default(), tol: 4.0 };
    // Right site of the ellipse (200,150) → left site of the rect (400,150), a little off.
    s.pointer(ev(tools::PointerKind::Down, 202.0, 151.0)).unwrap();
    s.pointer(ev(tools::PointerKind::Drag, 300.0, 150.0)).unwrap();
    assert!(!s.tool.sites.is_empty() || s.tool.glue.is_none());
    s.pointer(ev(tools::PointerKind::Drag, 398.0, 149.0)).unwrap();
    assert!(s.tool.glue.is_some());
    s.pointer(ev(tools::PointerKind::Up, 398.0, 149.0)).unwrap();
    let st = s.doc().unwrap();
    let line = st.shapes().iter().find(|x| x.is_line()).unwrap();
    match line.kind {
        deckcraft_model::ShapeKind::Connector { start: Some((x, _)), end: Some((y, _)) } => {
            assert_eq!((x.0 as u64, y.0 as u64), (a, b));
        }
        ref k => panic!("not glued: {k:?}"),
    }
    let id = line.id.0 as u64;
    let (p0, p1) = line_ends(&s, id);
    assert!((p0.x - 200.0).abs() < 1e-6 && (p1.x - 400.0).abs() < 1e-6, "{p0:?} {p1:?}");
}

#[test]
fn freeform_tools_and_command() {
    let mut s = session();
    let before = s.doc().unwrap().shapes().len();
    // Composite command: one undo step.
    let r = s.execute("shape.freeform", &json!({"points": [[100, 100], [200, 120], [150, 220]], "closed": true})).unwrap();
    assert!(r["id"].as_u64().is_some());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), before);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().shapes().len(), before + 1);

    let ev = |kind, x: f64, y: f64| tools::PointerEvent { kind, x, y, mods: Default::default(), tol: 3.0 };
    // Polygon: click, click, click, click on the start → closed, filled.
    s.set_tool(tools::ToolKind::Shape { preset: "freeform".into() });
    for (x, y) in [(300.0, 300.0), (400.0, 300.0), (400.0, 400.0)] {
        s.pointer(ev(tools::PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(tools::PointerKind::Up, x, y)).unwrap();
    }
    assert!(s.tool.drawing_freeform());
    s.pointer(ev(tools::PointerKind::Down, 301.0, 301.0)).unwrap();
    assert!(!s.tool.drawing_freeform());
    let st = s.doc().unwrap();
    let sh = st.shapes().last().unwrap();
    match &sh.geom {
        deckcraft_model::Geom::Custom { paths } => assert!(paths[0].d.ends_with('Z') && paths[0].d.matches(" L ").count() == 2, "{}", paths[0].d),
        g => panic!("{g:?}"),
    }
    let x = sh.xfrm.unwrap();
    assert_eq!((x.x, x.y, x.w, x.h), (300.0, 300.0, 100.0, 100.0));

    // Scribble: one drag, open path.
    s.set_tool(tools::ToolKind::Shape { preset: "scribble".into() });
    s.pointer(ev(tools::PointerKind::Down, 10.0, 10.0)).unwrap();
    for i in 1..20 {
        s.pointer(ev(tools::PointerKind::Drag, 10.0 + i as f64 * 5.0, 10.0 + (i as f64).sin() * 10.0)).unwrap();
    }
    let r = s.pointer(ev(tools::PointerKind::Up, 110.0, 10.0)).unwrap();
    assert!(r["id"].as_u64().is_some());
    assert_eq!(s.tool.kind, tools::ToolKind::Select);

    // Curve: clicks then Enter → smooth open path.
    s.set_tool(tools::ToolKind::Shape { preset: "curve".into() });
    for (x, y) in [(500.0, 100.0), (550.0, 50.0), (600.0, 100.0), (650.0, 50.0)] {
        s.pointer(ev(tools::PointerKind::Down, x, y)).unwrap();
        s.pointer(ev(tools::PointerKind::Up, x, y)).unwrap();
    }
    let r = s.key("Enter", Default::default()).unwrap();
    let id = deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let st = s.doc().unwrap();
    match &st.shape(id).unwrap().geom {
        deckcraft_model::Geom::Custom { paths } => assert_eq!(paths[0].d.matches(" C ").count(), 3),
        g => panic!("{g:?}"),
    }
}

#[test]
fn double_click_in_table_uses_grown_row_heights() {
    // Rows stored 4 pt tall are drawn tall enough for their text, so 6 pt down is still row 0.
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"], ["e", "f"]]);
    let id = s.execute("insert.table", &json!({"rows": 3, "cols": 2, "rect": [100.0, 100.0, 400.0, 120.0], "data": data})).unwrap()["id"].clone();
    for row in 0..3 {
        s.execute("table.rowHeight", &json!({"id": id, "row": row, "height": 4.0})).unwrap();
    }
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    s.pointer(ev(PointerKind::DoubleClick, 120.0, 106.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().and_then(|t| t.cell), Some((0, 0)));
}

#[test]
fn double_click_edits_text_inside_group() {
    // A text member of a group is still editable: double-click reaches it through the group
    // (regression: it used to be a no-op, so grouped text boxes could not be edited).
    let mut s = session();
    let a =
        s.execute("shape.insert", &json!({"preset": "rect", "rect": [100.0, 100.0, 120.0, 50.0], "text": "Hello"})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.insert", &json!({"preset": "rect", "rect": [300.0, 200.0, 50.0, 50.0]})).unwrap()["id"].as_u64().unwrap();
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    s.execute("arrange.group", &json!({})).unwrap();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    // Double-click the grouped text box: it becomes editable and targets that member.
    s.pointer(ev(PointerKind::DoubleClick, 160.0, 125.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.text.as_ref().map(|t| t.shape.0 as u64), Some(a));
}

#[test]
fn grown_table_frame_covers_its_drawn_rows() {
    // Rows stored 4 pt tall are drawn taller: the box, its handles and hit tests follow the drawn rows.
    let mut s = session();
    let data = json!([["a", "b"], ["c", "d"], ["e", "f"]]);
    let id = s.execute("insert.table", &json!({"rows": 3, "cols": 2, "rect": [100.0, 100.0, 400.0, 120.0], "data": data})).unwrap()["id"].clone();
    for row in 0..3 {
        s.execute("table.rowHeight", &json!({"id": id, "row": row, "height": 4.0})).unwrap();
    }
    let st = s.doc().unwrap();
    let sh = st.shapes().iter().find(|sh| json!(sh.id) == id).unwrap().clone();
    let deckcraft_model::ShapeKind::Table(t) = &sh.kind else { panic!("not a table") };
    let drawn: f64 = tools::table_rows(st, t).iter().sum();
    assert!(drawn > 12.0, "rows should grow: {drawn}");
    let x = cmd::xfrm_of(&st.doc, &st.selection, &sh);
    assert!((x.h - drawn).abs() < 1e-6, "frame {} vs drawn {drawn}", x.h);
    let below_stored = deckcraft_geom::Point::new(120.0, 100.0 + drawn - 2.0);
    assert!(matches!(s.hit_test(below_stored, 1.0), Some(tools::Hit::Shape { .. })));
}

#[test]
fn right_to_left_paragraphs() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "سنة 2024 (Retrait) فقط"})).unwrap();
    s.execute("format.rtl", &json!({})).unwrap();
    let para = |s: &Session| s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().unwrap().paragraphs[0].props.clone();
    assert_eq!(para(&s).rtl, Some(true));
    // Left (the default) becomes right: the text stays at its start edge.
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::Right));
    s.execute("format.align", &json!({"align": "justifyLow"})).unwrap();
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::JustLow));
    // Toggling back keeps a justified alignment.
    s.execute("format.rtl", &json!({})).unwrap();
    assert_eq!(para(&s).rtl, Some(false));
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::JustLow));
    s.execute("format.rtl", &json!({"on": true})).unwrap();
    s.execute("format.alignLeft", &json!({})).unwrap();
    s.execute("format.rtl", &json!({"on": false})).unwrap();
    assert_eq!(para(&s).align, Some(deckcraft_model::text::Align::Right));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(para(&s).rtl, Some(true));
}

#[test]
fn header_footer_fixed_date_is_the_slides_own_text() {
    use deckcraft_model::text::RunKind;
    let mut s = session();
    let date = |s: &Session| {
        let slide = s.doc().unwrap().current_slide().unwrap();
        slide.shapes.iter().find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date)).and_then(|x| x.text.clone()).unwrap()
    };
    let kind = |s: &Session| date(s).paragraphs[0].runs[0].kind.clone();
    // Automatic: a date field, saved with today's date.
    s.execute("design.headerFooter", &json!({"date": true})).unwrap();
    assert_eq!(kind(&s), RunKind::Field { field: "datetime1".into() });
    assert_eq!(date(&s).text().matches('/').count(), 2, "M/D/YYYY");
    // Fixed: plain text in the slide's own placeholder (other slides' fields aren't affected).
    s.execute("design.headerFooter", &json!({"date": true, "dateText": "Launch day"})).unwrap();
    assert_eq!((kind(&s), date(&s).text()), (RunKind::Text, "Launch day".to_string()));
    // Back to automatic.
    s.execute("design.headerFooter", &json!({"date": true, "dateText": ""})).unwrap();
    assert!(matches!(kind(&s), RunKind::Field { .. }));
    // A new date field is saved with the date it shows.
    s.execute("text.edit", &json!({"id": s.doc().unwrap().current_slide().unwrap().shapes[0].id.0})).unwrap();
    s.execute("text.insert", &json!({"text": "Updated "})).unwrap();
    s.execute("insert.dateTime", &json!({"format": "datetime10"})).unwrap();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
    let run = title.paragraphs[0].runs.iter().find(|r| matches!(r.kind, RunKind::Field { .. })).cloned().unwrap();
    assert!(run.text.contains(':'), "H:mm {run:?}");
}

/// (text, linked?) of each run in the first paragraph of the current slide's title.
fn title_runs(s: &Session) -> Vec<(String, bool)> {
    let sh = &s.doc().unwrap().current_slide().unwrap().shapes[0];
    let p = &sh.text.as_ref().unwrap().paragraphs[0];
    p.runs.iter().map(|r| (r.text.clone(), r.props.link.is_some())).collect()
}

#[test]
fn hyperlink_with_a_bare_caret_inserts_the_address() {
    // #31: with the caret at the end of the text, Insert Hyperlink used to format an empty range
    // and change nothing.
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "See "})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    s.execute("text.insert", &json!({"text": " now"})).unwrap();
    assert_eq!(
        title_runs(&s),
        vec![("See ".into(), false), ("https://example.org".into(), true), (" now".into(), false)],
        "the address is inserted as a link, and typing after it doesn't extend it"
    );
}

#[test]
fn hyperlink_display_text_and_targets() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "mailto:a@b.org"})).unwrap();
    assert_eq!(title_runs(&s), vec![("a@b.org".into(), true)], "email links show the address without mailto:");
    s.execute("text.insert", &json!({"text": " "})).unwrap();
    s.execute("insert.hyperlink", &json!({"slide": 0, "text": "start"})).unwrap();
    assert_eq!(title_runs(&s), vec![("a@b.org".into(), true), (" ".into(), false), ("start".into(), true)]);
}

#[test]
fn hyperlink_on_a_word_or_shape_links_it() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.move", &json!({"to": "left"})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    assert_eq!(title_runs(&s), vec![("Hello ".into(), false), ("world".into(), true)], "a caret inside a word links the word");
    s.execute("text.exit", &json!({})).unwrap();
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    assert!(s.doc().unwrap().current_slide().unwrap().shapes[0].click.is_some(), "a selected shape gets the link as its click action");
}

/// A .pptx (base64) whose first slide's title is set in `font`.
fn deck_in_font(font: &str) -> String {
    let mut p = deckcraft_model::Presentation::default();
    let slide = std::sync::Arc::make_mut(&mut p.slides[0]);
    let mut body = deckcraft_model::text::TextBody::from_text("Hello");
    body.paragraphs[0].runs[0].props.font = Some(font.into());
    slide.shapes[0].text = Some(body);
    crate::cmd::base64_encode(&crate::cmd::file::save_bytes(&p, "pptx").unwrap())
}

#[test]
fn opening_a_file_lists_the_fonts_it_uses_that_are_missing() {
    let mut s = Session::with_new();
    let r = s.execute("file.openBytes", &json!({"name": "a.pptx", "data": deck_in_font("Qqq Nonexistent Sans")})).unwrap();
    assert_eq!(r["missingFonts"], json!(["Qqq Nonexistent Sans"]));
    assert_eq!(s.doc().unwrap().missing_fonts, vec!["Qqq Nonexistent Sans".to_string()]);
    // A font that is there (bundled or installed) isn't listed; new documents list nothing.
    let have = deckcraft_fonts::FontDb::global().face(deckcraft_fonts::FALLBACK_FAMILY, "Regular").family.clone();
    let r = s.execute("file.openBytes", &json!({"name": "b.pptx", "data": deck_in_font(&have)})).unwrap();
    assert_eq!(r["missingFonts"], json!([]));
    s.execute("file.new", &json!({})).unwrap();
    assert!(s.doc().unwrap().missing_fonts.is_empty());
}

#[test]
fn an_embedded_face_is_used_when_the_machine_lacks_it() {
    let mut p = deckcraft_model::Presentation::default();
    let slide = std::sync::Arc::make_mut(&mut p.slides[0]);
    let mut body = deckcraft_model::text::TextBody::from_text("Hello");
    body.paragraphs[0].runs[0].props.font = Some("DeckCraft Colr Squares".into());
    slide.shapes[0].text = Some(body);
    let id = p.add_media("font1.fntdata", "application/x-fontdata", include_bytes!("../../render/tests/fixtures/colr-squares.ttf").to_vec());
    p.embedded_fonts.push(deckcraft_model::EmbeddedFont { typeface: "DeckCraft Colr Squares".into(), regular: Some(id), ..Default::default() });
    let data = crate::cmd::base64_encode(&crate::cmd::file::save_bytes(&p, "pptx").unwrap());
    let mut s = Session::with_new();
    let r = s.execute("file.openBytes", &json!({"name": "fonts.pptx", "data": data})).unwrap();
    assert_eq!(r["missingFonts"], json!([]));
    let font = s.doc().unwrap().doc.embedded_fonts.first().unwrap();
    assert_eq!(font.typeface, "DeckCraft Colr Squares");
    assert!(font.regular.is_some());
}

#[test]
fn clicks_in_flipped_shapes_hit_the_text_where_it_is_drawn() {
    // Flips don't mirror text, so a click lands on the character drawn under it.
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 400, 100], "text": "Hello flipped world"})).unwrap();
    let id = deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let pos = |s: &Session, x: f64, y: f64| {
        let st = s.doc().unwrap();
        tools::text_pos(st, st.shape(id).unwrap(), deckcraft_geom::Point::new(x, y))
    };
    let xs = [240.0, 300.0, 360.0];
    let plain = xs.map(|x| pos(&s, x, 150.0));
    assert!(plain[0].1 < plain[1].1 && plain[1].1 < plain[2].1, "{plain:?}");
    s.execute("shape.flip", &json!({"axis": "horizontal"})).unwrap();
    assert_eq!(xs.map(|x| pos(&s, x, 150.0)), plain);
    // flipV turns the text 180° about the box centre (300, 150).
    s.execute("shape.flip", &json!({"axis": "horizontal"})).unwrap();
    s.execute("shape.flip", &json!({"axis": "vertical"})).unwrap();
    assert_eq!(xs.map(|x| pos(&s, 600.0 - x, 150.0)), plain);
}

#[test]
fn inserted_hyperlinks_look_like_links() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Read the docs"})).unwrap();
    s.execute("text.move", &json!({"to": "wordLeft", "extend": true})).unwrap();
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], false);
    s.execute("insert.hyperlink", &json!({"url": "https://example.org"})).unwrap();
    // Underlined (and hlink-coloured: same resolution as files) without touching the run's own props.
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], true);
    let body = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
    let run = body.paragraphs[0].runs.iter().find(|r| r.props.link.is_some()).unwrap();
    assert!(run.props.fill.is_none() && run.props.underline.is_none(), "{run:?}");
    // Underline off is the run's own u="none", as in PowerPoint.
    s.execute("format.underline", &json!({})).unwrap();
    assert_eq!(s.execute("format.state", &json!({})).unwrap()["underline"], false);
}

/// #270: shapes off the slide (in the editor's pasteboard) are hit, selected, moved and marqueed
/// like any other; one can be dragged back onto the slide or out of it.
#[test]
fn shapes_off_the_slide_can_be_selected_and_moved() {
    let mut s = session();
    s.prefs.snap_to_grid = false;
    s.prefs.smart_guides = false;
    let off = s.execute("shape.insert", &json!({"preset": "rect", "rect": [1000.0, 100.0, 120.0, 80.0]})).unwrap()["id"].as_u64().unwrap();
    let half = s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [-60.0, 300.0, 120.0, 80.0]})).unwrap()["id"].as_u64().unwrap();
    // Inserting off the slide keeps the box where it was asked for.
    assert_eq!(s.execute("shape.inspect", &json!({"id": off})).unwrap()["box"]["x"], 1000.0);
    s.execute("edit.select", &json!({"ids": []})).unwrap();
    let ev = |kind, x, y| PointerEvent { kind, x, y, mods: Mods::default(), tol: 3.0 };
    // Hover and click wholly off the slide.
    assert!(matches!(s.hit_test(deckcraft_geom::Point::new(1060.0, 140.0), 3.0), Some(tools::Hit::Shape { id, .. }) if id.0 as u64 == off));
    s.pointer(ev(PointerKind::Down, 1060.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 1060.0, 140.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.iter().map(|i| i.0 as u64).collect::<Vec<_>>(), vec![off]);
    // Drag it further off, then back onto the slide.
    s.pointer(ev(PointerKind::Down, 1060.0, 140.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 1110.0, 160.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 1110.0, 160.0)).unwrap();
    let v = s.execute("shape.inspect", &json!({"id": off})).unwrap();
    assert_eq!((v["box"]["x"].as_f64(), v["box"]["y"].as_f64()), (Some(1050.0), Some(120.0)));
    s.pointer(ev(PointerKind::Down, 1110.0, 160.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 500.0, 160.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 500.0, 160.0)).unwrap();
    assert_eq!(s.execute("shape.inspect", &json!({"id": off})).unwrap()["box"]["x"], 440.0);
    // The off-slide half of a straddling shape is hit too.
    s.pointer(ev(PointerKind::Down, -30.0, 340.0)).unwrap();
    s.pointer(ev(PointerKind::Up, -30.0, 340.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.iter().map(|i| i.0 as u64).collect::<Vec<_>>(), vec![half]);
    // A marquee started on the pasteboard selects what it encloses there.
    s.pointer(ev(PointerKind::Down, -100.0, 250.0)).unwrap();
    s.pointer(ev(PointerKind::Drag, 80.0, 420.0)).unwrap();
    s.pointer(ev(PointerKind::Up, 80.0, 420.0)).unwrap();
    assert_eq!(s.doc().unwrap().selection.shapes.iter().map(|i| i.0 as u64).collect::<Vec<_>>(), vec![half]);
}

fn title_text(s: &Session) -> String {
    let sh = &s.doc().unwrap().current_slide().unwrap().shapes[0];
    sh.text.as_ref().map(|t| t.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

/// #101: Korean typed through an input method arrives as preedit updates (ㅎ → 하 → 한 → 한ㄱ →
/// 한그 → 한글) and a commit. The preedit is shown in place and replaced on each update; the
/// committed text is one undo step, and nothing on the way is an undo step of its own.
#[test]
fn ime_composition_replaces_its_preedit_and_commits_as_one_undo_step() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "A "})).unwrap();
    let undo0 = s.doc().unwrap().history.undo.len();
    for pre in ["ㅎ", "하", "한", "한ㄱ", "한그", "한글"] {
        s.compose_text(pre, None).unwrap();
        assert_eq!(title_text(&s), format!("A {pre}"), "the preedit is shown in place");
        let c = s.doc().unwrap().composition.clone().unwrap();
        assert_eq!((c.start, c.len), ((0, 2), pre.chars().count()));
        assert_eq!(s.doc().unwrap().selection.text.as_ref().unwrap().caret, (0, 2 + pre.chars().count()), "caret after the preedit");
        assert_eq!(s.doc().unwrap().history.undo.len(), undo0, "no undo step while composing");
    }
    s.commit_text("한글").unwrap();
    assert_eq!(title_text(&s), "A 한글");
    assert!(s.doc().unwrap().composition.is_none());
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1, "one undo step for the composition");
    assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Type Text");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "A ");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "A 한글");

    // Windows' Korean IME commits a syllable when the next begins: an empty preedit, the commit,
    // then a new preedit.
    s.compose_text("ㄴ", None).unwrap();
    s.compose_text("", None).unwrap();
    s.commit_text("ㄴ").unwrap();
    s.compose_text("ㅁ", None).unwrap();
    assert_eq!(title_text(&s), "A 한글ㄴㅁ");
    s.commit_text("마").unwrap();
    assert_eq!(title_text(&s), "A 한글ㄴ마");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "A 한글ㄴ");
}

/// #101: an empty preedit cancels the composition (Escape, or Backspace over its last jamo):
/// the text and the selection it replaced are as they were, with no undo step.
#[test]
fn ime_cancel_restores_the_text_and_selection() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.execute("text.insert", &json!({"text": "Hello world"})).unwrap();
    s.execute("text.select", &json!({"anchor": [0, 6], "caret": [0, 11]})).unwrap();
    let undo0 = s.doc().unwrap().history.undo.len();
    let rev = s.doc().unwrap().revision;
    s.compose_text("せかい", Some((0, 3))).unwrap();
    assert_eq!(title_text(&s), "Hello せかい", "the preedit replaces the selection");
    assert_eq!(s.doc().unwrap().composition.as_ref().unwrap().active, Some((0, 3)));
    s.compose_text("世界", Some((0, 9))).unwrap();
    assert_eq!(s.doc().unwrap().composition.as_ref().unwrap().active, Some((0, 2)), "clamped to the preedit");
    s.compose_text("", None).unwrap();
    assert_eq!(title_text(&s), "Hello world");
    let t = s.doc().unwrap().selection.text.clone().unwrap();
    assert_eq!((t.anchor, t.caret), ((0, 6), (0, 11)), "the selection is back");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0);
    assert!(s.doc().unwrap().revision > rev, "redrawn");
    // A commit of nothing without a composition is nothing.
    s.commit_text("").unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0);
    // A commit without a preedit (some input methods send only the result) is one step.
    s.commit_text("世界").unwrap();
    assert_eq!(title_text(&s), "Hello 世界");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1);
}

/// #101: anything else settles the composition first: its text stays and is its own undo step.
#[test]
fn ime_composition_is_kept_by_a_key_a_click_or_another_command() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    let undo0 = s.doc().unwrap().history.undo.len();
    s.compose_text("안", None).unwrap();
    s.key("Right", Mods::default()).unwrap();
    assert!(s.doc().unwrap().composition.is_none());
    assert_eq!(title_text(&s), "안");
    assert_eq!(s.doc().unwrap().history.undo.len(), undo0 + 1);

    s.compose_text("녕", None).unwrap();
    s.pointer(PointerEvent { kind: PointerKind::Move, x: 5.0, y: 5.0, mods: Mods::default(), tol: 4.0 }).unwrap();
    assert!(s.doc().unwrap().composition.is_some(), "hovering keeps composing");
    s.pointer(PointerEvent { kind: PointerKind::Down, x: 5.0, y: 5.0, mods: Mods::default(), tol: 4.0 }).unwrap();
    assert!(s.doc().unwrap().composition.is_none(), "a click settles it");
    assert_eq!(title_text(&s), "안녕");

    s.execute("text.edit", &json!({"id": title})).unwrap();
    s.compose_text("하", None).unwrap();
    s.execute("text.get", &json!({"id": title})).unwrap();
    assert!(s.doc().unwrap().composition.is_some(), "a query doesn't");
    s.execute("text.insert", &json!({"text": "!"})).unwrap();
    assert_eq!(title_text(&s), "안녕하!");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "안녕하");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(title_text(&s), "안녕");

    // Text changed under the composition (here behind its back): a new one starts, no panic.
    s.compose_text("ㄱ", None).unwrap();
    s.doc_mut().unwrap().composition.as_mut().unwrap().len = 99;
    s.compose_text("가", None).unwrap();
    assert!(title_text(&s).ends_with('가'));
    // Control characters (an Enter sent as text) never split the paragraph.
    s.commit_text("가\n\r\u{b}").unwrap();
    assert_eq!(s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().unwrap().paragraphs.len(), 1);
}

/// #101: the first preedit on a selected shape starts editing it and replaces its text, as typing
/// does; with nothing selected there is nowhere to compose.
#[test]
fn ime_on_a_selected_shape_replaces_its_text() {
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100], "text": "Old"})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.select(|_, sel| {
        sel.text = None;
        sel.shapes = vec![ShapeId(id as u32)];
    })
    .unwrap();
    assert_eq!(s.compose_text("", None).unwrap()["handled"], false, "an empty preedit doesn't start editing");
    s.compose_text("新", None).unwrap();
    s.commit_text("新しい").unwrap();
    let v = s.execute("text.get", &json!({"id": id})).unwrap();
    assert_eq!(v["text"], "新しい");
    s.select(|_, sel| {
        sel.text = None;
        sel.shapes.clear();
    })
    .unwrap();
    assert_eq!(s.compose_text("x", None).unwrap()["handled"], false);
    assert_eq!(s.commit_text("x").unwrap()["handled"], false);
}

/// #101: the composition is underlined in place, under the composed characters, with a thicker
/// line under the part being converted.
#[test]
fn ime_composition_underlines_follow_the_text() {
    let mut s = session();
    let title = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
    s.execute("text.edit", &json!({"id": title})).unwrap();
    assert!(cmd::text::composition_underlines(s.doc().unwrap()).is_empty());
    s.compose_text("にほんご", Some((2, 4))).unwrap();
    let u = cmd::text::composition_underlines(s.doc().unwrap());
    let thin: Vec<_> = u.iter().filter(|x| !x.2).collect();
    let thick: Vec<_> = u.iter().filter(|x| x.2).collect();
    assert_eq!((thin.len(), thick.len()), (1, 1), "{u:?}");
    let sh = s.doc().unwrap().current_slide().unwrap().shapes[0].clone();
    let x = cmd::xfrm_of(&s.doc().unwrap().doc, &s.doc().unwrap().selection, &sh);
    for (a, b, _) in &u {
        assert!(b.x > a.x && (a.y - b.y).abs() < 1e-9, "a horizontal line: {a:?} {b:?}");
        assert!(a.x >= x.x - 1.0 && b.x <= x.x + x.w + 1.0 && a.y > x.y && a.y < x.y + x.h, "inside the title box");
    }
    assert!(thick[0].0.x > thin[0].0.x && (thick[0].1.x - thin[0].1.x).abs() < 1e-6, "the converted part is the end");
    s.commit_text("日本語").unwrap();
    assert!(cmd::text::composition_underlines(s.doc().unwrap()).is_empty());
}

/// File › Info (#76): the active presentation's name, location, counts and properties.
#[test]
fn file_info_reports_the_presentation() {
    let mut s = session();
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.new", &json!({})).unwrap();
    s.execute("slide.go", &json!({"index": 1})).unwrap();
    s.execute("slide.hide", &json!({})).unwrap();
    s.execute("file.properties", &json!({"title": "Quarterly", "author": "Ada"})).unwrap();
    let v = s.execute("file.info", &json!({})).unwrap();
    assert_eq!(v["slides"], 3);
    assert_eq!(v["hiddenSlides"], 1);
    assert_eq!(v["path"], serde_json::Value::Null);
    assert_eq!(v["dirty"], true);
    assert_eq!(v["properties"]["title"], "Quarterly");
    assert_eq!(v["properties"]["author"], "Ada");
    assert!(v["title"].as_str().is_some_and(|t| !t.is_empty()));
    // A query: no undo step, and nothing changes.
    let before = s.doc().unwrap().history.undo.len();
    s.execute("file.info", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().history.undo.len(), before);
    // Saved to disk, it reports where and how big.
    let dir = std::env::temp_dir().join(format!("deckcraft-info-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Info test.pptx").to_string_lossy().to_string();
    s.execute("file.saveAs", &json!({"path": path})).unwrap();
    let v = s.execute("file.info", &json!({})).unwrap();
    assert_eq!(v["path"], path.as_str());
    assert_eq!(v["format"], "pptx");
    assert_eq!(v["dirty"], false);
    assert!(v["bytes"].as_u64().is_some_and(|b| b > 0));
    let _ = std::fs::remove_dir_all(&dir);
    // Nothing open: an error, not a panic.
    let mut empty = Session::new();
    assert!(empty.execute("file.info", &json!({})).is_err());
}

/// The Print page's Slides box (#76): PowerPoint-style ranges, 1-based.
#[test]
fn slide_ranges_parse_like_powerpoints_print_box() {
    use cmd::file::parse_slide_range as r;
    assert_eq!(r("1-3, 5", 6).unwrap(), vec![0, 1, 2, 4]);
    assert_eq!(r("3-1;2", 6).unwrap(), vec![0, 1, 2], "reversed ranges and repeats");
    assert_eq!(r(" 6 ", 6).unwrap(), vec![5]);
    assert!(r("7", 6).is_err(), "past the last slide");
    assert!(r("0", 6).is_err());
    assert!(r("a-b", 6).is_err());
    assert!(r(" , ", 6).is_err());
    assert!(r("1-99999999999999999999999", 6).is_err(), "hostile numbers are refused");
    // file.export takes the same text for a PDF.
    let mut s = session();
    s.execute("slide.new", &json!({})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    let one = cmd::file::pdf_bytes(&doc, &json!({"slides": "2"})).unwrap();
    let both = cmd::file::pdf_bytes(&doc, &json!({"slides": "1-2"})).unwrap();
    assert!(one.len() < both.len());
    assert!(cmd::file::pdf_bytes(&doc, &json!({"slides": "5"})).is_err());
    let opts = cmd::file::pdf_options(&json!({"slides": [0, 7, 1]}), 2).unwrap();
    assert_eq!(opts.slides, Some(vec![0, 1]), "indices past the end are dropped");
}

#[test]
fn cell_fill_walks_only_the_cells_a_huge_selection_covers() {
    // A selection may reach far past the table; filling it touches the real cells and nothing more.
    let mut s = session();
    let id = s.execute("insert.table", &json!({"rows": 2, "cols": 2, "rect": [100.0, 100.0, 200.0, 40.0]})).unwrap()["id"].clone();
    s.execute("table.selectCells", &json!({"id": id, "from": [1, 1], "to": [1_000_000_000u64, 1_000_000_000u64]})).unwrap();
    s.execute("table.cellFill", &json!({"none": true})).unwrap();
    let st = s.doc().unwrap();
    let sh = st.shapes().iter().find(|sh| json!(sh.id) == id).unwrap();
    let deckcraft_model::ShapeKind::Table(t) = &sh.kind else { panic!("not a table") };
    let fills: Vec<bool> = [(0, 0), (0, 1), (1, 0), (1, 1)].iter().map(|&(r, c)| t.cell(r, c).is_some_and(|x| x.fill.is_some())).collect();
    assert_eq!(fills, [false, false, false, true]);
}

/// Each slide as (title, [(level, bullet text)]).
fn outline_of(p: &deckcraft_model::Presentation) -> Vec<(String, Vec<(u8, String)>)> {
    p.slides
        .iter()
        .map(|s| {
            let body =
                s.shapes.get(1).and_then(|b| b.text.as_ref()).map(|t| t.paragraphs.iter().map(|p| (p.level, p.text())).collect()).unwrap_or_default();
            (s.title(), body)
        })
        .collect()
}

const MD_OUTLINE: &str = "# Intro\n- Point one\n- Point two\n  - Sub point\n\n# Second\n- More\n";

fn md_expected() -> Vec<(String, Vec<(u8, String)>)> {
    vec![
        ("Intro".into(), vec![(0, "Point one".into()), (0, "Point two".into()), (1, "Sub point".into())]),
        ("Second".into(), vec![(0, "More".into())]),
    ]
}

#[test]
fn markdown_outlines_take_headings_as_slides_and_lists_as_bullets() {
    let p = crate::cmd::file::open_presentation("outline.md", MD_OUTLINE.as_bytes()).unwrap();
    assert_eq!(outline_of(&p), md_expected());
    // numbered lists, `*` / `+` bullets, text under a heading, and closing #s
    let p = crate::cmd::file::open_presentation("o.md", b"## Plan ##\n1. First\n2) Second\n* star\n+ plus\nA plain line\n").unwrap();
    assert_eq!(
        outline_of(&p),
        vec![("Plan".into(), vec![(0, "First".into()), (0, "Second".into()), (0, "star".into()), (0, "plus".into()), (0, "A plain line".into())])]
    );
}

#[test]
fn slides_from_a_markdown_outline_file() {
    let mut s = session();
    s.execute("slide.fromOutline", &json!({"text": MD_OUTLINE, "markdown": true})).unwrap();
    let doc = &s.doc().unwrap().doc;
    assert_eq!(outline_of(doc)[1..], md_expected()[..]);
}

#[test]
fn plain_outlines_are_unchanged() {
    // .txt keeps the indentation format, and so does a .md without headings
    let indented = "Intro\n\tPoint one\n\t\tSub point\nSecond\n\tMore\n";
    let want =
        vec![("Intro".to_string(), vec![(0, "Point one".to_string()), (1, "Sub point".to_string())]), ("Second".into(), vec![(0, "More".into())])];
    for name in ["outline.txt", "outline.md"] {
        let p = crate::cmd::file::open_presentation(name, indented.as_bytes()).unwrap();
        assert_eq!(outline_of(&p), want, "{name}");
    }
    let p = crate::cmd::file::open_presentation("outline.txt", MD_OUTLINE.as_bytes()).unwrap();
    assert_eq!(p.slides.first().map(|s| s.title()).as_deref(), Some("# Intro"));
}

fn text_box_xfrm(s: &Session, id: u64) -> deckcraft_geom::Xfrm {
    let st = s.doc().unwrap();
    cmd::xfrm_of(&st.doc, &st.selection, st.shape(deckcraft_model::ShapeId(id as u32)).unwrap())
}

fn right_click(s: &mut Session, x: f64, y: f64) -> serde_json::Value {
    s.pointer(PointerEvent { kind: PointerKind::ContextClick, x, y, mods: Mods::default(), tol: 3.0 }).unwrap()
}

/// A blank slide with a rectangle A (100,100 200×100) and a text box B (400,100 200 wide),
/// nothing selected.
fn two_shapes() -> (Session, deckcraft_model::ShapeId, deckcraft_model::ShapeId) {
    let mut s = session();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    let a = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100.0, 100.0, 200.0, 100.0], "text": "Shape A"})).unwrap()["id"]
        .as_u64()
        .unwrap();
    let b = s.execute("insert.textBox", &json!({"rect": [400.0, 100.0, 200.0, 40.0], "text": "Box B"})).unwrap()["id"].as_u64().unwrap();
    s.select(|_, sel| {
        sel.shapes.clear();
        sel.text = None;
    })
    .unwrap();
    (s, deckcraft_model::ShapeId(a as u32), deckcraft_model::ShapeId(b as u32))
}

#[test]
fn right_click_selects_the_shape_under_the_pointer() {
    let (mut s, a, b) = two_shapes();
    let undo_before = s.doc().unwrap().history.undo.len();
    // An unselected shape: selected alone, no text editing, no drag.
    let r = right_click(&mut s, 150.0, 150.0);
    assert_eq!(r["selected"], json!([a]));
    let st = s.doc().unwrap();
    assert_eq!(st.selection.shapes, vec![a]);
    assert!(st.selection.text.is_none());
    assert!(!s.tool.dragging());
    s.pointer(PointerEvent { kind: PointerKind::Drag, x: 190.0, y: 190.0, mods: Mods::default(), tol: 3.0 }).unwrap();
    assert_eq!(text_box_xfrm(&s, a.0 as u64).x, 100.0);
    // A text box, which a left click would start editing: only selected.
    right_click(&mut s, 450.0, 115.0);
    let st = s.doc().unwrap();
    assert_eq!(st.selection.shapes, vec![b]);
    assert!(st.selection.text.is_none());
    // A multiple selection stays when the right click lands on one of its shapes.
    s.execute("edit.select", &json!({"ids": [a, b]})).unwrap();
    right_click(&mut s, 450.0, 115.0);
    assert_eq!(s.doc().unwrap().selection.shapes, vec![a, b]);
    // Empty slide: nothing selected (the slide menu).
    let r = right_click(&mut s, 700.0, 450.0);
    assert!(r["hit"].is_null(), "{r}");
    assert!(s.doc().unwrap().selection.shapes.is_empty());
    assert_eq!(s.doc().unwrap().history.undo.len(), undo_before);
}

#[test]
fn right_click_in_edited_text_keeps_the_text_selection() {
    let (mut s, a, b) = two_shapes();
    s.execute("text.edit", &json!({"id": a, "at": [0, 0]})).unwrap();
    s.execute("text.move", &json!({"to": "wordRight", "extend": true})).unwrap();
    let before = s.doc().unwrap().selection.text.clone().unwrap();
    assert!(before.is_range());
    // Anywhere in the box, even away from the selected word.
    let r = right_click(&mut s, 280.0, 185.0);
    assert_eq!(r["editing"], true);
    assert_eq!(s.doc().unwrap().selection.text.as_ref(), Some(&before));
    // The context menu's Copy and Cut then act on the selected text.
    assert_eq!(s.execute("edit.copy", &json!({})).unwrap()["text"].as_str().unwrap().trim(), "Shape");
    s.execute("edit.cut", &json!({})).unwrap();
    assert_eq!(s.execute("text.get", &json!({"id": a})).unwrap()["text"].as_str().unwrap().trim(), "A");
    // A right click on another shape leaves editing and selects that shape.
    right_click(&mut s, 450.0, 115.0);
    let st = s.doc().unwrap();
    assert!(st.selection.text.is_none());
    assert_eq!(st.selection.shapes, vec![b]);
}

mod copy_remap {
    use std::sync::Arc;

    use deckcraft_model::style::PictureFill;
    use deckcraft_model::{Animation, Shape, ShapeId, ShapeKind, Xfrm};
    use serde_json::json;

    use crate::*;

    /// Slide 0 of `s` gets a picture of `bytes`, a box, and a connector from the picture to the box.
    /// Returns (picture, box, connector).
    fn populate(s: &mut Session, bytes: &[u8]) -> (ShapeId, ShapeId, ShapeId) {
        let st = s.doc_mut().unwrap();
        let doc = Arc::make_mut(&mut st.doc);
        let media = doc.add_media("p.png", "image/png", bytes.to_vec());
        let (p, b, c) = (ShapeId(doc.alloc_id()), ShapeId(doc.alloc_id()), ShapeId(doc.alloc_id()));
        let x = Some(Xfrm::new(10.0, 10.0, 50.0, 50.0));
        let slide = Arc::make_mut(&mut doc.slides[0]);
        slide.shapes.push(Shape {
            id: p,
            kind: ShapeKind::Picture { fill: PictureFill { media, ..Default::default() } },
            xfrm: x,
            ..Default::default()
        });
        slide.shapes.push(Shape { id: b, xfrm: x, ..Default::default() });
        slide.shapes.push(Shape { id: c, kind: ShapeKind::Connector { start: Some((p, 1)), end: Some((b, 3)) }, xfrm: x, ..Default::default() });
        (p, b, c)
    }

    fn select(s: &mut Session, ids: &[ShapeId]) {
        let sel = &mut s.doc_mut().unwrap().selection;
        sel.slides.clear();
        sel.shapes = ids.to_vec();
    }

    fn shape(s: &Session, slide: usize, id: ShapeId) -> Shape {
        let d = s.doc().unwrap();
        d.doc.slides[slide].shapes.iter().find(|x| x.id == id).cloned().unwrap()
    }

    fn picture_bytes(s: &Session, sh: &Shape) -> Vec<u8> {
        let ShapeKind::Picture { fill } = &sh.kind else { panic!("not a picture") };
        s.doc().unwrap().doc.media(fill.media).unwrap().data.as_ref().clone()
    }

    #[test]
    fn paste_into_a_deck_with_a_colliding_media_id_shows_the_copied_picture() {
        let mut a = Session::with_new();
        let mut b = Session::with_new();
        let (pa, ..) = populate(&mut a, b"deck a picture");
        let (pb, ..) = populate(&mut b, b"deck b picture");
        assert_eq!(picture_bytes(&a, &shape(&a, 0, pa)), b"deck a picture");
        select(&mut a, &[pa]);
        a.execute("edit.copy", &json!({})).unwrap();
        b.clipboard = a.clipboard.clone();
        b.doc_mut().unwrap().selection.shapes.clear();
        let r = b.execute("edit.paste", &json!({})).unwrap();
        let new = ShapeId(r["ids"][0].as_u64().unwrap() as u32);
        assert_eq!(picture_bytes(&b, &shape(&b, 0, new)), b"deck a picture");
        // Deck b's own picture is untouched.
        assert_eq!(picture_bytes(&b, &shape(&b, 0, pb)), b"deck b picture");
    }

    #[test]
    fn copied_connectors_follow_copied_ends_and_detach_from_the_rest() {
        let mut s = Session::with_new();
        let (p, b, c) = populate(&mut s, b"x");
        // Both ends copied: the copy connects the copies.
        let r = s.execute("edit.duplicate", &json!({"ids": [p.0, b.0, c.0]})).unwrap();
        let ids: Vec<ShapeId> = r["ids"].as_array().unwrap().iter().map(|v| ShapeId(v.as_u64().unwrap() as u32)).collect();
        let ShapeKind::Connector { start, end } = shape(&s, 0, ids[2]).kind else { panic!() };
        assert_eq!((start, end), (Some((ids[0], 1)), Some((ids[1], 3))));
        // Only the connector copied: it doesn't stay glued to the originals.
        select(&mut s, &[c]);
        s.execute("edit.copy", &json!({})).unwrap();
        s.doc_mut().unwrap().selection.shapes.clear();
        let r = s.execute("edit.paste", &json!({})).unwrap();
        let only = ShapeId(r["ids"][0].as_u64().unwrap() as u32);
        let ShapeKind::Connector { start, end } = shape(&s, 0, only).kind else { panic!() };
        assert_eq!((start, end), (None, None));
        // The original keeps its ends.
        let ShapeKind::Connector { start, .. } = shape(&s, 0, c).kind else { panic!() };
        assert_eq!(start, Some((p, 1)));
    }

    fn triggered(s: &mut Session, target: ShapeId, trigger: ShapeId) {
        let st = s.doc_mut().unwrap();
        let doc = Arc::make_mut(&mut st.doc);
        Arc::make_mut(&mut doc.slides[0]).animations.push(Animation {
            shape: target,
            trigger: Some(trigger),
            effect: "fade".into(),
            ..Default::default()
        });
    }

    fn resolves(s: &Session, slide: usize) -> (ShapeId, ShapeId) {
        let d = s.doc().unwrap();
        let sl = &d.doc.slides[slide];
        let a = &sl.animations[0];
        let t = a.trigger.unwrap();
        assert!(sl.shapes.iter().any(|x| x.id == a.shape) && sl.shapes.iter().any(|x| x.id == t), "{a:?}");
        (a.shape, t)
    }

    #[test]
    fn duplicated_and_pasted_slides_retarget_animations_and_triggers() {
        let mut s = Session::with_new();
        let (p, b, _) = populate(&mut s, b"x");
        triggered(&mut s, p, b);
        s.execute("slide.duplicate", &json!({"index": 0})).unwrap();
        let (t, tr) = resolves(&s, 1);
        assert!(t != p && tr != b);
        // Pasted slide.
        s.execute("edit.copy", &json!({"scope": "slides"})).unwrap();
        s.execute("edit.paste", &json!({"scope": "slides"})).unwrap();
        let n = s.doc().unwrap().doc.slides.len();
        let (t2, tr2) = resolves(&s, n - 1);
        assert!(t2 != t && tr2 != tr);
        // Save and reload keeps them resolving.
        let bytes = crate::cmd::file::save_bytes(&s.doc().unwrap().doc, "deckcraft").unwrap();
        let back = deckcraft_format::load(&bytes).unwrap();
        for sl in &back.slides {
            for a in &sl.animations {
                assert!(sl.shapes.iter().any(|x| x.id == a.shape));
                assert!(a.trigger.is_none_or(|t| sl.shapes.iter().any(|x| x.id == t)));
            }
        }
    }
}

#[test]
fn font_add_refuses_non_fonts_and_rescan_reports_the_catalog() {
    let mut s = session();
    let not_font = crate::cmd::base64_encode(b"definitely not a font file");
    assert!(s.execute("font.add", &json!({"data": not_font})).is_err());
    assert!(s.execute("font.add", &json!({})).is_err());
    let r = s.execute("font.rescan", &json!({})).unwrap();
    assert!(r["families"].as_u64().unwrap() >= 1, "{r}");
}

/// The Print page's paper and orientation reach the PDF options; Letter portrait by default.
#[test]
fn pdf_options_take_paper_and_orientation() {
    let o = cmd::file::pdf_options(&json!({}), 1).unwrap();
    assert_eq!(o.paper, (612.0, 792.0));
    let o = cmd::file::pdf_options(&json!({"paper": "A4", "orientation": "landscape"}), 1).unwrap();
    assert_eq!(o.paper, (841.89, 595.28));
    let o = cmd::file::pdf_options(&json!({"paper": "legal"}), 1).unwrap();
    assert_eq!(o.paper, (612.0, 1008.0));
    assert!(cmd::file::pdf_options(&json!({"paper": "tabloid"}), 1).is_err());
    assert!(cmd::file::pdf_options(&json!({"orientation": "sideways"}), 1).is_err());
}

#[test]
fn shape_line_sets_compound_and_arrow_sizes() {
    // The Format Shape pane's Line page: compound type, arrowheads and their sizes.
    use deckcraft_model::style::Compound;
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "line", "rect": [100, 100, 200, 0]})).unwrap();
    s.execute("shape.line", &json!({"compound": "thickThin", "head": "oval", "tail": "triangle", "tailSize": "lg"})).unwrap();
    let line = |s: &Session| s.doc().unwrap().selected_shapes()[0].line.clone().unwrap();
    let l = line(&s);
    assert_eq!(l.compound, Some(Compound::ThickThin));
    assert_eq!(l.head.as_ref().map(|e| (e.kind.as_str(), e.w.as_str())), Some(("oval", "med")));
    assert_eq!(l.tail.as_ref().map(|e| (e.kind.as_str(), e.w.as_str(), e.len.as_str())), Some(("triangle", "lg", "lg")));
    // Changing the arrow keeps its size; an unknown size is ignored; "none" removes it.
    s.execute("shape.line", &json!({"tail": "stealth", "headSize": "huge"})).unwrap();
    let l = line(&s);
    assert_eq!(l.tail.as_ref().map(|e| (e.kind.as_str(), e.w.as_str())), Some(("stealth", "lg")));
    assert_eq!(l.head.as_ref().map(|e| e.w.as_str()), Some("med"));
    s.execute("shape.line", &json!({"head": "none", "compound": "single"})).unwrap();
    let l = line(&s);
    assert!(l.head.is_none());
    assert_eq!(l.compound, Some(Compound::Single));
}

#[test]
fn shape_effects_props_adjust_shadow_glow_and_reflection() {
    // The Format Shape pane's Effects page: shadow, glow and reflection details.
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100]})).unwrap();
    let fx = |s: &Session| s.doc().unwrap().selected_shapes()[0].effects.clone().unwrap_or_default();
    // No shadow yet: the props add an outer one and set it.
    s.execute("shape.effects", &json!({"shadowProps": {"blur": 10, "dist": 6, "dir": 405, "transparency": 0.25, "color": "accent2"}})).unwrap();
    let sw = fx(&s).outer_shadow.unwrap();
    assert_eq!((sw.blur, sw.dist, sw.dir), (10.0, 6.0, 45.0));
    assert!((sw.color.alpha() - 0.75).abs() < 1e-6);
    // A new colour keeps the transparency; an inner shadow is adjusted in place.
    s.execute("shape.effects", &json!({"shadowProps": {"color": "accent3"}})).unwrap();
    assert!((fx(&s).outer_shadow.unwrap().color.alpha() - 0.75).abs() < 1e-6);
    s.execute("shape.effects", &json!({"shadow": "inner"})).unwrap();
    s.execute("shape.effects", &json!({"shadowProps": {"blur": 3}})).unwrap();
    let f = fx(&s);
    assert!(f.outer_shadow.is_none());
    assert_eq!(f.inner_shadow.unwrap().blur, 3.0);
    // Glow: size, colour, transparency; size 0 removes it.
    s.execute("shape.effects", &json!({"glowProps": {"radius": 12, "transparency": 0.5}})).unwrap();
    let g = fx(&s).glow.unwrap();
    assert_eq!(g.radius, 12.0);
    assert!((g.color.alpha() - 0.5).abs() < 1e-6);
    s.execute("shape.effects", &json!({"glowProps": {"radius": 0}})).unwrap();
    assert!(fx(&s).glow.is_none());
    // Reflection: transparency, size, distance, blur; hostile values are clamped or ignored.
    s.execute("shape.effects", &json!({"reflectionProps": {"transparency": 0.3, "size": 2.0, "dist": 5, "blur": "x"}})).unwrap();
    let r = fx(&s).reflection.unwrap();
    assert!((r.start_alpha - 0.7).abs() < 1e-9);
    assert_eq!((r.end_pos, r.dist, r.blur), (1.0, 5.0, 0.5));
}

/// Text Fill › Transparency (#32): the text keeps its colour, own or inherited, and gets an alpha;
/// 0 makes it opaque again, and it is one undo step.
#[test]
fn text_transparency_keeps_the_colour_and_sets_alpha() {
    use deckcraft_model::{ColorTransform, Fill};
    let mut s = session();
    let r = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100], "text": "Hi"})).unwrap();
    let id = deckcraft_model::ShapeId(r["id"].as_u64().unwrap() as u32);
    let fill = |s: &Session| {
        let st = s.doc().unwrap();
        st.shape(id).unwrap().text.as_ref().unwrap().paragraphs[0].runs[0].props.fill.clone()
    };
    // Inherited colour: the run gets the colour it showed, now 25% transparent.
    s.execute("format.textTransparency", &json!({"value": 0.25})).unwrap();
    let Some(Fill::Solid { color }) = fill(&s) else { panic!("solid fill expected: {:?}", fill(&s)) };
    assert!((color.alpha() - 0.75).abs() < 1e-9, "{color:?}");
    // Its own colour stays; the alpha is replaced, not stacked.
    s.execute("format.color", &json!({"color": "#C00000"})).unwrap();
    s.execute("format.textTransparency", &json!({"value": 0.6})).unwrap();
    s.execute("format.textTransparency", &json!({"value": 0.5})).unwrap();
    let Some(Fill::Solid { color }) = fill(&s) else { panic!() };
    assert_eq!(color.base, deckcraft_model::ColorBase::Rgb { rgb: deckcraft_model::Rgba::rgb(0xC0, 0, 0) });
    assert_eq!(color.mods.iter().filter(|m| matches!(m, ColorTransform::Alpha(_))).count(), 1);
    assert!((color.alpha() - 0.5).abs() < 1e-9);
    s.execute("format.textTransparency", &json!({"value": 0})).unwrap();
    let Some(Fill::Solid { color }) = fill(&s) else { panic!() };
    assert!(color.mods.is_empty(), "opaque drops the alpha: {color:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    let Some(Fill::Solid { color }) = fill(&s) else { panic!() };
    assert!((color.alpha() - 0.5).abs() < 1e-9);
    assert!(s.execute("format.textTransparency", &json!({})).is_err());
}

#[test]
fn edit_points_turns_a_preset_into_a_freeform_and_moves_adds_deletes() {
    use deckcraft_model::Geom;
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100]})).unwrap();
    let r = s.execute("shape.editPoints", &json!({})).unwrap();
    let id = r["editing"].as_u64().unwrap();
    assert!(matches!(s.doc().unwrap().selected_shapes()[0].geom, Geom::Custom { .. }));
    let pts =
        |s: &mut Session| -> Vec<[f64; 2]> { serde_json::from_value(s.execute("shape.points", &json!({})).unwrap()["paths"][0].clone()).unwrap() };
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6;
    let v = pts(&mut s);
    assert_eq!(v.len(), 4, "{v:?}");
    assert!(near(v[0], [100.0, 100.0]) && near(v[2], [300.0, 200.0]));
    // Drag the bottom-right corner out: the box grows to fit, other points stay on the slide.
    s.execute("shape.movePoint", &json!({"index": 2, "x": 350, "y": 260})).unwrap();
    let v = pts(&mut s);
    assert!(near(v[0], [100.0, 100.0]) && near(v[2], [350.0, 260.0]) && near(v[3], [100.0, 200.0]), "{v:?}");
    let x = s.doc().unwrap().selected_shapes()[0].xfrm.unwrap();
    assert!((x.w - 250.0).abs() < 1e-6 && (x.h - 160.0).abs() < 1e-6);
    // Move the first point up-left: the box moves with it.
    s.execute("shape.movePoint", &json!({"index": 0, "x": 80, "y": 90})).unwrap();
    let x = s.doc().unwrap().selected_shapes()[0].xfrm.unwrap();
    assert!((x.x - 80.0).abs() < 1e-6 && (x.y - 90.0).abs() < 1e-6);
    assert!(near(pts(&mut s)[0], [80.0, 90.0]));
    // Add a point in the middle of the closing edge (from the last point back to the first) and
    // of the top edge, then delete one.
    s.execute("shape.addPoint", &json!({"segment": 0})).unwrap();
    assert_eq!(pts(&mut s).len(), 5);
    s.execute("shape.addPoint", &json!({"segment": 1, "t": 0.5})).unwrap();
    let v = pts(&mut s);
    assert_eq!(v.len(), 6);
    assert!(near(v[1], [(80.0 + 300.0) / 2.0, (90.0 + 100.0) / 2.0]), "{v:?}");
    s.execute("shape.deletePoint", &json!({"index": 1})).unwrap();
    s.execute("shape.deletePoint", &json!({"index": 0})).unwrap();
    assert_eq!(pts(&mut s).len(), 4);
    // Bad input is an error, not a panic; an outline keeps two points.
    assert!(s.execute("shape.movePoint", &json!({"index": 99, "x": 1, "y": 1})).is_err());
    assert!(s.execute("shape.movePoint", &json!({"index": 0, "x": f64::NAN, "y": 1})).is_err());
    s.execute("shape.deletePoint", &json!({"index": 0})).unwrap();
    s.execute("shape.deletePoint", &json!({"index": 0})).unwrap();
    assert!(s.execute("shape.deletePoint", &json!({"index": 0})).is_err());
    // Each edit is one undo step.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(pts(&mut s).len(), 3);
    // It saves as custom geometry.
    let bytes = cmd::file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap();
    s.execute("file.openBytes", &json!({"name": "e.pptx", "data": cmd::base64_encode(&bytes)})).unwrap();
    let back = s.doc().unwrap().doc.slides.iter().flat_map(|sl| sl.shapes.iter()).find(|sh| sh.id.0 == id as u32).map(|sh| sh.geom.clone());
    assert!(matches!(back, Some(Geom::Custom { .. })), "{back:?}");
}

#[test]
fn edit_points_with_the_pointer_and_curves() {
    use crate::tools::{Mods, PointerEvent, PointerKind};
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "ellipse", "rect": [100, 100, 200, 100]})).unwrap();
    s.execute("shape.editPoints", &json!({"on": true})).unwrap();
    let pts =
        |s: &mut Session| -> Vec<[f64; 2]> { serde_json::from_value(s.execute("shape.points", &json!({})).unwrap()["paths"][0].clone()).unwrap() };
    let v = pts(&mut s);
    assert!(v.len() >= 4, "{v:?}");
    let ev = |kind, x: f64, y: f64, cmd: bool| PointerEvent { kind, x, y, mods: Mods { cmd, ..Default::default() }, ..Default::default() };
    // Drag the first vertex 30 pt left: one undo step.
    let [x0, y0] = v[0];
    s.pointer(ev(PointerKind::Down, x0, y0, false)).unwrap();
    s.pointer(ev(PointerKind::Drag, x0 - 15.0, y0, false)).unwrap();
    s.pointer(ev(PointerKind::Drag, x0 - 30.0, y0, false)).unwrap();
    s.pointer(ev(PointerKind::Up, x0 - 30.0, y0, false)).unwrap();
    let w = pts(&mut s);
    assert!((w[0][0] - (x0 - 30.0)).abs() < 1e-3 && (w[0][1] - y0).abs() < 1e-3, "{w:?}");
    assert!(w[1..].iter().zip(&v[1..]).all(|(a, b)| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3));
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((pts(&mut s)[0][0] - x0).abs() < 1e-3);
    // Ctrl-click a vertex deletes it.
    let n = pts(&mut s).len();
    let [x1, y1] = pts(&mut s)[1];
    s.pointer(ev(PointerKind::Down, x1, y1, true)).unwrap();
    s.pointer(ev(PointerKind::Up, x1, y1, true)).unwrap();
    assert_eq!(pts(&mut s).len(), n - 1);
    // Escape leaves Edit Points; then a click on empty slide doesn't touch the shape.
    s.key("Escape", Mods::default()).unwrap();
    assert!(s.tool.edit_points.is_none());
}

#[test]
fn edit_points_path_parse_and_write() {
    use crate::cmd::points::{Seg, parse, vertices, write};
    let segs = parse("M 0 0 L 10 0 C 10 5 5 10 0 10 Z junk L");
    // The closing edge is made explicit; its end is not a vertex of its own.
    assert_eq!(segs.len(), 5);
    assert!(matches!(segs[3], Seg::L(_)));
    assert_eq!(vertices(&segs), vec![0, 1, 2]);
    assert_eq!(write(&parse(&write(&segs))), write(&segs));
    assert!(parse("C 1 2 3").is_empty());
}

#[test]
fn edit_points_types_and_handles() {
    use crate::tools::{Mods, PointerEvent, PointerKind};
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100]})).unwrap();
    s.execute("shape.editPoints", &json!({"on": true})).unwrap();
    let q = |s: &mut Session| s.execute("shape.points", &json!({})).unwrap();
    assert_eq!(q(&mut s)["types"][0][1], "corner");
    // A rectangle's corner has no handles until it is a smooth point.
    let id = deckcraft_model::ShapeId(q(&mut s)["id"].as_u64().unwrap() as u32);
    let handles = |s: &Session| {
        let st = s.doc().unwrap();
        crate::cmd::points::slide_handles(&st.doc, &st.selection, st.shape(id).unwrap(), 0, 1).unwrap()
    };
    assert_eq!((handles(&s).0, handles(&s).1), (None, None));
    assert!(s.execute("shape.moveHandle", &json!({"index": 1, "side": "out", "x": 0, "y": 0})).is_err());
    s.execute("shape.pointType", &json!({"index": 1, "type": "smooth"})).unwrap();
    assert_eq!(q(&mut s)["types"][0][1], "smooth");
    let (hi, ho, v) = handles(&s);
    let (hi, ho) = (hi.unwrap(), ho.unwrap());
    assert!(((hi - v).hypot() - (ho - v).hypot()).abs() < 1e-6);
    assert!(((hi - v).normalize() + (ho - v).normalize()).hypot() < 1e-6);
    // Dragging one handle of a smooth point mirrors the other.
    let to = v + deckcraft_geom::Vec2::new(40.0, 10.0);
    s.execute("shape.moveHandle", &json!({"index": 1, "side": "out", "x": to.x, "y": to.y})).unwrap();
    let (hi, ho, v) = handles(&s);
    assert!((ho.unwrap() - to).hypot() < 1e-6 || q(&mut s)["types"][0][1] == "smooth");
    assert!(((hi.unwrap() - v) + (ho.unwrap() - v)).hypot() < 1e-6);
    // Straight: in line, lengths kept; corner: independent.
    s.execute("shape.pointType", &json!({"index": 1, "type": "straight"})).unwrap();
    let (_, ho, v) = handles(&s);
    let far = v + (ho.unwrap() - v) * 3.0;
    s.execute("shape.moveHandle", &json!({"index": 1, "side": "out", "x": far.x, "y": far.y, "type": "straight"})).unwrap();
    assert_eq!(q(&mut s)["types"][0][1], "straight");
    s.execute("shape.pointType", &json!({"index": 1, "type": "corner"})).unwrap();
    let (hi0, ho, v) = handles(&s);
    let off = v + deckcraft_geom::Vec2::new(0.0, -30.0);
    let _ = ho;
    // (The pointer passes the type last chosen for the vertex, as here.)
    s.execute("shape.moveHandle", &json!({"index": 1, "side": "out", "x": off.x, "y": off.y, "type": "corner"})).unwrap();
    let (hi1, _, v1) = handles(&s);
    assert!(((hi1.unwrap() - v1) - (hi0.unwrap() - v)).hypot() < 1e-6);
    assert_eq!(q(&mut s)["types"][0][1], "corner");
    assert!(s.execute("shape.pointType", &json!({"index": 1, "type": "wobbly"})).is_err());
    // With the pointer: click a vertex to show its handles, drag one (one undo step).
    s.execute("shape.pointType", &json!({"index": 2, "type": "smooth"})).unwrap();
    let st = s.doc().unwrap();
    let (_, ho, v) = crate::cmd::points::slide_handles(&st.doc, &st.selection, st.shape(id).unwrap(), 0, 2).unwrap();
    let ho = ho.unwrap();
    let ev = |kind, x: f64, y: f64| PointerEvent { kind, x, y, mods: Mods::default(), ..Default::default() };
    s.pointer(ev(PointerKind::Down, v.x, v.y)).unwrap();
    s.pointer(ev(PointerKind::Up, v.x, v.y)).unwrap();
    assert_eq!(s.tool.edit_vertex, Some((0, 2)));
    s.pointer(ev(PointerKind::Down, ho.x, ho.y)).unwrap();
    s.pointer(ev(PointerKind::Drag, ho.x + 20.0, ho.y + 20.0)).unwrap();
    s.pointer(ev(PointerKind::Up, ho.x + 20.0, ho.y + 20.0)).unwrap();
    let st = s.doc().unwrap();
    let (_, ho2, _) = crate::cmd::points::slide_handles(&st.doc, &st.selection, st.shape(id).unwrap(), 0, 2).unwrap();
    assert!((ho2.unwrap() - (ho + deckcraft_geom::Vec2::new(20.0, 20.0))).hypot() < 1e-3, "{ho2:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let (_, ho3, _) = crate::cmd::points::slide_handles(&st.doc, &st.selection, st.shape(id).unwrap(), 0, 2).unwrap();
    assert!((ho3.unwrap() - ho).hypot() < 1e-3);
}

/// #392: a `.deckcraft` whose embedded picture is damaged opens as an untitled "(Repaired)"
/// copy with a message, so Save can't overwrite the original with the empty media.
#[test]
fn damaged_native_file_opens_as_repaired_copy() {
    let mut p = deckcraft_model::Presentation::default();
    p.add_media("a.png", "image/png", b"PICTURE-BYTES-0123456789".to_vec());
    let good = deckcraft_format::save(&p).unwrap();
    // PNGs are stored as they are, so flipping a byte breaks the entry's CRC.
    let mut bad = good.clone();
    let at = bad.windows(13).position(|w| w == b"PICTURE-BYTES").unwrap();
    bad[at] ^= 0xff;
    let dir = std::env::temp_dir().join(format!("deckcraft-392-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes, damaged) in [("good.deckcraft", &good, false), ("bad.deckcraft", &bad, true)] {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut s = Session::new();
        s.ui_requests.clear();
        let r = s.execute("file.open", &json!({"path": path.to_string_lossy()})).unwrap();
        let st = s.doc().unwrap();
        assert_eq!(r["damagedMedia"].as_array().map(|a| a.len()), Some(usize::from(damaged)), "{r}");
        assert_eq!(st.path.is_none(), damaged, "{name}");
        assert_eq!(st.title.ends_with("(Repaired)"), damaged);
        assert_eq!(s.ui_requests.iter().any(|u| matches!(u, crate::UiRequest::Message { .. })), damaged);
        // Bytes route too.
        let r = s.execute("file.openBytes", &json!({"name": name, "data": crate::cmd::base64_encode(bytes)})).unwrap();
        assert_eq!(r["damagedMedia"].as_array().map(|a| a.len()), Some(usize::from(damaged)));
    }
    assert_eq!(std::fs::read(dir.join("bad.deckcraft")).unwrap(), bad, "original untouched");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn motion_path_presets_have_paths_ids_and_round_trip() {
    use deckcraft_model::anim::{ANIMATIONS, AnimClass, preset_path};
    let paths: Vec<_> = ANIMATIONS.iter().filter(|a| a.2 == AnimClass::Path).collect();
    // Preset ids identify the effect when PowerPoint's file is read back: no two share one.
    let mut ids: Vec<u32> = paths.iter().map(|a| a.3).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate motion-path preset ids");
    assert!(n >= 40, "{n} motion paths");
    let mut s = session();
    let id = s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 50, 50]})).unwrap()["id"].clone();
    s.execute("animation.set", &json!({"ids": [id], "effect": "heart", "class": "path"})).unwrap();
    let a = s.doc().unwrap().doc.slides[s.doc().unwrap().selection.slide].animations[0].clone();
    assert_eq!(a.path, preset_path("heart"));
    let p = s.doc().unwrap().doc.as_ref().clone();
    let back = deckcraft_pptx::import(&deckcraft_pptx::export(&p).unwrap()).unwrap();
    let b = back.slides.iter().flat_map(|sl| &sl.animations).next().unwrap();
    assert_eq!((b.effect.as_str(), b.class), ("heart", AnimClass::Path));
    assert!(b.path.as_deref().is_some_and(|p| p.contains('L')));
}

/// Bullets and Numbering dialog: numbering takes a colour and a size like bullets do, and sizes
/// stay within PowerPoint's 25–400 %.
#[test]
fn numbering_takes_color_and_size() {
    let mut s = Session::with_new();
    s.execute("insert.textBox", &json!({"rect": [10, 10, 300, 100], "text": "One\nTwo"})).unwrap();
    s.execute("format.numbering", &json!({"on": true, "scheme": "romanUcPeriod", "start": 3, "color": "accent2", "size": 150})).unwrap();
    let para = |s: &Session| {
        let st = s.doc().unwrap();
        let id = st.selection.shapes[0];
        st.shape(id).unwrap().text.as_ref().unwrap().paragraphs[0].props.clone()
    };
    let pp = para(&s);
    assert!(matches!(&pp.bullet, Some(deckcraft_model::text::Bullet::AutoNum { scheme, start_at: 3 }) if scheme == "romanUcPeriod"));
    assert_eq!(pp.bullet_size, Some(1.5));
    assert_eq!(pp.bullet_color, Some(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Accent2)));
    s.execute("format.bullets", &json!({"on": true, "char": "★", "size": 9000})).unwrap();
    assert_eq!(para(&s).bullet_size, Some(4.0));
}

#[test]
fn paste_special_keeps_source_look_picture_or_text() {
    use deckcraft_model::style::{ColorBase, Fill};
    let mut s = session();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 200, 100], "text": "Q3 plan"})).unwrap();
    s.execute("shape.fill", &json!({"color": "accent1"})).unwrap();
    let old_accent = {
        let d = s.doc().unwrap();
        let sl = d.current_slide().unwrap();
        d.doc.master_for(sl).unwrap().0.scheme().get(deckcraft_color::SchemeSlot::Accent1)
    };
    s.execute("edit.copy", &json!({})).unwrap();
    s.execute("design.theme", &json!({"name": "Ember"})).unwrap();
    {
        let d = s.doc().unwrap();
        let sl = d.current_slide().unwrap();
        assert_ne!(d.doc.master_for(sl).unwrap().0.scheme().get(deckcraft_color::SchemeSlot::Accent1), old_accent);
    }
    let fill_of = |s: &Session, id: &Value| {
        let sid = deckcraft_model::ShapeId(id.as_u64().unwrap() as u32);
        s.doc().unwrap().shape(sid).unwrap().fill.clone()
    };

    let dest = s.execute("edit.pasteSpecial", &json!({"as": "destinationTheme"})).unwrap();
    assert!(matches!(fill_of(&s, &dest["ids"][0]), Some(Fill::Solid { color }) if matches!(color.base, ColorBase::Scheme { .. })));

    let keep = s.execute("edit.pasteSpecial", &json!({"as": "keepSource"})).unwrap();
    match fill_of(&s, &keep["ids"][0]) {
        Some(Fill::Solid { color }) => assert_eq!(color.base, ColorBase::Rgb { rgb: old_accent }),
        f => panic!("{f:?}"),
    }
    // The clipboard itself still holds the theme-following shape.
    assert!(matches!(&s.clipboard.shapes[0].fill, Some(Fill::Solid { color }) if matches!(color.base, ColorBase::Scheme { .. })));

    let pic = s.execute("edit.pasteSpecial", &json!({"as": "picture"})).unwrap();
    let pid = deckcraft_model::ShapeId(pic["id"].as_u64().or_else(|| pic["ids"][0].as_u64()).unwrap() as u32);
    let d = s.doc().unwrap();
    let sh = d.shape(pid).unwrap();
    let deckcraft_model::ShapeKind::Picture { fill } = &sh.kind else { panic!("{:?}", sh.kind) };
    assert_eq!(d.doc.media(fill.media).unwrap().content_type, "image/png");
    let x = sh.xfrm.unwrap();
    assert!((x.w - 216.0).abs() < 1.0 && (x.h - 116.0).abs() < 1.0, "{x:?}");

    let n = s.doc().unwrap().current_slide().unwrap().shapes.len();
    s.execute("edit.pasteSpecial", &json!({"as": "text"})).unwrap();
    let d = s.doc().unwrap();
    let last = d.current_slide().unwrap().shapes.last().unwrap();
    assert_eq!(d.current_slide().unwrap().shapes.len(), n + 1);
    assert_eq!(last.text.as_ref().unwrap().text(), "Q3 plan");
    assert!(s.execute("edit.pasteSpecial", &json!({"as": "bogus"})).is_err());
}

/// Image export keeps the whole slide: the scale is lowered so the picture fits the renderer's
/// largest side (past it the renderer clips, and big exports came out cropped).
#[test]
fn image_export_scale_fits_the_largest_picture() {
    let mut s = Session::with_new();
    let doc = s.doc().unwrap().doc.clone();
    assert_eq!(crate::cmd::file::image_scale(&doc, 2.0), 2.0);
    assert_eq!(crate::cmd::file::image_scale(&doc, f64::NAN), 1.0);
    let max = f64::from(deckcraft_render::MAX_SIDE);
    assert_eq!(crate::cmd::file::image_scale(&doc, 16.0), 16.0, "16× of 960 pt fits");
    s.execute("design.slideSize", &json!({"w": 4032, "h": 72})).unwrap();
    let doc = s.doc().unwrap().doc.clone();
    let k = crate::cmd::file::image_scale(&doc, 4.0);
    assert!(k * 4032.0 <= max + 1e-6 && k * 4032.0 > max - 1.0, "{k}");
}

/// Exporting every slide as images: when one file can't be written, the error says which
/// slides already were.
#[test]
fn a_failed_all_slides_export_says_what_was_written() {
    let dir = std::env::temp_dir().join(format!("deckcraft-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("deck2.png")).unwrap();
    let mut s = Session::with_new();
    s.execute("slide.new", &json!({})).unwrap();
    let e = s.execute("file.export", &json!({"path": dir.join("deck.png").to_string_lossy(), "all": true, "scale": 0.1})).unwrap_err().to_string();
    assert!(e.contains("slides 1–1 were written"), "{e}");
    assert!(dir.join("deck1.png").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Undo keeps a bounded amount of memory: deck-wide edits (each step holds a copy of every
/// slide) are dropped oldest first once the steps pass the byte budget, while ordinary small
/// edits still get the full step count.
#[test]
fn undo_history_is_capped_by_memory_as_well_as_steps() {
    let mut s = Session::with_new();
    for _ in 0..40 {
        s.execute("slide.new", &json!({})).unwrap();
    }
    let one_copy = crate::undo_cost(&s.doc().unwrap().doc, &Presentation::default());
    assert!(one_copy > 0);
    s.doc_mut().unwrap().history = History { limit: 500, byte_limit: one_copy * 10, ..Default::default() };
    for i in 0..30 {
        // Resizing the slides rescales every shape on every slide: a new copy of each.
        let w = if i % 2 == 0 { 720 } else { 960 };
        s.execute("design.slideSize", &json!({"w": w, "h": 540})).unwrap();
    }
    let n = s.doc().unwrap().history.undo.len();
    assert!((5..=11).contains(&n), "{n} deck-wide steps kept");
    // Still undoable, newest first.
    s.execute("edit.undo", &json!({})).unwrap();
    // Small edits: each keeps one slide's copy, so many fit.
    let mut s = Session::with_new();
    s.doc_mut().unwrap().history.byte_limit = one_copy * 10;
    for _ in 0..120 {
        s.execute("slide.new", &json!({})).unwrap();
    }
    assert_eq!(s.doc().unwrap().history.undo.len(), 120);
}
