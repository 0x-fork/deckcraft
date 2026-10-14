//! Design tab and masters: themes, colours, fonts, backgrounds, slide size, header & footer,
//! Slide Master view.

use std::sync::Arc;

use deckcraft_geom::Size;
use deckcraft_model::text::{Run, RunKind, TextBody};
use deckcraft_model::theme::{builtin_color_schemes, builtin_font_schemes, builtin_themes};
use deckcraft_model::{Background, Fill, PhType, Presentation, Shape, ShapeId, defaults};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, Target};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("design.theme", "Themes", ["Design", "Themes"], None, "{name} (see design.themes)", has_doc, theme),
        cmd!(query "design.themes", "Theme Gallery", [], None, "{} → themes, colour schemes and font schemes", always, themes),
        cmd!("design.colors", "Colors", ["Design", "Variants"], None, "{name} | {colors: {accent1: #RRGGBB, …}}", has_doc, colors),
        cmd!("design.fonts", "Fonts", ["Design", "Variants"], None, "{name} | {major, minor}", has_doc, fonts),
        cmd!(
            "design.background",
            "Format Background",
            ["Design", "Customize"],
            None,
            "{color? | gradient? | picture?: base64 | pattern? | style?: 1..12 | reset?, all?: bool, index?, transparency?: 0..1, pictureProps?: {tile?, offsetX?, offsetY?: pt, scaleX?, scaleY?: 0.01..10, align?, mirror?, stretch?: [l,t,r,b], rotateWithShape?}, copy?: bool}",
            has_slide,
            background
        ),
        cmd!("design.hideBackgroundGraphics", "Hide Background Graphics", ["Format Background"], None, "{hide?: bool}", has_slide, hide_bg_graphics),
        cmd!(
            "design.slideSize",
            "Slide Size",
            ["Design", "Customize"],
            None,
            "{preset?: widescreen|standard|… | w, h: pt, scale?: maximize|ensureFit|none}",
            has_doc,
            slide_size
        ),
        cmd!(
            "design.headerFooter",
            "Header and Footer…",
            ["Insert", "Text"],
            None,
            "{date?: bool, dateText?: fixed text, slideNumber?: bool, footer?: bool, footerText?, hideOnTitle?: bool, all?: bool (default true)}",
            has_doc,
            header_footer
        ),
        cmd!(noundo "view.slideMaster", "Slide Master", ["View", "Master Views"], None, "{master?: index, layout?: index}", has_doc, master_view),
        cmd!(noundo "view.closeMaster", "Close Master View", ["Slide Master", "Close"], None, "{}", has_doc, close_master),
        cmd!("master.insertLayout", "Insert Layout", ["Slide Master", "Edit Master"], None, "{name?}", has_doc, insert_layout),
        cmd!("master.renameLayout", "Rename Layout", ["Slide Master", "Edit Master"], None, "{name, master?, layout?}", has_doc, rename_layout),
        cmd!("master.deleteLayout", "Delete Layout", ["Slide Master", "Edit Master"], None, "{master?, layout?}", has_doc, delete_layout),
        cmd!(
            "master.insertPlaceholder",
            "Insert Placeholder",
            ["Slide Master", "Master Layout"],
            None,
            "{kind: content|text|picture|chart|table|media, rect?: [x,y,w,h]}",
            has_doc,
            insert_placeholder
        ),
    ]
}

fn theme(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("design.theme", "missing `name`"))?;
    let t =
        builtin_themes().into_iter().find(|t| t.name.eq_ignore_ascii_case(name)).ok_or_else(|| bad("design.theme", format!("no theme `{name}`")))?;
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        let m = doc.masters.get_mut(mi).ok_or_else(|| bad("design.theme", "no master"))?;
        let m = Arc::make_mut(m);
        m.theme = t.clone();
        m.name = t.name.clone();
        m.color_map = vec![];
        Ok(())
    })?;
    Ok(json!({"theme": name}))
}

fn master_index(doc: &Presentation, sel: &crate::Selection) -> usize {
    match sel.target {
        Target::Master { master } | Target::Layout { master, .. } => master,
        Target::Slides => doc.slides.get(sel.slide).and_then(|s| doc.masters.iter().position(|m| m.layout(s.layout).is_some())).unwrap_or(0),
    }
}

