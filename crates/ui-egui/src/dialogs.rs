//! Dialogs (Insert Table, Slide Size, Header & Footer, Hyperlink, Set Up Show, Zoom, …) and the
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
        "renameShape" => "Rename",
        "renameLayout" => "Rename Layout",
        "paragraph" => "Paragraph",
        "about" => "About DeckCraft",
        "preferences" => "Preferences",
        "export" => "Export",
        "symbol" => "Symbol",
        "photoAlbum" => "Photo Album",
        "replaceFonts" => "Replace Font",
        "equation" => "Equation",
        "spelling" => "Spelling",
        "pasteSpecial" => "Paste Special",
        "accessibility" => "Accessibility Checker",
        "language" => "Language",
        "customShow" => "Custom Shows",
        "chartData" => "Chart Data",
        "trim" => "Trim Media",
        "smartart" => "Choose a SmartArt Graphic",
        "bullets" => "Bullets and Numbering",
        "quit" => "DeckCraft",
        "themeColors" => "Create New Theme Colors",
        "themeFonts" => "Create New Theme Fonts",
        _ => "DeckCraft",
    }
}

pub fn show(app: &mut SlideApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    let mut open = true;
    let mut close = false;
    let mut window = egui::Window::new(title(&d.id))
        .id(egui::Id::new(("dialog", d.id.clone())))
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
    if open && !close && !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.dialog = Some(d);
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

fn field(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str, default: &str) {
    let mut v = d.get(key, default);
    ui.horizontal(|ui| {
        ui.add_sized(vec2(120.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(180.0));
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

fn album_items(d: &Dialog) -> Option<Vec<Value>> {
    let raw = d.fields.get("items").map(String::as_str).unwrap_or("[]");
    serde_json::from_str::<Value>(raw).ok().and_then(|v| v.as_array().cloned())
}

fn store_album_items(d: &mut Dialog, items: &[Value]) {
    let raw = serde_json::to_string(items).unwrap_or_else(|_| "[]".into());
    d.fields.insert("items".into(), raw);
}

fn album_item_label(v: &Value) -> String {
    if let Some(text) = v.get("text").and_then(Value::as_str) {
        let text = text.trim();
        if text.is_empty() { "Text Box".to_string() } else { format!("Text: {text}") }
    } else {
        v.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Picture").to_string()
    }
}

fn pick_album_picture(app: &mut SlideApp) -> Option<Value> {
    let path = if let Some(pick) = app.services.pick_open.as_mut() {
        pick("picture")
    } else {
        app.set_status("Pass pictures to insert.photoAlbum");
        None
    };
    let path = path?;
    let loaded = if let Some(read) = app.services.read.as_ref() {
        match read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                app.set_status(e);
                return None;
            }
        }
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    app.set_status(format!("{path}: {e}"));
                    return None;
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            app.set_status("The web build inserts album pictures from data passed to insert.photoAlbum");
            return None;
        }
    };
    if loaded.is_empty() {
        app.set_status("That file is empty");
        return None;
    }
    if loaded.len() > 32 * 1024 * 1024 {
        app.set_status("That picture is larger than 32 MB");
        return None;
    }
    let name = std::path::Path::new(&path).file_name().and_then(|n| n.to_str()).filter(|s| !s.is_empty()).unwrap_or("picture").to_string();
    Some(json!({"name": name, "data": deckcraft_engine::cmd::base64_encode(&loaded)}))
}

fn album_choice(ui: &mut Ui, d: &mut Dialog, id: &str, key: &str, default: &str, choices: &[(&str, &str)]) {
    let cur = d.get(key, default);
    let label = choices.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or(default);
    let opened = egui::ComboBox::from_id_salt(id).selected_text(label).width(220.0).show_ui(ui, |ui| {
        for (k, l) in choices {
            if ui.selectable_label(cur == *k, *l).clicked() {
                d.fields.insert(key.into(), (*k).to_string());
            }
        }
    });
    opened.response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, label));
}

const ALBUM_THEMES: &[&str] = &["DeckCraft", "Harbor", "Ember", "Meadow", "Nocturne", "Paper", "Coral Reef", "Slate"];

fn current_theme(app: &SlideApp) -> String {
    app.session
        .active()
        .and_then(|d| d.doc.masters.first())
        .map(|m| m.theme.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "DeckCraft".into())
}

fn album_json_id(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Copy a command's dialog params into fields once. A fresh album has no `items`.
fn seed_photo_album(app: &SlideApp, d: &mut Dialog) {
    if d.fields.contains_key("seeded") {
        return;
    }
    d.fields.insert("seeded".into(), "1".into());
    let p = d.params.clone();
    if let Some(items) = p.get("items").and_then(Value::as_array) {
        store_album_items(d, items);
        d.fields.insert("editing".into(), "1".into());
    }
    for key in ["layout", "frame", "theme"] {
        if let Some(v) = p.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
            d.fields.insert(key.into(), v.to_string());
        }
    }
    for key in ["captions", "grayscale"] {
        if let Some(v) = p.get(key).and_then(Value::as_bool) {
            d.fields.insert(key.into(), if v { "1".into() } else { "0".into() });
        }
    }
    if let Some(id) = p.get("album").and_then(album_json_id).filter(|n| *n != 0) {
        d.fields.insert("album".into(), id.to_string());
    }
    let theme_was = d.fields.get("theme").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| current_theme(app));
    if !d.fields.contains_key("theme") {
        d.fields.insert("theme".into(), theme_was.clone());
    }
    d.fields.entry("themeWas".into()).or_insert(theme_was);
}

fn selected_album_picture(d: &Dialog, items: &[Value]) -> bool {
    let i = d.fields.get("sel").and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
    items.get(i).is_some_and(|item| item.get("data").is_some() || item.get("path").is_some())
}

fn with_selected_picture(d: &mut Dialog, f: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    let Some(mut list) = album_items(d) else { return };
    let i = d.fields.get("sel").and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
    let Some(item) = list.get_mut(i) else { return };
    if item.get("data").is_none() && item.get("path").is_none() {
        return;
    }
    let Some(obj) = item.as_object_mut() else { return };
    f(obj);
    store_album_items(d, &list);
}

