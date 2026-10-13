//! Review tab: comments, spelling, accessibility; Slide Show requests.

use std::sync::Arc;

use deckcraft_model::Comment;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session, UiRequest};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("comment.add", "New Comment", ["Review", "Comments"], Some("Cmd+Alt+M"), "{text, x?, y?, id?: shape}", has_slide, add),
        cmd!("comment.reply", "Reply", ["Comments"], None, "{index, text}", has_slide, reply),
        cmd!("comment.delete", "Delete Comment", ["Review", "Comments"], None, "{index? | all?: bool}", has_slide, delete),
        cmd!("comment.resolve", "Resolve Thread", ["Comments"], None, "{index, resolved?: bool}", has_slide, resolve),
        cmd!(query "comment.list", "Comments", [], None, "{slide?} → comments", has_slide, list),
        cmd!(query "review.accessibility", "Check Accessibility", ["Review", "Accessibility"], None, "{} → [{slide, shape, issue}]", has_doc, accessibility),
        cmd!(noundo "review.compare", "Compare", ["Review", "Compare"], None, "{path} | {name, data: base64} — compare with a revised copy → {name, changes}", has_doc, compare),
        cmd!(query "review.revisions", "Revisions", [], None, "{} → {name, changes: [{kind: inserted|deleted|changed, slide, revised, details}]}", has_doc, revisions),
        cmd!("review.accept", "Accept Revision", ["Review", "Compare"], None, "{index} | {all: true}", comparing, accept),
        cmd!(noundo "review.endCompare", "End Review", ["Review", "Compare"], None, "{}", comparing, end_compare),
        cmd!(query "review.spelling", "Spelling", ["Review", "Proofing"], Some("F7"), "{} → [{slide, shape, word}] (words not in the built-in list)", has_doc, spelling),
        cmd!(noundo "show.fromStart", "Play from Start", ["Slide Show", "Start Slide Show"], Some("Cmd+Shift+Enter"), "{}", has_slide, |s, _| start(s, 0)),
        cmd!(noundo "show.fromCurrent", "Play from Current Slide", ["Slide Show", "Start Slide Show"], Some("Cmd+Enter"), "{}", has_slide, |s, _| {
            let i = s.doc()?.selection.slide;
            start(s, i)
        }),
        cmd!(
            "show.setup",
            "Set Up Slide Show…",
            ["Slide Show", "Set Up"],
            None,
            "{type?: speaker|browsed|kiosk, loop?: bool, noNarration?, noAnimation?, useTimings?, from?, to?}",
            has_doc,
            setup
        ),
        cmd!(
            "show.customShow",
            "Custom Slide Show",
            ["Slide Show", "Start Slide Show"],
            None,
            "{name, slides: [index]} | {name, delete: true}",
            has_doc,
            custom_show
        ),
    ]
}

fn now() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (y, m, d) = super::design::civil((secs / 86_400) as i64);
        let t = secs % 86_400;
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
    }
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let text = str_param(p, "text").ok_or_else(|| bad("comment.add", "missing `text`"))?.to_string();
    let author = s.prefs.author.clone();
    let shape = id_param(p, "id").or_else(|| s.active().and_then(|d| d.selection.shapes.first().copied()));
    let (x, y) = (f64_or(p, "x", 10.0), f64_or(p, "y", 10.0));
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.add", "no slide"))?);
        let initials: String = author.split_whitespace().filter_map(|w| w.chars().next()).collect();
        sl.comments.push(Comment { author: author.clone(), initials, text, date: now(), x, y, resolved: false, replies: vec![], shape });
        Ok(json!({"index": sl.comments.len() - 1}))
    })
}

fn reply(s: &mut Session, p: &Value) -> Result<Value> {
    let i = usize_param(p, "index").ok_or_else(|| bad("comment.reply", "missing `index`"))?;
    let text = str_param(p, "text").unwrap_or("").to_string();
    let author = s.prefs.author.clone();
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.reply", "no slide"))?);
        let c = sl.comments.get_mut(i).ok_or_else(|| bad("comment.reply", "no such comment"))?;
        c.replies.push(Comment { author, text, date: now(), ..Default::default() });
        Ok(())
    })?;
    ok()
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let all = bool_or(p, "all", false);
    let i = usize_param(p, "index");
    s.edit(|doc, sel| {
        if all {
            for sl in &mut doc.slides {
                Arc::make_mut(sl).comments.clear();
            }
            return Ok(());
        }
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.delete", "no slide"))?);
        match i {
            Some(i) if i < sl.comments.len() => {
                sl.comments.remove(i);
            }
            None => sl.comments.clear(),
            _ => return Err(bad("comment.delete", "no such comment")),
        }
        Ok(())
    })?;
    ok()
}

