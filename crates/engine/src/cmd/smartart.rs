//! SmartArt: a diagram's outline (the Text Pane) and the shapes generated from it.
//!
//! A SmartArt graphic is a group whose `diagram` holds its layout and its nodes (text + outline
//! level). Every edit changes the nodes and regenerates the group's children inside the group's
//! child rectangle, so the diagram keeps its size and place. Text typed straight into a diagram
//! shape is read back into the nodes before each regeneration.

use deckcraft_color::SchemeSlot;
use deckcraft_geom::Xfrm;
use deckcraft_model::text::{Align, AutoFit, Paragraph, TextBody};
use deckcraft_model::{Diagram, DiagramNode, Fill, Geom, Presentation, Shape, ShapeId, ShapeKind, ShapeStyle};
use serde_json::{Value, json};

use super::*;
use crate::{EngineError, Result, Session};

/// Deepest outline level the Text Pane allows.
pub const MAX_LEVEL: u8 = 8;
/// Most top-level shapes a layout draws.
const MAX_SHAPES: usize = 40;

pub const KINDS: [&str; 6] = ["list", "process", "cycle", "hierarchy", "pyramid", "matrix"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "smartArt.nodes",
            "Text Pane",
            ["SmartArt Design", "Create Graphic"],
            None,
            "{id?} → {id, kind, nodes: [{text, level}]} (id: the SmartArt group or one of its shapes; default the selection)",
            has_selection,
            nodes
        ),
        cmd!(
            "smartArt.setNodes",
            "Edit SmartArt Text",
            ["SmartArt Design", "Create Graphic"],
            None,
            "{id?, nodes?: [{text, level}], lines?: [string] (leading tabs set the level), index?, text?} (index + text edits one node)",
            has_selection,
            set_nodes
        ),
        cmd!(
            "smartArt.addNode",
            "Add Shape",
            ["SmartArt Design", "Create Graphic"],
            None,
            "{id?, index?, position?: after|before|below|above, text?} → {index} (default: after the last node)",
            has_selection,
            add_node
        ),
        cmd!(
            "smartArt.removeNode",
            "Remove Shape",
            ["SmartArt Design", "Create Graphic"],
            None,
            "{id?, index} (its sub-points move up one level)",
            has_selection,
            remove_node
        ),
        cmd!("smartArt.promote", "Promote", ["SmartArt Design", "Create Graphic"], None, "{id?, index}", has_selection, promote),
        cmd!("smartArt.demote", "Demote", ["SmartArt Design", "Create Graphic"], None, "{id?, index}", has_selection, demote),
        cmd!(
            "smartArt.layout",
            "Change Layout",
            ["SmartArt Design", "Layouts"],
            None,
            "{id?, kind: list|process|cycle|hierarchy|pyramid|matrix}",
            has_selection,
            change_layout
        ),
    ]
}

/// Nodes from text lines: each leading tab is one outline level.
pub fn nodes_from_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<DiagramNode> {
    lines
        .into_iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let tabs = l.chars().take_while(|c| *c == '\t').count();
            DiagramNode { text: l.trim().to_string(), level: u8::try_from(tabs).unwrap_or(MAX_LEVEL).min(MAX_LEVEL) }
        })
        .collect()
}

/// Levels made consistent: the first node is top level and no node is more than one level
/// deeper than the node before it.
pub fn normalize(nodes: &mut [DiagramNode]) {
    let mut prev: Option<u8> = None;
    for n in nodes.iter_mut() {
        n.level = match prev {
            None => 0,
            Some(p) => n.level.min(p.saturating_add(1)).min(MAX_LEVEL),
        };
        prev = Some(n.level);
    }
}

/// Index one past the last descendant of node `i`.
fn subtree_end(nodes: &[DiagramNode], i: usize) -> usize {
    let lv = nodes.get(i).map(|n| n.level).unwrap_or(0);
    let mut j = i + 1;
    while nodes.get(j).is_some_and(|n| n.level > lv) {
        j += 1;
    }
    j
}