/// Photo Album. Returns true when the dialog should close.
fn photo_album_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    seed_photo_album(app, d);
    ui.set_min_width(440.0);
    egui::ScrollArea::vertical().max_height(520.0).id_salt("album_body").show(ui, |ui| {
        ui.set_min_width(420.0);
        ui.label("Pictures in album");
        let items = album_items(d).unwrap_or_default();
        let mut sel = d.get("sel", "0").parse::<usize>().unwrap_or(0);
        // Fixed height so adding pictures does not push Create past the window.
        egui::ScrollArea::vertical().max_height(120.0).min_scrolled_height(120.0).auto_shrink([false, false]).id_salt("album_list").show(ui, |ui| {
            if items.is_empty() {
                ui.label(egui::RichText::new("No pictures yet").color(Tokens::get(ui.ctx()).text_dim));
            }
            for (i, item) in items.iter().enumerate() {
                if ui.selectable_label(sel == i, album_item_label(item)).clicked() {
                    sel = i;
                }
            }
        });
        d.fields.insert("sel".into(), sel.to_string());
        ui.horizontal(|ui| {
            if ui.button("Add Pictures…").clicked()
                && let Some(item) = pick_album_picture(app)
            {
                match album_items(d) {
                    Some(mut list) if list.len() < 48 => {
                        list.push(item);
                        let next = list.len().saturating_sub(1);
                        store_album_items(d, &list);
                        d.fields.insert("sel".into(), next.to_string());
                    }
                    Some(_) => app.set_status("A photo album holds at most 48 pictures or text boxes"),
                    None => app.set_status("The picture list is invalid"),
                }
            }
            if ui.button("New Text Box").clicked() {
                let text = d.get("boxText", "Text Box");
                let text = text.trim();
                if text.is_empty() {
                    app.set_status("Type the text for the new text box");
                } else if text.chars().count() > 400 {
                    app.set_status("Text is longer than 400 characters");
                } else if let Some(mut list) = album_items(d) {
                    if list.len() >= 48 {
                        app.set_status("A photo album holds at most 48 pictures or text boxes");
                    } else {
                        list.push(json!({"text": text}));
                        let next = list.len().saturating_sub(1);
                        store_album_items(d, &list);
                        d.fields.insert("sel".into(), next.to_string());
                    }
                }
            }
            if ui.button("Remove").clicked()
                && let Some(mut list) = album_items(d)
                && let Some(i) = d.get("sel", "0").parse::<usize>().ok()
                && i < list.len()
            {
                list.remove(i);
                store_album_items(d, &list);
                let next = i.min(list.len().saturating_sub(1));
                d.fields.insert("sel".into(), next.to_string());
            }
        });
        field(ui, d, "Text box:", "boxText", "Text Box");
        let picture = selected_album_picture(d, &items);
        ui.label(album_picture_status(&items, sel));
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(picture, |ui| {
                if ui.button("Rotate Left").clicked() {
                    with_selected_picture(d, |obj| {
                        let cur = obj.get("rot").and_then(Value::as_f64).unwrap_or(0.0);
                        let next = ((cur - 90.0) % 360.0 + 360.0) % 360.0;
                        obj.insert("rot".into(), json!(next));
                    });
                }
                if ui.button("Rotate Right").clicked() {
                    with_selected_picture(d, |obj| {
                        let cur = obj.get("rot").and_then(Value::as_f64).unwrap_or(0.0);
                        let next = ((cur + 90.0) % 360.0 + 360.0) % 360.0;
                        obj.insert("rot".into(), json!(next));
                    });
                }
                if ui.button("More Brightness").clicked() {
                    nudge_picture(d, "brightness", 0.1);
                }
                if ui.button("Less Brightness").clicked() {
                    nudge_picture(d, "brightness", -0.1);
                }
                if ui.button("More Contrast").clicked() {
                    nudge_picture(d, "contrast", 0.1);
                }
                if ui.button("Less Contrast").clicked() {
                    nudge_picture(d, "contrast", -0.1);
                }
            });
        });
        ui.add_space(4.0);
        ui.label("Picture layout");
        album_choice(
            ui,
            d,
            "album_layout",
            "layout",
            "1",
            &[
                ("fit", "Fit to slide"),
                ("1", "1 picture"),
                ("2", "2 pictures"),
                ("4", "4 pictures"),
                ("1title", "1 picture with title"),
                ("2title", "2 pictures with title"),
                ("4title", "4 pictures with title"),
            ],
        );
        ui.label("Frame shape");
        album_choice(
            ui,
            d,
            "album_frame",
            "frame",
            "rect",
            &[("none", "None"), ("rect", "Rectangle"), ("rounded", "Rounded"), ("simple", "Simple, white")],
        );
        check(ui, d, "Captions under pictures", "captions", false);
        check(ui, d, "ALL pictures black and white", "grayscale", false);
        ui.label("Album theme");
        album_choice(ui, d, "album_theme", "theme", "DeckCraft", &ALBUM_THEMES.iter().map(|n| (*n, *n)).collect::<Vec<_>>());
    });
    let captions = d.fields.get("captions").is_some_and(|v| v == "1");
    let grayscale = d.fields.get("grayscale").is_some_and(|v| v == "1");
    let editing = d.fields.get("editing").is_some_and(|v| v == "1");
    let (ok, cancel) = buttons(ui, if editing { "Update" } else { "Create" });
    if ok {
        let Some(list) = album_items(d) else {
            app.set_status("The picture list is invalid");
            return false;
        };
        if list.is_empty() {
            app.set_status("Add a picture or a text box first");
            return false;
        }
        let layout = d.get("layout", "1");
        let frame = d.get("frame", "rect");
        let theme = d.get("theme", "DeckCraft");
        let theme_was = d.fields.get("themeWas").cloned().unwrap_or_default();
        let mut payload = json!({"pictures": list, "layout": layout, "frame": frame, "captions": captions, "grayscale": grayscale});
        if theme != theme_was
            && let Some(obj) = payload.as_object_mut()
        {
            obj.insert("theme".into(), json!(theme));
        }
        if editing {
            if let Some(album) = d.fields.get("album").filter(|s| !s.is_empty() && s.as_str() != "0")
                && let Ok(id) = album.parse::<u64>()
                && let Some(obj) = payload.as_object_mut()
            {
                obj.insert("album".into(), json!(id));
            }
            return app.run("insert.photoAlbumEdit", payload).is_ok();
        }
        return app.run("insert.photoAlbum", payload).is_ok();
    }
    cancel
}

fn album_picture_status(items: &[Value], sel: usize) -> String {
    let Some(item) = items.get(sel) else {
        return "Select a picture to rotate it or change brightness and contrast".into();
    };
    if item.get("data").is_none() && item.get("path").is_none() {
        return "Text boxes have no picture options".into();
    }
    let rot = item.get("rot").and_then(Value::as_f64).unwrap_or(0.0);
    let brightness = item.get("brightness").and_then(Value::as_f64).unwrap_or(0.0);
    let contrast = item.get("contrast").and_then(Value::as_f64).unwrap_or(0.0);
    format!("Rotation {rot:.0}°, brightness {brightness:.1}, contrast {contrast:.1}")
}

fn nudge_picture(d: &mut Dialog, key: &str, delta: f64) {
    with_selected_picture(d, |obj| {
        let cur = obj.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let next = (((cur + delta) * 10.0).round() / 10.0).clamp(-1.0, 1.0);
        obj.insert(key.into(), json!(next));
    });
}

fn seed_theme_colors(d: &mut Dialog) {
    if d.fields.contains_key("seeded") {
        return;
    }
    let name = d.params.get("name").and_then(Value::as_str).unwrap_or("Custom");
    d.fields.insert("schemeName".into(), name.to_string());
    let obj = d.params.get("colors").and_then(Value::as_object);
    for slot in SchemeSlot::THEME {
        let hex = obj.and_then(|o| o.get(slot.xml_name())).and_then(Value::as_str).unwrap_or("000000");
        d.fields.insert(slot.xml_name().into(), hex.to_string());
    }
    d.fields.insert("seeded".into(), "1".into());
}

fn theme_color_row(ui: &mut Ui, d: &mut Dialog, slot: SchemeSlot) {
    let key = slot.xml_name();
    let mut v = d.get(key, "000000");
    ui.horizontal(|ui| {
        ui.add_sized(vec2(188.0, 18.0), egui::Label::new(slot.label()));
        let fill = deckcraft_color::Rgba::from_hex(&v).map(theme::to_color32).unwrap_or_else(|| Color32::from_gray(200));
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        ui.painter().rect(rect, CornerRadius::same(2), fill, Stroke::new(1.0, Tokens::get(ui.ctx()).border), egui::StrokeKind::Inside);
        let resp = ui.add(egui::TextEdit::singleline(&mut v).desired_width(84.0).id_salt(("theme-hex", key)));
        let shown = v.clone();
        resp.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, format!("{} hex", slot.label()));
            info.current_text_value = Some(shown.clone());
            info
        });
    });
    d.fields.insert(key.into(), v);
}

fn theme_color_payload(d: &mut Dialog) -> Result<Value, String> {
    let name = d.get("schemeName", "").trim().to_string();
    if name.is_empty() {
        return Err("Enter a scheme name.".into());
    }
    let mut colors = serde_json::Map::new();
    for slot in SchemeSlot::THEME {
        let raw = d.get(slot.xml_name(), "");
        let Some(rgb) = deckcraft_color::Rgba::from_hex(raw.trim()) else {
            return Err(format!("{} needs a colour like #RRGGBB.", slot.label()));
        };
        colors.insert(slot.xml_name().into(), Value::String(format!("#{}", rgb.hex())));
    }
    Ok(json!({"name": name, "colors": colors}))
}

/// Create New Theme Colors. Returns true when the dialog should close.
fn theme_colors_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    seed_theme_colors(d);
    // The window keeps the height of its contents and will not grow after the first frame, so the
    // rows stay short enough that Save is inside that rect.
    ui.style_mut().spacing.interact_size.y = 16.0;
    ui.style_mut().spacing.item_spacing.y = 2.0;
    ui.style_mut().spacing.button_padding.y = 2.0;
    let left = [SchemeSlot::Dk1, SchemeSlot::Lt1, SchemeSlot::Dk2, SchemeSlot::Lt2, SchemeSlot::Hlink, SchemeSlot::FolHlink];
    let right = [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6];
    egui::Grid::new("theme-colors").spacing(vec2(18.0, 4.0)).show(ui, |ui| {
        for (a, b) in left.into_iter().zip(right) {
            theme_color_row(ui, d, a);
            theme_color_row(ui, d, b);
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    let mut name = d.get("schemeName", "");
    ui.horizontal(|ui| {
        ui.label("Name:");
        let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(280.0).id_salt("schemeName"));
        let shown = name.clone();
        resp.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Scheme name");
            info.current_text_value = Some(shown.clone());
            info
        });
    });
    d.fields.insert("schemeName".into(), name);
    ui.label(egui::RichText::new("Applies to this presentation's theme colours.").size(11.0).color(Tokens::get(ui.ctx()).text_dim));
    let (ok, cancel) = buttons(ui, "Save");
    if ok {
        match theme_color_payload(d) {
            Ok(payload) => return app.run("design.colors", payload).is_ok(),
            Err(msg) => {
                app.set_status(msg);
                return false;
            }
        }
    }
    cancel
}

