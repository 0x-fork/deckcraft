//! Insert tab: shapes, text boxes, pictures, tables, charts, audio, video, WordArt, fields.

use deckcraft_color::SchemeSlot;
use deckcraft_geom::Xfrm;
use deckcraft_model::chart::ChartType;
use deckcraft_model::style::PictureFill;
use deckcraft_model::text::{AutoFit, RunKind, TextBody};
use deckcraft_model::{Chart, ColorRef, Fill, Geom, MediaClip, Shape, ShapeId, ShapeKind, ShapeStyle, Table};
use serde_json::{Value, json};

use super::*;
use crate::{EngineError, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "shape.insert",
            "Shapes",
            ["Insert", "Illustrations"],
            None,
            "{preset: rect|roundRect|ellipse|triangle|rightArrow|star5|… (see shape.presets), rect: [x,y,w,h] pt, text?: string}",
            has_slide,
            insert_shape
        ),
        cmd!(query "shape.presets", "Shape Gallery", [], None, "{} → [{name, label, category}]", always, presets),
        cmd!("insert.textBox", "Text Box", ["Insert", "Text"], None, "{rect?: [x,y,w,h], text?: string}", has_slide, text_box),
        cmd!(
            "insert.picture",
            "Picture from File…",
            ["Insert", "Images", "Pictures"],
            None,
            "{path? | data: base64, name?, rect?: [x,y,w,h]}",
            has_slide,
            picture
        ),
        cmd!("insert.table", "Table…", ["Insert", "Tables"], None, "{rows, cols, rect?}", has_slide, table),
        cmd!(
            "insert.chart",
            "Chart…",
            ["Insert", "Illustrations"],
            None,
            "{type?: column|bar|line|pie|doughnut|area|scatter|stackedColumn|…, rect?, categories?: [..], series?: [{name, values}]}",
            has_slide,
            chart
        ),
        cmd!("insert.audio", "Audio from File…", ["Insert", "Media", "Audio"], None, "{path? | data: base64, name?, rect?}", has_slide, audio),
        cmd!("insert.video", "Video from File…", ["Insert", "Media", "Video"], None, "{path? | data: base64, name?, rect?}", has_slide, video),
        cmd!("insert.wordArt", "WordArt", ["Insert", "Text"], None, "{text?, style?: 0..}", has_slide, word_art),
        cmd!("insert.slideNumber", "Slide Number", ["Insert", "Text"], None, "{}", has_slide, slide_number),
        cmd!(
            "insert.dateTime",
            "Date & Time",
            ["Insert", "Text"],
            None,
            "{format?: datetime1..datetime13, update?: bool, language?: en-US|en-GB|fr-FR|ja-JP|sv-SE} — no keys opens the dialog",
            has_slide,
            date_time
        ),
        cmd!("insert.symbol", "Symbol", ["Insert", "Symbols"], None, "{text: character(s)}", has_slide, symbol),
        cmd!(
            "insert.hyperlink",
            "Link",
            ["Insert", "Links"],
            Some("Cmd+K"),
            "{url? | slide?: index, tooltip?, text?: shown when nothing is selected, ids?}",
            has_text_or_shapes,
            hyperlink
        ),
        cmd!("chart.type", "Change Chart Type", ["Chart Design", "Type"], None, "{type, id?}", has_selection, chart_type_cmd),
        cmd!(
            "chart.data",
            "Edit Data",
            ["Chart Design", "Data"],
            None,
            "{categories?: [..], series?: [{name, values}], id?}",
            has_selection,
            chart_data
        ),
        cmd!(
            "chart.options",
            "Chart Elements",
            ["Chart Design", "Chart Layouts"],
            None,
            "{title?: string|null, legend?: b|t|l|r|null, dataLabels?: bool, gridlines?: bool, palette?, id?}",
            has_selection,
            chart_options
        ),
        cmd!(
            "insert.smartArt",
            "SmartArt",
            ["Insert", "Illustrations"],
            None,
            "{kind: list|process|cycle|hierarchy|pyramid|matrix, items: [text]}",
            has_slide,
            smart_art
        ),
        cmd!(
            "insert.actionButton",
            "Action Buttons",
            ["Insert", "Illustrations"],
            None,
            "{kind: back|forward|beginning|end|home|information|return|movie|document|sound|help|custom, rect?}",
            has_slide,
            action_button
        ),
    ]
}

/// Default size for a click-inserted shape (1 inch square, PowerPoint style).
fn default_rect(s: &Session, w: f64, h: f64) -> Xfrm {
    let size = s.active().map(|d| d.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
    Xfrm::new((size.width - w) / 2.0, (size.height - h) / 2.0, w, h)
}

pub(crate) fn add_shape(s: &mut Session, mut shape: Shape, select: bool) -> Result<ShapeId> {
    s.edit(|doc, sel| {
        shape.id = ShapeId(doc.alloc_id());
        if shape.name.is_empty() {
            let base = match &shape.kind {
                ShapeKind::Picture { .. } => "Picture".to_string(),
                ShapeKind::Table(_) => "Table".to_string(),
                ShapeKind::Chart(_) => "Chart".to_string(),
                ShapeKind::Media(m) if m.video => "Video".to_string(),
                ShapeKind::Media(_) => "Audio".to_string(),
                _ if shape.text_box => "TextBox".to_string(),
                _ => shape
                    .geom
                    .preset_name()
                    .and_then(deckcraft_geom::preset::info)
                    .map(|i| i.label.split(':').next().unwrap_or(i.label).trim().to_string())
                    .unwrap_or_else(|| "Shape".into()),
            };
            let n = super::current_shapes(doc, sel).map(|v| v.len()).unwrap_or(0) + 1;
            shape.name = format!("{base} {n}");
        }
        let id = shape.id;
        let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("insert", "no slide"))?;
        list.push(shape);
        if select {
            sel.shapes = vec![id];
            sel.text = None;
        }
        Ok(id)
    })
}

