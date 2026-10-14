//! SmartArt (DrawingML diagrams): the outline ⇄ the data model, and the layout, quick style and
//! colour definitions DeckCraft writes for its own layouts.
//!
//! A diagram's data part holds its points (`dgm:pt`: the document, one per node, and the
//! transitions PowerPoint pairs with each link) and its links (`dgm:cxn` of type `parOf`, parent →
//! child, ordered by `srcOrd`). The outline is that tree flattened depth first.

use std::collections::HashMap;

use deckcraft_model::{Diagram, DiagramNode};

use crate::xml::{A, El, W};

pub const NS_DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
pub const NS_DSP: &str = "http://schemas.microsoft.com/office/drawing/2008/diagram";
pub const CT_DATA: &str = "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml";
pub const CT_LAYOUT: &str = "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml";
pub const CT_STYLE: &str = "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml";
pub const CT_COLORS: &str = "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml";
pub const CT_DRAWING: &str = "application/vnd.ms-office.drawingml.diagramDrawing+xml";
pub const RT_DRAWING: &str = "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
const QS_ID: &str = "urn:microsoft.com/office/officeart/2005/8/quickstyle/simple1";
const CS_ID: &str = "urn:microsoft.com/office/officeart/2005/8/colors/accent1_2";
/// Most nodes read from one diagram.
const MAX_NODES: usize = 2000;

/// PowerPoint's built-in layout DeckCraft's layout `kind` is written as.
pub fn layout_id(kind: &str) -> &'static str {
    match kind {
        "process" => "urn:microsoft.com/office/officeart/2005/8/layout/process1",
        "cycle" => "urn:microsoft.com/office/officeart/2005/8/layout/cycle2",
        "hierarchy" => "urn:microsoft.com/office/officeart/2005/8/layout/hierarchy1",
        "pyramid" => "urn:microsoft.com/office/officeart/2005/8/layout/pyramid1",
        "matrix" => "urn:microsoft.com/office/officeart/2005/8/layout/matrix3",
        _ => "urn:microsoft.com/office/officeart/2005/8/layout/vList2",
    }
}

/// The DeckCraft layout closest to a PowerPoint layout (by its id and category).
pub fn kind_for(layout_id: &str, category: &str) -> &'static str {
    let s = format!("{layout_id} {category}").to_ascii_lowercase();
    if s.contains("hierarchy") || s.contains("orgchart") {
        "hierarchy"
    } else if s.contains("cycle") || s.contains("radial") {
        "cycle"
    } else if s.contains("pyramid") {
        "pyramid"
    } else if s.contains("matrix") {
        "matrix"
    } else if s.contains("process") || s.contains("chevron") || s.contains("arrow") {
        "process"
    } else {
        "list"
    }
}

fn category(kind: &str) -> &'static str {
    match kind {
        "process" => "process",
        "cycle" => "cycle",
        "hierarchy" => "hierarchy",
        "pyramid" => "pyramid",
        "matrix" => "matrix",
        _ => "list",
    }
}

/// A model id, stable for a given diagram (`seed`) and point number.
fn guid(seed: u32, n: usize) -> String {
    format!("{{{seed:08X}-D1A6-4C0A-9E5B-{n:012X}}}")
}

fn text(w: &mut W, t: &str) {
    w.open0("dgm:t");
    w.empty0("a:bodyPr");
    w.empty0("a:lstStyle");
    w.open0("a:p");
    if t.is_empty() {
        w.empty("a:endParaRPr", A::new().a("lang", "en-US"));
    } else {
        w.open0("a:r");
        w.empty("a:rPr", A::new().a("lang", "en-US"));
        w.elt("a:t", t);
        w.close("a:r");
    }
    w.close("a:p");
    w.close("dgm:t");
}