fn themes(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(json!({
        "themes": builtin_themes().iter().map(|t| json!({"name": t.name, "major": t.fonts.major.latin, "minor": t.fonts.minor.latin, "colors": t.colors.colors.iter().map(|c| format!("#{}", c.hex())).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "colorSchemes": builtin_color_schemes().iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "fontSchemes": builtin_font_schemes().iter().map(|f| json!({"name": f.name, "major": f.major.latin, "minor": f.minor.latin})).collect::<Vec<_>>(),
    }))
}

fn colors(s: &mut Session, p: &Value) -> Result<Value> {
    let scheme = if let Some(name) = str_param(p, "name") {
        Some(
            builtin_color_schemes()
                .into_iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| bad("design.colors", format!("no colour scheme `{name}`")))?,
        )
    } else {
        None
    };
    let custom = p.get("colors").and_then(Value::as_object).cloned();
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        let m = Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("design.colors", "no master"))?);
        if let Some(sc) = &scheme {
            m.theme.colors = sc.clone();
        }
        if let Some(c) = &custom {
            for (k, v) in c {
                if let (Some(slot), Some(rgb)) = (deckcraft_color::SchemeSlot::from_xml(k), v.as_str().and_then(deckcraft_color::Rgba::from_hex)) {
                    m.theme.colors.set(slot, rgb);
                }
            }
            m.theme.colors.name = "Custom".into();
        }
        Ok(())
    })?;
    ok()
}

fn fonts(s: &mut Session, p: &Value) -> Result<Value> {
    let scheme = if let Some(name) = str_param(p, "name") {
        builtin_font_schemes()
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| bad("design.fonts", format!("no font scheme `{name}`")))?
    } else {
        let major = str_param(p, "major").ok_or_else(|| bad("design.fonts", "give `name`, or `major` and `minor`"))?;
        let minor = str_param(p, "minor").unwrap_or(major);
        deckcraft_model::theme::FontScheme {
            name: "Custom".into(),
            major: deckcraft_model::theme::FontSet { latin: major.into(), ..Default::default() },
            minor: deckcraft_model::theme::FontSet { latin: minor.into(), ..Default::default() },
        }
    };
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("design.fonts", "no master"))?).theme.fonts = scheme;
        Ok(())
    })?;
    ok()
}

fn background(s: &mut Session, p: &Value) -> Result<Value> {
    if bool_or(p, "copy", false) {
        return copy_slide_background(s);
    }
    let props = p.get("pictureProps").filter(|v| v.is_object()).cloned();
    let transparency = f64_param(p, "transparency");
    let reset = bool_or(p, "reset", false);
    let color = color_param(p, "color");
    let style = usize_param(p, "style");
    let op = if reset {
        BgOp::Set(None)
    } else if let Some(mut c) = color {
        if let Some(t) = transparency {
            set_solid_transparency(&mut c, t);
        }
        BgOp::Set(Some(Background::Fill { fill: Fill::solid(c) }))
    } else if let Some(i) = style {
        // Background styles: 3 theme bg fills × 4 colours (bg1, bg2, tx1, tx2).
        let slots =
            [deckcraft_color::SchemeSlot::Bg1, deckcraft_color::SchemeSlot::Bg2, deckcraft_color::SchemeSlot::Tx2, deckcraft_color::SchemeSlot::Tx1];
        let i = i.clamp(1, 12) - 1;
        BgOp::Set(Some(Background::Ref {
            idx: 1001 + (i / 4) as u32,
            color: deckcraft_model::ColorRef::scheme(slots.get(i % 4).copied().unwrap_or(deckcraft_color::SchemeSlot::Bg1)),
        }))
    } else if let Some(g) = p.get("gradient") {
        let mut g = super::shape::gradient_param(g).ok_or_else(|| bad("design.background", "gradient needs two or more `stops`"))?;
        g.rotate_with_shape = false;
        BgOp::Set(Some(Background::Fill { fill: Fill::Gradient(g) }))
    } else if let Some(pt) = p.get("pattern") {
        BgOp::Set(Some(Background::Fill {
            fill: Fill::Pattern(deckcraft_model::style::PatternFill {
                preset: pt.get("preset").and_then(Value::as_str).unwrap_or("pct50").to_string(),
                fg: pt.get("fg").and_then(color_value).unwrap_or(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Accent1)),
                bg: pt.get("bg").and_then(color_value).unwrap_or(deckcraft_model::ColorRef::scheme(deckcraft_color::SchemeSlot::Bg1)),
            }),
        }))
    } else if p.get("picture").is_some() {
        let (name, bytes) = super::insert::media_bytes(&with_param(p, "data", p.get("picture").cloned().unwrap_or_default()), "design.background")?;
        let ct = super::insert::content_type(&name, &bytes);
        let media = s.edit(|doc, _| Ok(doc.add_media(&name, ct, bytes)))?;
        let mut pf = deckcraft_model::style::PictureFill { media, ..Default::default() };
        if let Some(pp) = &props {
            apply_picture_props(&mut pf, pp);
        }
        if let Some(t) = transparency {
            set_picture_transparency(&mut pf, t);
        }
        BgOp::Set(Some(Background::Fill { fill: Fill::Picture(pf) }))
    } else if props.is_some() || transparency.is_some() {
        BgOp::Adjust
    } else {
        return Err(bad(
            "design.background",
            "give `color`, `gradient`, `pattern`, `picture`, `style`, `reset`, `pictureProps`, `transparency` or `copy`",
        ));
    };
    let all = bool_or(p, "all", false);
    let index = usize_param(p, "index");
    if matches!(op, BgOp::Adjust) {
        let next = {
            let st = s.doc()?;
            let current = background_of(&st.doc, &st.selection, index);
            let picture = matches!(current, Some(Background::Fill { fill: Fill::Picture(_) }));
            let solid = matches!(current, Some(Background::Fill { fill: Fill::Solid { .. } }));
            if props.is_some() && !picture {
                return Err(bad("design.background", "picture options need a picture background"));
            }
            if transparency.is_some() && !picture && !solid {
                return Err(bad("design.background", "transparency needs a solid or picture background"));
            }
            let mut next = current.clone();
            if let Some(bg) = next.as_mut() {
                adjust_background(bg, props.as_ref(), transparency);
            }
            if next == current { None } else { Some(next) }
        };
        let Some(next) = next else { return ok() };
        s.edit(|doc, sel| {
            write_background(doc, sel, index, false, next);
            Ok(())
        })?;
        return ok();
    }
    let BgOp::Set(bg) = op else { return ok() };
    let unchanged = {
        let st = s.doc()?;
        backgrounds_already(&st.doc, &st.selection, index, all, &bg)
    };
    if unchanged {
        return ok();
    }
    s.edit(|doc, sel| {
        write_background(doc, sel, index, all, bg);
        Ok(())
    })?;
    ok()
}