fn insert_shape(s: &mut Session, p: &Value) -> Result<Value> {
    let preset = str_param(p, "preset").unwrap_or("rect");
    let info = deckcraft_geom::preset::info(preset).ok_or_else(|| bad("shape.insert", format!("unknown preset `{preset}`")))?;
    let line_like = deckcraft_geom::preset::is_line_like(preset);
    let rect = rect_param(p, "rect").unwrap_or_else(|| default_rect(s, 72.0, if line_like { 0.0 } else { 72.0 }));
    let adj: Vec<f64> = p.get("adj").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let mut shape = Shape {
        xfrm: Some(rect),
        geom: Geom::Preset { name: info.name.into(), adj },
        style: Some(if line_like { ShapeStyle::line(SchemeSlot::Accent1) } else { ShapeStyle::accent(SchemeSlot::Accent1) }),
        ..Default::default()
    };
    if let Some(fl) = p.get("flip").and_then(Value::as_str)
        && let Some(x) = shape.xfrm.as_mut()
    {
        x.flip_h = fl.contains('h');
        x.flip_v = fl.contains('v');
    }
    if !line_like {
        shape.text = Some(TextBody { paragraphs: vec![], ..Default::default() });
        if let Some(t) = str_param(p, "text") {
            shape.text = Some(TextBody::from_text(t));
        }
        if let Some(tb) = shape.text.as_mut() {
            for para in &mut tb.paragraphs {
                para.props.align = Some(deckcraft_model::text::Align::Center);
            }
        }
    } else {
        shape.kind = if preset.contains("Connector") { ShapeKind::Connector { start: None, end: None } } else { ShapeKind::Shape };
        if preset == "straightConnector1" {
            shape.line = Some(deckcraft_model::Line {
                tail: Some(deckcraft_model::style::LineEnd { kind: "triangle".into(), w: "med".into(), len: "med".into() }),
                ..Default::default()
            });
        }
    }
    if let (Some(look), false) = (&s.default_look, line_like) {
        super::format::apply_painted(&mut shape, look);
    }
    let id = add_shape(s, shape, true)?;
    Ok(json!({"id": id}))
}

fn presets(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(Value::Array(
        deckcraft_geom::preset::CATALOG.iter().map(|i| json!({"name": i.name, "label": i.label, "category": i.category.label()})).collect(),
    ))
}

pub(crate) fn new_text_box(rect: Xfrm, text: &str) -> Shape {
    let mut body = TextBody::from_text(text);
    body.body.wrap = Some(true);
    body.body.autofit = Some(AutoFit::Shape);
    Shape { xfrm: Some(rect), text_box: true, fill: None, text: Some(body), geom: Geom::preset("rect"), ..Default::default() }
}

fn text_box(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").unwrap_or("");
    let rect = rect_param(p, "rect").unwrap_or_else(|| default_rect(s, 216.0, 36.0));
    let mut shape = new_text_box(rect, text);
    // Click-to-create (no width) text boxes grow sideways without wrapping.
    if rect.w < 1.0 {
        shape.xfrm = Some(Xfrm { w: 100.0, ..rect });
        if let Some(t) = shape.text.as_mut() {
            t.body.wrap = Some(false);
        }
    }
    let id = add_shape(s, shape, true)?;
    super::text::fit_text_box(s, id)?;
    if text.is_empty() {
        // Start typing immediately.
        s.select(|_, sel| {
            sel.shapes = vec![id];
            sel.text = Some(crate::TextSel { shape: id, ..Default::default() });
        })?;
    }
    Ok(json!({"id": id}))
}

/// Read media bytes from `data` (base64) or `path`.
pub(crate) fn media_bytes(p: &Value, cmd: &str) -> Result<(String, Vec<u8>)> {
    if let Some(d) = str_param(p, "data") {
        let bytes = base64_decode(d).ok_or_else(|| bad(cmd, "invalid base64 `data`"))?;
        return Ok((str_param(p, "name").unwrap_or("media").to_string(), bytes));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = str_param(p, "path") {
        let bytes = std::fs::read(path).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
        let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "media".into());
        return Ok((name, bytes));
    }
    Err(bad(cmd, "missing `path` or `data`"))
}

pub fn content_type(name: &str, bytes: &[u8]) -> &'static str {
    let l = name.to_ascii_lowercase();
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "audio/wav"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        "image/tiff"
    } else if bytes.starts_with(b"fLaC") {
        "audio/flac"
    } else if bytes.starts_with(b"OggS") {
        if l.ends_with(".ogv") { "video/ogg" } else { "audio/ogg" }
    } else if bytes.len() > 1 && bytes[0] == 0xFF && bytes[1] & 0xF6 == 0xF0 {
        // ADTS (layer bits 00): AAC.
        "audio/aac"
    } else if bytes.starts_with(b"ID3") || (bytes.len() > 1 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 && !l.ends_with(".aac")) {
        "audio/mpeg"
    } else if bytes.starts_with(b"FORM") {
        "audio/aiff"
    } else if bytes.get(4..8) == Some(b"ftyp") {
        if l.ends_with(".m4a") || l.ends_with(".m4b") || bytes.get(8..11) == Some(b"M4A") {
            "audio/mp4"
        } else if l.ends_with(".mov") || bytes.get(8..12) == Some(b"qt  ") {
            "video/quicktime"
        } else {
            "video/mp4"
        }
    } else if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        if l.ends_with(".mka") || l.ends_with(".weba") {
            "audio/webm"
        } else if l.ends_with(".mkv") {
            "video/x-matroska"
        } else {
            "video/webm"
        }
    } else if bytes.starts_with(&[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        if l.ends_with(".wmv") || l.ends_with(".asf") { "video/x-ms-wmv" } else { "audio/x-ms-wma" }
    } else if bytes.starts_with(b"caff") {
        "audio/x-caf"
    } else if l.ends_with(".svg") || bytes.windows(4).take(512).any(|w| w == b"<svg") {
        "image/svg+xml"
    } else if l.ends_with(".aac") {
        "audio/aac"
    } else {
        "application/octet-stream"
    }
}