fn seed_theme_fonts(d: &mut Dialog) {
    if d.fields.contains_key("seeded") {
        return;
    }
    let name = d.params.get("name").and_then(Value::as_str).unwrap_or("Custom");
    let major = d.params.get("major").and_then(Value::as_str).unwrap_or("Inter");
    let minor = d.params.get("minor").and_then(Value::as_str).unwrap_or(major);
    d.fields.insert("schemeName".into(), name.to_string());
    d.fields.insert("major".into(), major.to_string());
    d.fields.insert("minor".into(), minor.to_string());
    d.fields.insert("seeded".into(), "1".into());
}

fn theme_font_choices(app: &SlideApp, selected: &str) -> Vec<String> {
    let mut v = crate::ribbon::font_families(app);
    for scheme in deckcraft_model::theme::builtin_font_schemes() {
        v.push(scheme.major.latin);
        v.push(scheme.minor.latin);
    }
    if !selected.is_empty() {
        v.push(selected.to_string());
    }
    let mut seen = std::collections::HashSet::new();
    v.retain(|face| !face.is_empty() && seen.insert(face.clone()));
    v
}

fn font_combo(ui: &mut Ui, label: &str, choices: &[String], selected: &mut String) {
    let resp = egui::ComboBox::from_id_salt(label).selected_text(selected.as_str()).width(240.0).show_ui(ui, |ui| {
        for face in choices {
            if ui.selectable_label(selected == face, face).clicked() {
                *selected = face.clone();
            }
        }
    });
    let shown = selected.clone();
    let access = label.to_string();
    resp.response.widget_info(move || {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, access.clone());
        info.current_text_value = Some(shown.clone());
        info
    });
}

fn theme_font_payload(d: &mut Dialog) -> Result<Value, String> {
    let name = d.get("schemeName", "").trim().to_string();
    if name.is_empty() {
        return Err("Enter a scheme name.".into());
    }
    let major = d.get("major", "").trim().to_string();
    if major.is_empty() {
        return Err("Choose a heading font.".into());
    }
    let minor = d.get("minor", "").trim().to_string();
    if minor.is_empty() {
        return Err("Choose a body font.".into());
    }
    Ok(json!({"name": name, "major": major, "minor": minor}))
}

/// Create New Theme Fonts. Returns true when the dialog should close.
fn theme_fonts_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    seed_theme_fonts(d);
    let major = d.get("major", "Inter");
    let minor = d.get("minor", "Inter");
    let headings = theme_font_choices(app, &major);
    let bodies = theme_font_choices(app, &minor);
    let mut major = major;
    let mut minor = minor;
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 18.0), egui::Label::new("Heading:"));
        font_combo(ui, "Heading font", &headings, &mut major);
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 18.0), egui::Label::new("Body:"));
        font_combo(ui, "Body font", &bodies, &mut minor);
    });
    d.fields.insert("major".into(), major.clone());
    d.fields.insert("minor".into(), minor.clone());
    ui.add_space(4.0);
    // Theme faces are not egui families; naming an unbound family panics. The lines show the
    // chosen names at heading and body sizes.
    ui.label(egui::RichText::new(format!("Heading sample  {major}")).size(20.0));
    ui.label(egui::RichText::new(format!("Body sample  {minor}")).size(14.0));
    ui.add_space(4.0);
    let mut name = d.get("schemeName", "");
    ui.horizontal(|ui| {
        ui.add_sized(vec2(110.0, 18.0), egui::Label::new("Name:"));
        let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0).id_salt("schemeName"));
        let shown = name.clone();
        resp.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Scheme name");
            info.current_text_value = Some(shown.clone());
            info
        });
    });
    d.fields.insert("schemeName".into(), name);
    ui.label(egui::RichText::new("Heading and body fonts for this presentation.").size(11.0).color(Tokens::get(ui.ctx()).text_dim));
    let (ok, cancel) = buttons(ui, "Save");
    if ok {
        match theme_font_payload(d) {
            Ok(payload) => return app.run("design.fonts", payload).is_ok(),
            Err(msg) => {
                app.set_status(msg);
                return false;
            }
        }
    }
    cancel
}