enum BgOp {
    Set(Option<Background>),
    Adjust,
}

fn background_of(doc: &Presentation, sel: &crate::Selection, index: Option<usize>) -> Option<Background> {
    match sel.target {
        Target::Master { master } => doc.masters.get(master).and_then(|m| m.background.clone()),
        Target::Layout { master, layout } => doc.masters.get(master).and_then(|m| m.layouts.get(layout)).and_then(|l| l.background.clone()),
        Target::Slides => doc.slides.get(index.unwrap_or(sel.slide)).and_then(|sl| sl.background.clone()),
    }
}

fn backgrounds_already(doc: &Presentation, sel: &crate::Selection, index: Option<usize>, all: bool, bg: &Option<Background>) -> bool {
    match sel.target {
        Target::Master { master } => doc.masters.get(master).is_some_and(|m| m.background == *bg),
        Target::Layout { master, layout } => doc.masters.get(master).and_then(|m| m.layouts.get(layout)).is_some_and(|l| l.background == *bg),
        Target::Slides => {
            let list: Vec<usize> = if all { (0..doc.slides.len()).collect() } else { vec![index.unwrap_or(sel.slide)] };
            !list.is_empty() && list.iter().all(|i| doc.slides.get(*i).is_some_and(|sl| sl.background == *bg))
        }
    }
}

fn write_background(doc: &mut Presentation, sel: &crate::Selection, index: Option<usize>, all: bool, bg: Option<Background>) {
    match sel.target {
        Target::Master { master } => {
            if let Some(m) = doc.masters.get_mut(master) {
                Arc::make_mut(m).background = bg;
            }
        }
        Target::Layout { master, layout } => {
            if let Some(l) = doc.masters.get_mut(master).and_then(|m| Arc::make_mut(m).layouts.get_mut(layout)) {
                l.background = bg;
            }
        }
        Target::Slides => {
            let list: Vec<usize> = if all { (0..doc.slides.len()).collect() } else { vec![index.unwrap_or(sel.slide)] };
            for i in list {
                if let Some(sl) = doc.slides.get_mut(i) {
                    Arc::make_mut(sl).background = bg.clone();
                }
            }
        }
    }
}

