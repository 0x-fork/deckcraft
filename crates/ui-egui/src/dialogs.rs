//! Dialogs (Insert Table, Slide Size, Font, Paragraph, Tabs, Header & Footer, Hyperlink, Set Up Show, Zoom, …) and the
//! About window. The start screen is File › Home (`backstage.rs`).

use deckcraft_color::SchemeSlot;
use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::SlideApp;
use crate::theme::{self, Tokens};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Dialog {
    pub id: String,
    pub params: Value,
    /// Field values while the dialog is open.
    #[serde(skip)]
    pub fields: std::collections::HashMap<String, String>,
}

impl Dialog {
    pub fn new(id: &str) -> Self {
        Dialog { id: id.into(), params: Value::Null, fields: Default::default() }
    }
    pub fn modal(&self) -> bool {
        true
    }
    fn get(&mut self, k: &str, default: &str) -> String {
        self.fields.entry(k.into()).or_insert_with(|| default.to_string()).clone()
    }
}

fn title(id: &str) -> &'static str {
    match id {
        "table" => "Insert Table",
        "slideSize" => "Slide Size",
        "sizePosition" => "Size and Position",
        "headerFooter" => "Header and Footer",
        "hyperlink" => "Insert Hyperlink",
        "setupShow" => "Set Up Slide Show",
        "zoom" => "Zoom",
        "comment" => "New Comment",
        "altText" => "Alt Text",
        "outline" => "Slides from Outline",
        "renameSection" => "Rename Section",
        "reuseSlides" => "Reuse Slides",
        "renameShape" => "Rename",
        "renameLayout" => "Rename Layout",
        "paragraph" => "Paragraph",
        "tabs" => "Tabs",
        "font" => "Font",
        "about" => "About DeckCraft",
        "preferences" => "Preferences",
        "autocorrect" => "AutoCorrect",
        "gridOptions" => "Grid Options",
        "columns" => "Columns",
        "export" => "Export",
        "symbol" => "Symbol",
        "photoAlbum" => "Photo Album",
        "replaceFonts" => "Replace Font",
        "equation" => "Equation",
        "dateTime" => "Date and Time",
        "spelling" => "Spelling",
        "pasteSpecial" => "Paste Special",
        "find" => "Find and Replace",
        "accessibility" => "Accessibility Checker",
        "language" => "Language",
        "customShow" => "Custom Shows",
        "chartData" => "Chart Data",
        "trim" => "Trim Media",
        "smartart" => "Choose a SmartArt Graphic",
        "bullets" => "Bullets and Numbering",
        "action" => "Action Settings",
        "quit" => "DeckCraft",
        "themeColors" => "Create New Theme Colors",
        "themeFonts" => "Create New Theme Fonts",
        _ => "DeckCraft",
    }
}

/// Render the active dialog, clearing it when dismissed and preserving or restoring application
/// state as its close path requires.
pub fn show(app: &mut SlideApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let mut close = false;
    let mut window = egui::Window::new(title(&d.id))
        .id(egui::Id::unique(("dialog", d.id.clone())))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .open(&mut open);
    // Two columns of colour rows plus Save must fit the first frame (Theme Colors), as must Save
    // in Theme Fonts. A later resize clips the buttons (the window size sticks) and clicks miss them.
    if d.id == "themeColors" {
        window = window.default_size(vec2(760.0, 420.0)).min_width(720.0).min_height(420.0);
    } else if d.id == "themeFonts" {
        window = window.default_size(vec2(460.0, 320.0)).min_width(420.0).min_height(300.0);
    }
    window.show(ctx, |ui| {
        ui.set_min_width(match d.id.as_str() {
            "themeColors" => 700.0,
            "themeFonts" => 400.0,
            _ => 340.0,
        });
        if d.id == "themeColors" {
            ui.set_min_height(360.0);
        } else if d.id == "themeFonts" {
            ui.set_min_height(280.0);
        }
        close = body(app, ui, &mut d);
    });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if open && !close && !escape {
        app.dialog = Some(d);
    } else if !close && d.id == "preferences" {
        // Esc or the window's close button: the same as Cancel.
        restore_preferences(app, &d);
    } else if app.dialog.is_none()
        && let Some(ret) = d.params.get("returnParams").filter(|v| v.is_object()).cloned()
    {
        // Tabs… opened from Paragraph: closing it (OK, Cancel, Esc or the window) returns there.
        let id = d.params.get("returnId").and_then(Value::as_str).unwrap_or("paragraph");
        let mut back = Dialog::new(id);
        back.params = ret;
        app.dialog = Some(back);
    }
}

