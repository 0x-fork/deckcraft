//! Status bar (slide counter, language, notes/comments toggles, view buttons, zoom slider) and the
//! notes pane under the slide.

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};
use crate::{SlideApp, ViewMode};

pub fn status_bar(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status").exact_size(28.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let rect = ui.max_rect();
        let Some(st) = app.session.active() else { return };
        let n = st.doc.slides.len();
        let left = match st.selection.target {
            deckcraft_engine::Target::Slides => format!("Slide {} of {}", (st.selection.slide + 1).min(n.max(1)), n),
            _ => "Slide Master".to_string(),
        };
        let p = ui.painter();
        p.text(pos2(rect.min.x + 14.0, rect.center().y), Align2::LEFT_CENTER, &left, theme::font(12.0), t.text_dim);
        p.text(pos2(rect.min.x + 120.0, rect.center().y), Align2::LEFT_CENTER, "English (United States)", theme::font(12.0), t.text_dim);
        // Transient status message.
        if let Some(msg) = app.fresh_status() {
            p.text(pos2(rect.min.x + 290.0, rect.center().y), Align2::LEFT_CENTER, &msg, theme::font(12.0), t.accent);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(rect.max.x - 560.0, rect.min.y), pos2(rect.max.x - 8.0, rect.max.y)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 4.0;
        if small_icon(&mut right, Icon::FitSmall, "Fit slide to current window", false).clicked() {
            app.ui.zoom = None;
        }
        let pct = (app.ui.zoom.unwrap_or(app.canvas_scale) * 100.0).round();
        let lbl = right.add(egui::Label::new(egui::RichText::new(format!("{pct}%")).size(12.0).color(t.text_dim)).sense(Sense::click()));
        if lbl.clicked() {
            let _ = app.run("view.zoomDialog", json!({}));
        }
        if small_icon(&mut right, Icon::ZoomIn, "Zoom In", false).clicked() {
            let _ = app.run("view.zoomIn", json!({}));
        }
        // Slider 10%–400% (log scale).
        let mut z = app.ui.zoom.unwrap_or(app.canvas_scale).clamp(0.1, 4.0);
        let mut lz = z.ln();
        let s = right.add_sized(vec2(110.0, 18.0), egui::Slider::new(&mut lz, (0.1f32).ln()..=(4.0f32).ln()).show_value(false));
        if s.changed() {
            z = lz.exp();
            app.ui.zoom = Some(z);
        }
        if small_icon(&mut right, Icon::ZoomOut, "Zoom Out", false).clicked() {
            let _ = app.run("view.zoomOut", json!({}));
        }
        right.add_space(8.0);
        let view = app.ui.view;
        if small_icon(&mut right, Icon::ViewShowSmall, "Slide Show", false).clicked() {
            let i = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
            app.start_show(i, false);
        }
        if small_icon(&mut right, Icon::ViewReadingSmall, "Reading View", false).clicked() {
            let _ = app.run("view.reading", json!({}));
        }
        if small_icon(&mut right, Icon::ViewSorterSmall, "Slide Sorter", view == ViewMode::Sorter).clicked() {
            app.ui.view = ViewMode::Sorter;
        }
        if small_icon(&mut right, Icon::ViewNormalSmall, "Normal", view == ViewMode::Normal).clicked() {
            app.ui.view = ViewMode::Normal;
        }
        right.add_space(8.0);
        let c_on = app.ui.pane.as_deref() == Some("comments");
        if labeled(&mut right, Icon::CommentsSmall, "Comments", c_on).clicked() {
            let _ = app.run("view.pane", json!({"pane": "comments", "toggle": true}));
        }
        let n_on = app.ui.notes;
        if labeled(&mut right, Icon::NotesSmall, "Notes", n_on).clicked() {
            app.ui.notes = !n_on;
        }
        let _ = Stroke::NONE;
    });
}

fn small_icon(ui: &mut Ui, icon: Icon, tip: &str, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
    if on || resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), if on { t.selected } else { t.hover });
    }
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(16.0, 16.0)), icon, t.text_dim, false);
    resp.on_hover_text(tip)
}

fn labeled(ui: &mut Ui, icon: Icon, label: &str, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let w = 22.0 + ui.fonts_mut(|f| f.layout_no_wrap(label.into(), theme::font(12.0), t.text).size().x) + 8.0;
    let (r, resp) = ui.allocate_exact_size(vec2(w, 20.0), Sense::click());
    if on || resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), if on { t.selected } else { t.hover });
    }
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 11.0, r.center().y), vec2(15.0, 15.0)), icon, t.text_dim, false);
    ui.painter().text(pos2(r.min.x + 22.0, r.center().y), Align2::LEFT_CENTER, label, theme::font(12.0), t.text_dim);
    resp
}

/// The speaker notes editor under the slide.
pub fn notes_pane(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    if st.selection.target != deckcraft_engine::Target::Slides {
        return;
    }
    let (idx, uid) = (st.selection.slide, st.uid);
    // Save an edited draft once the field has lost focus or another slide is shown (before
    // anything reloads it), to the slide it was written on.
    let id = egui::Id::new("notes_edit");
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    if app.notes_buf.2.edited && (!focused || app.notes_buf.0 != uid || app.notes_buf.1 != idx) {
        app.notes_buf.2.edited = false;
        if app.notes_buf.0 == uid {
            let _ = app.run("slide.notes", json!({"body": app.notes_buf.2.body, "index": app.notes_buf.1}));
        }
    }
    let notes = app.session.active().and_then(|st| st.current_slide()).map(|s| s.notes.clone()).unwrap_or_default();
    egui::Panel::bottom("notes")
        .resizable(true)
        .default_size(app.ui.notes_height)
        .size_range(36.0..=400.0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(8, 6)))
        .show(ui, |ui| {
            app.ui.notes_height = ui.available_height() + 12.0;
            let rect = ui.max_rect();
            ui.painter()
                .line_segment([pos2(rect.min.x - 8.0, rect.min.y - 6.0), pos2(rect.max.x + 8.0, rect.min.y - 6.0)], Stroke::new(1.0, t.separator));
            // Keep a local draft per slide, following the notes while the field isn't focused.
            if app.notes_buf.0 != uid || app.notes_buf.1 != idx || (!focused && app.notes_buf.2.body != notes) {
                app.notes_buf = (uid, idx, crate::notes::Draft::new(&notes));
            }
            let mut draft = app.notes_buf.2.clone();
            let look = crate::notes::Look { size: 13.0, color: None };
            // Long notes scroll inside the pane instead of spilling over the slide (#221).
            let r = egui::ScrollArea::vertical()
                .max_height(ui.available_height())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    crate::notes::editor(ui, &mut draft, &look, |e| {
                        e.id(id).hint_text("Click to add notes").frame(egui::Frame::NONE).desired_width(f32::INFINITY).desired_rows(2)
                    })
                })
                .inner;
            if r.lost_focus() && draft.edited {
                draft.edited = false;
                let _ = app.run("slide.notes", json!({"body": draft.body, "index": idx}));
            }
            app.notes_buf.2 = draft;
        });
}