/// Add a node next to node `index` (PowerPoint's Add Shape After/Before/Below/Above). Returns the
/// new node's index.
pub fn add(nodes: &mut Vec<DiagramNode>, index: Option<usize>, position: &str, text: &str) -> usize {
    let Some(i) = index.filter(|i| *i < nodes.len()) else {
        nodes.push(DiagramNode { text: text.into(), level: 0 });
        return nodes.len() - 1;
    };
    let lv = nodes[i].level;
    let (at, level) = match position {
        "before" => (i, lv),
        "below" => (subtree_end(nodes, i), (lv + 1).min(MAX_LEVEL)),
        "above" => {
            let end = subtree_end(nodes, i);
            for n in &mut nodes[i..end] {
                n.level = (n.level + 1).min(MAX_LEVEL);
            }
            (i, lv)
        }
        _ => (subtree_end(nodes, i), lv),
    };
    nodes.insert(at, DiagramNode { text: text.into(), level });
    at
}

/// Remove node `i`; its sub-points move up one level.
pub fn remove(nodes: &mut Vec<DiagramNode>, i: usize) -> bool {
    if i >= nodes.len() {
        return false;
    }
    let end = subtree_end(nodes, i);
    for n in &mut nodes[i + 1..end] {
        n.level = n.level.saturating_sub(1);
    }
    nodes.remove(i);
    normalize(nodes);
    true
}

/// Move node `i` and its sub-points one level up. False when it is already top level.
pub fn promote_node(nodes: &mut [DiagramNode], i: usize) -> bool {
    if nodes.get(i).is_none_or(|n| n.level == 0) {
        return false;
    }
    let end = subtree_end(nodes, i);
    for n in &mut nodes[i..end] {
        n.level -= 1;
    }
    true
}

/// Move node `i` and its sub-points one level down, under the node before it. False when there is
/// nothing to go under.
pub fn demote_node(nodes: &mut [DiagramNode], i: usize) -> bool {
    let (Some(prev), Some(cur)) = (i.checked_sub(1).and_then(|p| nodes.get(p)), nodes.get(i)) else { return false };
    if cur.level > prev.level || cur.level >= MAX_LEVEL {
        return false;
    }
    let end = subtree_end(nodes, i);
    if nodes[i..end].iter().any(|n| n.level >= MAX_LEVEL) {
        return false;
    }
    for n in &mut nodes[i..end] {
        n.level += 1;
    }
    true
}

/// Which nodes each generated text shape shows, in child order (lines drawn after these carry none).
fn node_map(d: &Diagram) -> Vec<Vec<usize>> {
    if d.kind == "hierarchy" {
        return (0..d.nodes.len().min(MAX_SHAPES)).map(|i| vec![i]).collect();
    }
    let mut out: Vec<Vec<usize>> = vec![];
    for (i, n) in d.nodes.iter().enumerate() {
        match out.last_mut() {
            Some(g) if n.level > 0 => g.push(i),
            _ => out.push(vec![i]),
        }
    }
    let cap = if d.kind == "matrix" { 4 } else { MAX_SHAPES };
    out.truncate(cap);
    out
}

/// Read text typed into the diagram's shapes back into its nodes.
fn sync_text(d: &mut Diagram, children: &[Shape]) {
    for (shape, idx) in children.iter().zip(node_map(d)) {
        let Some(body) = &shape.text else { continue };
        if body.paragraphs.len() != idx.len() {
            continue;
        }
        for (p, i) in body.paragraphs.iter().zip(idx) {
            if let Some(n) = d.nodes.get_mut(i) {
                n.text = p.text();
            }
        }
    }
}

fn slot(i: usize) -> SchemeSlot {
    [SchemeSlot::Accent1, SchemeSlot::Accent2, SchemeSlot::Accent3, SchemeSlot::Accent4, SchemeSlot::Accent5, SchemeSlot::Accent6][i % 6]
}