fn buttons(ui: &mut Ui, ok_label: &str) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let t = Tokens::get(ui.ctx());
        if ui.add(egui::Button::new(egui::RichText::new(ok_label).color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0))).clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Enter))
        {
            ok = true;
        }
        if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

/// Draw a labelled single-line field bound to `key`, falling back to `default`, and store the
/// edited value in the dialog.
fn field(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: &str) {
    let mut v = d.get(key, default);
    ui.horizontal(|ui| {
        let l = ui.add_sized(vec2(120.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(180.0)).labelled_by(l.id);
    });
    d.fields.insert(key.into(), v);
}

fn check(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: bool) -> bool {
    let mut v = d.get(key, if default { "1" } else { "0" }) == "1";
    ui.checkbox(&mut v, label);
    d.fields.insert(key.into(), if v { "1".into() } else { "0".into() });
    v
}

fn num(d: &mut Dialog, key: &str, default: f64) -> f64 {
    d.get(key, "").trim().trim_end_matches(['"', '%', ' ']).parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(default)
}

fn num_text(v: f64) -> String {
    crate::widgets::fmt_num(v, "")
}

fn field_of(d: &Dialog, key: &str) -> String {
    d.fields.get(key).cloned().unwrap_or_default()
}

fn parse_shown(s: &str) -> Option<f64> {
    let t = s.trim().trim_end_matches(['%', '"']).trim();
    if t.is_empty() {
        return None;
    }
    t.parse().ok().filter(|v: &f64| v.is_finite())
}

fn edited(d: &Dialog, key: &str) -> bool {
    field_of(d, key) != field_of(d, &format!("seed{key}"))
}

fn exact_of(d: &Dialog, key: &str) -> Option<f64> {
    parse_shown(&field_of(d, &format!("{key}Pt")))
}

/// Draw a labelled single-line text row bound to a dialog key and store the edited value.
fn text_row(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str) {
    let mut v = field_of(d, key);
    ui.horizontal(|ui| {
        let l = ui.add_sized(vec2(148.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(96.0)).labelled_by(l.id);
    });
    d.fields.insert(key.into(), v);
}

use deckcraft_text::datetime::DATE_KINDS;

mod about;
mod alt_text;
mod body;
mod columns;
mod date_time;
mod equation;
mod export;
mod find;
mod font;
mod grid_options;
mod hyperlink;
mod language;
mod paragraph;
mod photo_album;
mod preferences;
mod size_position;
mod slide_size;
mod tabs;
mod theme_colors;
mod theme_fonts;
mod trim;
mod zoom;

#[cfg(test)]
use about::{CREDITS_SIZE, credits_block};
pub use about::{about, paint_logo, version_line};
#[cfg(test)]
use alt_text::alt_text_starts_decorative;
use body::body;
#[cfg(test)]
use hyperlink::{
    CurrentLink, HyperlinkSeed, common_click, hyperlink_command, hyperlink_seed_of, hyperlink_should_apply, link_covering, seed_from, seed_of_link,
};
pub use language::selected_language;
use preferences::restore_preferences;
#[cfg(test)]
use trim::{ms_as_seconds, seconds_text_to_ms, trim_seed};
pub use zoom::parse_zoom_percent;

fn seed_once(d: &mut Dialog, key: &str, val: String) {
    d.fields.entry(key.to_string()).or_insert(val);
}

fn json_text(v: Option<&Value>, default: &str) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map(|x| crate::widgets::fmt_num(x, "")).unwrap_or_else(|| default.to_string()),
        Some(Value::Bool(true)) => "1".into(),
        Some(Value::Bool(false)) => "0".into(),
        _ => default.to_string(),
    }
}