/// Natural size of an image in points (96 dpi), fitted inside the slide.
fn fitted(s: &Session, bytes: &[u8]) -> (f64, f64) {
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map(|(w, h)| (w as f64 * 0.75, h as f64 * 0.75))
        .unwrap_or((288.0, 216.0));
    let size = s.active().map(|d| d.doc.slide_size).unwrap_or(deckcraft_model::defaults::WIDE);
    let k = (size.width * 0.9 / w.max(1.0)).min(size.height * 0.9 / h.max(1.0)).min(1.0);
    (w * k, h * k)
}

fn picture(s: &mut Session, p: &Value) -> Result<Value> {
    let (name, bytes) = media_bytes(p, "insert.picture")?;
    let ct = content_type(&name, &bytes);
    if !ct.starts_with("image/") {
        return Err(bad("insert.picture", format!("{name} is not a picture")));
    }
    if ct != "image/svg+xml" && deckcraft_render::decode_image(&bytes).is_none() {
        return Err(EngineError::Other(format!("{name}: the picture can't be read")));
    }
    let (w, h) = fitted(s, &bytes);
    // Into a selected empty picture/content placeholder, if there is one.
    let target_ph = s.active().and_then(|d| {
        d.selected_shapes()
            .into_iter()
            .find(|x| {
                x.ph_type()
                    .is_some_and(|k| matches!(k, deckcraft_model::PhType::Picture | deckcraft_model::PhType::Obj | deckcraft_model::PhType::Body))
                    && x.text.as_ref().is_none_or(|t| t.is_empty())
            })
            .map(|x| (x.id, xfrm_of(&d.doc, &d.selection, x)))
    });
    let rect = rect_param(p, "rect").unwrap_or_else(|| match target_ph {
        Some((_, b)) => {
            // Fill the placeholder, cropping the picture to its aspect.
            b
        }
        None => default_rect(s, w, h),
    });
    let crop = match (target_ph, rect_param(p, "rect")) {
        (Some(_), None) => {
            let (ia, ba) = (w / h.max(1e-9), rect.w / rect.h.max(1e-9));
            if ia > ba {
                let c = (1.0 - ba / ia) / 2.0;
                [c, 0.0, c, 0.0]
            } else {
                let c = (1.0 - ia / ba) / 2.0;
                [0.0, c, 0.0, c]
            }
        }
        _ => [0.0; 4],
    };
    let media = s.edit(|doc, _| Ok(doc.add_media(&name, ct, bytes)))?;
    let shape = Shape {
        xfrm: Some(rect),
        kind: ShapeKind::Picture { fill: PictureFill { media, crop, ..Default::default() } },
        descr: name.clone(),
        ..Default::default()
    };
    if let Some((ph_id, _)) = target_ph {
        // Replace the placeholder with the picture (it keeps the placeholder link).
        let phv = s.doc()?.shape(ph_id).and_then(|x| x.ph.clone());
        s.edit(|doc, sel| {
            let list = crate::shapes_mut(doc, sel).ok_or_else(|| bad("insert.picture", "no slide"))?;
            if let Some(sh) = list.iter_mut().find(|x| x.id == ph_id) {
                sh.kind = shape.kind.clone();
                sh.xfrm = Some(rect);
                sh.ph = phv.map(|mut p| {
                    p.kind = deckcraft_model::PhType::Picture;
                    p
                });
                sh.text = None;
                sh.descr = name.clone();
            }
            sel.shapes = vec![ph_id];
            Ok(())
        })?;
        return Ok(json!({"id": ph_id}));
    }
    let id = add_shape(s, shape, true)?;
    Ok(json!({"id": id}))
}

fn table(s: &mut Session, p: &Value) -> Result<Value> {
    let rows = usize_param(p, "rows").unwrap_or(2).clamp(1, 75);
    let cols = usize_param(p, "cols").unwrap_or(2).clamp(1, 75);
    let size = s.doc()?.doc.slide_size;
    let w = size.width * 0.8;
    let rh = 40.0;
    let rect = rect_param(p, "rect").unwrap_or(Xfrm::new(size.width * 0.1, (size.height - rh * rows as f64).max(0.0) / 2.0, w, rh * rows as f64));
    let mut t = Table::new(rows, cols, rect.w, rect.h / rows as f64);
    if let Some(data) = p.get("data").and_then(Value::as_array) {
        for (r, row) in data.iter().enumerate() {
            for (c, v) in row.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().enumerate() {
                if let (Some(cell), Some(text)) = (t.cell_mut(r, c), v.as_str()) {
                    cell.text = TextBody::from_text(text);
                }
            }
        }
    }
    let id = add_shape(s, Shape { xfrm: Some(rect), kind: ShapeKind::Table(t), ..Default::default() }, true)?;
    Ok(json!({"id": id}))
}

pub fn chart_type(name: &str) -> ChartType {
    match name.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
        "bar" | "clusteredbar" => ChartType::Bar,
        "stackedbar" => ChartType::StackedBar,
        "percentbar" => ChartType::PercentBar,
        "line" => ChartType::Line,
        "linemarkers" | "linewithmarkers" => ChartType::LineMarkers,
        "stackedline" => ChartType::StackedLine,
        "area" => ChartType::Area,
        "stackedarea" => ChartType::StackedArea,
        "pie" => ChartType::Pie,
        "doughnut" | "donut" => ChartType::Doughnut,
        "scatter" | "xy" => ChartType::Scatter,
        "bubble" => ChartType::Bubble,
        "radar" => ChartType::Radar,
        "filledradar" => ChartType::FilledRadar,
        "stackedcolumn" => ChartType::StackedColumn,
        "percentcolumn" => ChartType::PercentColumn,
        "waterfall" => ChartType::Waterfall,
        "funnel" => ChartType::Funnel,
        "histogram" => ChartType::Histogram,
        "pareto" => ChartType::Pareto,
        "treemap" => ChartType::Treemap,
        "sunburst" => ChartType::Sunburst,
        "boxwhisker" | "box" => ChartType::BoxWhisker,
        "stock" => ChartType::Stock,
        "surface" => ChartType::Surface,
        "combo" => ChartType::Combo,
        _ => ChartType::Column,
    }
}