fn copy_slide_background(s: &mut Session) -> Result<Value> {
    let bg = {
        let st = s.doc()?;
        if !matches!(st.selection.target, Target::Slides) {
            return Err(bad("design.background", "copy applies to slides"));
        }
        let bg = st.doc.slides.get(st.selection.slide).and_then(|sl| sl.background.clone());
        if st.doc.slides.iter().all(|sl| sl.background == bg) {
            return ok();
        }
        bg
    };
    s.edit(|doc, sel| {
        write_background(doc, sel, None, true, bg);
        Ok(())
    })?;
    ok()
}

fn adjust_background(bg: &mut Background, props: Option<&Value>, transparency: Option<f64>) {
    match bg {
        Background::Fill { fill: Fill::Picture(pf) } => {
            if let Some(pp) = props {
                apply_picture_props(pf, pp);
            }
            if let Some(t) = transparency {
                set_picture_transparency(pf, t);
            }
        }
        Background::Fill { fill: Fill::Solid { color } } => {
            if let Some(t) = transparency {
                set_solid_transparency(color, t);
            }
        }
        _ => {}
    }
}

fn set_picture_transparency(pf: &mut deckcraft_model::style::PictureFill, t: f64) {
    let a = 1.0 - t.clamp(0.0, 1.0);
    pf.alpha = (a < 1.0).then_some(a);
}

fn set_solid_transparency(color: &mut deckcraft_model::ColorRef, t: f64) {
    color.mods.retain(|m| !matches!(m, deckcraft_color::ColorTransform::Alpha(_)));
    let t = t.clamp(0.0, 1.0);
    if t > 0.0 {
        color.mods.push(deckcraft_color::ColorTransform::Alpha(((1.0 - t) * 100_000.0).round() as i32));
    }
}

/// Picture-fill options for Format Background (tile, offsets, scale, alignment, mirror, stretch).
fn apply_picture_props(pf: &mut deckcraft_model::style::PictureFill, pp: &Value) {
    use deckcraft_model::style::PictureMode;
    let f = |k: &str| pp.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
    if let Some(up) = pp.get("rotateWithShape").and_then(Value::as_bool) {
        pf.upright = !up;
    }
    match pp.get("tile").and_then(Value::as_bool) {
        Some(true) if !matches!(pf.mode, PictureMode::Tile { .. }) => {
            pf.mode = PictureMode::Tile { tx: 0.0, ty: 0.0, sx: 1.0, sy: 1.0, flip: "none".into(), align: "tl".into() };
        }
        Some(false) if !matches!(pf.mode, PictureMode::Stretch { .. }) => pf.mode = PictureMode::default(),
        _ => {}
    }
    match &mut pf.mode {
        PictureMode::Tile { tx, ty, sx, sy, flip, align } => {
            if let Some(v) = f("offsetX") {
                *tx = v.clamp(-4000.0, 4000.0);
            }
            if let Some(v) = f("offsetY") {
                *ty = v.clamp(-4000.0, 4000.0);
            }
            if let Some(v) = f("scaleX") {
                *sx = v.clamp(0.01, 10.0);
            }
            if let Some(v) = f("scaleY") {
                *sy = v.clamp(0.01, 10.0);
            }
            if let Some(m) = pp.get("mirror").and_then(Value::as_str).filter(|m| matches!(*m, "none" | "x" | "y" | "xy")) {
                *flip = m.to_string();
            }
            if let Some(a) =
                pp.get("align").and_then(Value::as_str).filter(|a| matches!(*a, "tl" | "t" | "tr" | "l" | "ctr" | "r" | "bl" | "b" | "br"))
            {
                *align = a.to_string();
            }
        }
        PictureMode::Stretch { fill_rect } => {
            if let Some(arr) = pp.get("stretch").and_then(Value::as_array) {
                for (slot, v) in fill_rect.iter_mut().zip(arr) {
                    if let Some(v) = v.as_f64().filter(|v| v.is_finite()) {
                        *slot = v.clamp(-10.0, 0.99);
                    }
                }
            }
        }
    }
}

fn hide_bg_graphics(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, sel| {
        if let Some(sl) = doc.slides.get_mut(sel.slide) {
            let sl = Arc::make_mut(sl);
            sl.show_master_shapes = !bool_param(p, "hide").unwrap_or(sl.show_master_shapes);
        }
        Ok(())
    })?;
    ok()
}