/// Returns true when the dialog should close.
fn body(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    let run = |app: &mut SlideApp, id: &str, p: Value| {
        let _ = app.run(id, p);
    };
    match d.id.as_str() {
        "table" => {
            field(ui, d, "Number of columns:", "cols", "5");
            field(ui, d, "Number of rows:", "rows", "2");
            let (ok, cancel) = buttons(ui, "Insert");
            if ok {
                let (r, c) = (num(d, "rows", 2.0) as usize, num(d, "cols", 5.0) as usize);
                run(app, "insert.table", json!({"rows": r, "cols": c}));
            }
            ok || cancel
        }
        "slideSize" => {
            let cur = app.session.active().map(|s| s.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
            ui.label("Slides sized for:");
            let preset = d.get("preset", "");
            egui::ComboBox::from_id_salt("sizes").selected_text(if preset.is_empty() { "Custom" } else { &preset }).show_ui(ui, |ui| {
                for (label, w, h) in deckcraft_model::defaults::SLIDE_SIZES {
                    if ui.selectable_label(preset == *label, *label).clicked() {
                        d.fields.insert("preset".into(), label.to_string());
                        d.fields.insert("w".into(), format!("{:.2}", w / 72.0));
                        d.fields.insert("h".into(), format!("{:.2}", h / 72.0));
                    }
                }
            });
            field(ui, d, "Width (in):", "w", &format!("{:.2}", cur.width / 72.0));
            field(ui, d, "Height (in):", "h", &format!("{:.2}", cur.height / 72.0));
            let (w, h) = (num(d, "w", cur.width / 72.0), num(d, "h", cur.height / 72.0));
            ui.horizontal(|ui| {
                ui.label("Orientation:");
                let portrait = ui.radio(h > w, "Portrait").clicked();
                let landscape = ui.radio(w >= h, "Landscape").clicked();
                // Picking the other orientation swaps the width and height fields.
                if portrait && w > h || landscape && h > w {
                    d.fields.insert("w".into(), format!("{h:.2}"));
                    d.fields.insert("h".into(), format!("{w:.2}"));
                }
            });
            let maximize = check(ui, d, "Maximize content (else ensure fit)", "max", false);
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                let (w, h) = (num(d, "w", cur.width / 72.0) * 72.0, num(d, "h", cur.height / 72.0) * 72.0);
                run(app, "design.slideSize", json!({"w": w, "h": h, "scale": if maximize { "maximize" } else { "ensureFit" }}));
            }
            ok || cancel
        }
        "sizePosition" => size_position_dialog(app, ui, d),
        "headerFooter" => {
            let hf = app.session.active().map(|s| s.doc.header_footer.clone()).unwrap_or_default();
            let date = check(ui, d, "Date and time", "date", hf.date);
            field(ui, d, "  Fixed date (empty = update automatically):", "dateText", &hf.date_text);
            let num_on = check(ui, d, "Slide number", "num", hf.slide_number);
            let foot = check(ui, d, "Footer", "footer", hf.footer);
            field(ui, d, "  Footer text:", "footerText", &hf.footer_text);
            let hide = check(ui, d, "Don't show on title slide", "hide", hf.hide_on_title);
            ui.add_space(6.0);
            let mut apply_all = false;
            let mut apply = false;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                apply_all = ui.button("Apply to All").clicked();
                apply = ui.button("Apply").clicked();
                if ui.button("Cancel").clicked() {
                    d.fields.insert("cancel".into(), "1".into());
                }
            });
            if apply || apply_all {
                let dt = d.get("dateText", "");
                let ft = d.get("footerText", "");
                run(
                    app,
                    "design.headerFooter",
                    json!({"date": date, "dateText": dt, "slideNumber": num_on, "footer": foot, "footerText": ft, "hideOnTitle": hide, "all": apply_all}),
                );
            }
            apply || apply_all || d.fields.contains_key("cancel")
        }
        "hyperlink" => {
            let mode = d.get("mode", "url");
            ui.horizontal(|ui| {
                if ui.selectable_label(mode == "url", "Web Page or File").clicked() {
                    d.fields.insert("mode".into(), "url".into());
                }
                if ui.selectable_label(mode == "slide", "This Document").clicked() {
                    d.fields.insert("mode".into(), "slide".into());
                }
                if ui.selectable_label(mode == "email", "Email Address").clicked() {
                    d.fields.insert("mode".into(), "email".into());
                }
            });
            match mode.as_str() {
                "slide" => {
                    let titles: Vec<String> = app
                        .session
                        .active()
                        .map(|s| s.doc.slides.iter().enumerate().map(|(i, sl)| format!("{}. {}", i + 1, sl.title())).collect())
                        .unwrap_or_default();
                    let sel = d.get("slide", "0").parse::<usize>().unwrap_or(0);
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for (i, t) in titles.iter().enumerate() {
                            if ui.selectable_label(sel == i, t).clicked() {
                                d.fields.insert("slide".into(), i.to_string());
                            }
                        }
                    });
                }
                "email" => field(ui, d, "Email address:", "email", ""),
                _ => field(ui, d, "Address:", "url", "https://"),
            }
            field(ui, d, "ScreenTip:", "tip", "");
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                let tip = d.get("tip", "");
                let p = match mode.as_str() {
                    "slide" => json!({"slide": d.get("slide", "0").parse::<usize>().unwrap_or(0), "tooltip": tip}),
                    "email" => json!({"url": format!("mailto:{}", d.get("email", "")), "tooltip": tip}),
                    _ => json!({"url": d.get("url", ""), "tooltip": tip}),
                };
                run(app, "insert.hyperlink", p);
            }
            ok || cancel
        }
        "setupShow" => {
            let sh = app.session.active().map(|s| s.doc.show.clone()).unwrap_or_default();
            let ty = d.get("type", &sh.show_type);
            ui.label("Show type");
            for (l, v) in [
                ("Presented by a speaker (full screen)", "speaker"),
                ("Browsed by an individual (window)", "browsed"),
                ("Browsed at a kiosk (full screen)", "kiosk"),
            ] {
                if ui.radio(ty == v, l).clicked() {
                    d.fields.insert("type".into(), v.into());
                }
            }
            let lp = check(ui, d, "Loop continuously until 'Esc'", "loop", sh.loop_until_esc);
            let nn = check(ui, d, "Show without narration", "nn", sh.without_narration);
            let na = check(ui, d, "Show without animation", "na", sh.without_animation);
            let ut = check(ui, d, "Use timings, if present", "ut", sh.use_timings);
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                let ty = d.get("type", "speaker");
                run(app, "show.setup", json!({"type": ty, "loop": lp, "noNarration": nn, "noAnimation": na, "useTimings": ut}));
            }
            ok || cancel
        }
        "quit" => {
            let names: Vec<String> = app.session.documents().iter().filter(|d| d.is_dirty()).map(|d| d.title()).collect();
            if names.is_empty() {
                app.quit_confirmed = true;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return true;
            }
            let what = if names.len() == 1 { format!("“{}”", names[0]) } else { format!("{} presentations", names.len()) };
            ui.label(egui::RichText::new(format!("Do you want to save the changes you made to {what}?")).strong());
            ui.label("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            let mut done = false;
            ui.horizontal(|ui| {
                let t = Tokens::get(ui.ctx());
                if ui.add(egui::Button::new("Don't Save").min_size(vec2(84.0, 24.0))).clicked() {
                    let _ = app.session.execute("file.recovery.discard", &json!({}));
                    app.quit_confirmed = true;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui.add(egui::Button::new(egui::RichText::new("Save").color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0)));
                    if save.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if app.save_all() {
                            app.quit_confirmed = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        done = true;
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() {
                        done = true;
                    }
                });
            });
            done
        }
        "close" => {
            // File › Close and Ctrl+W on a presentation with unsaved changes.
            let Some(name) = app.session.active().map(|d| d.title()) else { return true };
            ui.label(egui::RichText::new(format!("Do you want to save the changes you made to “{name}”?")).strong());
            ui.label("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            let mut done = false;
            ui.horizontal(|ui| {
                let t = Tokens::get(ui.ctx());
                if ui.add(egui::Button::new("Don't Save").min_size(vec2(84.0, 24.0))).clicked() {
                    run(app, "file.close", json!({}));
                    done = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui.add(egui::Button::new(egui::RichText::new("Save").color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0)));
                    if save.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        app.save();
                        // In the browser a save is a download: the presentation stays "unsaved".
                        let web = app.services.pick_save.is_none() && app.services.download.is_some();
                        if web || app.session.active().is_some_and(|d| !d.is_dirty()) {
                            run(app, "file.close", json!({}));
                        }
                        done = true;
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() {
                        done = true;
                    }
                });
            });
            done
        }
        "zoom" => {
            for pct in [400, 200, 150, 100, 75, 66, 50, 33] {
                if ui.radio(false, format!("{pct}%")).clicked() {
                    let _ = app.run("view.zoom", json!({"percent": pct}));
                }
            }
            if ui.button("Fit").clicked() {
                app.ui.zoom = None;
            }
            let (ok, cancel) = buttons(ui, "OK");
            ok || cancel
        }
        "comment" => {
            let mut v = d.get("text", "");
            ui.add(egui::TextEdit::multiline(&mut v).hint_text("Start a conversation").desired_rows(4).desired_width(320.0));
            d.fields.insert("text".into(), v.clone());
            let (ok, cancel) = buttons(ui, "Post");
            if ok && !v.trim().is_empty() {
                run(app, "comment.add", json!({"text": v}));
                app.ui.pane = Some("comments".into());
            }
            ok || cancel
        }
        "bullets" => {
            // The current paragraph's list, as the dialog's starting point.
            let cur = app.session.execute("format.state", &json!({})).ok().unwrap_or(Value::Null);
            let numbered = cur.get("numbering").and_then(Value::as_bool).unwrap_or(false);
            let tab = d.get("tab", if numbered { "numbered" } else { "bulleted" });
            ui.horizontal(|ui| {
                for (k, l) in [("bulleted", "Bulleted"), ("numbered", "Numbered")] {
                    if ui.selectable_label(tab == k, l).clicked() {
                        d.fields.insert("tab".into(), k.into());
                    }
                }
            });
            ui.separator();
            let style_key = if tab == "numbered" { "scheme" } else { "char" };
            let current = d.get(style_key, if tab == "numbered" { "arabicPeriod" } else { "•" });
            let styles: &[(&str, &str)] = if tab == "numbered" {
                &[
                    ("arabicPeriod", "1. 2. 3."),
                    ("arabicParenR", "1) 2) 3)"),
                    ("romanUcPeriod", "I. II. III."),
                    ("romanLcPeriod", "i. ii. iii."),
                    ("alphaUcPeriod", "A. B. C."),
                    ("alphaLcParenR", "a) b) c)"),
                    ("alphaLcPeriod", "a. b. c."),
                ]
            } else {
                &[("•", "•"), ("○", "○"), ("■", "■"), ("□", "□"), ("◆", "◆"), ("➢", "➢"), ("✓", "✓"), ("–", "–"), ("★", "★")]
            };
            egui::Grid::new("bullet-styles").show(ui, |ui| {
                for (i, (k, l)) in styles.iter().enumerate() {
                    let r = ui.add_sized(
                        vec2(if tab == "numbered" { 88.0 } else { 44.0 }, 40.0),
                        egui::Button::selectable(current == *k, egui::RichText::new(*l).size(16.0)),
                    );
                    if r.clicked() {
                        d.fields.insert(style_key.into(), (*k).into());
                    }
                    if (i + 1) % if tab == "numbered" { 4 } else { 5 } == 0 {
                        ui.end_row();
                    }
                }
            });
            if tab == "bulleted" {
                field(ui, d, "Custom character:", "custom", "");
            } else {
                field(ui, d, "Start at:", "start", "1");
            }
            field(ui, d, "Size (% of text):", "size", "100");
            let colors = [
                ("", "Automatic"),
                ("tx1", "Text"),
                ("accent1", "Accent 1"),
                ("accent2", "Accent 2"),
                ("accent3", "Accent 3"),
                ("accent4", "Accent 4"),
                ("accent5", "Accent 5"),
                ("accent6", "Accent 6"),
            ];
            let color = d.get("color", "");
            ui.horizontal(|ui| {
                ui.add_sized(vec2(120.0, 20.0), egui::Label::new("Color:"));
                egui::ComboBox::from_id_salt("bullet-color")
                    .selected_text(colors.iter().find(|c| c.0 == color).map_or("Automatic", |c| c.1))
                    .show_ui(ui, |ui| {
                        for (k, l) in colors {
                            if ui.selectable_label(color == k, l).clicked() {
                                d.fields.insert("color".into(), k.into());
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                if ui.button("None").on_hover_text("Remove bullets and numbering").clicked() {
                    d.fields.insert("none".into(), "1".into());
                }
            });
            let (ok, cancel) = buttons(ui, "OK");
            let none = d.fields.contains_key("none");
            if none {
                run(app, if tab == "numbered" { "format.numbering" } else { "format.bullets" }, json!({"on": false}));
            } else if ok {
                let mut p = json!({"on": true});
                if let Some(size) = d.get("size", "100").trim().trim_end_matches('%').parse::<f64>().ok().filter(|v| v.is_finite()) {
                    p["size"] = json!(size.clamp(25.0, 400.0));
                }
                if !color.is_empty() {
                    p["color"] = json!(color);
                }
                if tab == "numbered" {
                    p["scheme"] = json!(d.get("scheme", "arabicPeriod"));
                    p["start"] = json!(d.get("start", "1").trim().parse::<u64>().unwrap_or(1).clamp(1, 32767));
                    run(app, "format.numbering", p);
                } else {
                    let custom = d.get("custom", "");
                    let ch = custom.chars().find(|c| !c.is_whitespace()).map(String::from).unwrap_or_else(|| d.get("char", "•"));
                    p["char"] = json!(ch);
                    run(app, "format.bullets", p);
                }
            }
            ok || cancel || none
        }
        "altText" => {
            let cur = app.session.active().and_then(|s| s.selected_shapes().first().map(|x| x.descr.clone())).unwrap_or_default();
            ui.label("How would you describe this object and its context to someone who is blind or has low vision?");
            let mut v = d.get("text", &cur);
            ui.add(egui::TextEdit::multiline(&mut v).desired_rows(4).desired_width(340.0));
            d.fields.insert("text".into(), v.clone());
            let deco = check(ui, d, "Mark as decorative", "deco", false);
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                run(app, "shape.altText", json!({"text": v, "decorative": deco}));
            }
            ok || cancel
        }
        "outline" => {
            ui.label("Paste or type an outline. Unindented lines become slide titles; tab-indented lines become bullets.");
            let mut v = d.get("text", "");
            ui.add(egui::TextEdit::multiline(&mut v).desired_rows(10).desired_width(380.0).code_editor());
            d.fields.insert("text".into(), v.clone());
            let (ok, cancel) = buttons(ui, "Insert");
            if ok {
                run(app, "slide.fromOutline", json!({"text": v}));
            }
            ok || cancel
        }
        "renameSection" | "renameShape" | "renameLayout" => {
            let cur = d.params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            field(ui, d, "Name:", "name", &cur);
            let (ok, cancel) = buttons(ui, "Rename");
            if ok {
                let name = d.get("name", "");
                match d.id.as_str() {
                    "renameSection" => run(app, "section.rename", json!({"index": d.params.get("index").cloned().unwrap_or_default(), "name": name})),
                    "renameShape" => run(app, "shape.rename", json!({"id": d.params.get("id").cloned().unwrap_or_default(), "name": name})),
                    _ => run(app, "master.renameLayout", json!({"name": name})),
                }
            }
            ok || cancel
        }
        "paragraph" => {
            field(ui, d, "Before text (in):", "left", "0");
            field(ui, d, "Special — hanging (in):", "hang", "0");
            field(ui, d, "Spacing before (pt):", "before", "0");
            field(ui, d, "Spacing after (pt):", "after", "0");
            field(ui, d, "Line spacing (lines):", "lines", "1.0");
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                run(
                    app,
                    "format.paragraph",
                    json!({"indentLeft": num(d, "left", 0.0) * 72.0, "indentFirst": -num(d, "hang", 0.0) * 72.0, "spaceBefore": num(d, "before", 0.0), "spaceAfter": num(d, "after", 0.0), "lineSpacing": num(d, "lines", 1.0)}),
                );
            }
            ok || cancel
        }
        "export" => {
            ui.label("Export the presentation as:");
            let fmt = d.get("fmt", "pdf");
            for (l, f) in [
                ("PDF document (.pdf)", "pdf"),
                ("PNG images (current slide)", "png"),
                ("PNG images (all slides)", "pngAll"),
                ("JPEG image (current slide)", "jpeg"),
                ("PowerPoint Presentation (.pptx)", "pptx"),
                ("Outline (.txt)", "outline"),
            ] {
                if ui.radio(fmt == f, l).clicked() {
                    d.fields.insert("fmt".into(), f.into());
                }
            }
            let pdf_layout = d.get("pdfLayout", "slides");
            if fmt == "pdf" {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Print layout:");
                    egui::ComboBox::from_id_salt("pdf_layout")
                        .selected_text(match pdf_layout.as_str() {
                            "notes" => "Notes Pages",
                            "h1" => "Handouts (1 slide per page)",
                            "h2" => "Handouts (2 slides per page)",
                            "h3" => "Handouts (3 slides per page)",
                            "h4" => "Handouts (4 slides per page)",
                            "h6" => "Handouts (6 slides per page)",
                            "h9" => "Handouts (9 slides per page)",
                            _ => "Full Page Slides",
                        })
                        .show_ui(ui, |ui| {
                            for (k, l) in [
                                ("slides", "Full Page Slides"),
                                ("notes", "Notes Pages"),
                                ("h1", "Handouts (1 slide per page)"),
                                ("h2", "Handouts (2 slides per page)"),
                                ("h3", "Handouts (3 slides per page)"),
                                ("h4", "Handouts (4 slides per page)"),
                                ("h6", "Handouts (6 slides per page)"),
                                ("h9", "Handouts (9 slides per page)"),
                            ] {
                                if ui.selectable_label(pdf_layout == k, l).clicked() {
                                    d.fields.insert("pdfLayout".into(), k.into());
                                }
                            }
                        });
                });
                let mut hidden = d.get("pdfHidden", "false") == "true";
                if ui.checkbox(&mut hidden, "Include hidden slides").changed() {
                    d.fields.insert("pdfHidden".into(), hidden.to_string());
                }
            }
            let pdf_params = {
                let (layout, per) = match pdf_layout.as_str() {
                    "notes" => ("notes", 0),
                    l if l.starts_with('h') => ("handouts", l[1..].parse().unwrap_or(6)),
                    _ => ("slides", 0),
                };
                json!({"layout": layout, "perPage": per, "includeHidden": d.get("pdfHidden", "false") == "true"})
            };
            let (ok, cancel) = buttons(ui, "Export");
            if ok {
                let extra = if fmt == "pdf" { pdf_params } else { json!({}) };
                app.export_as(&fmt, extra);
            }
            ok || cancel
        }
        "replaceFonts" => {
            let used: Vec<String> =
                app.session.execute("font.used", &json!({})).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
            let mut from = d.get("from", used.first().map(String::as_str).unwrap_or(""));
            let mut to = d.get("to", "");
            egui::Grid::new("replace_fonts").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
                ui.label("Replace:");
                egui::ComboBox::from_id_salt("rf_from").width(240.0).selected_text(from.clone()).show_ui(ui, |ui| {
                    for f in &used {
                        ui.selectable_value(&mut from, f.clone(), f);
                    }
                });
                ui.end_row();
                ui.label("With:");
                ui.vertical(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut to).desired_width(240.0).hint_text("Font name"));
                    let needle = to.trim().to_lowercase();
                    egui::ScrollArea::vertical().max_height(140.0).id_salt("rf_list").show(ui, |ui| {
                        for f in
                            crate::ribbon::font_families(app).iter().filter(|f| needle.is_empty() || f.to_lowercase().contains(&needle)).take(200)
                        {
                            if ui.selectable_label(f.eq_ignore_ascii_case(to.trim()), f).clicked() {
                                to = f.clone();
                            }
                        }
                    });
                });
                ui.end_row();
            });
            if let Some(msg) = d.fields.get("result") {
                ui.label(egui::RichText::new(msg.clone()).color(Tokens::get(ui.ctx()).text_dim));
            }
            d.fields.insert("from".into(), from.clone());
            d.fields.insert("to".into(), to.clone());
            let (ok, cancel) = buttons(ui, "Replace");
            if ok && !from.is_empty() && !to.trim().is_empty() {
                let n = app.run("font.replace", json!({"from": from, "to": to.trim()})).ok().and_then(|v| v["replaced"].as_u64()).unwrap_or(0);
                let msg =
                    if n == 0 { format!("“{from}” isn't used explicitly.") } else { format!("Replaced “{from}” with “{}”.", to.trim()) };
                d.fields.insert("result".into(), msg);
                d.fields.remove("from");
                // Stay open, like PowerPoint, so several fonts can be replaced in a row.
                return cancel;
            }
            cancel
        }
        "symbol" => {
            ui.label("Click a symbol to insert it at the insertion point.");
            let groups: [(&str, &str); 6] = [
                ("Punctuation", "–—‘’“”…•·§¶†‡‰′″‹›«»¡¿"),
                ("Currency", "$€£¥¢₹₩₽₺₿"),
                ("Math", "±×÷≠≈≤≥∞√∑∏∫∂∆∇∈∉∩∪⊂⊃∀∃¬∧∨⇒⇔°′"),
                ("Greek", "αβγδεζηθικλμνξοπρστυφχψωΓΔΘΛΞΠΣΦΨΩ"),
                ("Arrows", "←↑→↓↔↕⇐⇑⇒⇓↖↗↘↙⟵⟶"),
                ("Shapes", "■□▪▫▲△▶▷▼▽◀◁◆◇○●◎★☆✓✔✗✘♠♣♥♦"),
            ];
            for (g, chars) in groups {
                ui.label(egui::RichText::new(g).font(theme::bold(12.0)));
                ui.horizontal_wrapped(|ui| {
                    for c in chars.chars() {
                        if ui.add(egui::Button::new(egui::RichText::new(c.to_string()).size(16.0)).min_size(vec2(26.0, 26.0))).clicked() {
                            if app.session.active().is_some_and(|s| s.selection.text.is_none()) {
                                let _ = app.run("text.edit", json!({}));
                            }
                            let _ = app.run("insert.symbol", json!({"text": c.to_string()}));
                        }
                    }
                });
            }
            let (ok, cancel) = buttons(ui, "Close");
            ok || cancel
        }
        "equation" => {
            ui.label("Type an equation in linear form (e.g. a^2+b^2=c^2, x=(-b±√(b^2-4ac))/2a).");
            let mut v = d.get("eq", "");
            ui.add(egui::TextEdit::singleline(&mut v).desired_width(340.0).font(theme::font(15.0)));
            d.fields.insert("eq".into(), v.clone());
            let (ok, cancel) = buttons(ui, "Insert");
            if ok && !v.is_empty() {
                let pretty = pretty_equation(&v);
                let size = app.session.active().map(|s| s.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
                run(app, "insert.textBox", json!({"rect": [size.width / 2.0 - 150.0, size.height / 2.0 - 25.0, 300, 50], "text": pretty}));
                run(app, "text.exit", json!({}));
                run(app, "format.font", json!({"family": "Liberation Serif"}));
                run(app, "format.italic", json!({"on": true}));
                run(app, "format.size", json!({"size": 28}));
            }
            ok || cancel
        }
        "pasteSpecial" => {
            const OPTS: [(&str, &str); 4] = [
                ("destinationTheme", "Use Destination Theme"),
                ("keepSource", "Keep Source Formatting"),
                ("picture", "Picture (PNG)"),
                ("text", "Keep Text Only"),
            ];
            let has_shapes = !app.session.clipboard.shapes.is_empty();
            let mut cur = d.get("as", "destinationTheme");
            ui.label("As:");
            for (id, label) in OPTS {
                let ok = has_shapes || matches!(id, "destinationTheme" | "text");
                ui.add_enabled_ui(ok, |ui| ui.radio_value(&mut cur, id.to_string(), label));
            }
            d.fields.insert("as".into(), cur.clone());
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                run(app, "edit.pasteSpecial", json!({"as": cur}));
            }
            ok || cancel
        }
        "spelling" => {
            let v = app.session.execute("review.spelling", &json!({})).unwrap_or_default();
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ui.label("The spelling check is complete. No issues found.");
            }
            for item in &list {
                ui.horizontal(|ui| {
                    let w = item.get("word").and_then(Value::as_str).unwrap_or("");
                    ui.label(format!("Slide {}: “{w}”", item.get("slide").and_then(Value::as_u64).unwrap_or(0) + 1));
                    if ui.small_button("Go to").clicked() {
                        let _ = app.run("slide.go", json!({"index": item.get("slide").cloned().unwrap_or_default()}));
                    }
                });
            }
            let (ok, cancel) = buttons(ui, "Close");
            ok || cancel
        }
        "accessibility" => {
            let v = app.session.execute("review.accessibility", &json!({})).unwrap_or_default();
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ui.label("No accessibility issues found. People with disabilities should not have difficulty reading this document.");
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for item in &list {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Slide {}: {}{}",
                            item.get("slide").and_then(Value::as_u64).unwrap_or(0) + 1,
                            item.get("issue").and_then(Value::as_str).unwrap_or(""),
                            item.get("name").and_then(Value::as_str).map(|n| format!(" — {n}")).unwrap_or_default()
                        ));
                        if ui.small_button("Go to").clicked() {
                            let _ = app.run("slide.go", json!({"index": item.get("slide").cloned().unwrap_or_default()}));
                            if let Some(id) = item.get("shape") {
                                let _ = app.run("edit.select", json!({"ids": [id]}));
                            }
                        }
                    });
                }
            });
            let (ok, cancel) = buttons(ui, "Close");
            ok || cancel
        }
        "language" => {
            ui.label("Mark selected text as:");
            for l in [
                "English (United States)",
                "English (United Kingdom)",
                "Español",
                "Français",
                "Deutsch",
                "Italiano",
                "Português",
                "日本語",
                "中文",
                "العربية",
            ] {
                let _ = ui.selectable_label(l.starts_with("English (United States)"), l);
            }
            let (ok, cancel) = buttons(ui, "OK");
            ok || cancel
        }
        "customShow" => {
            let shows = app.session.active().map(|s| s.doc.custom_shows.clone()).unwrap_or_default();
            for s in &shows {
                ui.horizontal(|ui| {
                    ui.label(format!("{} ({} slides)", s.name, s.slides.len()));
                    if ui.small_button("Delete").clicked() {
                        let _ = app.run("show.customShow", json!({"name": s.name, "delete": true}));
                    }
                });
            }
            field(ui, d, "New show name:", "name", "Custom Show 1");
            field(ui, d, "Slides (e.g. 1,3,5):", "slides", "1");
            let (ok, cancel) = buttons(ui, "Create");
            if ok {
                let idx: Vec<usize> =
                    d.get("slides", "").split(',').filter_map(|s| s.trim().parse::<usize>().ok()).filter(|v| *v > 0).map(|v| v - 1).collect();
                run(app, "show.customShow", json!({"name": d.get("name", "Custom Show"), "slides": idx}));
            }
            ok || cancel
        }
        "chartData" => {
            let ch = app.session.active().and_then(|s| {
                s.selected_shapes()
                    .into_iter()
                    .find_map(|x| if let deckcraft_model::ShapeKind::Chart(c) = &x.kind { Some(((**c).clone(), x.id)) } else { None })
            });
            let Some((c, id)) = ch else {
                ui.label("Select a chart first.");
                let (ok, cancel) = buttons(ui, "Close");
                return ok || cancel;
            };
            ui.label("Edit the data: first row = series names, first column = categories.");
            let mut grid: Vec<Vec<String>> = d.params.get("grid").and_then(|g| serde_json::from_value(g.clone()).ok()).unwrap_or_else(|| {
                let mut g = vec![std::iter::once(String::new()).chain(c.series.iter().map(|s| s.name.clone())).collect::<Vec<_>>()];
                for (i, cat) in c.categories.iter().enumerate() {
                    let mut row = vec![cat.clone()];
                    for s in &c.series {
                        row.push(s.values.get(i).copied().flatten().map(|v| v.to_string()).unwrap_or_default());
                    }
                    g.push(row);
                }
                g
            });
            egui::Grid::new("chartdata").striped(true).show(ui, |ui| {
                for row in grid.iter_mut() {
                    for cell in row.iter_mut() {
                        ui.add(egui::TextEdit::singleline(cell).desired_width(70.0));
                    }
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("+ Row").clicked() {
                    let n = grid.first().map(|r| r.len()).unwrap_or(2);
                    grid.push(vec![String::new(); n]);
                }
                if ui.button("+ Series").clicked() {
                    for r in grid.iter_mut() {
                        r.push(String::new());
                    }
                }
            });
            d.params = json!({"grid": grid});
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                let names: Vec<String> = grid.first().map(|r| r.iter().skip(1).cloned().collect()).unwrap_or_default();
                let cats: Vec<String> = grid.iter().skip(1).map(|r| r.first().cloned().unwrap_or_default()).collect();
                let series: Vec<Value> = names.iter().enumerate().map(|(k, n)| json!({"name": n, "values": grid.iter().skip(1).map(|r| r.get(k + 1).and_then(|v| v.trim().parse::<f64>().ok())).collect::<Vec<_>>()})).collect();
                run(app, "chart.data", json!({"id": id.0, "categories": cats, "series": series}));
            }
            ok || cancel
        }
        "trim" => {
            field(ui, d, "Start time (s):", "start", "0");
            field(ui, d, "End trim (s):", "end", "0");
            field(ui, d, "Fade in (s):", "fin", "0");
            field(ui, d, "Fade out (s):", "fout", "0");
            let (ok, cancel) = buttons(ui, "OK");
            if ok {
                run(
                    app,
                    "media.options",
                    json!({"trimStart": (num(d, "start", 0.0) * 1000.0) as u64, "trimEnd": (num(d, "end", 0.0) * 1000.0) as u64, "fadeIn": (num(d, "fin", 0.0) * 1000.0) as u64, "fadeOut": (num(d, "fout", 0.0) * 1000.0) as u64}),
                );
            }
            ok || cancel
        }
        "smartart" => {
            ui.label("Pick a graphic; type one item per line.");
            let kind = d.get("kind", "process");
            ui.horizontal_wrapped(|ui| {
                for (k, l) in [
                    ("list", "Basic List"),
                    ("process", "Basic Process"),
                    ("cycle", "Basic Cycle"),
                    ("hierarchy", "Hierarchy"),
                    ("pyramid", "Pyramid"),
                    ("matrix", "Matrix"),
                ] {
                    if ui.selectable_label(kind == k, l).clicked() {
                        d.fields.insert("kind".into(), k.into());
                    }
                }
            });
            let mut v = d.get("text", "Plan\nBuild\nLaunch");
            ui.add(egui::TextEdit::multiline(&mut v).desired_rows(5).desired_width(320.0));
            d.fields.insert("text".into(), v.clone());
            let (ok, cancel) = buttons(ui, "Insert");
            if ok {
                run(app, "insert.smartArt", json!({"kind": kind, "items": v.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>()}));
            }
            ok || cancel
        }
        "about" => {
            let tab = d.get("tab", "about");
            ui.horizontal(|ui| {
                for (k, l) in [("about", "About"), ("contributors", "Contributors"), ("models", "Models")] {
                    if ui.selectable_label(tab == k, l).clicked() {
                        d.fields.insert("tab".into(), k.into());
                    }
                }
            });
            ui.separator();
            match tab.as_str() {
                "contributors" | "models" => {
                    // The credits need room for the table; the About tab keeps its compact size.
                    ui.set_min_size(vec2(640.0, 380.0));
                    ui.set_max_height(380.0);
                    if tab == "models" { crate::credits::models_ui(ui) } else { crate::credits::contributors_ui(ui) }
                }
                _ => about(app, ui),
            }
            let (ok, _) = buttons(ui, "Close");
            ok
        }
        "preferences" => {
            ui.label(egui::RichText::new("General").font(theme::bold(13.0)));
            let mut dark = app.ui.brightness == theme::Brightness::Dark;
            if ui.checkbox(&mut dark, "Dark appearance").changed() {
                let _ = app.run("view.dark", json!({"on": dark}));
            }
            let mut scale = app.ui.ui_scale;
            if ui.add(egui::Slider::new(&mut scale, 0.75..=1.75).text("Interface size")).changed() {
                app.ui.ui_scale = scale;
            }
            ui.label(egui::RichText::new("Editing").font(theme::bold(13.0)));
            ui.checkbox(&mut app.session.prefs.smart_guides, "Show smart guides");
            ui.checkbox(&mut app.session.prefs.snap_to_grid, "Snap objects to grid");
            ui.checkbox(&mut app.session.prefs.smart_quotes, "Replace straight quotes with smart quotes");
            ui.checkbox(&mut app.session.prefs.autocorrect, "AutoCorrect as you type");
            ui.horizontal(|ui| {
                ui.label("Your name (comments):");
                ui.text_edit_singleline(&mut app.session.prefs.author);
            });
            let (ok, _) = buttons(ui, "Done");
            ok
        }
        "photoAlbum" => photo_album_dialog(app, ui, d),
        "themeColors" => theme_colors_dialog(app, ui, d),
        "themeFonts" => theme_fonts_dialog(app, ui, d),
        _ => {
            ui.label(format!("“{}” is not available yet.", d.id));
            let (ok, cancel) = buttons(ui, "OK");
            ok || cancel
        }
    }
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