fn chart(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = chart_type(str_param(p, "type").unwrap_or("column"));
    let mut c = Chart::sample(kind);
    if let Some(cats) = p.get("categories").and_then(Value::as_array) {
        c.categories = cats.iter().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).collect();
    }
    if let Some(series) = p.get("series").and_then(Value::as_array) {
        c.series = series
            .iter()
            .map(|sv| deckcraft_model::chart::Series {
                name: sv.get("name").and_then(Value::as_str).unwrap_or("Series").to_string(),
                values: sv.get("values").and_then(Value::as_array).map(|a| a.iter().map(Value::as_f64).collect()).unwrap_or_default(),
                ..Default::default()
            })
            .collect();
    }
    if let Some(t) = p.get("title") {
        c.title = t.as_str().map(String::from);
    }
    let size = s.doc()?.doc.slide_size;
    let rect = rect_param(p, "rect").unwrap_or(Xfrm::new(size.width * 0.15, size.height * 0.15, size.width * 0.7, size.height * 0.7));
    let id = add_shape(s, Shape { xfrm: Some(rect), kind: ShapeKind::Chart(Box::new(c)), ..Default::default() }, true)?;
    Ok(json!({"id": id}))
}

fn media(s: &mut Session, p: &Value, video: bool) -> Result<Value> {
    let cmd = if video { "insert.video" } else { "insert.audio" };
    let (name, bytes) = media_bytes(p, cmd)?;
    let ct = content_type(&name, &bytes);
    let bytes = std::sync::Arc::new(bytes);
    let info = deckcraft_media::probe(&bytes);
    let has_video = info.as_ref().is_ok_and(|i| i.video.is_some());
    let has_audio = info.as_ref().is_ok_and(|i| i.audio.is_some());
    match &info {
        Ok(_) if video && !has_video => return Err(bad(cmd, format!("{name} has no video"))),
        Ok(_) if !video && !has_audio && !has_video => return Err(bad(cmd, format!("{name} has no audio"))),
        // Not something we can read: embed it anyway when the type says it's media (it still
        // round-trips through PPTX), otherwise refuse.
        Err(_) if video && !ct.starts_with("video/") => return Err(bad(cmd, format!("{name} is not a supported video ({ct})"))),
        Err(_) if !video && !ct.starts_with("audio/") => return Err(bad(cmd, format!("{name} is not a supported audio file ({ct})"))),
        _ => {}
    }
    let (duration_ms, width, height, playable) = match &info {
        Ok(i) => {
            let (w, h) = i.video.as_ref().map(|v| (v.width, v.height)).unwrap_or((0, 0));
            (i.duration_ms.min(u32::MAX as u64) as u32, w, h, i.playable())
        }
        Err(_) => (0, 0, 0, false),
    };
    let warning = (!playable).then(|| match &info {
        Ok(i) => {
            let codecs: Vec<&str> = i.audio.iter().map(|a| a.codec.as_str()).chain(i.video.iter().map(|v| v.codec.as_str())).collect();
            format!("{name} was inserted, but it can't be played ({} isn't supported yet)", codecs.join(" / "))
        }
        Err(e) => format!("{name} was inserted, but it can't be played ({e})"),
    });
    let decodable_video = info.as_ref().is_ok_and(|i| i.video.as_ref().is_some_and(|v| v.decodable));
    let poster_png = if video && decodable_video { super::media::poster_png(&bytes, 0) } else { None };
    let size = s.doc()?.doc.slide_size;
    let rect = rect_param(p, "rect").unwrap_or_else(|| {
        if video {
            // Natural size (96 dpi), fitted inside 90 % of the slide and centred.
            let (w, h) =
                if width > 0 && height > 0 { (width as f64 * 0.75, height as f64 * 0.75) } else { (size.width * 0.6, size.width * 0.6 * 9.0 / 16.0) };
            let k = (size.width * 0.9 / w).min(size.height * 0.9 / h).min(1.0);
            let (w, h) = (w * k, h * k);
            Xfrm::new((size.width - w) / 2.0, (size.height - h) / 2.0, w, h)
        } else {
            Xfrm::new((size.width - 48.0) / 2.0, (size.height - 48.0) / 2.0, 48.0, 48.0)
        }
    });
    let data = std::sync::Arc::try_unwrap(bytes).unwrap_or_else(|a| (*a).clone());
    let (media, poster) = s.edit(|doc, _| {
        let media = doc.add_media(&name, ct, data);
        let poster = poster_png.map(|png| doc.add_media(&format!("{name}-poster.png"), "image/png", png));
        Ok((media, poster))
    })?;
    let clip = MediaClip { media, video, poster, volume: 1.0, duration_ms, width, height, ..Default::default() };
    let id = add_shape(s, Shape { xfrm: Some(rect), kind: ShapeKind::Media(clip), descr: name, ..Default::default() }, true)?;
    if let Some(w) = &warning {
        s.ui_requests.push(crate::UiRequest::Message { text: w.clone() });
    }
    let mut r =
        json!({"id": id, "contentType": ct, "durationMs": duration_ms, "width": width, "height": height, "playable": playable, "warning": warning});
    if let Ok(i) = &info {
        r["probe"] = super::media::probe_json(i);
    }
    Ok(r)
}

fn audio(s: &mut Session, p: &Value) -> Result<Value> {
    media(s, p, false)
}
fn video(s: &mut Session, p: &Value) -> Result<Value> {
    media(s, p, true)
}

fn word_art(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").unwrap_or("Your text here");
    let size = s.doc()?.doc.slide_size;
    let style = usize_param(p, "style").unwrap_or(0);
    let mut shape = new_text_box(Xfrm::new(size.width * 0.2, size.height * 0.4, size.width * 0.6, 80.0), text);
    if let Some(t) = shape.text.as_mut() {
        for para in &mut t.paragraphs {
            para.props.align = Some(deckcraft_model::text::Align::Center);
            for r in &mut para.runs {
                r.props.size = Some(54.0);
                r.props.bold = Some(style % 2 == 1);
                r.props.fill = Some(Fill::solid(ColorRef::scheme(match style % 4 {
                    0 => SchemeSlot::Tx1,
                    1 => SchemeSlot::Accent1,
                    2 => SchemeSlot::Accent2,
                    _ => SchemeSlot::Accent4,
                })));
                if style % 3 == 2 {
                    r.props.outline = Some(deckcraft_model::Line::solid(ColorRef::scheme(SchemeSlot::Bg1), 1.0));
                }
            }
        }
    }
    let id = add_shape(s, shape, true)?;
    super::text::fit_text_box(s, id)?;
    Ok(json!({"id": id}))
}

