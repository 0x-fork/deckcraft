//! Home ▸ New Slide ▸ Reuse Slides: insert slides from another presentation.
//!
//! The slides take the destination's look by default (each one moves to the destination layout
//! with the same name, else the same kind); with `keepFormatting` the source master and its
//! layouts come along. Pictures, media and sounds are copied under fresh ids, and shape ids are
//! renumbered with animations, triggers and connector ends following them.

use std::collections::HashMap;
use std::sync::Arc;

use deckcraft_model::{LayoutId, MasterId, MediaId, Presentation, ShapeId, ShapeKind, Slide, SlideId};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "slide.reuse",
        "Reuse Slides…",
        ["Home", "Slides", "New Slide"],
        None,
        "{path? | data: base64, name?; slides?: [index] (0-based, default all); keepFormatting?: bool (default false: use the destination theme); at?: index (default after the current slide)}",
        has_doc,
        reuse
    )]
}

/// Keys under which the model stores a [`MediaId`] (pictures, media, posters, previews, sounds).
const MEDIA_KEYS: [&str; 4] = ["media", "poster", "preview", "sound"];

/// Rewrite every media id in `v` through `map`.
fn remap_media(v: &mut Value, map: &HashMap<u32, u32>, depth: usize) {
    if depth > 256 {
        return;
    }
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if MEDIA_KEYS.contains(&k.as_str())
                    && let Some(n) = x.as_u64().and_then(|n| u32::try_from(n).ok())
                {
                    if let Some(to) = map.get(&n) {
                        *x = json!(to);
                    }
                    continue;
                }
                remap_media(x, map, depth + 1);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| remap_media(x, map, depth + 1)),
        _ => {}
    }
}

/// `value` with its media ids rewritten, via its JSON form.
fn with_media<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T, map: &HashMap<u32, u32>) -> Result<T> {
    let mut v = serde_json::to_value(value).map_err(|e| EngineError::Other(e.to_string()))?;
    remap_media(&mut v, map, 0);
    serde_json::from_value(v).map_err(|e| EngineError::Other(e.to_string()))
}

fn shape_ids(shapes: &[deckcraft_model::Shape]) -> Vec<ShapeId> {
    let mut v = vec![];
    deckcraft_model::walk(shapes, &mut |sh, _| v.push(sh.id));
    v
}

/// Point connector ends at the renumbered shapes.
fn remap_connectors(shapes: &mut [deckcraft_model::Shape], map: &HashMap<ShapeId, ShapeId>, depth: usize) {
    if depth > 64 {
        return;
    }
    for sh in shapes {
        if let ShapeKind::Connector { start, end } = &mut sh.kind {
            for c in [start, end] {
                if let Some((id, _)) = c.as_mut() {
                    *id = map.get(id).copied().unwrap_or(*id);
                }
            }
        }
        if let Some(ch) = sh.children_mut() {
            remap_connectors(ch, map, depth + 1);
        }
    }
}

/// Fresh shape ids for `shapes`, and the old → new map.
fn renumber(doc: &mut Presentation, shapes: &mut [deckcraft_model::Shape]) -> HashMap<ShapeId, ShapeId> {
    let old = shape_ids(shapes);
    super::edit::reid(doc, shapes);
    let map: HashMap<ShapeId, ShapeId> = old.into_iter().zip(shape_ids(shapes)).collect();
    remap_connectors(shapes, &map, 0);
    map
}

/// The destination layout a reused slide moves to: same name, else same kind, else the first.
fn matching_layout(doc: &Presentation, name: &str, kind: deckcraft_model::LayoutType) -> LayoutId {
    let all = || doc.masters.iter().flat_map(|m| m.layouts.iter());
    all()
        .find(|l| !name.is_empty() && l.name.eq_ignore_ascii_case(name))
        .or_else(|| all().find(|l| l.kind == kind))
        .or_else(|| all().next())
        .map(|l| l.id)
        .unwrap_or_default()
}