fn text_shape(preset: &str, x: Xfrm, color: usize, lines: &[&DiagramNode]) -> Shape {
    let mut body = TextBody::default();
    body.body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
    let top = lines.first().map(|n| n.level).unwrap_or(0);
    for (k, n) in lines.iter().enumerate() {
        let mut para = Paragraph::new(&n.text);
        if k == 0 {
            para.props.align = Some(Align::Center);
        } else {
            para.level = n.level.saturating_sub(top).saturating_sub(1).min(MAX_LEVEL);
            para.props.align = Some(Align::Left);
            para.props.bullet = Some(deckcraft_model::text::Bullet::Char { char: "•".into() });
            para.props.margin_left = Some(12.0 * (para.level as f64 + 1.0));
            para.props.indent = Some(-10.0);
        }
        let size = if k == 0 { 20.0 } else { 14.0 };
        for r in &mut para.runs {
            r.props.size = Some(size);
        }
        para.end_props.size = Some(size);
        body.paragraphs.push(para);
    }
    if body.paragraphs.is_empty() {
        body.paragraphs.push(Paragraph::new(""));
    }
    Shape {
        name: format!("SmartArt shape {}", color + 1),
        xfrm: Some(x),
        geom: Geom::preset(preset),
        style: Some(ShapeStyle::accent(slot(color))),
        text: Some(body),
        ..Default::default()
    }
}

fn line_shape(a: (f64, f64), b: (f64, f64)) -> Shape {
    let mut x = Xfrm::new(a.0.min(b.0), a.1.min(b.1), (b.0 - a.0).abs(), (b.1 - a.1).abs());
    x.flip_h = (b.0 < a.0) != (b.1 < a.1);
    Shape {
        name: "SmartArt connector".into(),
        xfrm: Some(x),
        geom: Geom::preset("line"),
        style: Some(ShapeStyle::accent(SchemeSlot::Accent1)),
        fill: Some(Fill::None),
        ..Default::default()
    }
}

/// The diagram's shapes laid out in `area` (ids not yet assigned).
pub fn layout(d: &Diagram, area: Xfrm) -> Vec<Shape> {
    let map = node_map(d);
    let groups: Vec<Vec<&DiagramNode>> = map.iter().map(|g| g.iter().filter_map(|i| d.nodes.get(*i)).collect()).collect();
    let n = groups.len().max(1);
    let mut out = vec![];
    match d.kind.as_str() {
        "hierarchy" => return hierarchy(d, area),
        "cycle" => {
            let r = area.h.min(area.w) * 0.38;
            let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
            let dd = r * 0.62;
            for (i, g) in groups.iter().enumerate() {
                let a = (i as f64 / n as f64 * 360.0 - 90.0).to_radians();
                out.push(text_shape("ellipse", Xfrm::new(cx + r * a.cos() - dd / 2.0, cy + r * a.sin() - dd / 2.0, dd, dd), i, g));
            }
        }
        "pyramid" => {
            let h = area.h / n as f64;
            for (i, g) in groups.iter().enumerate() {
                let w = area.w * 0.25 + area.w * 0.75 * (i as f64 + 1.0) / n as f64;
                out.push(text_shape("trapezoid", Xfrm::new(area.x + (area.w - w) / 2.0, area.y + i as f64 * h, w, h * 0.95), i, g));
            }
        }
        "matrix" => {
            let (w, h) = (area.w / 2.0, area.h / 2.0);
            for (i, g) in groups.iter().enumerate() {
                let x = Xfrm::new(area.x + (i % 2) as f64 * w + 4.0, area.y + (i / 2) as f64 * h + 4.0, w - 8.0, h - 8.0);
                out.push(text_shape("roundRect", x, i, g));
            }
        }
        "list" => {
            let h = area.h / n as f64;
            for (i, g) in groups.iter().enumerate() {
                out.push(text_shape("roundRect", Xfrm::new(area.x, area.y + i as f64 * h, area.w, h * 0.85), i, g));
            }
        }
        _ => {
            let gap = 24.0;
            let w = ((area.w - gap * (n as f64 - 1.0)) / n as f64).max(1.0);
            for (i, g) in groups.iter().enumerate() {
                let preset = if i + 1 < n { "homePlate" } else { "roundRect" };
                out.push(text_shape(preset, Xfrm::new(area.x + i as f64 * (w + gap), area.y + area.h * 0.25, w, area.h * 0.5), i, g));
            }
        }
    }
    out
}

