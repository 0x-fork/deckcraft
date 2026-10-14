//! Task panes on the right: Format Shape, Format Background, Animation, Selection, Comments and
//! Design Ideas.

use deckcraft_model::{AnimClass, AnimStart, Fill, ShapeKind};
use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::ribbon::{cref_param, paint_anim_tile};
use crate::theme::{self, Tokens};
use crate::{SlideApp, widgets};

pub fn show(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(pane) = app.ui.pane.clone() else { return };
    egui::Panel::right("task_pane")
        .resizable(true)
        .default_size(app.ui.pane_width)
        .size_range(240.0..=520.0)
        .frame(egui::Frame::NONE.fill(t.card).inner_margin(egui::Margin::same(10)))
        .show(ui, |ui| {
            app.ui.pane_width = ui.available_width() + 20.0;
            let title = match pane.as_str() {
                "format" => "Format Shape",
                "background" => "Format Background",
                "animation" => "Animation Pane",
                "selection" => "Selection",
                "comments" => "Comments",
                "designer" => "Design Ideas",
                _ => "Pane",
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).font(theme::bold(16.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(4), t.hover);
                    }
                    icons::paint(ui.painter(), r.shrink(3.0), Icon::Close, t.text_dim, false);
                    if resp.clicked() {
                        app.ui.pane = None;
                    }
                });
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match pane.as_str() {
                "format" => format_shape(app, ui),
                "background" => background(app, ui),
                "animation" => animation(app, ui),
                "selection" => crate::selpane::show(app, ui),
                "comments" => comments(app, ui),
                "designer" => designer(app, ui),
                _ => {}
            });
        });
}

fn run(app: &mut SlideApp, id: &str, p: Value) {
    let _ = app.run(id, p);
}

fn scheme(app: &SlideApp) -> deckcraft_color::ColorScheme {
    app.session.active().and_then(|d| d.doc.masters.first().map(|m| m.scheme())).unwrap_or_else(|| deckcraft_model::Theme::default().colors)
}

fn color_button(
    app: &mut SlideApp,
    ui: &mut Ui,
    label: &str,
    none_label: Option<&str>,
    on_pick: impl Fn(Option<deckcraft_model::ColorRef>) -> (String, Value),
) {
    let sc = scheme(app);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = widgets::drop_button(ui, "Color", vec2(70.0, 22.0), true);
            widgets::color_popup(&r).show(|ui| {
                if let Some(c) = widgets::color_grid(ui, &sc, none_label) {
                    let (id, p) = on_pick(c);
                    run(app, &id, p);
                }
            });
        });
    });
}

