//! The Spelling dialog with a dictionary: suggestions change the word, Ignore All hides it.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use deckcraft_engine::Session;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp};

#[test]
fn spelling_dialog_changes_and_ignores_words() {
    let aff = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzxjq\nSFX S Y 1\nSFX S 0 s .\n";
    let dic = "6\nthe/S\nslide/S\nquarter/S\nplan/S\nour\nfor\n";
    deckcraft_engine::spell::install("en-US", deckcraft_engine::spell::Dictionary::parse(aff.as_bytes(), dic.as_bytes()));
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.session.execute("shape.insert", &json!({"preset": "rect", "rect": [100, 100, 500, 120], "text": "Our plna for teh quarter"})).unwrap();
    app.dialog = Some(crate::dialogs::Dialog::new("spelling"));
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 700.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    if let (Ok(path), Ok(img)) = (std::env::var("DECKCRAFT_PNG_OUT"), h.render()) {
        let _ = img.save(path);
    }
    h.get_by_label("plan").click();
    h.run_steps(3);
    let text = |h: &Harness<'_, SlideApp>| {
        let st = h.state().session.active().unwrap();
        let mut out = String::new();
        deckcraft_model::walk(&st.doc.slides[0].shapes, &mut |s, _| {
            if let Some(t) = &s.text {
                out += &t.text();
            }
        });
        out
    };
    assert!(text(&h).contains("Our plan for teh"), "{}", text(&h));
    h.get_by_label("Ignore All").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("No issues found").is_some());
}

/// Misspelt words on the slide get squiggles from the background check; not the word being
/// typed; the context menu's correction and Add to Dictionary clear them.
#[test]
fn squiggles_mark_misspelt_words_in_the_background() {
    let aff = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzxjq\nSFX S Y 1\nSFX S 0 s .\n";
    let dic = "7\nthe/S\nslide/S\nquarter/S\nplan/S\nour\nfor\nreview\n";
    deckcraft_engine::spell::install("xx-SQ", deckcraft_engine::spell::Dictionary::parse(aff.as_bytes(), dic.as_bytes()));
    let dir = std::env::temp_dir().join(format!("deckcraft-ui-userdic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    deckcraft_engine::spell::set_user_dictionary_path(dir.join("user.dic"));
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.session.execute("slide.layout", &json!({"layout": "blank"})).ok();
    let id = app
        .session
        .execute("shape.insert", &json!({"preset": "rect", "rect": [80, 120, 700, 160], "text": "Our plna for teh quarter reveiw"}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    app.session.execute("format.size", &json!({"size": 40})).unwrap();
    // Mark the runs' language directly (what a PPTX run's `lang` gives).
    let sid = deckcraft_model::ShapeId(id as u32);
    app.session
        .edit(|doc, _| {
            let sl = std::sync::Arc::make_mut(&mut doc.slides[0]);
            let sh = deckcraft_model::find_shape_mut(&mut sl.shapes, sid).unwrap();
            for r in sh.text.as_mut().unwrap().paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
                r.props.lang = Some("xx-SQ".into());
            }
            Ok(())
        })
        .unwrap();
    app.session.execute("edit.select", &json!({"ids": []})).unwrap();
    let st = app.session.active().unwrap().clone();
    let lines = app.squiggles.settle(&st);
    assert_eq!(lines.len(), 3, "{lines:?}");
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 700.0)).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    if let (Ok(path), Ok(img)) = (std::env::var("DECKCRAFT_PNG_OUT_SQUIGGLES"), h.render()) {
        let _ = img.save(path);
    }
    // The context menu's word under the first squiggle, and its correction.
    let st = h.state().session.active().unwrap().clone();
    let (a, _) = lines[0];
    let w = h.state().squiggles.word_at(&st, deckcraft_geom::Point::new(a.x + 5.0, a.y - 5.0), 2.0).unwrap();
    assert_eq!(w.word, "plna");
    assert_eq!(w.suggestions.first().map(String::as_str), Some("plan"));
    h.state_mut()
        .session
        .execute("review.spellingChange", &json!({"shape": id, "para": w.para, "start": w.start, "word": w.word, "to": "plan"}))
        .unwrap();
    h.state_mut().session.execute("review.addToDictionary", &json!({"word": "reveiw"})).unwrap();
    let st = h.state().session.active().unwrap().clone();
    assert_eq!(h.state_mut().squiggles.settle(&st).len(), 1);
    // Typing at the end of a misspelt word: no squiggle under it until the caret leaves.
    h.state_mut().session.execute("edit.select", &json!({"ids": [id]})).unwrap();
    h.state_mut().session.execute("text.edit", &json!({})).unwrap();
    h.state_mut().session.execute("text.select", &json!({"anchor": [0, 16], "caret": [0, 16]})).unwrap();
    let st = h.state().session.active().unwrap().clone();
    assert_eq!(st.selection.text.as_ref().map(|t| t.caret), Some((0, 16)));
    assert_eq!(h.state_mut().squiggles.settle(&st).len(), 0, "the word being typed isn't marked");
    let _ = std::fs::remove_dir_all(&dir);
}