/// An organisation chart: one box per node, rows by level, each subtree as wide as its leaves.
fn hierarchy(d: &Diagram, area: Xfrm) -> Vec<Shape> {
    let nodes = &d.nodes[..d.nodes.len().min(MAX_SHAPES)];
    if nodes.is_empty() {
        return vec![];
    }
    let mut parent: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut stack: Vec<usize> = vec![];
    for (i, n) in nodes.iter().enumerate() {
        while stack.last().is_some_and(|&p| nodes[p].level >= n.level) {
            stack.pop();
        }
        parent[i] = stack.last().copied();
        stack.push(i);
    }
    let kids = |i: Option<usize>| -> Vec<usize> { (0..nodes.len()).filter(|&k| parent[k] == i).collect() };
    let mut leaves = vec![0usize; nodes.len()];
    for i in (0..nodes.len()).rev() {
        let k = kids(Some(i));
        leaves[i] = if k.is_empty() { 1 } else { k.iter().map(|&c| leaves[c]).sum() };
    }
    let depth: Vec<usize> = (0..nodes.len())
        .map(|i| {
            let (mut d, mut p) = (0, parent[i]);
            while let Some(q) = p {
                d += 1;
                p = parent[q];
            }
            d
        })
        .collect();
    let rows = depth.iter().max().copied().unwrap_or(0) + 1;
    let total: usize = kids(None).iter().map(|&r| leaves[r]).sum::<usize>().max(1);
    let cell_w = area.w / total as f64;
    let row_h = area.h / rows as f64;
    let (bw, bh) = (cell_w * 0.85, row_h * 0.6);
    // Left edge (in leaves) of each node's span.
    let mut left = vec![0usize; nodes.len()];
    let mut next_root = 0;
    for i in 0..nodes.len() {
        let mut start = match parent[i] {
            None => {
                let s = next_root;
                next_root += leaves[i];
                s
            }
            Some(p) => left[p],
        };
        if let Some(p) = parent[i] {
            for s in kids(Some(p)) {
                if s == i {
                    break;
                }
                start += leaves[s];
            }
        }
        left[i] = start;
    }
    let boxes: Vec<Xfrm> = (0..nodes.len())
        .map(|i| {
            let cx = area.x + (left[i] as f64 + leaves[i] as f64 / 2.0) * cell_w;
            Xfrm::new(cx - bw / 2.0, area.y + depth[i] as f64 * row_h + (row_h - bh) / 2.0, bw, bh)
        })
        .collect();
    let mut out: Vec<Shape> = nodes.iter().enumerate().map(|(i, n)| text_shape("roundRect", boxes[i], depth[i], &[n])).collect();
    for i in 0..nodes.len() {
        if let Some(p) = parent[i] {
            let (a, b) = (boxes[p], boxes[i]);
            out.push(line_shape((a.x + a.w / 2.0, a.y + a.h), (b.x + b.w / 2.0, b.y)));
        }
    }
    out
}

/// The SmartArt group that `id` is, or is inside.
fn diagram_group(shapes: &[Shape], id: ShapeId) -> Option<&Shape> {
    for sh in shapes {
        if sh.id == id && sh.diagram.is_some() {
            return Some(sh);
        }
        if let ShapeKind::Group { children, .. } = &sh.kind {
            if sh.diagram.is_some() && deckcraft_model::find_shape(children, id).is_some() {
                return Some(sh);
            }
            if let Some(g) = diagram_group(children, id) {
                return Some(g);
            }
        }
    }
    None
}

/// The SmartArt graphic the selection is (or is inside), with its outline as the Text Pane shows it.
pub fn selected(st: &crate::DocState) -> Option<(ShapeId, Diagram)> {
    let shapes = current_shapes(&st.doc, &st.selection)?;
    let g = st.selection.shapes.iter().find_map(|id| diagram_group(shapes, *id))?;
    let mut d = g.diagram.clone()?;
    if let ShapeKind::Group { children, .. } = &g.kind {
        sync_text(&mut d, children);
    }
    Some((g.id, d))
}

fn target_group(s: &Session, p: &Value, cmd: &str) -> Result<ShapeId> {
    let st = s.doc()?;
    let shapes = current_shapes(&st.doc, &st.selection).ok_or_else(|| bad(cmd, "no slide"))?;
    let ids = targets(s, p)?;
    ids.iter().find_map(|id| diagram_group(shapes, *id)).map(|g| g.id).ok_or_else(|| bad(cmd, "select a SmartArt graphic"))
}

/// Children with fresh ids for diagram `d` inside a group whose child space is `area`.
fn regenerate(doc: &mut Presentation, d: &Diagram, area: Xfrm) -> Vec<Shape> {
    let mut v = layout(d, area);
    for sh in &mut v {
        sh.id = ShapeId(doc.alloc_id());
    }
    v
}