fn format_shape(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    let Some(sh) = st.selected_shapes().first().map(|s| (*s).clone()) else {
        ui.label(egui::RichText::new("Select a shape to format it.").color(t.text_dim));
        return;
    };
    let x = deckcraft_engine::cmd::xfrm_of(&st.doc, &st.selection, &sh);
    let uid = st.uid;
    // Tabs as icons.
    ui.horizontal(|ui| {
        for (tab, icon, tip) in [
            ("fill", Icon::ShapeFill, "Fill & Line"),
            ("effects", Icon::ShapeEffects, "Effects"),
            ("size", Icon::SlideSize, "Size & Properties"),
            ("text", Icon::TextBox, "Text Options"),
        ] {
            let on = app.ui.format_tab == tab;
            if widgets::icon_toggle(ui, icon, tip, true, on).clicked() {
                app.ui.format_tab = tab.into();
            }
        }
        if matches!(sh.kind, ShapeKind::Picture { .. })
            && widgets::icon_toggle(ui, Icon::Picture, "Picture", true, app.ui.format_tab == "picture").clicked()
        {
            app.ui.format_tab = "picture".into();
        }
    });
    ui.separator();
    match app.ui.format_tab.as_str() {
        "effects" => {
            ui.label(egui::RichText::new("Shadow").font(theme::bold(13.0)));
            ui.horizontal_wrapped(|ui| {
                for (l, v) in [
                    ("None", "none"),
                    ("Outer", "outer"),
                    ("Bottom", "bottom"),
                    ("Center", "center"),
                    ("Inner", "inner"),
                    ("Perspective", "perspective"),
                ] {
                    if ui.button(l).clicked() {
                        run(app, "shape.effects", json!({"shadow": v}));
                    }
                }
            });
            let fx = sh.effects.clone().unwrap_or_default();
            let shadow = fx.outer_shadow.as_ref().or(fx.inner_shadow.as_ref());
            if let Some(sw) = shadow {
                color_button(app, ui, "Color", None, |c| ("shape.effects".into(), json!({"shadowProps": {"color": c.as_ref().map(cref_param)}})));
                fx_slider(
                    app,
                    ui,
                    ("shadow", sh.id.0, "t"),
                    "Transparency",
                    (1.0 - sw.color.alpha()) * 100.0,
                    0.0..=100.0,
                    "%",
                    "shadowProps",
                    "transparency",
                    0.01,
                );
                fx_slider(app, ui, ("shadow", sh.id.0, "b"), "Blur", sw.blur, 0.0..=100.0, " pt", "shadowProps", "blur", 1.0);
                fx_slider(app, ui, ("shadow", sh.id.0, "a"), "Angle", sw.dir, 0.0..=359.0, "°", "shadowProps", "dir", 1.0);
                fx_slider(app, ui, ("shadow", sh.id.0, "d"), "Distance", sw.dist, 0.0..=200.0, " pt", "shadowProps", "dist", 1.0);
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Reflection").font(theme::bold(13.0)));
            ui.horizontal_wrapped(|ui| {
                for (l, v) in [("None", "none"), ("Tight", "tight"), ("Half", "half"), ("Full", "full")] {
                    if ui.button(l).clicked() {
                        run(app, "shape.effects", json!({"reflection": v}));
                    }
                }
            });
            if let Some(r) = &fx.reflection {
                fx_slider(
                    app,
                    ui,
                    ("refl", sh.id.0, "t"),
                    "Transparency",
                    (1.0 - r.start_alpha) * 100.0,
                    0.0..=100.0,
                    "%",
                    "reflectionProps",
                    "transparency",
                    0.01,
                );
                fx_slider(app, ui, ("refl", sh.id.0, "s"), "Size", r.end_pos * 100.0, 0.0..=100.0, "%", "reflectionProps", "size", 0.01);
                fx_slider(app, ui, ("refl", sh.id.0, "d"), "Distance", r.dist, 0.0..=100.0, " pt", "reflectionProps", "dist", 1.0);
                fx_slider(app, ui, ("refl", sh.id.0, "b"), "Blur", r.blur, 0.0..=100.0, " pt", "reflectionProps", "blur", 1.0);
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Glow").font(theme::bold(13.0)));
            let radius = fx.glow.as_ref().map_or(0.0, |g| g.radius);
            fx_slider(app, ui, ("glow", sh.id.0, "r"), "Size", radius, 0.0..=150.0, " pt", "glowProps", "radius", 1.0);
            if let Some(g) = &fx.glow {
                color_button(app, ui, "Color", None, |c| ("shape.effects".into(), json!({"glowProps": {"color": c.as_ref().map(cref_param)}})));
                fx_slider(
                    app,
                    ui,
                    ("glow", sh.id.0, "t"),
                    "Transparency",
                    (1.0 - g.color.alpha()) * 100.0,
                    0.0..=100.0,
                    "%",
                    "glowProps",
                    "transparency",
                    0.01,
                );
            }
            ui.label(egui::RichText::new("Soft Edges").font(theme::bold(13.0)));
            let mut s = sh.effects.as_ref().and_then(|e| e.soft_edge).unwrap_or(0.0);
            if ui.add(egui::Slider::new(&mut s, 0.0..=50.0).text("size (pt)")).drag_stopped() {
                run(app, "shape.effects", json!({"softEdges": s}));
            }
            if ui.button("Reset Effects").clicked() {
                run(app, "shape.effects", json!({"reset": true}));
            }
        }
        "size" => {
            let lock_id = egui::Id::new("fmt-lock-aspect");
            let mut lock: bool = ui.data_mut(|d| d.get_temp(lock_id).unwrap_or(false));
            let locked = lock;
            let mut fields = |ui: &mut Ui, label: &str, id: &str, v: f64, key: &str, cmd: &str, unit: f64| {
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(90.0, 20.0), egui::Label::new(label));
                    if let Some(nv) = widgets::spinner(ui, id, v / unit, 0.1, if unit == 72.0 { "\"" } else { "°" }, 70.0) {
                        let mut p = json!({key: nv * unit});
                        if cmd == "shape.resize" && locked {
                            p["lockAspect"] = json!(true);
                        }
                        run(app, cmd, p);
                    }
                });
            };
            ui.label(egui::RichText::new("Size").font(theme::bold(13.0)));
            fields(ui, "Height", "fh", x.h, "h", "shape.resize", 72.0);
            fields(ui, "Width", "fw", x.w, "w", "shape.resize", 72.0);
            fields(ui, "Rotation", "fr", x.rot, "deg", "shape.rotate", 1.0);
            if ui.checkbox(&mut lock, "Lock aspect ratio").changed() {
                ui.data_mut(|d| d.insert_temp(lock_id, lock));
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Position").font(theme::bold(13.0)));
            fields(ui, "Horizontal", "fx", x.x, "x", "shape.move", 72.0);
            fields(ui, "Vertical", "fy", x.y, "y", "shape.move", 72.0);
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Alt Text").font(theme::bold(13.0)));
            let id = ui.id().with(("alt", uid, sh.id.0));
            let stored: Option<(String, String)> = ui.data_mut(|d| d.get_temp(id));
            let mut alt = widgets::text_draft(stored, &sh.descr);
            let r = ui.add(egui::TextEdit::multiline(&mut alt).desired_rows(3).desired_width(f32::INFINITY));
            if r.lost_focus() && alt != sh.descr {
                run(app, "shape.altText", json!({"text": alt}));
            }
            ui.data_mut(|d| d.insert_temp(id, (sh.descr.clone(), alt)));
        }
        "text" => {
            ui.label(egui::RichText::new("Text Fill").font(theme::bold(13.0)));
            color_button(app, ui, "Color", None, |c| match c {
                Some(c) => ("format.color".into(), json!({"color": cref_param(&c)})),
                None => ("format.color".into(), json!({"color": "tx1"})),
            });
            text_transparency_slider(app, ui, &sh);
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Text Box").font(theme::bold(13.0)));
            ui.horizontal(|ui| {
                ui.label("Vertical alignment");
                for (l, a) in [("Top", "top"), ("Middle", "middle"), ("Bottom", "bottom")] {
                    if ui.small_button(l).clicked() {
                        run(app, "format.anchor", json!({"anchor": a}));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (l, m) in [("Do not Autofit", "none"), ("Shrink text on overflow", "shrink"), ("Resize shape to fit text", "resize")] {
                    if ui.small_button(l).clicked() {
                        run(app, "format.autofit", json!({"mode": m}));
                    }
                }
            });
            let wrap = sh.text.as_ref().and_then(|t| t.body.wrap).unwrap_or(true);
            let mut w = wrap;
            if ui.checkbox(&mut w, "Wrap text in shape").changed() {
                run(app, "format.wrap", json!({"on": w}));
            }
            ui.label("Margins (pt)");
            let b = sh.text.as_ref().map(|t| t.body.clone()).unwrap_or_default();
            for (l, k, v) in [
                ("Left", "left", b.inset_l.unwrap_or(7.2)),
                ("Right", "right", b.inset_r.unwrap_or(7.2)),
                ("Top", "top", b.inset_t.unwrap_or(3.6)),
                ("Bottom", "bottom", b.inset_b.unwrap_or(3.6)),
            ] {
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(60.0, 20.0), egui::Label::new(l));
                    if let Some(nv) = widgets::spinner(ui, &format!("m{k}"), v, 1.0, " pt", 60.0) {
                        run(app, "format.margins", json!({k: nv.max(0.0)}));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Columns");
                for n in 1..=3 {
                    if ui.small_button(n.to_string()).clicked() {
                        run(app, "format.columns", json!({"count": n, "spacing": 18}));
                    }
                }
            });
        }
        "picture" => {
            let adj = if let ShapeKind::Picture { fill } = &sh.kind { fill.adjust.clone() } else { Default::default() };
            let mut b = adj.brightness as f32;
            if ui.add(egui::Slider::new(&mut b, -1.0..=1.0).text("Brightness")).drag_stopped() {
                run(app, "picture.adjust", json!({"brightness": b}));
            }
            let mut c = adj.contrast as f32;
            if ui.add(egui::Slider::new(&mut c, -1.0..=1.0).text("Contrast")).drag_stopped() {
                run(app, "picture.adjust", json!({"contrast": c}));
            }
            let mut s = adj.saturation.unwrap_or(1.0) as f32;
            if ui.add(egui::Slider::new(&mut s, 0.0..=4.0).text("Saturation")).drag_stopped() {
                run(app, "picture.adjust", json!({"saturation": s}));
            }
            let alpha = if let ShapeKind::Picture { fill } = &sh.kind { fill.alpha.unwrap_or(1.0) } else { 1.0 };
            let mut tr = (1.0 - alpha) as f32;
            if ui.add(egui::Slider::new(&mut tr, 0.0..=1.0).text("Transparency")).drag_stopped() {
                run(app, "picture.adjust", json!({"transparency": tr}));
            }
            if ui.button("Reset Picture").clicked() {
                run(app, "picture.reset", json!({}));
            }
        }
        _ => {
            // Fill & Line.
            ui.label(egui::RichText::new("Fill").font(theme::bold(13.0)));
            let kind = match &sh.fill {
                None => "auto",
                Some(Fill::None) => "none",
                Some(Fill::Solid { .. }) => "solid",
                Some(Fill::Gradient(_)) => "gradient",
                Some(Fill::Picture(_)) => "picture",
                Some(Fill::Pattern(_)) => "pattern",
                _ => "auto",
            };
            for (l, k) in
                [("No fill", "none"), ("Solid fill", "solid"), ("Gradient fill", "gradient"), ("Pattern fill", "pattern"), ("Automatic", "auto")]
            {
                if ui.radio(kind == k, l).clicked() {
                    match k {
                        "none" => run(app, "shape.fill", json!({"none": true})),
                        "solid" => run(app, "shape.fill", json!({"color": "accent1"})),
                        "gradient" => run(
                            app,
                            "shape.fill",
                            json!({"gradient": {"stops": [[0, "accent1"], [1, {"scheme": "accent1", "lumMod": 50000}]], "angle": 90}}),
                        ),
                        "pattern" => run(app, "shape.fill", json!({"pattern": {"preset": "pct25", "fg": "accent1", "bg": "bg1"}})),
                        _ => run(app, "shape.fill", json!({"reset": true})),
                    }
                }
            }
            color_button(app, ui, "Color", Some("No Fill"), |c| match c {
                Some(c) => ("shape.fill".into(), json!({"color": cref_param(&c)})),
                None => ("shape.fill".into(), json!({"none": true})),
            });
            ui.horizontal(|ui| {
                ui.label("Transparency");
                let id = ui.id().with("ftr");
                let mut v: f32 = ui.data_mut(|d| d.get_temp(id).unwrap_or(0.0));
                let r = ui.add(egui::Slider::new(&mut v, 0.0..=100.0).suffix("%"));
                ui.data_mut(|d| d.insert_temp(id, v));
                if r.drag_stopped() || r.lost_focus() {
                    let _ = app.run("shape.fill", json!({"transparency": v as f64 / 100.0}));
                }
            });
            if let Some(Fill::Gradient(g)) = &sh.fill {
                let sc = scheme(app);
                if let Some(ng) = crate::fillui::gradient_editor(ui, "shape-grad", g, &sc) {
                    run(app, "shape.fill", json!({"gradient": crate::fillui::gradient_json(&ng)}));
                }
                let mut rot = g.rotate_with_shape;
                if ui.checkbox(&mut rot, "Rotate with shape").changed() {
                    let ng = deckcraft_model::style::Gradient { rotate_with_shape: rot, ..g.clone() };
                    run(app, "shape.fill", json!({"gradient": crate::fillui::gradient_json(&ng)}));
                }
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Line").font(theme::bold(13.0)));
            let none = matches!(sh.line.as_ref().and_then(|l| l.fill.as_ref()), Some(Fill::None));
            if ui.radio(none, "No line").clicked() {
                run(app, "shape.line", json!({"none": true}));
            }
            if ui.radio(!none && sh.line.is_some(), "Solid line").clicked() {
                run(app, "shape.line", json!({"color": "tx1", "width": 1}));
            }
            if ui.radio(sh.line.is_none(), "Automatic").clicked() {
                run(app, "shape.line", json!({"reset": true}));
            }
            color_button(app, ui, "Color", Some("No Outline"), |c| match c {
                Some(c) => ("shape.line".into(), json!({"color": cref_param(&c)})),
                None => ("shape.line".into(), json!({"none": true})),
            });
            ui.horizontal(|ui| {
                ui.label("Width");
                let w = sh.line.as_ref().and_then(|l| l.width).unwrap_or(0.75);
                if let Some(v) = widgets::spinner(ui, "lw", w, 0.25, " pt", 60.0) {
                    run(app, "shape.line", json!({"width": v.max(0.0)}));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Dash");
                for (l, d) in [("—", "solid"), ("···", "sysDot"), ("- -", "dash"), ("-·-", "dashDot"), ("— —", "lgDash")] {
                    if ui.small_button(l).clicked() {
                        run(app, "shape.line", json!({"dash": d}));
                    }
                }
            });
            line_details(app, ui, &sh);
        }
    }
}

/// A labelled drop-down on the Format Shape pane: picks one of `items` (label, value) and runs
/// `shape.line` with `{key: value}`.
fn line_choice(app: &mut SlideApp, ui: &mut Ui, label: &str, key: &str, current: &str, items: &[(&str, &str)]) {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(90.0, 20.0), egui::Label::new(label));
        let shown = items.iter().find(|(_, v)| *v == current).map_or(current, |(l, _)| *l);
        egui::ComboBox::from_id_salt(("fmt-line", key)).selected_text(shown).width(120.0).show_ui(ui, |ui| {
            for (l, v) in items {
                if ui.selectable_label(*v == current, *l).clicked() {
                    run(app, "shape.line", json!({key: v}));
                }
            }
        });
    });
}