fn field_run(s: &mut Session, field: &str, text: &str) -> Result<Value> {
    if s.doc()?.selection.text.is_some() {
        return super::text::insert_field(s, field, text);
    }
    // Not editing: turn on the footer field for the slide (Header & Footer).
    let key = if field == "slidenum" { "slideNumber" } else { "date" };
    s.execute("design.headerFooter", &json!({key: true}))
}

fn slide_number(s: &mut Session, _p: &Value) -> Result<Value> {
    field_run(s, "slidenum", "‹#›")
}

const DATE_FORMATS: [&str; 13] = [
    "datetime1",
    "datetime2",
    "datetime3",
    "datetime4",
    "datetime5",
    "datetime6",
    "datetime7",
    "datetime8",
    "datetime9",
    "datetime10",
    "datetime11",
    "datetime12",
    "datetime13",
];

struct DateOpts {
    format: String,
    update: bool,
    /// BCP 47 tag when the caller set `language`; absent keeps the surrounding run's language.
    lang: Option<String>,
    sample: String,
}

/// `true` when the call carries a format, update flag or language (apply it). Empty params open
/// the dialog instead.
fn date_time_apply(p: &Value) -> bool {
    p.as_object().is_some_and(|o| o.contains_key("format") || o.contains_key("update") || o.contains_key("language"))
}

fn canon_lang(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "en" | "en-us" => Some("en-US"),
        "en-gb" => Some("en-GB"),
        "fr" | "fr-fr" => Some("fr-FR"),
        "ja" | "ja-jp" => Some("ja-JP"),
        "sv" | "sv-se" => Some("sv-SE"),
        _ => None,
    }
}

fn date_opts(p: &Value) -> Result<DateOpts> {
    if p.get("format").is_some() {
        let f = str_param(p, "format").filter(|f| DATE_FORMATS.contains(f));
        if f.is_none() {
            return Err(bad("insert.dateTime", "`format` must be datetime1..datetime13"));
        }
    }
    if p.get("update").is_some() && bool_param(p, "update").is_none() {
        return Err(bad("insert.dateTime", "`update` must be true or false"));
    }
    let lang = if p.get("language").is_some() {
        let raw = str_param(p, "language").filter(|l| !l.trim().is_empty());
        let Some(raw) = raw else {
            return Err(bad("insert.dateTime", "`language` must be en-US, en-GB, fr-FR, ja-JP or sv-SE"));
        };
        Some(canon_lang(raw).ok_or_else(|| bad("insert.dateTime", "`language` must be en-US, en-GB, fr-FR, ja-JP or sv-SE"))?.to_string())
    } else {
        None
    };
    let format = str_param(p, "format").unwrap_or("datetime1").to_string();
    let sample = date_sample(&format, lang.as_deref())?;
    Ok(DateOpts { format, update: bool_or(p, "update", true), lang, sample })
}

/// The text `format` shows now in `lang`. Errors when that language has no names for the format
/// (month names and AM/PM are English only). An empty string where there is no clock (wasm).
fn date_sample(format: &str, lang: Option<&str>) -> Result<String> {
    use deckcraft_text::datetime::{DateTime, field_text};
    let probe = DateTime::new(2026, 10, 9, 20, 5, 7);
    if field_text(format, lang, probe).is_none() {
        return Err(bad("insert.dateTime", "this format isn't available in that language"));
    }
    Ok(DateTime::now_local().and_then(|now| field_text(format, lang, now)).unwrap_or_default())
}

fn date_time(s: &mut Session, p: &Value) -> Result<Value> {
    if !date_time_apply(p) {
        s.ui_requests.push(crate::UiRequest::Dialog { id: "dateTime".into(), params: json!({}) });
        return ok();
    }
    let opts = date_opts(p)?;
    let (doc, sel) = {
        let st = s.doc()?;
        (st.doc.clone(), st.selection.clone())
    };
    let r = apply_date(s, &opts);
    if r.is_err()
        && let Some(st) = s.active_mut()
    {
        st.doc = doc;
        st.selection = sel;
    }
    r
}

fn apply_date(s: &mut Session, o: &DateOpts) -> Result<Value> {
    if s.doc()?.selection.text.is_some() {
        if o.update {
            super::text::insert_field(s, &o.format, &o.sample)?;
            if let Some(lang) = &o.lang {
                stamp_caret_lang(s, lang)?;
            }
        } else {
            insert_plain(s, &o.sample, o.lang.as_deref())?;
        }
        return Ok(json!({"where": "caret", "format": o.format, "update": o.update, "text": o.sample}));
    }
    // Not editing: the slide's date placeholder (this slide only). A layout with no date
    // placeholder gets a text box instead, so the chosen format is still inserted.
    s.execute("design.headerFooter", &json!({"date": true, "all": false}))?;
    if !write_date_placeholder(s, o)? {
        return insert_date_box(s, o);
    }
    Ok(json!({"where": "footer", "format": o.format, "update": o.update, "text": o.sample}))
}

/// Point the current slide's date placeholder at this format (a field, or the sample as plain text).
fn write_date_placeholder(s: &mut Session, o: &DateOpts) -> Result<bool> {
    let format = o.format.clone();
    let sample = o.sample.clone();
    let lang = o.lang.clone();
    let update = o.update;
    s.edit(|doc, sel| {
        let Some(sl) = doc.slides.get_mut(sel.slide) else { return Ok(false) };
        let sl = std::sync::Arc::make_mut(sl);
        let Some(sh) = sl.shapes.iter_mut().find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date)) else {
            return Ok(false);
        };
        let mut props = sh.text.as_ref().and_then(|t| t.paragraphs.first()).and_then(|p| p.runs.first()).map(|r| r.props.clone()).unwrap_or_default();
        if let Some(lang) = lang {
            props.lang = Some(lang);
        }
        let kind = if update { RunKind::Field { field: format } } else { RunKind::Text };
        let para = deckcraft_model::Paragraph { runs: vec![deckcraft_model::Run { text: sample, props, kind }], ..Default::default() };
        let body = sh.text.clone().unwrap_or_default();
        sh.text = Some(TextBody { paragraphs: vec![para], ..body });
        Ok(true)
    })
}