/// Read the diagram (with typed text synced), let `f` change it, and regenerate the group.
fn edit_diagram<T>(s: &mut Session, p: &Value, cmd: &str, f: impl FnOnce(&mut Diagram) -> Result<T>) -> Result<T> {
    let gid = target_group(s, p, cmd)?;
    s.edit(|doc, sel| {
        let g = current_shapes(doc, sel).and_then(|v| deckcraft_model::find_shape(v, gid)).cloned().ok_or_else(|| bad(cmd, "no such shape"))?;
        let (ShapeKind::Group { children, child }, Some(mut d)) = (g.kind, g.diagram) else { return Err(bad(cmd, "not a SmartArt graphic")) };
        sync_text(&mut d, &children);
        let out = f(&mut d)?;
        normalize(&mut d.nodes);
        let kids = regenerate(doc, &d, child);
        let shapes = crate::shapes_mut(doc, sel).ok_or_else(|| bad(cmd, "no slide"))?;
        let sh = deckcraft_model::find_shape_mut(shapes, gid).ok_or_else(|| bad(cmd, "no such shape"))?;
        sh.kind = ShapeKind::Group { children: kids, child };
        sh.diagram = Some(d);
        sel.shapes = vec![gid];
        sel.text = None;
        Ok(out)
    })
}

fn index(p: &Value, cmd: &str) -> Result<usize> {
    p.get("index").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).ok_or_else(|| bad(cmd, "index is required"))
}

/// Insert a new SmartArt graphic (`insert.smartArt`).
pub fn insert(s: &mut Session, kind: &str, mut nodes: Vec<DiagramNode>) -> Result<Value> {
    let kind = if KINDS.contains(&kind) { kind } else { "process" };
    if nodes.is_empty() {
        nodes = (0..3).map(|_| DiagramNode { text: "Text".into(), level: 0 }).collect();
    }
    normalize(&mut nodes);
    let d = Diagram { kind: kind.into(), nodes };
    let size = s.doc()?.doc.slide_size;
    let area = Xfrm::new(size.width * 0.1, size.height * 0.28, size.width * 0.8, size.height * 0.6);
    let id = s.edit(|doc, sel| {
        let children = regenerate(doc, &d, area);
        let id = ShapeId(doc.alloc_id());
        let g = Shape {
            id,
            name: format!("SmartArt ({kind})"),
            xfrm: Some(area),
            kind: ShapeKind::Group { children, child: area },
            diagram: Some(d),
            ..Default::default()
        };
        crate::shapes_mut(doc, sel).ok_or_else(|| EngineError::Other("no slide".into()))?.push(g);
        sel.shapes = vec![id];
        sel.text = None;
        Ok(id)
    })?;
    Ok(json!({"id": id.0}))
}

fn nodes(s: &mut Session, p: &Value) -> Result<Value> {
    let gid = target_group(s, p, "smartArt.nodes")?;
    let g = shape_of(s, gid)?;
    let mut d = g.diagram.unwrap_or_default();
    if let ShapeKind::Group { children, .. } = &g.kind {
        sync_text(&mut d, children);
    }
    Ok(json!({"id": gid.0, "kind": d.kind, "nodes": d.nodes}))
}

fn set_nodes(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.setNodes";
    let given: Option<Vec<DiagramNode>> = match (p.get("nodes"), p.get("lines").and_then(Value::as_array)) {
        (Some(v), _) => Some(serde_json::from_value(v.clone()).map_err(|e| bad(CMD, e.to_string()))?),
        (None, Some(a)) => Some(nodes_from_lines(a.iter().filter_map(Value::as_str))),
        _ => None,
    };
    let one = p.get("index").map(|_| index(p, CMD)).transpose()?;
    let text = str_param(p, "text").map(String::from);
    edit_diagram(s, p, CMD, |d| {
        if let Some(n) = given {
            d.nodes = n;
        }
        if let (Some(i), Some(t)) = (one, text) {
            d.nodes.get_mut(i).ok_or_else(|| bad(CMD, "no such node"))?.text = t;
        }
        Ok(Value::Null)
    })
}