/// The rest of PowerPoint's Line page: compound type, cap and join types, and the begin and end
/// arrows with their sizes.
fn line_details(app: &mut SlideApp, ui: &mut Ui, sh: &deckcraft_model::Shape) {
    use deckcraft_model::style::{Compound, LineCap, LineJoin};
    let l = sh.line.clone().unwrap_or_default();
    let compound = match l.compound.unwrap_or_default() {
        Compound::Single => "single",
        Compound::Double => "double",
        Compound::ThickThin => "thickThin",
        Compound::ThinThick => "thinThick",
        Compound::Triple => "triple",
    };
    line_choice(
        app,
        ui,
        "Compound type",
        "compound",
        compound,
        &[("Single", "single"), ("Double", "double"), ("Thick Thin", "thickThin"), ("Thin Thick", "thinThick"), ("Triple", "triple")],
    );
    let cap = match l.cap.unwrap_or_default() {
        LineCap::Flat => "flat",
        LineCap::Round => "round",
        LineCap::Square => "square",
    };
    line_choice(app, ui, "Cap type", "cap", cap, &[("Flat", "flat"), ("Round", "round"), ("Square", "square")]);
    let join = match l.join.unwrap_or_default() {
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
        LineJoin::Miter => "miter",
    };
    line_choice(app, ui, "Join type", "join", join, &[("Round", "round"), ("Bevel", "bevel"), ("Miter", "miter")]);
    let arrows = [("None", "none"), ("Arrow", "triangle"), ("Open Arrow", "arrow"), ("Stealth", "stealth"), ("Diamond", "diamond"), ("Oval", "oval")];
    let sizes = [("Small", "sm"), ("Medium", "med"), ("Large", "lg")];
    for (label, key, size_label, size_key, end) in
        [("Begin arrow", "head", "Begin size", "headSize", &l.head), ("End arrow", "tail", "End size", "tailSize", &l.tail)]
    {
        line_choice(app, ui, label, key, end.as_ref().map_or("none", |e| e.kind.as_str()), &arrows);
        if let Some(e) = end {
            line_choice(app, ui, size_label, size_key, &e.w, &sizes);
        }
    }
}