fn scale_shapes(shapes: &mut [Shape], sx: f64, sy: f64, k: f64, ox: f64, oy: f64) {
    for sh in shapes {
        if let Some(x) = sh.xfrm.as_mut() {
            x.x = x.x * sx + ox;
            x.y = x.y * sy + oy;
            x.w *= sx;
            x.h *= sy;
        }
        if let Some(t) = sh.text.as_mut() {
            deckcraft_model::edit::format_all(t, &|r| {
                if let Some(sz) = r.size.as_mut() {
                    *sz = (*sz * k).max(1.0);
                }
            });
        }
    }
}

fn slide_size(s: &mut Session, p: &Value) -> Result<Value> {
    let size = if let Some(pre) = str_param(p, "preset") {
        let k = pre.to_ascii_lowercase();
        let found = defaults::SLIDE_SIZES.iter().find(|(l, _, _)| l.to_ascii_lowercase().starts_with(&k) || l.to_ascii_lowercase().contains(&k));
        let (_, w, h) = found.ok_or_else(|| bad("design.slideSize", format!("unknown preset `{pre}`")))?;
        Size::new(*w, *h)
    } else {
        Size::new(
            f64_param(p, "w").ok_or_else(|| bad("design.slideSize", "missing `w`"))?.clamp(72.0, 4032.0),
            f64_param(p, "h").ok_or_else(|| bad("design.slideSize", "missing `h`"))?.clamp(72.0, 4032.0),
        )
    };
    let mode = str_param(p, "scale").unwrap_or("ensureFit").to_string();
    s.edit(|doc, _| {
        let old = doc.slide_size;
        let (sx, sy) = (size.width / old.width.max(1.0), size.height / old.height.max(1.0));
        let (sx, sy, k, ox, oy) = match mode.as_str() {
            "none" => (1.0, 1.0, 1.0, 0.0, 0.0),
            "maximize" => {
                let k = sx.max(sy);
                (k, k, k, (size.width - old.width * k) / 2.0, (size.height - old.height * k) / 2.0)
            }
            _ => {
                let k = sx.min(sy);
                (k, k, k, (size.width - old.width * k) / 2.0, (size.height - old.height * k) / 2.0)
            }
        };
        // Masters and layouts stretch to the new size; slides scale by mode.
        let (msx, msy) = (size.width / old.width.max(1.0), size.height / old.height.max(1.0));
        for m in &mut doc.masters {
            let m = Arc::make_mut(m);
            scale_shapes(&mut m.shapes, msx, msy, 1.0, 0.0, 0.0);
            for l in &mut m.layouts {
                scale_shapes(&mut l.shapes, msx, msy, 1.0, 0.0, 0.0);
            }
        }
        for sl in &mut doc.slides {
            scale_shapes(&mut Arc::make_mut(sl).shapes, sx, sy, k, ox, oy);
        }
        doc.slide_size = size;
        Ok(())
    })?;
    Ok(json!({"w": size.width, "h": size.height}))
}

