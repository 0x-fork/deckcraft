//! SmartArt: a diagram's outline (the Text Pane) and the shapes generated from it.
//!
//! A SmartArt graphic is a group whose `diagram` holds its layout and its nodes (text + outline
//! level). Every edit changes the nodes and regenerates the group's children inside the group's
//! child rectangle, so the diagram keeps its size and place. Text typed straight into a diagram
//! shape is read back into the nodes before each regeneration.

use deckcraft_color::SchemeSlot;
use deckcraft_geom::Xfrm;
use deckcraft_model::style::{Gradient, GradientShape, GradientStop, Shadow};
use deckcraft_model::text::{Align, AutoFit, Paragraph, TextBody};
use deckcraft_model::{
    ColorRef, ColorTransform, Diagram, DiagramNode, Effects, Fill, Geom, Line, Presentation, Shape, ShapeId, ShapeKind, ShapeStyle,
};
use serde_json::{Value, json};

use super::*;
use crate::{EngineError, Result, Session};

/// Deepest outline level the Text Pane allows.
pub const MAX_LEVEL: u8 = 8;
/// Most top-level shapes a layout draws.
const MAX_SHAPES: usize = 40;

/// DeckCraft's layouts, with the names PowerPoint's gallery gives them.
pub const LAYOUTS: [(&str, &str); 15] = [
    ("list", "Vertical Bullet List"),
    ("blockList", "Basic Block List"),
    ("hBulletList", "Horizontal Bullet List"),
    ("process", "Basic Process"),
    ("chevron", "Basic Chevron Process"),
    ("verticalProcess", "Vertical Process"),
    ("stepUp", "Step Up Process"),
    ("cycle", "Basic Cycle"),
    ("blockCycle", "Block Cycle"),
    ("radial", "Basic Radial"),
    ("hierarchy", "Hierarchy"),
    ("pyramid", "Basic Pyramid"),
    ("invertedPyramid", "Inverted Pyramid"),
    ("matrix", "Basic Matrix"),
    ("titledMatrix", "Titled Matrix"),
];
/// SmartArt Styles.
pub const STYLES: [(&str, &str); 4] =
    [("flat", "Simple Fill"), ("subtle", "Subtle Effect"), ("intense", "Intense Effect"), ("outline", "White Outline")];
/// Change Colors.
pub const COLORS: [(&str, &str); 7] = [
    ("colorful", "Colorful"),
    ("accent1", "Accent 1"),
    ("accent2", "Accent 2"),
    ("accent3", "Accent 3"),
    ("accent4", "Accent 4"),
    ("accent5", "Accent 5"),
    ("accent6", "Accent 6"),
];

pub fn is_kind(k: &str) -> bool {
    LAYOUTS.iter().any(|(id, _)| *id == k)
}

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
            "{id?, kind: list|blockList|hBulletList|process|chevron|verticalProcess|stepUp|cycle|blockCycle|radial|hierarchy|pyramid|invertedPyramid|matrix|titledMatrix}",
            has_selection,
            change_layout
        ),
        cmd!(
            "smartArt.style",
            "SmartArt Styles",
            ["SmartArt Design", "SmartArt Styles"],
            None,
            "{id?, style: flat|subtle|intense|outline}",
            has_selection,
            change_style
        ),
        cmd!(
            "smartArt.colors",
            "Change Colors",
            ["SmartArt Design", "SmartArt Styles"],
            None,
            "{id?, colors: colorful|accent1|…|accent6}",
            has_selection,
            change_colors
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

/// A generated shape and the nodes whose text it shows (none for arrows and lines).
type Item = (Shape, Vec<usize>);

/// Top-level nodes, each with its sub-points.
fn groups(d: &Diagram) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = vec![];
    for (i, n) in d.nodes.iter().enumerate() {
        match out.last_mut() {
            Some(g) if n.level > 0 => g.push(i),
            _ => out.push(vec![i]),
        }
    }
    out.truncate(MAX_SHAPES);
    out
}