/// A slider for one property of a shape effect: shows `value` (in display units), and when the
/// drag ends runs `shape.effects` with `{key: {prop: shown × scale}}`.
#[allow(clippy::too_many_arguments)]
fn fx_slider(
    app: &mut SlideApp,
    ui: &mut Ui,
    id: (&str, u32, &str),
    label: &str,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    key: &str,
    prop: &str,
    scale: f64,
) {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(90.0, 20.0), egui::Label::new(label));
        // Hold the value while dragging; otherwise follow the shape.
        let sid = ui.id().with(id);
        let mut v: f64 = ui.data_mut(|d| d.get_temp(sid)).unwrap_or(value);
        let r = ui.add(egui::Slider::new(&mut v, range).suffix(suffix).max_decimals(0));
        if r.dragged() || r.has_focus() {
            ui.data_mut(|d| d.insert_temp(sid, v));
        } else {
            ui.data_mut(|d| d.remove::<f64>(sid));
        }
        if r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged()) {
            ui.data_mut(|d| d.remove::<f64>(sid));
            run(app, "shape.effects", json!({key: {prop: v * scale}}));
        }
    });
}

fn background(app: &mut SlideApp, ui: &mut Ui) {
    let sc = scheme(app);
    let bg = app.session.active().and_then(|d| d.current_slide().and_then(|s| s.background.clone()));
    let fill = match &bg {
        Some(deckcraft_model::Background::Fill { fill }) => Some(fill.clone()),
        _ => None,
    };
    let kind = match &fill {
        Some(Fill::Solid { .. }) => "solid",
        Some(Fill::Gradient(_)) => "gradient",
        Some(Fill::Picture(_)) => "picture",
        Some(Fill::Pattern(_)) => "pattern",
        _ => "auto",
    };
    // What "Apply to All" re-applies: the current slide's background as command params.
    let mut current: Option<Value> = None;
    ui.label(egui::RichText::new("Fill").font(theme::bold(13.0)));
    for (l, k) in [
        ("Solid fill", "solid"),
        ("Gradient fill", "gradient"),
        ("Picture or texture fill", "picture"),
        ("Pattern fill", "pattern"),
        ("Follow the layout", "auto"),
    ] {
        if ui.radio(kind == k, l).clicked() && kind != k {
            match k {
                "solid" => run(app, "design.background", json!({"color": "bg1"})),
                "gradient" => {
                    let mut g = crate::fillui::default_gradient(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Bg2), 90.0);
                    g.rotate_with_shape = false;
                    run(app, "design.background", json!({"gradient": crate::fillui::gradient_json(&g)}));
                }
                "picture" => {
                    if let Some(pick) = app.services.pick_open.as_mut()
                        && let Some(path) = pick("picture")
                        && let Some(Ok(bytes)) = app.services.read.as_ref().map(|r| r(&path))
                    {
                        let _ = app.run("design.background", json!({"picture": deckcraft_engine::cmd::base64_encode(&bytes)}));
                    }
                }
                "pattern" => run(app, "design.background", json!({"pattern": {"preset": "pct20", "fg": "accent1", "bg": "bg1"}})),
                _ => run(app, "design.background", json!({"reset": true})),
            }
        }
    }
    ui.add_space(4.0);
    match &fill {
        Some(Fill::Solid { color }) => {
            current = Some(json!({"color": cref_param(color)}));
            ui.horizontal(|ui| {
                ui.label("Color");
                let r = widgets::drop_button(ui, "Color", vec2(70.0, 20.0), true);
                widgets::color_popup(&r).show(|ui| {
                    if let Some(Some(c)) = widgets::color_grid(ui, &sc, None) {
                        run(app, "design.background", json!({"color": cref_param(&c)}));
                    }
                });
            });
        }
        Some(Fill::Gradient(g)) => {
            current = Some(json!({"gradient": crate::fillui::gradient_json(g)}));
            if let Some(ng) = crate::fillui::gradient_editor(ui, "bg-grad", g, &sc) {
                run(app, "design.background", json!({"gradient": crate::fillui::gradient_json(&ng)}));
            }
        }
        Some(Fill::Pattern(pt)) => {
            current = Some(json!({"pattern": {"preset": pt.preset, "fg": cref_param(&pt.fg), "bg": cref_param(&pt.bg)}}));
            ui.horizontal_wrapped(|ui| {
                for preset in ["pct5", "pct20", "pct50", "ltHorz", "ltVert", "smGrid", "dkDnDiag", "wave", "dotGrid", "weave"] {
                    if ui.selectable_label(pt.preset == preset, preset).clicked() {
                        run(app, "design.background", json!({"pattern": {"preset": preset, "fg": cref_param(&pt.fg), "bg": cref_param(&pt.bg)}}));
                    }
                }
            });
            for (label, fg) in [("Foreground", true), ("Background", false)] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    let r = widgets::drop_button(ui, "Color", vec2(70.0, 20.0), true);
                    widgets::color_popup(&r).show(|ui| {
                        if let Some(Some(c)) = widgets::color_grid(ui, &sc, None) {
                            let (f, b) = if fg { (c, pt.bg.clone()) } else { (pt.fg.clone(), c) };
                            run(app, "design.background", json!({"pattern": {"preset": pt.preset, "fg": cref_param(&f), "bg": cref_param(&b)}}));
                        }
                    });
                });
            }
        }
        Some(Fill::Picture(_)) => {
            if ui.button("Insert picture from file…").clicked()
                && let Some(pick) = app.services.pick_open.as_mut()
                && let Some(path) = pick("picture")
                && let Some(Ok(bytes)) = app.services.read.as_ref().map(|r| r(&path))
            {
                let _ = app.run("design.background", json!({"picture": deckcraft_engine::cmd::base64_encode(&bytes)}));
            }
        }
        _ => {}
    }
    ui.add_space(4.0);
    let hide = app.session.active().and_then(|d| d.current_slide().map(|s| !s.show_master_shapes)).unwrap_or(false);
    let mut h = hide;
    if ui.checkbox(&mut h, "Hide background graphics").changed() {
        run(app, "design.hideBackgroundGraphics", json!({"hide": h}));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.add_enabled(current.is_some(), egui::Button::new("Apply to All")).clicked()
            && let Some(Value::Object(mut o)) = current.clone()
        {
            o.insert("all".into(), json!(true));
            run(app, "design.background", Value::Object(o));
        }
        if ui.button("Reset Background").clicked() {
            run(app, "design.background", json!({"reset": true}));
        }
    });
}