fn put_dim(d: &mut Dialog, key: &str, exact: f64, shown: String) {
    d.fields.insert(key.into(), shown.clone());
    d.fields.insert(format!("seed{key}"), shown.clone());
    d.fields.insert(format!("prev{key}"), shown);
    d.fields.insert(format!("{key}Pt"), exact.to_string());
}

fn publish_dim(d: &mut Dialog, key: &str, exact: f64, shown: String) {
    d.fields.insert(key.into(), shown.clone());
    d.fields.insert(format!("seed{key}"), shown);
    d.fields.insert(format!("{key}Pt"), exact.to_string());
}

fn edited(d: &Dialog, key: &str) -> bool {
    field_of(d, key) != field_of(d, &format!("seed{key}"))
}

fn exact_of(d: &Dialog, key: &str) -> Option<f64> {
    parse_shown(&field_of(d, &format!("{key}Pt")))
}

fn selection_xfrm(app: &SlideApp) -> Option<deckcraft_geom::Xfrm> {
    let st = app.session.active()?;
    let id = *st.selection.shapes.first()?;
    let sh = st.shape(id)?;
    Some(deckcraft_engine::cmd::xfrm_of(&st.doc, &st.selection, sh))
}

fn seed_size_position(app: &SlideApp, d: &mut Dialog) {
    if d.fields.contains_key("baseW") {
        return;
    }
    let num = |k: &str| d.params.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
    let sel = selection_xfrm(app);
    let x = num("x").or_else(|| sel.map(|b| b.x)).unwrap_or(0.0);
    let y = num("y").or_else(|| sel.map(|b| b.y)).unwrap_or(0.0);
    let w = num("w").or_else(|| sel.map(|b| b.w)).unwrap_or(72.0);
    let h = num("h").or_else(|| sel.map(|b| b.h)).unwrap_or(72.0);
    let rot = num("rot").or_else(|| sel.map(|b| b.rot)).unwrap_or(0.0);
    let sw = num("scaleW").unwrap_or(100.0);
    let sh = num("scaleH").unwrap_or(100.0);
    let lock = d.params.get("lockAspect").and_then(Value::as_bool).unwrap_or(false);
    put_dim(d, "x", x, num_text(x / 72.0));
    put_dim(d, "y", y, num_text(y / 72.0));
    put_dim(d, "w", w, num_text(w / 72.0));
    put_dim(d, "h", h, num_text(h / 72.0));
    put_dim(d, "rot", rot, num_text(rot));
    put_dim(d, "scaleW", sw, num_text(sw));
    put_dim(d, "scaleH", sh, num_text(sh));
    d.fields.insert("baseW".into(), w.to_string());
    d.fields.insert("baseH".into(), h.to_string());
    let bit = if lock { "1" } else { "0" };
    d.fields.insert("lock".into(), bit.into());
    d.fields.insert("prevlock".into(), bit.into());
}