fn reuse(s: &mut Session, p: &Value) -> Result<Value> {
    let (name, bytes) = super::insert::media_bytes(p, "slide.reuse")?;
    let src = super::file::open_presentation(&name, &bytes)?;
    let n = src.slides.len();
    let picked: Vec<usize> = match p.get("slides") {
        Some(Value::Array(a)) => {
            let mut v = vec![];
            for x in a {
                let i = x.as_u64().map(|i| i as usize).filter(|i| *i < n);
                let i = i.ok_or_else(|| bad("slide.reuse", format!("{name} has {n} slides; {x} isn't one of them (indices count from 0)")))?;
                if !v.contains(&i) {
                    v.push(i);
                }
            }
            v
        }
        Some(Value::String(t)) if !t.trim().is_empty() => super::file::parse_slide_range(t, n)?,
        _ => (0..n).collect(),
    };
    if picked.is_empty() {
        return Err(bad("slide.reuse", format!("{name} has no slides to reuse")));
    }
    let keep = bool_or(p, "keepFormatting", false);
    let at = usize_param(p, "at");
    s.edit(|doc, sel| {
        let at = at.unwrap_or(sel.slide + 1).min(doc.slides.len());
        // Media: what the slides use is copied under fresh ids.
        let mut media_map: HashMap<u32, u32> = HashMap::new();
        // Keep Source Formatting: bring each source master used, once, with fresh ids.
        let mut layout_map: HashMap<LayoutId, LayoutId> = HashMap::new();
        if keep {
            let mut masters_done: Vec<MasterId> = vec![];
            for &i in &picked {
                let Some((m, _)) = src.master_for(&src.slides[i]) else { continue };
                if masters_done.contains(&m.id) {
                    continue;
                }
                masters_done.push(m.id);
                let used = serde_json::to_value(m).map_err(|e| EngineError::Other(e.to_string()))?;
                copy_media(doc, &src, &used, &mut media_map);
                let mut m: deckcraft_model::Master = with_media(m, &media_map)?;
                m.id = MasterId(doc.alloc_id());
                renumber(doc, &mut m.shapes);
                for l in &mut m.layouts {
                    let new = LayoutId(doc.alloc_id());
                    layout_map.insert(l.id, new);
                    l.id = new;
                    renumber(doc, &mut l.shapes);
                }
                doc.masters.push(Arc::new(m));
            }
        }
        let mut new_ids = vec![];
        for (k, &i) in picked.iter().enumerate() {
            let from = &src.slides[i];
            let used = serde_json::to_value(from.as_ref()).map_err(|e| EngineError::Other(e.to_string()))?;
            copy_media(doc, &src, &used, &mut media_map);
            let mut sl: Slide = with_media(from.as_ref(), &media_map)?;
            sl.id = SlideId(doc.alloc_id());
            let map = renumber(doc, &mut sl.shapes);
            for a in &mut sl.animations {
                a.shape = map.get(&a.shape).copied().unwrap_or(a.shape);
                a.trigger = a.trigger.map(|t| map.get(&t).copied().unwrap_or(t));
            }
            sl.layout = match layout_map.get(&sl.layout) {
                Some(l) => *l,
                None => {
                    let (lname, kind) =
                        src.layout(from.layout).map(|(_, l)| (l.name.clone(), l.kind)).unwrap_or_else(|| (String::new(), Default::default()));
                    matching_layout(doc, &lname, kind)
                }
            };
            new_ids.push(sl.id);
            doc.slides.insert((at + k).min(doc.slides.len()), Arc::new(sl));
        }
        sel.slide = (at + new_ids.len()).saturating_sub(1).min(doc.slides.len().saturating_sub(1));
        sel.slides = new_ids.clone();
        sel.shapes.clear();
        sel.text = None;
        Ok(json!({"slides": new_ids.len(), "ids": new_ids, "from": name, "keepFormatting": keep}))
    })
}

/// Copy the source media `used` refers to into `doc` under fresh ids, once each.
fn copy_media(doc: &mut Presentation, src: &Presentation, used: &Value, map: &mut HashMap<u32, u32>) {
    let mut ids = vec![];
    collect_media(used, &mut ids, 0);
    for id in ids {
        if map.contains_key(&id) {
            continue;
        }
        if let Some(m) = src.media(MediaId(id)) {
            let new = doc.add_media(&m.name, &m.content_type, m.data.as_ref().clone());
            map.insert(id, new.0);
        }
    }
}

/// The media ids `v` refers to.
fn collect_media(v: &Value, out: &mut Vec<u32>, depth: usize) {
    if depth > 256 {
        return;
    }
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                if MEDIA_KEYS.contains(&k.as_str())
                    && let Some(n) = x.as_u64().and_then(|n| u32::try_from(n).ok())
                {
                    out.push(n);
                } else {
                    collect_media(x, out, depth + 1);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_media(x, out, depth + 1)),
        _ => {}
    }
}