fn insert_date_box(s: &mut Session, o: &DateOpts) -> Result<Value> {
    let size = s.doc()?.doc.slide_size;
    let y = (size.height - 48.0).max(0.0);
    s.execute("insert.textBox", &json!({"rect": [36.0, y, 360.0, 28.0], "text": ""}))?;
    if o.update {
        super::text::insert_field(s, &o.format, &o.sample)?;
    } else {
        super::text::insert_text(s, &o.sample, None)?;
    }
    if let Some(lang) = &o.lang {
        stamp_caret_lang(s, lang)?;
    }
    Ok(json!({"where": "textBox", "format": o.format, "update": o.update, "text": o.sample}))
}

/// Insert `text` at the caret. `lang`, when set, is on that run before runs with equal props merge,
/// so the date stays its own run next to the surrounding text.
fn insert_plain(s: &mut Session, text: &str, lang: Option<&str>) -> Result<()> {
    let text = text.to_string();
    let lang = lang.map(str::to_string);
    super::text::edit_text(s, move |body, t| {
        let (a, b) = t.ordered();
        let at = deckcraft_model::edit::delete(body, a, b);
        let mut props = deckcraft_model::edit::props_at(body, at);
        if let Some(lang) = lang.clone() {
            props.lang = Some(lang);
        }
        let c = deckcraft_model::edit::insert(body, at, &text, Some(props));
        t.anchor = c;
        t.caret = c;
    })?;
    Ok(())
}

/// Tag the run that ends at the caret with `lang`, so a date field keeps that language when it updates.
fn stamp_caret_lang(s: &mut Session, lang: &str) -> Result<()> {
    let lang = lang.to_string();
    super::text::edit_text(s, move |body, t| {
        let Some(p) = body.paragraphs.get_mut(t.caret.0) else { return };
        let mut at = 0usize;
        for r in &mut p.runs {
            let n = r.char_len();
            if n > 0 && at < t.caret.1 && t.caret.1 <= at + n {
                r.props.lang = Some(lang);
                return;
            }
            at += n;
        }
    })?;
    Ok(())
}

fn symbol(s: &mut Session, p: &Value) -> Result<Value> {
    let t = str_param(p, "text").ok_or_else(|| bad("insert.symbol", "missing `text`"))?.to_string();
    if s.doc()?.selection.text.is_none() {
        return Err(bad("insert.symbol", "place the insertion point in text first"));
    }
    super::text::insert_text(s, &t, None)
}

fn hyperlink(s: &mut Session, p: &Value) -> Result<Value> {
    use deckcraft_model::text::{Action, Hyperlink};
    // `display`: what a caret with nothing to link inserts, the address or the slide's title.
    let (action, display) = if let Some(u) = str_param(p, "url") {
        (Action::Url { url: u.to_string() }, u.strip_prefix("mailto:").unwrap_or(u).to_string())
    } else if let Some(i) = usize_param(p, "slide") {
        let slide = s.doc()?.doc.slides.get(i).ok_or_else(|| bad("insert.hyperlink", "no such slide"))?;
        let title = slide.title();
        (Action::Slide { slide: slide.id }, if title.is_empty() { format!("Slide {}", i + 1) } else { title })
    } else {
        return Err(bad("insert.hyperlink", "missing `url` or `slide`"));
    };
    let display = str_param(p, "text").map_or(display, str::to_string);
    let link = Hyperlink { action, tooltip: str_param(p, "tooltip").unwrap_or("").to_string(), highlight_click: false };
    if s.doc()?.selection.text.is_some() {
        return super::text::link_text(s, link, &display);
    }
    edit_shapes(s, p, "insert.hyperlink", |sh| {
        sh.click = Some(link.clone());
        Ok(())
    })
}

fn action_button(s: &mut Session, p: &Value) -> Result<Value> {
    use deckcraft_model::text::{Action, Hyperlink};
    let kind = str_param(p, "kind").unwrap_or("custom");
    let (preset, action) = match kind {
        "back" | "previous" => ("actionButtonBackPrevious", Some(Action::PreviousSlide)),
        "forward" | "next" => ("actionButtonForwardNext", Some(Action::NextSlide)),
        "beginning" => ("actionButtonBeginning", Some(Action::FirstSlide)),
        "end" => ("actionButtonEnd", Some(Action::LastSlide)),
        "home" => ("actionButtonHome", Some(Action::FirstSlide)),
        "information" => ("actionButtonInformation", None),
        "return" => ("actionButtonReturn", Some(Action::LastViewed)),
        "movie" => ("actionButtonMovie", None),
        "document" => ("actionButtonDocument", None),
        "sound" => ("actionButtonSound", None),
        "help" => ("actionButtonHelp", None),
        _ => ("actionButtonBlank", None),
    };
    let rect = rect_param(p, "rect").unwrap_or_else(|| default_rect(s, 48.0, 48.0));
    let shape = Shape {
        xfrm: Some(rect),
        geom: Geom::preset(preset),
        style: Some(ShapeStyle::accent(SchemeSlot::Accent1)),
        click: action.map(|a| Hyperlink { action: a, tooltip: String::new(), highlight_click: true }),
        ..Default::default()
    };
    let id = add_shape(s, shape, true)?;
    Ok(json!({"id": id}))
}

#[allow(dead_code)]
fn _kinds() -> RunKind {
    RunKind::Text
}

fn with_chart(s: &mut Session, p: &Value, cmd: &str, f: impl Fn(&mut Chart) -> Result<()>) -> Result<Value> {
    edit_shapes(s, p, cmd, |sh| match &mut sh.kind {
        ShapeKind::Chart(c) => f(c),
        _ => Err(bad(cmd, "not a chart")),
    })
}