/// Read text typed into the diagram's shapes back into its nodes.
fn sync_text(d: &mut Diagram, children: &[Shape]) {
    if d.imported {
        return;
    }
    let map: Vec<Vec<usize>> = layout_items(d, Xfrm::new(0.0, 0.0, 100.0, 100.0)).into_iter().map(|(_, m)| m).collect();
    for (shape, idx) in children.iter().zip(map) {
        let Some(body) = &shape.text else { continue };
        if idx.is_empty() || body.paragraphs.len() != idx.len() {
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

/// The accent shape `i` is drawn in under Change Colors: `colorful` cycles the accents, `accentN`
/// keeps one.
fn accent(d: &Diagram, i: usize) -> SchemeSlot {
    match d.colors.strip_prefix("accent").and_then(|n| n.parse::<usize>().ok()) {
        Some(n @ 1..=6) => slot(n - 1),
        _ => slot(i),
    }
}

/// Fill, outline, text colour and effects for the SmartArt style. `light` shapes (the bullet
/// boxes of the bullet lists) are a pale tint of the accent with dark text.
fn paint(d: &Diagram, sh: &mut Shape, i: usize, light: bool) {
    let a = accent(d, i);
    let base = ColorRef::scheme(a);
    let tint = |pct: i32| ColorRef::scheme(a).with(ColorTransform::LumMod(100_000 - pct * 1000)).with(ColorTransform::LumOff(pct * 1000));
    let dark_text = light || d.style == "outline";
    sh.style = Some(ShapeStyle::accent(a));
    match d.style.as_str() {
        _ if light => {
            sh.fill = Some(Fill::solid(tint(80)));
            sh.line = Some(Line::solid(tint(60), 1.0));
        }
        "outline" => {
            sh.fill = Some(Fill::solid(ColorRef::scheme(SchemeSlot::Lt1)));
            sh.line = Some(Line::solid(base.clone(), 2.25));
        }
        "subtle" => {
            sh.fill = Some(Fill::solid(base.clone()));
            sh.line = Some(Line::solid(ColorRef::scheme(SchemeSlot::Lt1), 1.5));
            sh.effects = Some(Effects { outer_shadow: Some(shadow(4.0, 3.0)), ..Default::default() });
        }
        "intense" => {
            sh.fill = Some(Fill::Gradient(Gradient {
                stops: vec![
                    GradientStop { pos: 0.0, color: tint(30) },
                    GradientStop { pos: 1.0, color: base.clone().with(ColorTransform::LumMod(75_000)) },
                ],
                shape: GradientShape::Linear { angle: 90.0, scaled: false },
                rotate_with_shape: true,
                tile: [0.0; 4],
            }));
            sh.line = Some(Line::solid(ColorRef::scheme(SchemeSlot::Lt1), 1.0));
            sh.effects = Some(Effects { outer_shadow: Some(shadow(8.0, 5.0)), ..Default::default() });
        }
        _ => {
            sh.fill = Some(Fill::solid(base.clone()));
            sh.line = Some(Line::solid(ColorRef::scheme(SchemeSlot::Lt1), 1.5));
        }
    }
    if let Some(body) = &mut sh.text {
        let c = if dark_text { ColorRef::scheme(SchemeSlot::Tx1) } else { ColorRef::scheme(SchemeSlot::Lt1) };
        for p in &mut body.paragraphs {
            for r in &mut p.runs {
                r.props.fill = Some(Fill::solid(c.clone()));
            }
        }
    }
}

fn shadow(blur: f64, dist: f64) -> Shadow {
    Shadow {
        color: ColorRef::rgb(deckcraft_model::Rgba { r: 0, g: 0, b: 0, a: 255 }).with(ColorTransform::Alpha(40_000)),
        blur,
        dist,
        dir: 90.0,
        inner: false,
        sx: 1.0,
        sy: 1.0,
        kx: 0.0,
        ky: 0.0,
        align: String::new(),
        rotate_with_shape: false,
    }
}

/// A shape with text: the first line as the title (centred, larger), the rest as bullets. With
/// `title` false every line is a bullet (the bullet boxes of the bullet lists).
fn text_shape(preset: &str, x: Xfrm, lines: &[&DiagramNode], title: bool) -> Shape {
    let mut body = TextBody::default();
    body.body.autofit = Some(AutoFit::Shrink { font_scale: 1.0, line_reduction: 0.0 });
    let top = lines.first().map(|n| if title { n.level } else { n.level.saturating_sub(1) }).unwrap_or(0);
    for (k, n) in lines.iter().enumerate() {
        let mut para = Paragraph::new(&n.text);
        let head = title && k == 0;
        if head {
            para.props.align = Some(Align::Center);
        } else {
            para.level = n.level.saturating_sub(top).saturating_sub(1).min(MAX_LEVEL);
            para.props.align = Some(Align::Left);
            para.props.bullet = Some(deckcraft_model::text::Bullet::Char { char: "•".into() });
            para.props.margin_left = Some(12.0 * (para.level as f64 + 1.0));
            para.props.indent = Some(-10.0);
        }
        let size = if head { 20.0 } else { 14.0 };
        for r in &mut para.runs {
            r.props.size = Some(size);
        }
        para.end_props.size = Some(size);
        body.paragraphs.push(para);
    }
    if body.paragraphs.is_empty() {
        body.paragraphs.push(Paragraph::new(""));
    }
    if !title {
        body.body.anchor = Some(deckcraft_model::text::Anchor::Top);
    }
    Shape { name: "SmartArt shape".into(), xfrm: Some(x), geom: Geom::preset(preset), text: Some(body), ..Default::default() }
}

/// A plain shape with no text (arrows between steps).
fn deco(preset: &str, x: Xfrm, rot: f64) -> Shape {
    let mut x = x;
    x.rot = rot;
    Shape { name: "SmartArt arrow".into(), xfrm: Some(x), geom: Geom::preset(preset), ..Default::default() }
}

fn line_shape(a: (f64, f64), b: (f64, f64)) -> Shape {
    let mut x = Xfrm::new(a.0.min(b.0), a.1.min(b.1), (b.0 - a.0).abs(), (b.1 - a.1).abs());
    x.flip_h = (b.0 < a.0) != (b.1 < a.1);
    Shape {
        name: "SmartArt connector".into(),
        xfrm: Some(x),
        geom: Geom::preset("line"),
        fill: Some(Fill::None),
        line: Some(Line::solid(ColorRef::scheme(SchemeSlot::Accent1).with(ColorTransform::LumMod(60_000)), 1.25)),
        ..Default::default()
    }
}

/// The diagram's shapes laid out in `area` (ids not yet assigned).
pub fn layout(d: &Diagram, area: Xfrm) -> Vec<Shape> {
    layout_items(d, area).into_iter().map(|(s, _)| s).collect()
}

/// Lay out `d`, painted with its style and colours, each shape with the nodes it shows.
fn layout_items(d: &Diagram, area: Xfrm) -> Vec<Item> {
    let gs = groups(d);
    let nodes = |g: &[usize]| -> Vec<&DiagramNode> { g.iter().filter_map(|i| d.nodes.get(*i)).collect() };
    let n = gs.len().max(1);
    let nf = n as f64;
    let mut out: Vec<Item> = vec![];
    // A titled shape for top-level group `i`, painted in its accent.
    let titled = |out: &mut Vec<Item>, preset: &str, x: Xfrm, i: usize, g: &[usize]| {
        let mut sh = text_shape(preset, x, &nodes(g), true);
        paint(d, &mut sh, i, false);
        out.push((sh, g.to_vec()));
    };
    let arrow = |out: &mut Vec<Item>, x: Xfrm, rot: f64, i: usize| {
        let mut sh = deco("rightArrow", x, rot);
        paint(d, &mut sh, i, true);
        out.push((sh, vec![]));
    };
    match d.kind.as_str() {
        "hierarchy" => return hierarchy(d, area),
        "list" | "hBulletList" => {
            // Vertical / Horizontal Bullet List: a coloured title bar, sub-points in a pale box below.
            let vertical = d.kind == "list";
            for (i, g) in gs.iter().enumerate() {
                let (cell, bar_h) = if vertical {
                    (
                        Xfrm::new(area.x, area.y + i as f64 * area.h / nf, area.w, area.h / nf * 0.92),
                        (area.h / nf * 0.92).min(area.h * 0.12).max(18.0),
                    )
                } else {
                    let w = area.w / nf;
                    (Xfrm::new(area.x + i as f64 * w + w * 0.03, area.y, w * 0.94, area.h), area.h * 0.22)
                };
                let has_sub = g.len() > 1;
                let bar_h = bar_h.min(cell.h);
                titled(&mut out, "roundRect", Xfrm::new(cell.x, cell.y, cell.w, bar_h), i, &g[..1]);
                if has_sub {
                    let pad = cell.w * 0.03;
                    let x = Xfrm::new(cell.x + pad, cell.y + bar_h, cell.w - 2.0 * pad, (cell.h - bar_h).max(1.0));
                    let mut sh = text_shape("rect", x, &nodes(&g[1..]), false);
                    paint(d, &mut sh, i, true);
                    out.push((sh, g[1..].to_vec()));
                }
            }
        }
        "blockList" => {
            // Basic Block List: equal blocks in rows, as square as the area allows.
            let cols = ((nf * area.w / area.h.max(1.0) / 1.6).sqrt().ceil() as usize).clamp(1, n);
            let rows = n.div_ceil(cols);
            let (w, h) = (area.w / cols as f64, area.h / rows as f64);
            let (bw, bh) = (w * 0.9, (h * 0.85).min(w * 0.9 * 0.6));
            for (i, g) in gs.iter().enumerate() {
                let (c, r) = (i % cols, i / cols);
                let in_row = if r + 1 == rows { n - r * cols } else { cols };
                let x0 = area.x + (area.w - in_row as f64 * w) / 2.0;
                titled(&mut out, "rect", Xfrm::new(x0 + c as f64 * w + (w - bw) / 2.0, area.y + r as f64 * h + (h - bh) / 2.0, bw, bh), i, g);
            }
        }
        "process" | "chevron" => {
            let chevron = d.kind == "chevron";
            let gap = if chevron { -area.w * 0.02 } else { area.w * 0.06 };
            let w = ((area.w - gap * (nf - 1.0)) / nf).max(1.0);
            let h = (area.h * 0.5).min(w * if chevron { 0.45 } else { 0.65 });
            let y = area.y + (area.h - h) / 2.0;
            for (i, g) in gs.iter().enumerate() {
                let x = area.x + i as f64 * (w + gap);
                titled(&mut out, if chevron { "chevron" } else { "roundRect" }, Xfrm::new(x, y, w, h), i, g);
                if !chevron && i + 1 < n {
                    let aw = gap * 0.7;
                    arrow(&mut out, Xfrm::new(x + w + gap * 0.15, y + h / 2.0 - aw * 0.45, aw, aw * 0.9), 0.0, i);
                }
            }
        }
        "verticalProcess" => {
            let gap = area.h * 0.06;
            let h = ((area.h - gap * (nf - 1.0)) / nf).max(1.0);
            let w = area.w * 0.6;
            let x = area.x + (area.w - w) / 2.0;
            for (i, g) in gs.iter().enumerate() {
                let y = area.y + i as f64 * (h + gap);
                titled(&mut out, "roundRect", Xfrm::new(x, y, w, h), i, g);
                if i + 1 < n {
                    let s = gap * 0.8;
                    // A right arrow turned to point down.
                    arrow(&mut out, Xfrm::new(area.x + area.w / 2.0 - s / 2.0, y + h + gap * 0.1, s, s), 90.0, i);
                }
            }
        }
        "stepUp" => {
            // Step Up Process: each step higher than the one before.
            let w = area.w / nf;
            let h = area.h / (nf + 1.0) * 1.6;
            for (i, g) in gs.iter().enumerate() {
                let y = area.y + area.h - h - i as f64 * (area.h - h) / (nf - 1.0).max(1.0);
                titled(&mut out, "rect", Xfrm::new(area.x + i as f64 * w + w * 0.04, y, w * 0.92, h), i, g);
            }
        }
        "cycle" | "blockCycle" => {
            let block = d.kind == "blockCycle";
            let r = area.h.min(area.w) * 0.36;
            let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
            let dd = (r * if block { 0.75 } else { 0.8 }).min(2.0 * std::f64::consts::PI * r / nf * 0.62);
            for (i, g) in gs.iter().enumerate() {
                let a = (i as f64 / nf * 360.0 - 90.0).to_radians();
                let (w, h) = if block { (dd * 1.3, dd * 0.75) } else { (dd, dd) };
                titled(
                    &mut out,
                    if block { "roundRect" } else { "ellipse" },
                    Xfrm::new(cx + r * a.cos() - w / 2.0, cy + r * a.sin() - h / 2.0, w, h),
                    i,
                    g,
                );
                if n > 1 {
                    // An arrow halfway to the next node, along the circle.
                    let m = ((i as f64 + 0.5) / nf * 360.0 - 90.0).to_radians();
                    let s = (dd * 0.35).max(6.0);
                    let rot = (i as f64 + 0.5) / nf * 360.0;
                    arrow(&mut out, Xfrm::new(cx + r * m.cos() - s / 2.0, cy + r * m.sin() - s * 0.4, s, s * 0.8), rot, i);
                }
            }
        }
        "radial" => {
            // Basic Radial: the first node in the middle, the others around it with spokes.
            let Some((centre, rest)) = gs.split_first() else { return out };
            let m = rest.len().max(1) as f64;
            let r = area.h.min(area.w) * 0.36;
            let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
            let dc = r * 0.75;
            let dd = (r * 0.75).min(2.0 * std::f64::consts::PI * r / m * 0.7);
            for (k, _) in rest.iter().enumerate() {
                let a = (k as f64 / m * 360.0 - 90.0).to_radians();
                let (ex, ey) = (cx + (r - dd / 2.0) * a.cos(), cy + (r - dd / 2.0) * a.sin());
                out.push((line_shape((cx + dc / 2.0 * a.cos(), cy + dc / 2.0 * a.sin()), (ex, ey)), vec![]));
            }
            titled(&mut out, "ellipse", Xfrm::new(cx - dc / 2.0, cy - dc / 2.0, dc, dc), 0, centre);
            for (k, g) in rest.iter().enumerate() {
                let a = (k as f64 / m * 360.0 - 90.0).to_radians();
                titled(&mut out, "ellipse", Xfrm::new(cx + r * a.cos() - dd / 2.0, cy + r * a.sin() - dd / 2.0, dd, dd), k + 1, g);
            }
        }
        "pyramid" | "invertedPyramid" => {
            // One triangle cut into bands; inverted, the wide band is on top. Each band is drawn as
            // its own polygon so the sides keep the triangle's slope and the text stays upright.
            let inverted = d.kind == "invertedPyramid";
            let h = area.h / nf;
            let pw = area.h.min(area.w) * 1.15;
            let px = area.x + (area.w - pw) / 2.0;
            for (i, g) in gs.iter().enumerate() {
                let (a, b) = (i as f64 / nf, (i as f64 + 1.0) / nf);
                let (t, bt) = if inverted { (1.0 - a, 1.0 - b) } else { (a, b) };
                let (wt, wb) = (pw * t, pw * bt);
                let w = wt.max(wb).max(1.0);
                let bh = h * 0.96;
                let x = px + (pw - w) / 2.0;
                let (it, ib) = ((w - wt) / 2.0, (w - wb) / 2.0);
                let path = format!("M {it:.2} 0 L {:.2} 0 L {:.2} {bh:.2} L {ib:.2} {bh:.2} Z", w - it, w - ib);
                let mut sh = text_shape("rect", Xfrm::new(x, area.y + i as f64 * h, w, bh), &nodes(g), true);
                sh.geom = Geom::Custom {
                    paths: vec![deckcraft_model::CustomPath { w, h: bh, d: path, fill: deckcraft_geom::preset::FillMode::Norm, stroke: true }],
                };
                // Keep the text inside the sloped sides: inset it to a bit past the band's mid width.
                let inset = ((it + ib) / 2.0 + (it - ib).abs() / 4.0 + 4.0).min(w * 0.2);
                if let Some(b) = &mut sh.text {
                    b.body.inset_l = Some(inset);
                    b.body.inset_r = Some(inset);
                }
                paint(d, &mut sh, i, false);
                out.push((sh, g.clone()));
            }
        }
        "matrix" | "titledMatrix" => {
            let titled_m = d.kind == "titledMatrix";
            let s = area.h.min(area.w);
            let (x0, y0) = (area.x + (area.w - s) / 2.0, area.y + (area.h - s) / 2.0);
            let (quads, title): (&[Vec<usize>], Option<&Vec<usize>>) =
                if titled_m { gs.split_first().map(|(t, r)| (r, Some(t))).unwrap_or((&[], None)) } else { (&gs, None) };
            for (i, g) in quads.iter().take(4).enumerate() {
                let x = Xfrm::new(x0 + (i % 2) as f64 * s / 2.0 + 3.0, y0 + (i / 2) as f64 * s / 2.0 + 3.0, s / 2.0 - 6.0, s / 2.0 - 6.0);
                titled(&mut out, "roundRect", x, i + usize::from(titled_m), g);
            }
            if let Some(t) = title {
                let c = s * 0.42;
                let mut sh = text_shape("roundRect", Xfrm::new(x0 + (s - c) / 2.0, y0 + (s - c * 0.6) / 2.0, c, c * 0.6), &nodes(t), true);
                paint(d, &mut sh, 0, true);
                out.push((sh, t.clone()));
            }
        }
        _ => {}
    }
    out
}

/// An organisation chart: one box per node, rows by level, each subtree as wide as its leaves.
fn hierarchy(d: &Diagram, area: Xfrm) -> Vec<Item> {
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
    let mut out: Vec<Item> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let mut sh = text_shape("roundRect", boxes[i], &[n], true);
            paint(d, &mut sh, depth[i], false);
            (sh, vec![i])
        })
        .collect();
    for i in 0..nodes.len() {
        if let Some(p) = parent[i] {
            let (a, b) = (boxes[p], boxes[i]);
            out.push((line_shape((a.x + a.w / 2.0, a.y + a.h), (b.x + b.w / 2.0, b.y)), vec![]));
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
        d.imported = false;
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
    let kind = if is_kind(kind) { kind } else { "process" };
    if nodes.is_empty() {
        nodes = (0..3).map(|_| DiagramNode { text: "Text".into(), level: 0 }).collect();
    }
    normalize(&mut nodes);
    let d = Diagram { kind: kind.into(), nodes, ..Default::default() };
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

fn change_style(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.style";
    let v = str_param(p, "style").filter(|k| STYLES.iter().any(|(id, _)| id == k)).ok_or_else(|| bad(CMD, "unknown style"))?.to_string();
    edit_diagram(s, p, CMD, |d| {
        if d.style != v {
            d.style_xml = None;
        }
        d.style = v;
        Ok(Value::Null)
    })
}

fn change_colors(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.colors";
    let v = str_param(p, "colors").filter(|k| COLORS.iter().any(|(id, _)| id == k)).ok_or_else(|| bad(CMD, "unknown colours"))?.to_string();
    edit_diagram(s, p, CMD, |d| {
        if d.colors != v {
            d.colors_xml = None;
        }
        d.colors = v;
        Ok(Value::Null)
    })
}

fn change_layout(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "smartArt.layout";
    let kind = str_param(p, "kind").filter(|k| is_kind(k)).ok_or_else(|| bad(CMD, "unknown layout"))?.to_string();
    edit_diagram(s, p, CMD, |d| {
        if d.kind != kind {
            // PowerPoint's own layout no longer applies.
            d.layout_id.clear();
            d.layout_xml = None;
        }
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
        let d = Diagram { kind: "blockList".into(), nodes: mk(&[0, 1, 1, 0]), ..Default::default() };
        let v = layout(&d, area);
        assert_eq!(v.len(), 2);
        let t = v[0].text.as_ref().map(|b| b.paragraphs.len());
        assert_eq!(t, Some(3), "sub-points are bullets in their parent's shape");
        let h = Diagram { kind: "hierarchy".into(), nodes: mk(&[0, 1, 2, 1]), ..Default::default() };
        let v = layout(&h, area);
        assert_eq!(v.len(), 4 + 3, "a box per node and a line per parent link");
        for sh in &v {
            let x = sh.xfrm.unwrap_or_default();
            assert!(x.x >= -0.01 && x.x + x.w <= 800.01 && x.y >= -0.01 && x.y + x.h <= 400.01, "{x:?}");
        }
        assert!(layout(&Diagram { kind: "cycle".into(), ..Default::default() }, area).is_empty());
        // Vertical Bullet List: a bar per top node, its sub-points in a box below.
        let v = layout_items(&Diagram { kind: "list".into(), nodes: mk(&[0, 1, 1, 0]), ..Default::default() }, area);
        let maps: Vec<Vec<usize>> = v.into_iter().map(|(_, m)| m).collect();
        assert_eq!(maps, [vec![0], vec![1, 2], vec![3]]);
    }

    #[test]
    fn every_layout_shows_every_node_inside_its_area() {
        let area = Xfrm::new(50.0, 40.0, 800.0, 400.0);
        for (kind, _) in LAYOUTS {
            for style in ["", "subtle", "intense", "outline"] {
                let d = Diagram {
                    kind: kind.into(),
                    nodes: mk(&[0, 1, 0, 0, 1, 0]),
                    style: style.into(),
                    colors: "accent2".into(),
                    ..Default::default()
                };
                let items = layout_items(&d, area);
                let mut shown: Vec<usize> = items.iter().flat_map(|(_, m)| m.clone()).collect();
                shown.sort_unstable();
                let want: Vec<usize> = (0..6).collect();
                assert_eq!(shown, want, "{kind}");
                for (sh, _) in &items {
                    let x = sh.xfrm.unwrap_or_default();
                    let (cx, cy) = (x.x + x.w / 2.0, x.y + x.h / 2.0);
                    assert!(x.w.is_finite() && x.h.is_finite() && x.w >= 0.0 && x.h >= 0.0, "{kind} {x:?}");
                    assert!((49.0..=851.0).contains(&cx) && (39.0..=441.0).contains(&cy), "{kind} {x:?}");
                }
            }
        }
    }
}