fn color_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("scheme").or_else(|| o.get("rgb")).and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// Draw a uniquely identified accessible choice combo box and store the selected key;
/// the caller supplies the visible label.
fn choice(ui: &mut Ui, d: &mut Dialog, salt: &str, name: &str, key: &str, choices: &[(&str, &str)]) {
    let cur = d.get(key, "");
    let label = choices.iter().find(|(v, _)| *v == cur).map(|(_, l)| *l).unwrap_or(cur.as_str());
    let combo = egui::ComboBox::from_id_salt(salt).selected_text(label).width(140.0).show_ui(ui, |ui| {
        for (v, l) in choices {
            if ui.selectable_label(cur == *v, *l).clicked() {
                d.fields.insert((*key).into(), (*v).into());
            }
        }
    });
    combo.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name));
}

fn labeled_text(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, width: f32) -> String {
    let mut v = d.get(key, "");
    let l = ui.label(label);
    ui.add(egui::TextEdit::singleline(&mut v).desired_width(width)).labelled_by(l.id);
    d.fields.insert(key.into(), v.clone());
    v
}

fn finite_pt(text: &str) -> Option<f64> {
    text.trim().trim_end_matches("pt").trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn pt_text(v: Option<&Value>, default: f64) -> String {
    match v.and_then(Value::as_f64).filter(|n| n.is_finite()) {
        Some(n) => crate::widgets::fmt_num(n, ""),
        None => crate::widgets::fmt_num(default, ""),
    }
}

fn inches_text(v: Option<&Value>) -> String {
    crate::widgets::fmt_num(v.and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0) / 72.0, "")
}

pub mod credits;
#[cfg(test)]
mod font_dialog_tests;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod updates_ui;

#[cfg(test)]
mod size_position_tests;

#[cfg(test)]
mod columns_dialog_tests;

#[cfg(test)]
mod photo_album_tests;

#[cfg(test)]
mod theme_colors_tests;

#[cfg(test)]
mod theme_fonts_tests;

#[cfg(test)]
mod alt_text_tests {
    use super::alt_text_starts_decorative;