fn chart_type_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let k = chart_type(str_param(p, "type").unwrap_or("column"));
    with_chart(s, p, "chart.type", |c| {
        c.kind = k;
        c.vary_colors = matches!(k, ChartType::Pie | ChartType::Doughnut) || c.series.len() == 1 && c.vary_colors;
        c.raw = None;
        Ok(())
    })
}

fn chart_data(s: &mut Session, p: &Value) -> Result<Value> {
    let cats: Option<Vec<String>> = p
        .get("categories")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).collect());
    let series: Option<Vec<deckcraft_model::chart::Series>> = p.get("series").and_then(Value::as_array).map(|a| {
        a.iter()
            .map(|sv| deckcraft_model::chart::Series {
                name: sv.get("name").and_then(Value::as_str).unwrap_or("Series").to_string(),
                values: sv.get("values").and_then(Value::as_array).map(|v| v.iter().map(Value::as_f64).collect()).unwrap_or_default(),
                ..Default::default()
            })
            .collect()
    });
    with_chart(s, p, "chart.data", |c| {
        if let Some(cs) = &cats {
            c.categories = cs.clone();
        }
        if let Some(se) = &series {
            // Keep series formatting by position.
            let old = std::mem::take(&mut c.series);
            c.series = se
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let mut x = old.get(i).cloned().unwrap_or_default();
                    x.name = n.name.clone();
                    x.values = n.values.clone();
                    x
                })
                .collect();
        }
        c.raw = None;
        Ok(())
    })
}

fn chart_options(s: &mut Session, p: &Value) -> Result<Value> {
    with_chart(s, p, "chart.options", |c| {
        if let Some(t) = p.get("title") {
            c.title = t.as_str().map(String::from);
        }
        if let Some(l) = p.get("legend") {
            c.legend = l.as_str().map(String::from);
        }
        if let Some(v) = bool_param(p, "dataLabels") {
            c.data_labels = v;
        }
        if let Some(v) = bool_param(p, "gridlines") {
            c.gridlines = v;
        }
        if let Some(v) = str_param(p, "palette") {
            c.palette = v.to_string();
        }
        c.raw = None;
        Ok(())
    })
}