fn animation(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    let Some(slide) = st.current_slide() else { return };
    let anims = slide.animations.clone();
    let names: Vec<(u32, String)> = st.shapes().iter().map(|s| (s.id.0, s.name.clone())).collect();
    let selected: Vec<u32> = st.selection.shapes.iter().map(|i| i.0).collect();
    ui.horizontal(|ui| {
        if ui.button("▶ Play All").clicked() {
            let from = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
            app.start_show(from, true);
            if let Some(s) = app.show.as_mut() {
                s.preview_only = true;
                s.autoplay = true;
            }
        }
    });
    ui.separator();
    if anims.is_empty() {
        ui.label(egui::RichText::new("Select an object on the slide and add an animation from the Animations tab.").color(t.text_dim));
        return;
    }
    let mut click = 0;
    for (i, a) in anims.iter().enumerate() {
        if a.start == AnimStart::OnClick {
            click += 1;
        }
        let name = names.iter().find(|(id, _)| *id == a.shape.0).map(|(_, n)| n.clone()).unwrap_or_else(|| format!("Shape {}", a.shape));
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let sel = selected.contains(&a.shape.0);
        if sel || resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(4), if sel { t.selected } else { t.hover });
        }
        let p = ui.painter();
        p.text(
            pos2(r.min.x + 6.0, r.center().y),
            Align2::LEFT_CENTER,
            if a.start == AnimStart::OnClick { click.to_string() } else { String::new() },
            theme::font(12.0),
            t.text_dim,
        );
        let icon = match a.class {
            AnimClass::Entrance => Icon::AnimEntrance,
            AnimClass::Emphasis => Icon::AnimEmphasis,
            AnimClass::Exit => Icon::AnimExit,
            _ => Icon::AnimPath,
        };
        icons::paint(p, Rect::from_center_size(pos2(r.min.x + 30.0, r.center().y), vec2(18.0, 18.0)), icon, t.text, false);
        p.text(pos2(r.min.x + 44.0, r.center().y), Align2::LEFT_CENTER, &name, theme::font(12.5), t.text);
        // Timeline bar.
        let bar_x = r.max.x - 80.0;
        let w = (a.duration_ms as f32 / 1000.0 * 20.0).clamp(4.0, 70.0);
        let col = match a.class {
            AnimClass::Entrance => Color32::from_rgb(0x2E, 0xA3, 0x6F),
            AnimClass::Emphasis => Color32::from_rgb(0xE3, 0xB2, 0x1C),
            AnimClass::Exit => Color32::from_rgb(0xD8, 0x3F, 0x6B),
            _ => Color32::from_rgb(0x2E, 0x6F, 0xD8),
        };
        p.rect_filled(
            Rect::from_min_size(pos2(bar_x + (a.delay_ms as f32 / 1000.0 * 20.0).min(40.0), r.center().y - 4.0), vec2(w, 8.0)),
            CornerRadius::same(2),
            col,
        );
        if resp.clicked() {
            run(app, "edit.select", json!({"ids": [a.shape.0]}));
        }
        resp.context_menu(|ui| {
            for (l, v) in [("Start On Click", "onClick"), ("Start With Previous", "withPrevious"), ("Start After Previous", "afterPrevious")] {
                if ui.button(l).clicked() {
                    run(app, "animation.timing", json!({"index": i, "start": v}));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Move Up").clicked() {
                run(app, "animation.move", json!({"index": i, "to": i.saturating_sub(1)}));
                ui.close();
            }
            if ui.button("Move Down").clicked() {
                run(app, "animation.move", json!({"index": i, "to": i + 1}));
                ui.close();
            }
            if ui.button("Remove").clicked() {
                run(app, "animation.remove", json!({"index": i}));
                ui.close();
            }
        });
    }
    let _ = paint_anim_tile;
    let _ = Stroke::NONE;
}