fn resolve(s: &mut Session, p: &Value) -> Result<Value> {
    let i = usize_param(p, "index").ok_or_else(|| bad("comment.resolve", "missing `index`"))?;
    let v = bool_or(p, "resolved", true);
    s.edit(|doc, sel| {
        let sl = Arc::make_mut(doc.slides.get_mut(sel.slide).ok_or_else(|| bad("comment.resolve", "no slide"))?);
        sl.comments.get_mut(i).ok_or_else(|| bad("comment.resolve", "no such comment"))?.resolved = v;
        Ok(())
    })?;
    ok()
}

fn list(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let i = usize_param(p, "slide").unwrap_or(st.selection.slide);
    Ok(serde_json::to_value(st.doc.slides.get(i).map(|x| x.comments.clone()).unwrap_or_default()).unwrap_or_default())
}

fn accessibility(s: &mut Session, _p: &Value) -> Result<Value> {
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        if sl.title().trim().is_empty() {
            out.push(json!({"slide": i, "issue": "Missing slide title"}));
        }
        deckcraft_model::walk(&sl.shapes, &mut |sh, _| {
            let needs_alt = matches!(
                sh.kind,
                deckcraft_model::ShapeKind::Picture { .. }
                    | deckcraft_model::ShapeKind::Chart(_)
                    | deckcraft_model::ShapeKind::Media(_)
                    | deckcraft_model::ShapeKind::Group { .. }
            );
            if needs_alt && sh.descr.trim().is_empty() && !sh.decorative {
                out.push(json!({"slide": i, "shape": sh.id, "name": sh.name, "issue": "Missing alternative text"}));
            }
        });
        // Low-contrast text against the slide background.
        if let Some(ctx) = deckcraft_model::resolve::Ctx::for_slide(&st.doc, sl) {
            let bg = match deckcraft_model::resolve::background(&ctx, Some(sl)) {
                (deckcraft_model::Fill::Solid { color }, ph) => Some(ctx.color(&color, ph)),
                _ => None,
            };
            if let Some(bg) = bg {
                for sh in &sl.shapes {
                    let Some(t) = &sh.text else { continue };
                    let Some(para) = t.paragraphs.iter().find(|p| !p.is_empty()) else { continue };
                    let rp = deckcraft_model::resolve::run(&ctx, sh, para, &para.runs.first().map(|r| r.props.clone()).unwrap_or_default());
                    let (fill, _) = deckcraft_model::resolve::fill(&ctx, sh);
                    if fill.as_ref().is_some_and(|f| !f.is_none()) {
                        continue;
                    }
                    let fg = deckcraft_model::resolve::text_color(&ctx, &rp);
                    if fg.contrast(bg) < 3.0 {
                        out.push(json!({"slide": i, "shape": sh.id, "name": sh.name, "issue": format!("Hard-to-read text contrast ({:.1}:1)", fg.contrast(bg))}));
                    }
                }
            }
        }
    }
    Ok(Value::Array(out))
}

fn spelling(s: &mut Session, _p: &Value) -> Result<Value> {
    // A small built-in list of frequent misspellings (full dictionaries come later).
    const COMMON: &[&str] = &[
        "teh",
        "recieve",
        "seperate",
        "occured",
        "untill",
        "wich",
        "becuase",
        "definately",
        "thier",
        "alot",
        "accomodate",
        "acheive",
        "adress",
        "begining",
        "beleive",
        "calender",
        "commited",
        "enviroment",
        "goverment",
        "independant",
        "neccessary",
        "occassion",
        "publically",
        "reccomend",
        "tommorow",
        "wierd",
    ];
    let st = s.doc()?;
    let mut out = vec![];
    for (i, sl) in st.doc.slides.iter().enumerate() {
        deckcraft_model::walk(&sl.shapes, &mut |sh, _| {
            if let Some(t) = &sh.text {
                for w in t.text().split(|c: char| !c.is_alphanumeric() && c != '\'') {
                    if COMMON.contains(&w.to_lowercase().as_str()) {
                        out.push(json!({"slide": i, "shape": sh.id, "word": w}));
                    }
                }
            }
        });
    }
    Ok(Value::Array(out))
}

fn start(s: &mut Session, from: usize) -> Result<Value> {
    s.ui_requests.push(UiRequest::StartShow { from });
    Ok(json!({"from": from}))
}