fn scale_of(pt: f64, base: f64) -> f64 {
    if base > 0.0 { pt / base * 100.0 } else { 100.0 }
}

fn current_points(d: &Dialog, key: &str) -> Option<f64> {
    if edited(d, key) { parse_shown(&field_of(d, key)).map(|inches| inches * 72.0) } else { exact_of(d, key) }
}

/// Keep scale tied to size, and the other side tied when aspect is locked.
fn reconcile_size_fields(d: &mut Dialog) {
    let lock = field_of(d, "lock") == "1";
    let was_locked = field_of(d, "prevlock") == "1";
    let base_w = parse_shown(&field_of(d, "baseW")).unwrap_or(1.0);
    let base_h = parse_shown(&field_of(d, "baseH")).unwrap_or(1.0);
    let ar = if base_h > 0.0 { base_w / base_h } else { 1.0 };
    let (w_s, h_s, sw_s, sh_s) = (field_of(d, "w"), field_of(d, "h"), field_of(d, "scaleW"), field_of(d, "scaleH"));
    let (prev_w, prev_h, prev_sw, prev_sh) = (field_of(d, "prevw"), field_of(d, "prevh"), field_of(d, "prevscaleW"), field_of(d, "prevscaleH"));
    if w_s != prev_w {
        if let Some(inches) = parse_shown(&w_s) {
            let w_pt = inches * 72.0;
            d.fields.insert("wPt".into(), w_pt.to_string());
            let pct = scale_of(w_pt, base_w);
            publish_dim(d, "scaleW", pct, num_text(pct));
            if lock {
                let h_pt = if ar > 0.0 { w_pt / ar } else { w_pt };
                publish_dim(d, "h", h_pt, num_text(h_pt / 72.0));
                let hp = scale_of(h_pt, base_h);
                publish_dim(d, "scaleH", hp, num_text(hp));
            }
        }
    } else if h_s != prev_h {
        if let Some(inches) = parse_shown(&h_s) {
            let h_pt = inches * 72.0;
            d.fields.insert("hPt".into(), h_pt.to_string());
            let pct = scale_of(h_pt, base_h);
            publish_dim(d, "scaleH", pct, num_text(pct));
            if lock {
                let w_pt = h_pt * ar;
                publish_dim(d, "w", w_pt, num_text(w_pt / 72.0));
                let wp = scale_of(w_pt, base_w);
                publish_dim(d, "scaleW", wp, num_text(wp));
            }
        }
    } else if sw_s != prev_sw {
        if let Some(pct) = parse_shown(&sw_s) {
            let w_pt = base_w * pct / 100.0;
            d.fields.insert("scaleWPt".into(), pct.to_string());
            publish_dim(d, "w", w_pt, num_text(w_pt / 72.0));
            if lock {
                let h_pt = if ar > 0.0 { w_pt / ar } else { w_pt };
                publish_dim(d, "h", h_pt, num_text(h_pt / 72.0));
                publish_dim(d, "scaleH", pct, num_text(pct));
            }
        }
    } else if sh_s != prev_sh {
        if let Some(pct) = parse_shown(&sh_s) {
            let h_pt = base_h * pct / 100.0;
            d.fields.insert("scaleHPt".into(), pct.to_string());
            publish_dim(d, "h", h_pt, num_text(h_pt / 72.0));
            if lock {
                let w_pt = h_pt * ar;
                publish_dim(d, "w", w_pt, num_text(w_pt / 72.0));
                publish_dim(d, "scaleW", pct, num_text(pct));
            }
        }
    } else if lock
        && !was_locked
        && let Some(w_pt) = current_points(d, "w")
    {
        let h_pt = if ar > 0.0 { w_pt / ar } else { w_pt };
        publish_dim(d, "h", h_pt, num_text(h_pt / 72.0));
        let hp = scale_of(h_pt, base_h);
        publish_dim(d, "scaleH", hp, num_text(hp));
        let wp = scale_of(w_pt, base_w);
        publish_dim(d, "scaleW", wp, num_text(wp));
    }
    for key in ["x", "y", "w", "h", "rot", "scaleW", "scaleH"] {
        let v = field_of(d, key);
        d.fields.insert(format!("prev{key}"), v);
    }
    d.fields.insert("prevlock".into(), if lock { "1".into() } else { "0".into() });
}