fn comments(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if ui.button("+ New Comment").clicked() {
        app.dialog = Some(crate::dialogs::Dialog::new("comment"));
    }
    ui.separator();
    let threads: Vec<(usize, Vec<deckcraft_model::Comment>)> = app
        .session
        .active()
        .map(|st| st.doc.slides.iter().enumerate().filter(|(_, sl)| !sl.comments.is_empty()).map(|(i, sl)| (i, sl.comments.clone())).collect())
        .unwrap_or_default();
    if threads.is_empty() {
        ui.label(egui::RichText::new("No comments.").color(t.text_dim));
        return;
    }
    let focus = app.ui.comment_focus;
    for (si, list) in threads {
        ui.label(egui::RichText::new(format!("Slide {}", si + 1)).font(theme::bold(12.0)).color(t.text_dim));
        ui.add_space(2.0);
        for (i, c) in list.iter().enumerate() {
            comment_card(app, ui, si, i, c, focus == Some((si, i)));
            ui.add_space(6.0);
        }
    }
}

fn focus_comment(app: &mut SlideApp, slide: usize, index: usize) {
    let _ = app.run("slide.go", json!({"index": slide}));
    app.ui.comment_focus = Some((slide, index));
}

fn comment_card(app: &mut SlideApp, ui: &mut Ui, slide: usize, index: usize, c: &deckcraft_model::Comment, focused: bool) {
    let t = Tokens::get(ui.ctx());
    let stroke = if focused { Stroke::new(2.0, t.accent) } else { Stroke::NONE };
    let shown = egui::Frame::NONE
        .fill(if focused { t.selected } else { t.chrome })
        .stroke(stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let head = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 12.0, t.accent);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, &c.initials, theme::bold(11.0), Color32::WHITE);
                ui.vertical(|ui| {
                    let name = ui.add(egui::Label::new(egui::RichText::new(&c.author).font(theme::bold(12.5))).sense(Sense::click()));
                    if name.clicked() {
                        focus_comment(app, slide, index);
                    }
                    let when = c.date.replace('T', " ").trim_end_matches('Z').to_string();
                    let when = if c.resolved { format!("{when} · Resolved") } else { when };
                    ui.label(egui::RichText::new(when).size(10.5).color(t.text_faint));
                });
            });
            if head.response.clicked() {
                focus_comment(app, slide, index);
            }
            let body = ui.add(egui::Label::new(&c.text).sense(Sense::click()));
            if body.clicked() {
                focus_comment(app, slide, index);
            }
            for r in &c.replies {
                ui.label(egui::RichText::new(format!("↳ {}: {}", r.author, r.text)).color(t.text_dim));
            }
            ui.horizontal(|ui| {
                let id = ui.id().with(("reply", slide, index));
                let mut txt: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
                let r = ui.add(egui::TextEdit::singleline(&mut txt).hint_text("Reply…").desired_width(ui.available_width() - 70.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !txt.trim().is_empty() {
                    focus_comment(app, slide, index);
                    run(app, "comment.reply", json!({"index": index, "text": txt}));
                    txt.clear();
                }
                ui.data_mut(|d| d.insert_temp(id, txt));
                if ui.small_button(if c.resolved { "Reopen" } else { "Resolve" }).clicked() {
                    focus_comment(app, slide, index);
                    run(app, "comment.resolve", json!({"index": index, "resolved": !c.resolved}));
                }
                if ui.small_button("🗑").clicked() {
                    let _ = app.run("slide.go", json!({"index": slide}));
                    let _ = app.run("comment.delete", json!({"index": index}));
                    match app.ui.comment_focus {
                        Some((fs, fi)) if fs == slide && fi == index => app.ui.comment_focus = None,
                        Some((fs, fi)) if fs == slide && fi > index => app.ui.comment_focus = Some((fs, fi - 1)),
                        _ => {}
                    }
                }
            });
        });
    if focused {
        let key = egui::Id::new("comment-focus-scrolled");
        let seen = ui.data(|d| d.get_temp::<(usize, usize)>(key));
        if seen != Some((slide, index)) {
            ui.scroll_to_rect(shown.response.rect, Some(egui::Align::Center));
            ui.data_mut(|d| d.insert_temp(key, (slide, index)));
        }
    }
}