fn setup(s: &mut Session, p: &Value) -> Result<Value> {
    s.edit(|doc, _| {
        let sh = &mut doc.show;
        if let Some(t) = str_param(p, "type") {
            sh.show_type = t.to_string();
            if t == "kiosk" {
                sh.loop_until_esc = true;
            }
        }
        if let Some(v) = bool_param(p, "loop") {
            sh.loop_until_esc = v;
        }
        if let Some(v) = bool_param(p, "noNarration") {
            sh.without_narration = v;
        }
        if let Some(v) = bool_param(p, "noAnimation") {
            sh.without_animation = v;
        }
        if let Some(v) = bool_param(p, "useTimings") {
            sh.use_timings = v;
        }
        match (p.get("from").and_then(Value::as_u64), p.get("to").and_then(Value::as_u64)) {
            (Some(a), Some(b)) => sh.range = Some((a.min(u32::MAX as u64) as u32, b.min(u32::MAX as u64) as u32)),
            (None, None) if p.get("all").is_some() => sh.range = None,
            _ => {}
        }
        Ok(serde_json::to_value(&*sh).unwrap_or_default())
    })
}

fn custom_show(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("show.customShow", "missing `name`"))?.to_string();
    let delete = bool_or(p, "delete", false);
    let idx: Vec<usize> =
        p.get("slides").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|v| v as usize).collect()).unwrap_or_default();
    s.edit(|doc, _| {
        doc.custom_shows.retain(|c| c.name != name);
        if !delete {
            let slides = idx.iter().filter_map(|i| doc.slides.get(*i).map(|x| x.id)).collect();
            doc.custom_shows.push(deckcraft_model::CustomShow { name: name.clone(), slides });
        }
        Ok(())
    })?;
    ok()
}

fn comparing(s: &Session) -> std::result::Result<(), String> {
    if s.active().is_some_and(|d| d.compare.is_some()) { Ok(()) } else { Err("not comparing".into()) }
}

fn revised_bytes(p: &Value) -> Result<(String, Vec<u8>)> {
    if let Some(data) = str_param(p, "data") {
        let bytes = base64_decode(data).ok_or_else(|| bad("review.compare", "invalid base64 `data`"))?;
        return Ok((str_param(p, "name").unwrap_or("Revised").to_string(), bytes));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = str_param(p, "path") {
        let bytes = std::fs::read(path).map_err(|e| crate::EngineError::Other(format!("{path}: {e}")))?;
        return Ok((path.to_string(), bytes));
    }
    Err(bad("review.compare", "give `path` or `data`"))
}

fn revisions_json(st: &crate::DocState) -> Result<Value> {
    let c = st.compare.as_ref().ok_or_else(|| bad("review.revisions", "not comparing"))?;
    let name = std::path::Path::new(&c.name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| c.name.clone());
    Ok(json!({"name": name, "changes": crate::compare::diff(&st.doc, &c.revised, c.by_position), "byPosition": c.by_position}))
}

fn compare(s: &mut Session, p: &Value) -> Result<Value> {
    let (name, bytes) = revised_bytes(p)?;
    let revised = super::file::open_presentation(&name, &bytes)?;
    let st = s.doc_mut()?;
    let by_position = crate::compare::unrelated(&st.doc, &revised);
    st.compare = Some(crate::compare::Compare { name, revised: Arc::new(revised), by_position });
    s.ui_requests.push(UiRequest::Pane { id: "revisions".into() });
    revisions_json(s.doc()?)
}

fn revisions(s: &mut Session, _: &Value) -> Result<Value> {
    revisions_json(s.doc()?)
}

fn accept(s: &mut Session, p: &Value) -> Result<Value> {
    let (rev, by_pos) = s.doc()?.compare.as_ref().map(|c| (c.revised.clone(), c.by_position)).ok_or_else(|| bad("review.accept", "not comparing"))?;
    let all = bool_param(p, "all").unwrap_or(false);
    let index = usize_param(p, "index");
    if !all && index.is_none() {
        return Err(bad("review.accept", "give `index` or `all`"));
    }
    s.edit(|doc, sel| {
        let mut accepted = 0usize;
        // Each accept shifts indices, so the list is recomputed; bounded by the slide counts.
        let limit = doc.slides.len().saturating_add(rev.slides.len()).saturating_add(1);
        for _ in 0..limit {
            let changes = crate::compare::diff(doc, &rev, by_pos);
            let c = if all {
                changes.into_iter().find(|c| !(c.kind == crate::compare::ChangeKind::Deleted && doc.slides.len() <= 1))
            } else {
                changes.into_iter().nth(index.unwrap_or(0))
            };
            let Some(c) = c else { break };
            let Some(at) = crate::compare::accept(doc, &rev, &c) else { break };
            accepted += 1;
            sel.slide = at;
            sel.shapes.clear();
            sel.text = None;
            if !all {
                break;
            }
        }
        if accepted == 0 {
            return Err(bad("review.accept", "no such revision"));
        }
        Ok(json!({"accepted": accepted}))
    })
}

fn end_compare(s: &mut Session, _: &Value) -> Result<Value> {
    s.doc_mut()?.compare = None;
    ok()
}