/// `ppt/diagrams/dataN.xml` for the outline. `drawing_rel` is the slide's relationship id of the
/// cached drawing.
pub fn data_xml(d: &Diagram, seed: u32, drawing_rel: &str) -> Vec<u8> {
    let lo = if d.layout_id.is_empty() { layout_id(&d.kind).to_string() } else { d.layout_id.clone() };
    let mut w = W::new();
    w.open("dgm:dataModel", A::new().a("xmlns:dgm", NS_DGM).a("xmlns:a", crate::opc::NS_A).a("xmlns:r", crate::opc::NS_R));
    w.open0("dgm:ptLst");
    let doc = guid(seed, 0);
    w.open("dgm:pt", A::new().a("modelId", &doc).a("type", "doc"));
    w.empty(
        "dgm:prSet",
        A::new()
            .a("loTypeId", &lo)
            .a("loCatId", category(&d.kind))
            .a("qsTypeId", QS_ID)
            .a("qsCatId", "simple")
            .a("csTypeId", CS_ID)
            .a("csCatId", "accent1")
            .a("phldr", "1"),
    );
    w.empty0("dgm:spPr");
    text(&mut w, "");
    w.close("dgm:pt");
    // Node k: model id 1 + 3k; its link's parent and sibling transitions 2 + 3k and 3 + 3k.
    let node = |k: usize| guid(seed, 1 + 3 * k);
    for (k, n) in d.nodes.iter().enumerate() {
        w.open("dgm:pt", A::new().a("modelId", node(k)));
        w.empty("dgm:prSet", A::new().a("phldrT", "[Text]"));
        w.empty0("dgm:spPr");
        text(&mut w, &n.text);
        w.close("dgm:pt");
        let cxn = guid(seed, 0x10_0000 + k);
        for (i, ty) in [(2, "parTrans"), (3, "sibTrans")] {
            w.open("dgm:pt", A::new().a("modelId", guid(seed, i + 3 * k)).a("type", ty).a("cxnId", &cxn));
            w.empty0("dgm:prSet");
            w.empty0("dgm:spPr");
            text(&mut w, "");
            w.close("dgm:pt");
        }
    }
    w.close("dgm:ptLst");
    w.open0("dgm:cxnLst");
    let mut stack: Vec<(u8, String)> = vec![];
    let mut ords: HashMap<String, usize> = HashMap::new();
    for (k, n) in d.nodes.iter().enumerate() {
        while stack.last().is_some_and(|(l, _)| *l >= n.level) {
            stack.pop();
        }
        let parent = stack.last().map(|(_, id)| id.clone()).unwrap_or_else(|| doc.clone());
        let ord = ords.entry(parent.clone()).or_insert(0);
        w.empty(
            "dgm:cxn",
            A::new()
                .a("modelId", guid(seed, 0x10_0000 + k))
                .a("srcId", &parent)
                .a("destId", node(k))
                .a("srcOrd", *ord)
                .a("destOrd", 0)
                .a("parTransId", guid(seed, 2 + 3 * k))
                .a("sibTransId", guid(seed, 3 + 3 * k)),
        );
        *ord += 1;
        stack.push((n.level, node(k)));
    }
    w.close("dgm:cxnLst");
    w.empty0("dgm:bg");
    w.empty0("dgm:whole");
    w.open0("dgm:extLst");
    w.open("a:ext", A::new().a("uri", "http://schemas.microsoft.com/office/drawing/2008/diagram"));
    w.empty(
        "dsp:dataModelExt",
        A::new().a("xmlns:dsp", NS_DSP).a("relId", drawing_rel).a("minVer", "http://schemas.openxmlformats.org/drawingml/2006/diagram"),
    );
    w.close("a:ext");
    w.close("dgm:extLst");
    w.close("dgm:dataModel");
    w.finish()
}

/// The outline of a data part: the `parOf` tree under the document point, depth first.
pub fn outline(data: &El) -> Vec<DiagramNode> {
    let Some(pts) = data.child("ptLst") else { return vec![] };
    let mut texts: HashMap<&str, String> = HashMap::new();
    let mut doc: Option<&str> = None;
    for pt in pts.children_named("pt").take(MAX_NODES * 4) {
        let Some(id) = pt.attr("modelId") else { continue };
        match pt.attr("type").unwrap_or("node") {
            "doc" => doc = doc.or(Some(id)),
            "node" => {
                let t = pt.child("t").map(paragraphs).unwrap_or_default();
                texts.insert(id, t);
            }
            _ => {}
        }
    }
    let Some(doc) = doc else { return vec![] };
    let mut kids: HashMap<&str, Vec<(i64, &str)>> = HashMap::new();
    if let Some(cx) = data.child("cxnLst") {
        for c in cx.children_named("cxn").take(MAX_NODES * 4) {
            if c.attr("type").unwrap_or("parOf") != "parOf" {
                continue;
            }
            let (Some(src), Some(dst)) = (c.attr("srcId"), c.attr("destId")) else { continue };
            if texts.contains_key(dst) {
                kids.entry(src).or_default().push((c.i64("srcOrd").unwrap_or(0), dst));
            }
        }
    }
    for v in kids.values_mut() {
        v.sort_by_key(|(o, _)| *o);
    }
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    // Depth first, iteratively; a node reached twice (a cycle in a hostile file) is skipped.
    let mut stack: Vec<(&str, u8)> = kids.get(doc).map(|v| v.iter().rev().map(|(_, id)| (*id, 0u8)).collect()).unwrap_or_default();
    while let Some((id, level)) = stack.pop() {
        if out.len() >= MAX_NODES || !seen.insert(id) {
            continue;
        }
        out.push(DiagramNode { text: texts.get(id).cloned().unwrap_or_default(), level: level.min(8) });
        if let Some(v) = kids.get(id) {
            stack.extend(v.iter().rev().map(|(_, c)| (*c, level.saturating_add(1))));
        }
    }
    out
}