/// Design Ideas: offline suggestions — the current slide in each theme and layout variations.
/// Rendered idea thumbnails, kept until the deck, slide or pane width changes (rendering eight
/// slides every frame made the pane cost ~25 ms a frame).
#[derive(Clone, Default)]
struct IdeaThumbs {
    key: (u64, u64, usize, u32),
    tex: Vec<(String, egui::TextureHandle, u32, u32)>,
}

fn designer(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new("Pick a design for this slide. Ideas are generated on your device.").color(t.text_dim));
    ui.add_space(6.0);
    let Some(st) = app.session.active() else { return };
    let doc = st.doc.clone();
    let idx = st.selection.slide;
    let sel = st.selection.clone();
    let key_doc = (st.uid, st.revision);
    let w = ui.available_width() - 8.0;
    let ppp = ui.ctx().pixels_per_point();
    let px = (w * ppp).max(16.0) as u32;
    let ideas = deckcraft_engine::cmd::ideas::ideas_for(&doc, idx);
    let themes = deckcraft_model::theme::builtin_themes();
    let cache_id = egui::Id::new("design-idea-thumbs");
    let key = (key_doc.0, key_doc.1, idx, px);
    let mut cache: IdeaThumbs = ui.ctx().data_mut(|d| d.get_temp(cache_id)).unwrap_or_default();
    if cache.key != key || cache.tex.is_empty() {
        cache = IdeaThumbs { key, tex: vec![] };
        let mut render = |name: String, d: &deckcraft_model::Presentation| {
            let h = (px as f64 * d.slide_size.height / d.slide_size.width.max(1.0)) as u32;
            let img = deckcraft_render::render_slide(
                d,
                idx,
                &deckcraft_render::RenderOpts { scale: px as f64 / d.slide_size.width.max(1.0), size: Some((px, h)), ..Default::default() },
            );
            let tex = ui.ctx().load_texture(format!("idea-{name}"), crate::textures::to_color_image(&img, false), egui::TextureOptions::LINEAR);
            cache.tex.push((name, tex, px, h));
        };
        for (id, _) in &ideas {
            let mut d = (*doc).clone();
            if deckcraft_engine::cmd::ideas::apply_idea(&mut d, &sel, idx, id).is_ok() {
                render(format!("layout:{id}"), &d);
            }
        }
        for th in &themes {
            let mut d = (*doc).clone();
            if let Some(m) = d.masters.first_mut() {
                std::sync::Arc::make_mut(m).theme = th.clone();
            }
            render(format!("theme:{}", th.name), &d);
        }
        ui.ctx().data_mut(|d| d.insert_temp(cache_id, cache.clone()));
    }
    let thumb = |ui: &mut Ui, name: &str, tip: &str| -> bool {
        let Some((_, tex, tw, th)) = cache.tex.iter().find(|e| e.0 == name) else { return false };
        let (r, resp) = ui.allocate_exact_size(vec2(w, w * *th as f32 / (*tw).max(1) as f32), Sense::click());
        ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        ui.painter().rect_stroke(
            r,
            CornerRadius::same(2),
            Stroke::new(if resp.hovered() { 2.5 } else { 1.0 }, if resp.hovered() { t.accent } else { t.border }),
            egui::StrokeKind::Outside,
        );
        let clicked = resp.on_hover_text(tip).clicked();
        ui.add_space(8.0);
        clicked
    };
    if ideas.is_empty() {
        ui.label(egui::RichText::new("Add a title with a picture or some text to get layout ideas.").color(t.text_dim));
    }
    for (id, label) in &ideas {
        if thumb(ui, &format!("layout:{id}"), label) {
            run(app, "design.applyIdea", json!({"id": id}));
        }
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Themes (whole presentation)").color(t.text_dim));
    ui.add_space(4.0);
    for th in &themes {
        if thumb(ui, &format!("theme:{}", th.name), &th.name) {
            run(app, "design.theme", json!({"name": th.name}));
        }
    }
}

