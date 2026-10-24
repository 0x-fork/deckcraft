//! The Columns dialog: Home › Columns › More Columns… and Format Shape › Text Box › Columns… open it
//! with the text box's values, and OK sets the number and spacing in one undo step.

use deckcraft_engine::Session;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::{Services, SlideApp};

fn harness() -> Harness<'static, SlideApp> {
    let mut s = Session::with_new();
    s.execute("shape.insert", &json!({"preset": "rect", "rect": [80.0, 80.0, 320.0, 160.0], "text": "Hello"})).unwrap();
    let _ = s.execute("text.exit", &json!({}));
    let app = SlideApp::new(s, Services::default());
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut SlideApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn columns_of(h: &Harness<'_, SlideApp>) -> (Option<u32>, Option<f64>) {
    let st = h.state().session.active().unwrap();
    let body = st.selected_shapes().first().and_then(|sh| sh.text.as_ref()).map(|t| t.body.clone()).unwrap_or_default();
    (body.columns, body.col_spacing)
}

fn field(h: &Harness<'_, SlideApp>, key: &str) -> Option<String> {
    h.state().dialog.as_ref().and_then(|d| d.fields.get(key).cloned())
}

#[test]
fn more_columns_sets_the_number_and_spacing_in_one_undo() {
    let mut h = harness();
    h.state_mut().session.execute("format.columns", &json!({"count": 3, "spacing": 9.0})).unwrap();
    h.run_steps(2);
    h.get_by_label("Columns").click();
    h.run_steps(2);
    h.get_by_label("More Columns…").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("columns"), "status {:?}", h.state().status);
    assert_eq!(field(&h, "count").as_deref(), Some("3"));
    assert_eq!(field(&h, "spacing").as_deref(), Some("0.12"));
    {
        let d = h.state_mut().dialog.as_mut().unwrap();
        d.fields.insert("count".into(), "2".into());
        d.fields.insert("spacing".into(), "0.5".into());
    }
    let n = h.state().session.active().unwrap().history.undo.len();
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    assert_eq!(columns_of(&h), (Some(2), Some(36.0)));
    let undo = &h.state().session.active().unwrap().history.undo;
    assert_eq!(undo.len(), n + 1);
    assert_eq!(undo.last().unwrap().label, "Columns");
}

#[test]
fn ok_with_the_spacing_unchanged_keeps_the_exact_spacing() {
    let mut h = harness();
    h.state_mut().session.execute("format.columns", &json!({"count": 3, "spacing": 9.0})).unwrap();
    h.state_mut().run("format.columns", json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(field(&h, "spacing").as_deref(), Some("0.12"), "shown rounded to two places");
    h.state_mut().dialog.as_mut().unwrap().fields.insert("count".into(), "4".into());
    h.get_by_label("OK").click_accesskit();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "status {:?}", h.state().status);
    assert_eq!(columns_of(&h), (Some(4), Some(9.0)));
}

#[test]
fn format_shape_columns_button_opens_the_dialog() {
    let mut h = harness();
    h.state_mut().ui.pane = Some("format".into());
    h.state_mut().ui.format_tab = "text".into();
    h.run_steps(2);
    h.get_by_label("Columns…").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("columns"), "status {:?}", h.state().status);
    assert_eq!(field(&h, "count").as_deref(), Some("1"));
    assert_eq!(field(&h, "spacing").as_deref(), Some("0"));
}

#[test]
fn a_bad_number_keeps_the_dialog_open_and_changes_nothing() {
    let mut h = harness();
    h.state_mut().run("format.columns", json!({})).unwrap();
    h.run_steps(2);
    for (count, spacing) in [("0", "0.25"), ("17", "0.25"), ("two", "0.25"), ("2", "-1"), ("2", "wide")] {
        {
            let d = h.state_mut().dialog.as_mut().unwrap();
            d.fields.insert("count".into(), count.into());
            d.fields.insert("spacing".into(), spacing.into());
        }
        h.get_by_label("OK").click_accesskit();
        h.run_steps(2);
        assert_eq!(h.state().dialog.as_ref().map(|d| d.id.as_str()), Some("columns"), "{count} / {spacing}");
        assert!(field(&h, "error").is_some_and(|e| !e.is_empty()), "{count} / {spacing}");
        assert_eq!(columns_of(&h), (None, None), "{count} / {spacing}");
    }
}