/// A text body's paragraphs joined by spaces.
fn paragraphs(t: &El) -> String {
    t.children_named("p")
        .map(|p| {
            let mut v = vec![];
            p.find_all("t", &mut v);
            v.iter().map(|e| e.text()).collect::<String>()
        })
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The layout algorithm and node shape of a DeckCraft layout.
fn alg(kind: &str) -> (&'static str, Vec<(&'static str, &'static str)>, &'static str) {
    match kind {
        "process" => ("lin", vec![("linDir", "fromL"), ("nodeVertAlign", "mid")], "roundRect"),
        "cycle" => ("cycle", vec![("stAng", "0"), ("spanAng", "360")], "ellipse"),
        "pyramid" => ("pyra", vec![("linDir", "fromT"), ("txDir", "fromT"), ("pyraAcctPos", "bef")], "trapezoid"),
        "matrix" => ("snake", vec![("grDir", "tL"), ("flowDir", "row"), ("contDir", "sameDir"), ("off", "ctr")], "roundRect"),
        _ => ("lin", vec![("linDir", "fromT"), ("vertAlign", "mid")], "roundRect"),
    }
}

fn node_layout(w: &mut W, shape: &str, bullets: bool) {
    w.open("dgm:layoutNode", A::new().a("name", "node"));
    w.open0("dgm:varLst");
    w.empty("dgm:bulletEnabled", A::new().a("val", if bullets { "1" } else { "0" }));
    w.close("dgm:varLst");
    w.empty("dgm:alg", A::new().a("type", "tx"));
    w.open("dgm:shape", A::new().a("type", shape).a("r:blip", ""));
    w.empty0("dgm:adjLst");
    w.close("dgm:shape");
    w.empty("dgm:presOf", A::new().a("axis", if bullets { "desOrSelf" } else { "self" }).a("ptType", "node"));
    w.open0("dgm:constrLst");
    for side in ["tMarg", "bMarg", "lMarg", "rMarg"] {
        w.empty("dgm:constr", A::new().a("type", side).a("refType", "primFontSz").a("fact", "0.3"));
    }
    w.close("dgm:constrLst");
    w.open0("dgm:ruleLst");
    w.empty("dgm:rule", A::new().a("type", "primFontSz").a("val", "5").a("fact", "NaN").a("max", "NaN"));
    w.close("dgm:ruleLst");
    w.close("dgm:layoutNode");
}

fn space(w: &mut W) {
    w.open("dgm:forEach", A::new().a("name", "sibling").a("axis", "followSib").a("ptType", "sibTrans").a("cnt", "1"));
    w.open("dgm:layoutNode", A::new().a("name", "space"));
    w.empty("dgm:alg", A::new().a("type", "sp"));
    w.open("dgm:shape", A::new().a("r:blip", ""));
    w.empty0("dgm:adjLst");
    w.close("dgm:shape");
    w.empty0("dgm:presOf");
    w.empty0("dgm:constrLst");
    w.empty0("dgm:ruleLst");
    w.close("dgm:layoutNode");
    w.close("dgm:forEach");
}

/// `ppt/diagrams/layoutN.xml`: a layout definition for DeckCraft's layout `kind`, under the id of
/// the closest built-in PowerPoint layout.
pub fn layout_xml(kind: &str) -> Vec<u8> {
    let mut w = W::new();
    w.open(
        "dgm:layoutDef",
        A::new().a("xmlns:dgm", NS_DGM).a("xmlns:a", crate::opc::NS_A).a("xmlns:r", crate::opc::NS_R).a("uniqueId", layout_id(kind)),
    );
    w.empty("dgm:title", A::new().a("val", ""));
    w.empty("dgm:desc", A::new().a("val", ""));
    w.open0("dgm:catLst");
    w.empty("dgm:cat", A::new().a("type", category(kind)).a("pri", "1000"));
    w.close("dgm:catLst");
    w.open("dgm:layoutNode", A::new().a("name", "diagram"));
    w.open0("dgm:varLst");
    w.empty0("dgm:dir");
    w.empty("dgm:resizeHandles", A::new().a("val", "exact"));
    w.close("dgm:varLst");
    if kind == "hierarchy" {
        // Each node over the row of its children, recursively.
        w.empty("dgm:alg", A::new().a("type", "hierChild"));
        w.open("dgm:shape", A::new().a("r:blip", ""));
        w.empty0("dgm:adjLst");
        w.close("dgm:shape");
        w.empty0("dgm:presOf");
        w.empty0("dgm:constrLst");
        w.empty0("dgm:ruleLst");
        w.open("dgm:forEach", A::new().a("name", "level").a("axis", "ch").a("ptType", "node"));
        w.open("dgm:layoutNode", A::new().a("name", "hierRoot"));
        w.empty("dgm:alg", A::new().a("type", "hierRoot"));
        w.open("dgm:shape", A::new().a("r:blip", ""));
        w.empty0("dgm:adjLst");
        w.close("dgm:shape");
        w.empty0("dgm:presOf");
        w.empty0("dgm:constrLst");
        w.empty0("dgm:ruleLst");
        node_layout(&mut w, "roundRect", false);
        w.open("dgm:layoutNode", A::new().a("name", "hierChild"));
        w.empty("dgm:alg", A::new().a("type", "hierChild"));
        w.open("dgm:shape", A::new().a("r:blip", ""));
        w.empty0("dgm:adjLst");
        w.close("dgm:shape");
        w.empty0("dgm:presOf");
        w.empty0("dgm:constrLst");
        w.empty0("dgm:ruleLst");
        w.empty("dgm:forEach", A::new().a("name", "repeat").a("ref", "level"));
        w.close("dgm:layoutNode");
        w.close("dgm:layoutNode");
        w.close("dgm:forEach");
    } else {
        let (ty, params, shape) = alg(kind);
        w.open("dgm:alg", A::new().a("type", ty));
        for (k, v) in params {
            w.empty("dgm:param", A::new().a("type", k).a("val", v));
        }
        w.close("dgm:alg");
        w.open("dgm:shape", A::new().a("r:blip", ""));
        w.empty0("dgm:adjLst");
        w.close("dgm:shape");
        w.empty0("dgm:presOf");
        w.open0("dgm:constrLst");
        w.empty("dgm:constr", A::new().a("type", "w").a("for", "ch").a("forName", "node").a("refType", "w"));
        w.empty(
            "dgm:constr",
            A::new().a("type", "h").a("for", "ch").a("forName", "node").a("refType", "w").a("refFor", "ch").a("refForName", "node").a("fact", "0.6"),
        );
        w.empty(
            "dgm:constr",
            A::new().a("type", "w").a("for", "ch").a("forName", "space").a("refType", "w").a("refFor", "ch").a("refForName", "node").a("fact", "0.1"),
        );
        w.empty("dgm:constr", A::new().a("type", "primFontSz").a("for", "ch").a("forName", "node").a("op", "equ").a("val", "65"));
        w.close("dgm:constrLst");
        w.empty0("dgm:ruleLst");
        w.open("dgm:forEach", A::new().a("name", "nodes").a("axis", "ch").a("ptType", "node"));
        node_layout(&mut w, shape, true);
        if kind != "cycle" {
            space(&mut w);
        }
        w.close("dgm:forEach");
    }
    w.close("dgm:layoutNode");
    w.close("dgm:layoutDef");
    w.finish()
}

/// `ppt/diagrams/quickStyleN.xml`: flat shapes drawn from the theme's first styles.
pub fn style_xml() -> Vec<u8> {
    let mut w = W::new();
    w.open("dgm:styleDef", A::new().a("xmlns:dgm", NS_DGM).a("xmlns:a", crate::opc::NS_A).a("uniqueId", QS_ID));
    w.empty("dgm:title", A::new().a("val", ""));
    w.empty("dgm:desc", A::new().a("val", ""));
    w.open0("dgm:catLst");
    w.empty("dgm:cat", A::new().a("type", "simple").a("pri", "10100"));
    w.close("dgm:catLst");
    w.open0("dgm:scene3d");
    w.empty("a:camera", A::new().a("prst", "orthographicFront"));
    w.empty("a:lightRig", A::new().a("rig", "threePt").a("dir", "t"));
    w.close("dgm:scene3d");
    w.open("dgm:styleLbl", A::new().a("name", "node1"));
    w.open0("dgm:scene3d");
    w.empty("a:camera", A::new().a("prst", "orthographicFront"));
    w.empty("a:lightRig", A::new().a("rig", "threePt").a("dir", "t"));
    w.close("dgm:scene3d");
    w.empty0("dgm:sp3d");
    w.empty("dgm:txPr", A::new());
    w.open0("dgm:style");
    for (tag, idx) in [("a:lnRef", "2"), ("a:fillRef", "1"), ("a:effectRef", "0")] {
        w.open(tag, A::new().a("idx", idx));
        w.empty("a:scrgbClr", A::new().a("r", "0").a("g", "0").a("b", "0"));
        w.close(tag);
    }
    w.open("a:fontRef", A::new().a("idx", "minor"));
    w.empty("a:schemeClr", A::new().a("val", "lt1"));
    w.close("a:fontRef");
    w.close("dgm:style");
    w.close("dgm:styleLbl");
    w.close("dgm:styleDef");
    w.finish()
}

/// `ppt/diagrams/colorsN.xml`: accent 1 fills with light text.
pub fn colors_xml() -> Vec<u8> {
    let mut w = W::new();
    w.open("dgm:colorsDef", A::new().a("xmlns:dgm", NS_DGM).a("xmlns:a", crate::opc::NS_A).a("uniqueId", CS_ID));
    w.empty("dgm:title", A::new().a("val", ""));
    w.empty("dgm:desc", A::new().a("val", ""));
    w.open0("dgm:catLst");
    w.empty("dgm:cat", A::new().a("type", "accent1").a("pri", "11200"));
    w.close("dgm:catLst");
    w.open("dgm:styleLbl", A::new().a("name", "node1"));
    for (tag, clr) in [("dgm:fillClrLst", "accent1"), ("dgm:linClrLst", "lt1")] {
        w.open(tag, A::new().a("meth", "repeat"));
        w.empty("a:schemeClr", A::new().a("val", clr));
        w.close(tag);
    }
    w.empty0("dgm:effectClrLst");
    w.empty0("dgm:txLinClrLst");
    w.empty0("dgm:txFillClrLst");
    w.empty0("dgm:txEffectClrLst");
    w.close("dgm:styleLbl");
    w.close("dgm:colorsDef");
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(v: &[(&str, u8)]) -> Vec<DiagramNode> {
        v.iter().map(|(t, l)| DiagramNode { text: (*t).into(), level: *l }).collect()
    }

    #[test]
    fn outline_survives_the_data_model() {
        let d =
            Diagram { kind: "hierarchy".into(), nodes: nodes(&[("CEO", 0), ("CTO", 1), ("Dev", 2), ("CFO", 1), ("Board", 0)]), ..Default::default() };
        let xml = data_xml(&d, 7, "rId9");
        let doc = crate::xml::parse(&xml).unwrap();
        assert_eq!(outline(&doc.root), d.nodes);
        let s = String::from_utf8(xml).unwrap();
        assert!(s.contains("hierarchy1") && s.contains("relId=\"rId9\""));
        for x in [layout_xml("hierarchy"), layout_xml("cycle"), style_xml(), colors_xml()] {
            assert!(crate::xml::parse(&x).is_ok());
        }
    }

    #[test]
    fn hostile_links_are_bounded() {
        let xml = br#"<dgm:dataModel xmlns:dgm="d"><dgm:ptLst><dgm:pt modelId="0" type="doc"/><dgm:pt modelId="1"><dgm:t><a:p xmlns:a="a"><a:r><a:t>A</a:t></a:r></a:p></dgm:t></dgm:pt><dgm:pt modelId="2"/></dgm:ptLst><dgm:cxnLst><dgm:cxn srcId="0" destId="1"/><dgm:cxn srcId="1" destId="2"/><dgm:cxn srcId="2" destId="1"/><dgm:cxn type="presOf" srcId="0" destId="2"/></dgm:cxnLst></dgm:dataModel>"#;
        let doc = crate::xml::parse(xml).unwrap();
        assert_eq!(outline(&doc.root), nodes(&[("A", 0), ("", 1)]));
        assert_eq!(kind_for("urn:microsoft.com/office/officeart/2005/8/layout/orgChart1", ""), "hierarchy");
        assert_eq!(kind_for("urn:x/layout/chevron2", "process"), "process");
        assert_eq!(kind_for("urn:x/layout/default", "list"), "list");
    }
}