/// The transparency (0–1) of a shape's first text run as stored on it.
pub(crate) fn text_transparency_of(sh: &deckcraft_model::Shape) -> f64 {
    let run = sh.text.as_ref().and_then(|t| t.paragraphs.iter().flat_map(|p| p.runs.iter()).next());
    match run.and_then(|r| r.props.fill.as_ref()) {
        Some(Fill::Solid { color }) => 1.0 - color.alpha(),
        Some(Fill::Gradient(g)) => g.stops.first().map_or(0.0, |s| 1.0 - s.color.alpha()),
        _ => 0.0,
    }
}

/// Text Fill › Transparency, as in PowerPoint's Format Shape › Text Options (#32).
pub(crate) fn text_transparency_slider(app: &mut SlideApp, ui: &mut Ui, sh: &deckcraft_model::Shape) {
    ui.horizontal(|ui| {
        ui.label("Transparency");
        let id = ui.id().with(("text-transparency", sh.id));
        let stored = (text_transparency_of(sh) * 100.0) as f32;
        let mut v: f32 = ui.data_mut(|d| d.get_temp(id).unwrap_or(stored));
        let r = ui.add(egui::Slider::new(&mut v, 0.0..=100.0).suffix("%"));
        if r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged()) {
            let _ = app.run("format.textTransparency", json!({"value": v as f64 / 100.0}));
            ui.data_mut(|d| d.remove::<f32>(id));
        } else if r.dragged() {
            ui.data_mut(|d| d.insert_temp(id, v));
        }
    });
}