fn add_node(s: &mut Session, p: &Value) -> Result<Value> {
    let i = p.get("index").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok());
    let pos = str_param(p, "position").unwrap_or("after").to_string();
    let text = str_param(p, "text").unwrap_or("").to_string();
    edit_diagram(s, p, "smartArt.addNode", |d| {
        let at = add(&mut d.nodes, i, &pos, &text);
        Ok(json!({"index": at}))
    })
}

fn remove_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.removeNode";
    let i = index(p, CMD)?;
    edit_diagram(s, p, CMD, |d| if remove(&mut d.nodes, i) { Ok(Value::Null) } else { Err(bad(CMD, "no such node")) })
}

fn promote(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.promote";
    let i = index(p, CMD)?;
    edit_diagram(s, p, CMD, |d| if promote_node(&mut d.nodes, i) { Ok(Value::Null) } else { Err(bad(CMD, "already at the top level")) })
}

fn demote(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.demote";
    let i = index(p, CMD)?;
    edit_diagram(s, p, CMD, |d| if demote_node(&mut d.nodes, i) { Ok(Value::Null) } else { Err(bad(CMD, "nothing above to demote under")) })
}

fn change_layout(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.layout";
    let kind = str_param(p, "kind").filter(|k| KINDS.contains(k)).ok_or_else(|| bad(CMD, "unknown layout"))?.to_string();
    edit_diagram(s, p, CMD, |d| {
        d.kind = kind;
        Ok(Value::Null)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(n: &[DiagramNode]) -> Vec<u8> {
        n.iter().map(|x| x.level).collect()
    }
    fn mk(levels: &[u8]) -> Vec<DiagramNode> {
        levels.iter().enumerate().map(|(i, l)| DiagramNode { text: format!("n{i}"), level: *l }).collect()
    }

    #[test]
    fn outline_edits() {
        let mut n = mk(&[0, 1, 1, 0]);
        assert_eq!(add(&mut n, Some(0), "after", "x"), 3, "after the node's sub-points");
        assert_eq!(lv(&n), [0, 1, 1, 0, 0]);
        assert_eq!(add(&mut n, Some(0), "below", "y"), 3);
        assert_eq!(lv(&n), [0, 1, 1, 1, 0, 0]);
        assert_eq!(add(&mut n, Some(0), "above", "z"), 0);
        assert_eq!(lv(&n), [0, 1, 2, 2, 2, 0, 0]);
        assert!(remove(&mut n, 1));
        assert_eq!(lv(&n), [0, 1, 1, 1, 0, 0]);
        assert!(!promote_node(&mut n, 0));
        assert!(!demote_node(&mut n, 0), "nothing above the first node");
        assert!(!demote_node(&mut n, 2) || lv(&n)[2] == 2);
        let mut m = mk(&[0, 0, 1]);
        assert!(demote_node(&mut m, 1));
        assert_eq!(lv(&m), [0, 1, 2], "sub-points move with their node");
        assert!(promote_node(&mut m, 1));
        assert_eq!(lv(&m), [0, 0, 1]);
        let mut bad = mk(&[2, 4, 0]);
        normalize(&mut bad);
        assert_eq!(lv(&bad), [0, 1, 0]);
        assert_eq!(lv(&nodes_from_lines(["A", "\tB", "\t\tC", "", "D"])), [0, 1, 2, 0]);
    }

    #[test]
    fn layouts_map_nodes_to_shapes() {
        let area = Xfrm::new(0.0, 0.0, 800.0, 400.0);
        let d = Diagram { kind: "list".into(), nodes: mk(&[0, 1, 1, 0]) };
        let v = layout(&d, area);
        assert_eq!(v.len(), 2);
        let t = v[0].text.as_ref().map(|b| b.paragraphs.len());
        assert_eq!(t, Some(3), "sub-points are bullets in their parent's shape");
        let h = Diagram { kind: "hierarchy".into(), nodes: mk(&[0, 1, 2, 1]) };
        let v = layout(&h, area);
        assert_eq!(v.len(), 4 + 3, "a box per node and a line per parent link");
        for sh in &v {
            let x = sh.xfrm.unwrap_or_default();
            assert!(x.x >= -0.01 && x.x + x.w <= 800.01 && x.y >= -0.01 && x.y + x.h <= 400.01, "{x:?}");
        }
        assert!(layout(&Diagram { kind: "cycle".into(), nodes: vec![] }, area).is_empty());
    }
}