    #[test]
    fn decorative_starts_checked_only_when_every_shape_is() {
        assert!(!alt_text_starts_decorative([]));
        assert!(!alt_text_starts_decorative([false]));
        assert!(alt_text_starts_decorative([true]));
        assert!(alt_text_starts_decorative([true, true]));
        assert!(!alt_text_starts_decorative([true, false]));
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod zoom_tests {
    use super::parse_zoom_percent;

    fn near(v: f32, want: f32) -> bool {
        (v - want).abs() < 1.0e-4
    }

    #[test]
    fn zoom_percent_accepts_a_number_and_clamps() {
        assert!(parse_zoom_percent("150").is_some_and(|v| near(v, 1.5)));
        assert!(parse_zoom_percent("150%").is_some_and(|v| near(v, 1.5)));
        assert!(parse_zoom_percent("  100 ").is_some_and(|v| near(v, 1.0)));
        assert!(parse_zoom_percent("1").is_some_and(|v| near(v, 0.1)));
        assert!(parse_zoom_percent("900").is_some_and(|v| near(v, 4.0)));
        assert!(parse_zoom_percent("").is_none());
        assert!(parse_zoom_percent("fit").is_none());
        assert!(parse_zoom_percent("nan").is_none());
        assert!(parse_zoom_percent("inf").is_none());
    }
}

#[cfg(test)]
mod trim_media_tests {
    use std::sync::Arc;

    use deckcraft_model::{MediaClip, Presentation, ShapeId, ShapeKind};

    use super::{ms_as_seconds, seconds_text_to_ms, trim_seed};

    #[test]
    fn trim_seconds_round_trip_the_stored_milliseconds() {
        for ms in [0, 1, 250, 333, 1000, 1500, 60_000] {
            let text = ms_as_seconds(ms);
            assert_eq!(seconds_text_to_ms(&text), ms, "{text}");
        }
        assert_eq!(seconds_text_to_ms(""), 0);
        assert_eq!(seconds_text_to_ms("nope"), 0);
        assert_eq!(seconds_text_to_ms("-2"), 0);
        assert_eq!(seconds_text_to_ms("1.5"), 1500);
        assert_eq!(seconds_text_to_ms("999999999"), u32::MAX);
    }

    #[test]
    fn trim_dialog_reads_the_selected_clip() {
        assert_eq!(trim_seed(None).start, "0");
        let mut doc = Presentation::default();
        let media_id = {
            let slide = Arc::make_mut(&mut doc.slides[0]);
            let title = slide.shapes[0].id;
            let clip = MediaClip { trim_start_ms: 1500, trim_end_ms: 250, fade_in_ms: 333, fade_out_ms: 1000, ..Default::default() };
            let mut media = slide.shapes[0].clone();
            media.id = ShapeId(900);
            media.kind = ShapeKind::Media(clip);
            slide.shapes.push(media);
            (title, ShapeId(900))
        };
        let mut st = deckcraft_engine::DocState::new(doc, None, "t".into());
        st.selection.shapes = vec![media_id.0];
        assert_eq!(trim_seed(Some(&st)).start, "0", "a title shape is not a clip");
        st.selection.shapes = vec![media_id.0, media_id.1];
        let seed = trim_seed(Some(&st));
        assert_eq!(seed.start, "1.5");
        assert_eq!(seed.end, "0.25");
        assert_eq!(seed.fade_in, "0.333");
        assert_eq!(seed.fade_out, "1");
    }
}

#[cfg(test)]
mod spelling_tests;

#[cfg(test)]
mod paragraph_dialog_tests;

#[cfg(test)]
mod about_credits_tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::{CREDITS_SIZE, Dialog, credits_block};
    use crate::{Services, SlideApp};

    fn tab_of(h: &Harness<'_, SlideApp>) -> String {
        let d = h.state().dialog.as_ref().expect("the About dialog is open");
        d.fields.get("tab").cloned().unwrap_or_else(|| "about".into())
    }

    /// The reported symptom (#843): in the real dialog the tabs stopped answering once Contributors
    /// or Models was open. Click through Contributors, Models and back to About.
    #[test]
    fn about_dialog_tabs_keep_responding() {
        let mut app = SlideApp::new(deckcraft_engine::Session::with_new(), Services::default());
        app.dialog = Some(Dialog::new("about"));
        let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, app: &mut SlideApp| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
            app,
        );
        h.run_steps(4);
        assert_eq!(tab_of(&h), "about");
        for label in ["Contributors", "Models", "About", "Models", "Contributors"] {
            h.get_by_label(label).click();
            h.run_steps(3);
            assert_eq!(tab_of(&h), label.to_lowercase(), "clicking {label} switches the tab");
        }
    }

    #[test]
    fn credits_get_their_own_block_below_the_tab_row() {
        for models in [false, true] {
            let ctx = egui::Context::default();
            let mut tabs = egui::Rect::NOTHING;
            let mut block = egui::Rect::NOTHING;
            let mut whole = egui::Rect::NOTHING;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                tabs = ui.label("About").rect;
                block = credits_block(ui, models);
                whole = ui.min_rect();
            });
            out.textures_delta.clear();
            assert_eq!(block.size(), CREDITS_SIZE, "models={models}");
            assert!(block.min.y >= tabs.max.y, "the credits start below the tab row (models={models})");
            assert!(whole.max.y >= block.max.y, "the dialog grows to hold the block (models={models})");
            assert!(whole.height() > CREDITS_SIZE.y, "the tab row is not squeezed into the credits height (models={models})");
        }
    }
}

#[cfg(test)]
mod preferences_tests;