fn header_footer(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, sel| {
        let hf = &mut doc.header_footer;
        if let Some(v) = bool_param(p, "date") {
            hf.date = v;
        }
        if let Some(v) = str_param(p, "dateText") {
            hf.date_text = v.into();
        }
        if let Some(v) = bool_param(p, "slideNumber") {
            hf.slide_number = v;
        }
        if let Some(v) = bool_param(p, "footer") {
            hf.footer = v;
        }
        if let Some(v) = str_param(p, "footerText") {
            hf.footer_text = v.into();
            hf.footer = hf.footer || !v.is_empty();
        }
        if let Some(v) = bool_param(p, "hideOnTitle") {
            hf.hide_on_title = v;
        }
        let hf = hf.clone();
        let date_given = str_param(p, "dateText").is_some();
        let all = bool_or(p, "all", true);
        let list: Vec<usize> = if all { (0..doc.slides.len()).collect() } else { vec![sel.slide] };
        for i in list {
            let Some(layout) = doc.slides.get(i).map(|x| x.layout) else { continue };
            let lay = doc.layout(layout).map(|(_, l)| l.clone());
            let is_title = lay.as_ref().is_some_and(|l| l.kind == deckcraft_model::LayoutType::Title);
            let want = |k: PhType| {
                !(hf.hide_on_title && is_title)
                    && match k {
                        PhType::Date => hf.date,
                        PhType::SlideNum => hf.slide_number,
                        PhType::Footer => hf.footer,
                        _ => false,
                    }
            };
            let mut new_ids = vec![];
            for _ in 0..3 {
                new_ids.push(ShapeId(doc.alloc_id()));
            }
            let Some(sl) = doc.slides.get_mut(i) else { continue };
            let sl = Arc::make_mut(sl);
            for (n, k) in [PhType::Date, PhType::Footer, PhType::SlideNum].into_iter().enumerate() {
                let has = sl.shapes.iter().position(|x| x.ph_type() == Some(k));
                match (has, want(k)) {
                    (Some(pos), false) => {
                        sl.shapes.remove(pos);
                    }
                    (None, true) => {
                        let lp = lay.as_ref().and_then(|l| l.shapes.iter().find(|x| x.ph_type() == Some(k)).cloned());
                        let Some(lp) = lp else { continue };
                        let body = match k {
                            PhType::SlideNum => TextBody {
                                paragraphs: vec![deckcraft_model::Paragraph {
                                    runs: vec![Run {
                                        text: "‹#›".into(),
                                        props: Default::default(),
                                        kind: RunKind::Field { field: "slidenum".into() },
                                    }],
                                    ..Default::default()
                                }],
                                ..Default::default()
                            },
                            PhType::Date if hf.date_text.is_empty() => TextBody {
                                paragraphs: vec![deckcraft_model::Paragraph {
                                    runs: vec![Run { text: today(), props: Default::default(), kind: RunKind::Field { field: "datetime1".into() } }],
                                    ..Default::default()
                                }],
                                ..Default::default()
                            },
                            PhType::Date => TextBody::from_text(&hf.date_text),
                            _ => TextBody::from_text(&hf.footer_text),
                        };
                        sl.shapes.push(Shape {
                            id: new_ids.get(n).copied().unwrap_or_default(),
                            name: lp.name.clone(),
                            ph: lp.ph.clone(),
                            text: Some(body),
                            ..Default::default()
                        });
                    }
                    (Some(pos), true) if k == PhType::Footer => {
                        if let Some(sh) = sl.shapes.get_mut(pos) {
                            sh.text = Some(TextBody::from_text(&hf.footer_text));
                        }
                    }
                    (Some(pos), true) if k == PhType::Date && date_given => {
                        if let Some(sh) = sl.shapes.get_mut(pos) {
                            set_date(sh, &hf.date_text);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })?;
    ok()
}

/// Point a slide's date placeholder at a fixed date (its own plain text) or, for an empty
/// `fixed`, at an automatic date field (one it already has keeps its format).
fn set_date(sh: &mut Shape, fixed: &str) {
    let runs = || sh.text.iter().flat_map(|t| &t.paragraphs).flat_map(|p| &p.runs);
    let has_field = runs().any(|r| matches!(&r.kind, RunKind::Field { field } if deckcraft_text::datetime::is_date_field(field)));
    let props = runs().next().map(|r| r.props.clone()).unwrap_or_default();
    let (text, kind) = match fixed {
        "" if has_field => return,
        "" => (today(), RunKind::Field { field: "datetime1".into() }),
        _ if !has_field && sh.text.as_ref().is_some_and(|t| t.text() == fixed) => return,
        _ => (fixed.to_string(), RunKind::Text),
    };
    let para = deckcraft_model::Paragraph { runs: vec![Run { text, props, kind }], ..Default::default() };
    sh.text = Some(TextBody { paragraphs: vec![para], ..sh.text.clone().unwrap_or_default() });
}

/// Today's local date as a `datetime1` field shows it (M/D/YYYY): the saved text of a new date
/// field. Empty where there is no clock (wasm).
pub fn today() -> String {
    date_field_text("datetime1")
}

/// What date field `kind` shows now (empty if unknown or there is no clock).
pub fn date_field_text(kind: &str) -> String {
    use deckcraft_text::datetime::{DateTime, field_text};
    DateTime::now_local().and_then(|now| field_text(kind, None, now)).unwrap_or_default()
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
pub fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn master_view(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mi = usize_param(p, "master").unwrap_or_else(|| master_index(&st.doc, &st.selection));
    let layout = usize_param(p, "layout")
        .or_else(|| st.current_slide().and_then(|sl| st.doc.masters.get(mi).and_then(|m| m.layouts.iter().position(|l| l.id == sl.layout))));
    let target = match layout {
        Some(l) => Target::Layout { master: mi, layout: l },
        None => Target::Master { master: mi },
    };
    s.select(|_, sel| {
        sel.target = target;
        sel.shapes.clear();
        sel.text = None;
    })?;
    Ok(serde_json::to_value(target).unwrap_or_default())
}

fn close_master(s: &mut Session, _p: &Value) -> Result<Value> {
    s.select(|_, sel| {
        sel.target = Target::Slides;
        sel.shapes.clear();
        sel.text = None;
    })?;
    ok()
}

fn insert_layout(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").unwrap_or("Custom Layout").to_string();
    s.edit(|doc, sel| {
        let mi = master_index(doc, sel);
        let id = deckcraft_model::LayoutId(doc.alloc_id());
        let tid = ShapeId(doc.alloc_id());
        let size = doc.slide_size;
        let m = Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("master.insertLayout", "no master"))?);
        let title = m.shapes.iter().find(|x| x.ph_type() == Some(PhType::Title)).cloned().map(|mut t| {
            t.id = tid;
            t.text = Some(TextBody::default());
            t
        });
        let _ = size;
        let at = match sel.target {
            Target::Layout { layout, .. } => layout + 1,
            _ => m.layouts.len(),
        };
        m.layouts.insert(
            at.min(m.layouts.len()),
            deckcraft_model::Layout {
                id,
                name,
                kind: deckcraft_model::LayoutType::Custom,
                shapes: title.into_iter().collect(),
                background: None,
                show_master_shapes: true,
                preserve: true,
                color_map: None,
                raw_ext: None,
            },
        );
        sel.target = Target::Layout { master: mi, layout: at.min(m.layouts.len().saturating_sub(1)) };
        Ok(json!({"id": id}))
    })
}

fn layout_target(s: &Session, p: &Value) -> Result<(usize, usize)> {
    let st = s.doc()?;
    let (m, l) = match st.selection.target {
        Target::Layout { master, layout } => (master, layout),
        _ => (master_index(&st.doc, &st.selection), 0),
    };
    Ok((usize_param(p, "master").unwrap_or(m), usize_param(p, "layout").unwrap_or(l)))
}

fn rename_layout(s: &mut Session, p: &Value) -> Result<Value> {
    let (mi, li) = layout_target(s, p)?;
    let name = str_param(p, "name").ok_or_else(|| bad("master.renameLayout", "missing `name`"))?.to_string();
    s.edit(|doc, _| {
        let l =
            doc.masters.get_mut(mi).and_then(|m| Arc::make_mut(m).layouts.get_mut(li)).ok_or_else(|| bad("master.renameLayout", "no such layout"))?;
        l.name = name;
        Ok(())
    })?;
    ok()
}

fn delete_layout(s: &mut Session, p: &Value) -> Result<Value> {
    let (mi, li) = layout_target(s, p)?;
    s.edit(|doc, sel| {
        let id = doc.masters.get(mi).and_then(|m| m.layouts.get(li)).map(|l| l.id).ok_or_else(|| bad("master.deleteLayout", "no such layout"))?;
        if doc.slides.iter().any(|x| x.layout == id) {
            return Err(bad("master.deleteLayout", "slides use this layout"));
        }
        let m = Arc::make_mut(doc.masters.get_mut(mi).ok_or_else(|| bad("master.deleteLayout", "no master"))?);
        if m.layouts.len() <= 1 {
            return Err(bad("master.deleteLayout", "a master needs at least one layout"));
        }
        m.layouts.remove(li);
        sel.target = Target::Layout { master: mi, layout: li.min(m.layouts.len().saturating_sub(1)) };
        Ok(())
    })?;
    ok()
}

fn insert_placeholder(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = match str_param(p, "kind").unwrap_or("content") {
        "text" => PhType::Body,
        "picture" => PhType::Picture,
        "chart" => PhType::Chart,
        "table" => PhType::Table,
        "media" => PhType::Media,
        _ => PhType::Obj,
    };
    if !matches!(s.doc()?.selection.target, Target::Layout { .. }) {
        return Err(bad("master.insertPlaceholder", "select a layout in Slide Master view"));
    }
    let size = s.doc()?.doc.slide_size;
    let rect = rect_param(p, "rect").unwrap_or(deckcraft_geom::Xfrm::new(size.width * 0.25, size.height * 0.3, size.width * 0.5, size.height * 0.4));
    let idx = s.doc()?.shapes().iter().filter_map(|x| x.ph.as_ref().map(|p| p.idx)).max().unwrap_or(0) + 1;
    let shape = Shape {
        xfrm: Some(rect),
        ph: Some(deckcraft_model::Placeholder { kind, idx, ..Default::default() }),
        text: Some(TextBody::default()),
        ..Default::default()
    };
    let id = super::insert::add_shape(s, shape, true)?;
    Ok(json!({"id": id}))
}

#[cfg(test)]
mod tests {
    use deckcraft_model::style::PictureMode;
    use deckcraft_model::{Background, Fill};
    use serde_json::json;

    use crate::Session;

    fn png() -> String {
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        crate::cmd::base64_encode(&bytes)
    }

    fn picture(s: &Session) -> deckcraft_model::style::PictureFill {
        match &s.doc().unwrap().doc.slides[0].background {
            Some(Background::Fill { fill: Fill::Picture(pf) }) => pf.clone(),
            other => panic!("expected a picture background, got {other:?}"),
        }
    }

    fn undo_len(s: &Session) -> usize {
        s.doc().unwrap().history.undo.len()
    }

    #[test]
    fn civil_dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_734), (2026, 10, 8));
    }

    #[test]
    fn background_picture_options_tile_transparency_and_copy() {
        let mut s = Session::with_new();
        let n = undo_len(&s);
        let err = s.execute("design.background", &json!({"pictureProps": {"tile": true}})).unwrap_err().to_string();
        assert!(err.contains("picture background"), "{err}");
        assert!(s.execute("design.background", &json!({"transparency": 0.2})).is_err());
        assert_eq!(undo_len(&s), n);

        s.execute("design.background", &json!({"color": "accent1", "transparency": 0.25})).unwrap();
        match &s.doc().unwrap().doc.slides[0].background {
            Some(Background::Fill { fill: Fill::Solid { color } }) => assert!((color.alpha() - 0.75).abs() < 1e-4, "{}", color.alpha()),
            other => panic!("expected a solid background, got {other:?}"),
        }
        let n = undo_len(&s);
        s.execute("design.background", &json!({"transparency": 0.25})).unwrap();
        assert_eq!(undo_len(&s), n, "the same transparency is not an undo step");

        s.execute("design.background", &json!({"picture": png(), "name": "dot.png", "transparency": 0.4, "pictureProps": {"tile": true}})).unwrap();
        let pf = picture(&s);
        assert!((pf.alpha.unwrap_or(1.0) - 0.6).abs() < 1e-6);
        assert!(matches!(pf.mode, PictureMode::Tile { .. }));
        let n = undo_len(&s);

        s.execute(
            "design.background",
            &json!({"pictureProps": {"offsetX": 12.0, "offsetY": -3.0, "scaleX": 0.5, "scaleY": 50.0, "align": "ctr", "mirror": "xy", "rotateWithShape": false}}),
        )
        .unwrap();
        match picture(&s).mode {
            PictureMode::Tile { tx, ty, sx, sy, flip, align } => {
                assert_eq!((tx, ty, sx, sy), (12.0, -3.0, 0.5, 10.0));
                assert_eq!(flip, "xy");
                assert_eq!(align, "ctr");
            }
            other => panic!("expected a tile, got {other:?}"),
        }
        assert!(picture(&s).upright);
        s.execute("design.background", &json!({"pictureProps": {"align": "bogus", "offsetX": "nope"}})).unwrap();
        match picture(&s).mode {
            PictureMode::Tile { tx, align, .. } => {
                assert_eq!(tx, 12.0);
                assert_eq!(align, "ctr");
            }
            other => panic!("expected the tile to stay, got {other:?}"),
        }
        assert_eq!(undo_len(&s), n + 1, "ignored options are not an undo step");

        s.execute("design.background", &json!({"pictureProps": {"tile": false, "stretch": [0.1, 0.2, 0.0, 0.0]}})).unwrap();
        match picture(&s).mode {
            PictureMode::Stretch { fill_rect } => assert_eq!(fill_rect, [0.1, 0.2, 0.0, 0.0]),
            other => panic!("expected a stretch, got {other:?}"),
        }

        s.execute("slide.new", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.slides[1].background.is_none());
        s.execute("slide.go", &json!({"index": 0})).unwrap();
        let n = undo_len(&s);
        s.execute("design.background", &json!({"copy": true})).unwrap();
        assert_eq!(s.doc().unwrap().doc.slides[1].background, s.doc().unwrap().doc.slides[0].background);
        assert_eq!(undo_len(&s), n + 1);
        assert_eq!(s.doc().unwrap().history.undo.last().unwrap().label, "Format Background");
        s.execute("design.background", &json!({"copy": true})).unwrap();
        assert_eq!(undo_len(&s), n + 1, "copying an already shared background is not an undo step");
    }
}