fn shown_points(d: &Dialog, key: &str, label: &str) -> Result<f64, String> {
    if edited(d, key) {
        let n = parse_shown(&field_of(d, key)).ok_or_else(|| format!("{label} must be a number"))?;
        return Ok(n * 72.0);
    }
    exact_of(d, key).ok_or_else(|| format!("{label} must be a number"))
}

fn shown_number(d: &Dialog, key: &str, label: &str) -> Result<f64, String> {
    if edited(d, key) {
        return parse_shown(&field_of(d, key)).ok_or_else(|| format!("{label} must be a number"));
    }
    exact_of(d, key).ok_or_else(|| format!("{label} must be a number"))
}

fn size_position_value(d: &Dialog) -> Result<Value, String> {
    let x = shown_points(d, "x", "Horizontal")?;
    let y = shown_points(d, "y", "Vertical")?;
    let rot = shown_number(d, "rot", "Rotation")?;
    let base_w = parse_shown(&field_of(d, "baseW")).unwrap_or(1.0);
    let base_h = parse_shown(&field_of(d, "baseH")).unwrap_or(1.0);
    let mut w = if edited(d, "w") {
        shown_points(d, "w", "Width")?
    } else if edited(d, "scaleW") {
        base_w * shown_number(d, "scaleW", "Scale width")? / 100.0
    } else {
        shown_points(d, "w", "Width")?
    };
    let mut h = if edited(d, "h") {
        shown_points(d, "h", "Height")?
    } else if edited(d, "scaleH") {
        base_h * shown_number(d, "scaleH", "Scale height")? / 100.0
    } else {
        shown_points(d, "h", "Height")?
    };
    if field_of(d, "lock") == "1" {
        let ar = if base_h > 0.0 { base_w / base_h } else { 1.0 };
        let width_drives = edited(d, "w") || edited(d, "scaleW") || (!edited(d, "h") && !edited(d, "scaleH"));
        if width_drives {
            h = if ar > 0.0 { w / ar } else { h };
        } else {
            w = h * ar;
        }
    }
    Ok(json!({"x": x, "y": y, "w": w, "h": h, "rot": rot, "lockAspect": field_of(d, "lock") == "1"}))
}