/// SmartArt-style graphics built from shapes (grouped), with our own layouts.
fn smart_art(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = str_param(p, "kind").unwrap_or("process").to_string();
    let items: Vec<String> =
        p.get("items").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let items = if items.is_empty() { vec!["Text".to_string(); 3] } else { items };
    let n = items.len().clamp(1, 12);
    let size = s.doc()?.doc.slide_size;
    let area = Xfrm::new(size.width * 0.1, size.height * 0.28, size.width * 0.8, size.height * 0.6);
    let slots = [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6];
    let mut shapes: Vec<(String, Xfrm, usize)> = vec![];
    match kind.as_str() {
        "cycle" => {
            let r = area.h.min(area.w) * 0.38;
            let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
            let d = r * 0.62;
            for i in 0..n {
                let a = (i as f64 / n as f64 * 360.0 - 90.0).to_radians();
                shapes.push(("ellipse".into(), Xfrm::new(cx + r * a.cos() - d / 2.0, cy + r * a.sin() - d / 2.0, d, d), i));
            }
        }
        "hierarchy" => {
            let w = area.w / (n.max(2) - 1) as f64 * 0.8;
            shapes.push(("roundRect".into(), Xfrm::new(area.x + area.w / 2.0 - w / 2.0, area.y, w, area.h * 0.3), 0));
            for i in 1..n {
                let x = area.x + (i - 1) as f64 * area.w / (n - 1).max(1) as f64;
                shapes.push(("roundRect".into(), Xfrm::new(x + w * 0.1, area.y + area.h * 0.55, w, area.h * 0.3), i));
            }
        }
        "pyramid" => {
            let h = area.h / n as f64;
            for i in 0..n {
                let w = area.w * 0.25 + area.w * 0.75 * (i as f64 + 1.0) / n as f64;
                shapes.push(("trapezoid".into(), Xfrm::new(area.x + (area.w - w) / 2.0, area.y + i as f64 * h, w, h * 0.95), i));
            }
        }
        "matrix" => {
            let (w, h) = (area.w / 2.0, area.h / 2.0);
            for i in 0..n.min(4) {
                shapes.push((
                    "roundRect".into(),
                    Xfrm::new(area.x + (i % 2) as f64 * w + 4.0, area.y + (i / 2) as f64 * h + 4.0, w - 8.0, h - 8.0),
                    i,
                ));
            }
        }
        "list" => {
            let h = area.h / n as f64;
            for i in 0..n {
                shapes.push(("roundRect".into(), Xfrm::new(area.x, area.y + i as f64 * h, area.w, h * 0.85), i));
            }
        }
        _ => {
            let gap = 24.0;
            let w = (area.w - gap * (n as f64 - 1.0)) / n as f64;
            for i in 0..n {
                shapes.push((
                    if i + 1 < n { "homePlate".into() } else { "roundRect".into() },
                    Xfrm::new(area.x + i as f64 * (w + gap), area.y + area.h * 0.25, w, area.h * 0.5),
                    i,
                ));
            }
        }
    }
    let mut ids = vec![];
    for (preset, x, i) in shapes {
        let mut sh = Shape {
            xfrm: Some(x),
            geom: Geom::preset(&preset),
            style: Some(ShapeStyle::accent(slots.get(i % 6).copied().unwrap_or(SchemeSlot::Accent1))),
            ..Default::default()
        };
        let mut body = TextBody::from_text(items.get(i).map(String::as_str).unwrap_or(""));
        body.body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
        for para in &mut body.paragraphs {
            para.props.align = Some(deckcraft_model::text::Align::Center);
            for r in &mut para.runs {
                r.props.size = Some(20.0);
            }
        }
        sh.text = Some(body);
        ids.push(add_shape(s, sh, false)?.0);
    }
    let g = s.execute("arrange.group", &json!({"ids": ids}))?;
    if let Some(gid) = g.get("id").and_then(Value::as_u64) {
        let _ = s.execute("shape.rename", &json!({"id": gid, "name": format!("SmartArt ({kind})")}));
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use deckcraft_model::text::RunKind;
    use serde_json::json;

    use crate::Session;

    fn session() -> Session {
        Session::with_new()
    }

    fn undo(s: &Session) -> usize {
        s.doc().unwrap().history.undo.len()
    }

    fn date_run(s: &Session) -> Option<deckcraft_model::Run> {
        s.doc()
            .unwrap()
            .current_slide()
            .unwrap()
            .shapes
            .iter()
            .find(|x| x.ph_type() == Some(deckcraft_model::PhType::Date))
            .and_then(|x| x.text.as_ref())
            .and_then(|t| t.paragraphs.first())
            .and_then(|p| p.runs.first())
            .cloned()
    }

    fn title_field(s: &Session) -> deckcraft_model::Run {
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
        title.paragraphs[0].runs.iter().find(|r| r.text.contains(':') || matches!(r.kind, RunKind::Field { .. })).cloned().unwrap()
    }

    fn edit_title(s: &mut Session) {
        let id = s.doc().unwrap().current_slide().unwrap().shapes[0].id.0;
        s.execute("text.edit", &json!({"id": id})).unwrap();
        s.execute("text.insert", &json!({"text": "On "})).unwrap();
    }

    #[test]
    fn no_params_opens_the_dialog_and_edits_nothing() {
        let mut s = session();
        let rev = s.doc().unwrap().revision;
        for p in [json!({}), serde_json::Value::Null] {
            s.ui_requests.clear();
            s.execute("insert.dateTime", &p).unwrap();
            assert!(matches!(&s.ui_requests[..], [crate::UiRequest::Dialog { id, .. }] if id == "dateTime"));
            assert_eq!(s.doc().unwrap().revision, rev);
            assert_eq!(undo(&s), 0);
            assert!(date_run(&s).is_none());
        }
    }

    #[test]
    fn a_format_at_the_caret_is_one_updating_field() {
        let mut s = session();
        edit_title(&mut s);
        let before = undo(&s);
        let v = s.execute("insert.dateTime", &json!({"format": "datetime10", "update": true})).unwrap();
        assert_eq!(v["where"], "caret");
        let run = title_field(&s);
        assert!(matches!(run.kind, RunKind::Field { ref field } if field == "datetime10"), "{run:?}");
        assert!(run.text.contains(':'), "{run:?}");
        assert_eq!(undo(&s), before + 1);
        assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Date & Time");
        s.execute("edit.undo", &json!({})).unwrap();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].text.as_ref().unwrap().text();
        assert_eq!(title, "On ");
    }

    #[test]
    fn update_false_inserts_plain_text_in_the_chosen_language() {
        let mut s = session();
        edit_title(&mut s);
        let before = undo(&s);
        let now = deckcraft_text::datetime::DateTime::now_local().unwrap();
        let expect = deckcraft_text::datetime::field_text("datetime1", Some("en-GB"), now).unwrap();
        s.execute("insert.dateTime", &json!({"format": "datetime1", "update": false, "language": "en-GB"})).unwrap();
        let title = s.doc().unwrap().current_slide().unwrap().shapes[0].text.clone().unwrap();
        let run = title.paragraphs[0].runs.iter().find(|r| r.text == expect).cloned().unwrap();
        assert_eq!(run.kind, RunKind::Text);
        assert_eq!(run.props.lang.as_deref(), Some("en-GB"));
        assert_eq!(undo(&s), before + 1);
    }

    #[test]
    fn a_named_format_in_french_and_a_bad_value_change_nothing() {
        let mut s = session();
        edit_title(&mut s);
        let rev = s.doc().unwrap().revision;
        let undos = undo(&s);
        let err = s.execute("insert.dateTime", &json!({"format": "datetime2", "language": "fr-FR"})).unwrap_err();
        assert!(err.to_string().contains("isn't available"), "{err}");
        assert_eq!(s.doc().unwrap().revision, rev);
        assert_eq!(undo(&s), undos);
        for p in [
            json!({"format": "datetime14"}),
            json!({"format": "nope"}),
            json!({"update": "yes"}),
            json!({"language": "de-DE"}),
            json!({"language": ""}),
        ] {
            let err = s.execute("insert.dateTime", &p).unwrap_err();
            assert!(err.to_string().contains("insert.dateTime"), "{err}");
            assert_eq!(s.doc().unwrap().revision, rev);
            assert_eq!(undo(&s), undos);
        }
    }

    #[test]
    fn without_a_caret_the_date_lands_on_this_slide_only() {
        let mut s = session();
        s.execute("slide.new", &json!({"layout": "blank"})).unwrap();
        s.execute("slide.go", &json!({"index": 0})).unwrap();
        let now = deckcraft_text::datetime::DateTime::now_local().unwrap();
        let clock = deckcraft_text::datetime::field_text("datetime10", Some("en-US"), now).unwrap();
        let v = s.execute("insert.dateTime", &json!({"format": "datetime10", "language": "en_US"})).unwrap();
        assert_eq!(v["where"], "footer");
        let run = date_run(&s).unwrap();
        assert!(matches!(run.kind, RunKind::Field { ref field } if field == "datetime10"), "{run:?}");
        assert_eq!(run.text, clock);
        assert_eq!(run.props.lang.as_deref(), Some("en-US"));
        s.execute("slide.go", &json!({"index": 1})).unwrap();
        assert!(date_run(&s).is_none(), "the other slide keeps its own footer");
        s.execute("slide.go", &json!({"index": 0})).unwrap();
        let fixed = deckcraft_text::datetime::field_text("datetime12", None, now).unwrap();
        s.execute("insert.dateTime", &json!({"format": "datetime12", "update": false})).unwrap();
        let run = date_run(&s).unwrap();
        assert_eq!((run.kind, run.text), (RunKind::Text, fixed));
        s.execute("edit.undo", &json!({})).unwrap();
        let run = date_run(&s).unwrap();
        assert!(matches!(run.kind, RunKind::Field { .. }), "one undo restores the field");
    }
}
