//! Text fields must show where they are: in the light theme the field was as white as the dialog,
//! pane or ribbon it sits on and had no outline, so only its label (or hint) was visible (#1204).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use deckcraft_engine::Session;
use egui::accesskit::Role;
use egui::epaint::Shape;
use egui::{Color32, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp, theme};

fn harness(app: SlideApp) -> Harness<'static, SlideApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    // The app keeps asking for frames, so step instead of run().
    h.run_steps(3);
    h
}

/// The largest per-channel difference between two colours.
fn distance(a: Color32, b: Color32) -> u8 {
    a.r().abs_diff(b.r()).max(a.g().abs_diff(b.g())).max(a.b().abs_diff(b.b()))
}

/// The frame egui painted for the text field at `field` (its fill and outline).
fn frame_at(h: &Harness<'_, SlideApp>, field: Rect) -> (Color32, egui::Stroke) {
    h.output()
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            Shape::Rect(r) if r.rect.min.distance(field.min) < 2.0 && r.rect.max.distance(field.max) < 2.0 => Some((r.fill, r.stroke)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no frame painted for the field at {field:?}"))
}

/// The field stands apart from `surface` by its fill or by its outline.
fn assert_visible(h: &Harness<'_, SlideApp>, what: &str, surface: Color32) {
    let fields: Vec<Rect> = h.query_all_by_role(Role::TextInput).map(|n| n.rect()).collect();
    assert!(!fields.is_empty(), "{what}: no text field");
    for field in fields {
        let (fill, stroke) = frame_at(h, field);
        let outlined = stroke.width >= 1.0 && distance(stroke.color, surface) >= 24;
        assert!(
            outlined || distance(fill, surface) >= 16,
            "{what}: the field at {field:?} (fill {fill:?}, outline {stroke:?}) can't be told from the {surface:?} behind it"
        );
    }
}

#[test]
fn light_text_fields_show_in_dialogs_panes_and_the_ribbon() {
    let light = theme::Tokens::light();

    // Home › Find: "Find what:" in the dialog card.
    let mut h = harness(SlideApp::new(Session::with_new(), Services::default()));
    h.state_mut().run("app.find", json!({})).unwrap();
    h.run_steps(3);
    assert_visible(&h, "Find and Replace", h.ctx.global_style().visuals.window_fill);
    // Still visible with the keyboard in it.
    let field = h.get_by_role(Role::TextInput).rect();
    h.get_by_role(Role::TextInput).click();
    h.run_steps(2);
    assert!(h.ctx.memory(|m| m.focused().is_some()), "the click puts the keyboard in the field");
    let (fill, stroke) = frame_at(&h, field);
    assert!(distance(fill, light.card) >= 16 || distance(stroke.color, light.card) >= 24, "focused field: fill {fill:?}, outline {stroke:?}");

    // Review › Thesaurus: the search box on the task pane.
    let mut h = harness(SlideApp::new(Session::with_new(), Services::default()));
    h.state_mut().ui.pane = Some("thesaurus".into());
    h.run_steps(3);
    assert_visible(&h, "Thesaurus pane", light.card);

    // Transitions › Duration: the spinner on the ribbon.
    let mut app = SlideApp::new(Session::with_new(), Services::default());
    app.ui.tab = "Transitions".into();
    let h = harness(app);
    assert_visible(&h, "Transitions ribbon", light.card);
}