fn text_row(ui: &mut Ui, d: &mut Dialog, label: &str, key: &str) {
    let mut v = field_of(d, key);
    ui.horizontal(|ui| {
        ui.add_sized(vec2(148.0, 20.0), egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(&mut v).desired_width(96.0));
    });
    d.fields.insert(key.into(), v);
}

fn size_position_dialog(app: &mut SlideApp, ui: &mut Ui, d: &mut Dialog) -> bool {
    seed_size_position(app, d);
    ui.label(egui::RichText::new("Size").strong());
    text_row(ui, d, "Height (in):", "h");
    text_row(ui, d, "Width (in):", "w");
    text_row(ui, d, "Rotation (°):", "rot");
    text_row(ui, d, "Scale height (%):", "scaleH");
    text_row(ui, d, "Scale width (%):", "scaleW");
    check(ui, d, "Lock aspect ratio", "lock", false);
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Position").strong());
    ui.label("From the top-left corner of the slide");
    text_row(ui, d, "Horizontal (in):", "x");
    text_row(ui, d, "Vertical (in):", "y");
    reconcile_size_fields(d);
    let (ok, cancel) = buttons(ui, "OK");
    if ok {
        match size_position_value(d) {
            Ok(p) => return app.run("shape.sizePosition", p).is_ok(),
            Err(msg) => {
                app.set_status(msg);
                return false;
            }
        }
    }
    cancel
}

fn pretty_equation(s: &str) -> String {
    let sup = |c: char| match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        '+' => '⁺',
        '-' => '⁻',
        _ => c,
    };
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '^' => {
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphanumeric() || n == '+' || n == '-' {
                        out.push(sup(n));
                        chars.next();
                        if !n.is_ascii_digit() {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            '*' => out.push('·'),
            _ => out.push(c),
        }
    }
    out.replace("sqrt", "√").replace("<=", "≤").replace(">=", "≥").replace("!=", "≠").replace("pi", "π").replace("+-", "±")
}

pub fn about(_app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        paint_logo(ui.painter(), r);
        ui.label(egui::RichText::new("DeckCraft").font(theme::bold(22.0)));
        ui.label(egui::RichText::new(version_line()).color(t.text_dim));
        ui.add_space(6.0);
        ui.label("Presentations and slide shows, rebuilt from scratch in pure Rust.");
        ui.label("An open-source, clean-room project. Not affiliated with Microsoft.");
        ui.add_space(6.0);
        ui.hyperlink_to("Join our community on Discord", deckcraft_engine::links::DISCORD);
        ui.hyperlink_to("getartcraft.com/apps/deckcraft", deckcraft_engine::links::APP_PAGE);
        ui.hyperlink_to("Source on GitHub", deckcraft_engine::links::GITHUB);
        ui.add_space(6.0);
        ui.label(egui::RichText::new("MIT OR Apache-2.0 · © 2026 ArtCraft Team and the DeckCraft contributors").size(11.0).color(t.text_faint));
    });
}

/// "Version X.Y.Z" plus, for release builds, the short commit and build date. The version comes
/// from `[workspace.package] version` (the single source of truth, `cargo xtask version`); the
/// release workflow sets `DECKCRAFT_BUILD_SHA` / `DECKCRAFT_BUILD_DATE` at build time.
pub fn version_line() -> String {
    let mut s = format!("Version {}", env!("CARGO_PKG_VERSION"));
    let sha = option_env!("DECKCRAFT_BUILD_SHA").map(|s| s.get(..9).unwrap_or(s)).filter(|s| !s.is_empty());
    let date = option_env!("DECKCRAFT_BUILD_DATE").filter(|s| !s.is_empty());
    match (sha, date) {
        (Some(sha), Some(date)) => s.push_str(&format!(" ({sha}, {date})")),
        (Some(x), None) | (None, Some(x)) => s.push_str(&format!(" ({x})")),
        (None, None) => {}
    }
    s
}

/// DeckCraft's mark, drawn in code: a slide card with a play triangle in our orange.
pub fn paint_logo(p: &egui::Painter, r: Rect) {
    let orange = Color32::from_rgb(0xF2, 0x6B, 0x1D);
    let deep = Color32::from_rgb(0xC2, 0x4E, 0x14);
    let card = r.shrink(r.width() * 0.08);
    p.rect_filled(card.translate(vec2(r.width() * 0.04, r.width() * 0.05)), CornerRadius::same((r.width() * 0.14) as u8), deep);
    p.rect_filled(card, CornerRadius::same((r.width() * 0.14) as u8), orange);
    let c = card.center();
    let s = card.width() * 0.22;
    p.add(egui::Shape::convex_polygon(
        vec![pos2(c.x - s * 0.7, c.y - s), pos2(c.x + s, c.y), pos2(c.x - s * 0.7, c.y + s)],
        Color32::WHITE,
        Stroke::NONE,
    ));
}

#[cfg(test)]
#[path = "size_position_tests.rs"]
mod size_position_tests;

#[cfg(test)]
#[path = "photo_album_tests.rs"]
mod photo_album_tests;

#[cfg(test)]
#[path = "theme_colors_tests.rs"]
mod theme_colors_tests;

#[cfg(test)]
#[path = "theme_fonts_tests.rs"]
mod theme_fonts_tests;
