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
