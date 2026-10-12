//! Chart parts (`c:chartSpace`) → [`Chart`]. The original XML is kept in `Chart.raw`.

use deckcraft_model::chart::{Chart, ChartType, Series};
use deckcraft_model::style::Fill;

use crate::xml::El;

/// Fills of chart parts never reference images we keep; read colour fills only.
fn simple_fill(sp: Option<&El>) -> Option<Fill> {
    let sp = sp?;
    for e in sp.elements() {
        match e.local() {
            "noFill" => return Some(Fill::None),
            "solidFill" => return super::dml::color(e).map(|color| Fill::Solid { color }),
            "gradFill" => return Some(Fill::Gradient(super::dml::gradient(e))),
            _ => {}
        }
    }
    None
}

fn simple_line(sp: Option<&El>) -> Option<deckcraft_model::Line> {
    let ln = sp?.child("ln")?;
    let mut l = deckcraft_model::Line { width: super::dml::pt(ln, "w"), ..Default::default() };
    l.fill = simple_fill(Some(ln));
    Some(l)
}

fn rich_text(t: &El) -> Option<String> {
    let tx = t.child("tx")?;
    if let Some(r) = tx.child("rich") {
        let s = r.children_named("p").map(|p| p.text()).collect::<Vec<_>>().join("\n");
        return Some(s);
    }
    tx.find("v").map(|v| v.text())
}

/// String values from `c:cat`/`c:tx` (str or num refs, caches or literals).
fn strings(e: &El) -> Vec<String> {
    let cache = e.find("strCache").or_else(|| e.find("numCache")).or_else(|| e.find("strLit")).or_else(|| e.find("numLit"));
    if let Some(c) = cache {
        let n = c.child("ptCount").and_then(|p| p.u32("val")).unwrap_or(0).min(100_000) as usize;
        let mut out = vec![String::new(); n];
        for pt in c.children_named("pt").take(100_000) {
            let i = pt.u32("idx").unwrap_or(0) as usize;
            let v = pt.child("v").map(|v| v.text()).unwrap_or_default();
            if i >= out.len() {
                if i >= 100_000 {
                    continue;
                }
                out.resize(i + 1, String::new());
            }
            if let Some(s) = out.get_mut(i) {
                *s = v;
            }
        }
        return out;
    }
    if let Some(ml) = e.find("multiLvlStrCache") {
        // Use the innermost level.
        if let Some(l) = ml.child("lvl") {
            return l.children_named("pt").map(|p| p.child("v").map(|v| v.text()).unwrap_or_default()).collect();
        }
    }
    if let Some(v) = e.child("v") {
        return vec![v.text()];
    }
    vec![]
}

fn numbers(e: &El) -> Vec<Option<f64>> {
    strings(e).into_iter().map(|s| s.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

/// Parse a chart part. Returns `None` for something that isn't a chart we can read.
pub fn read_chart_xml(bytes: &[u8]) -> Option<Chart> {
    let doc = crate::xml::parse(bytes).ok()?;
    let mut root = doc.root;
    super::resolve_mc(&mut root, 0);
    if !root.is("chartSpace") {
        return None;
    }
    let mut c = chart_from(&root)?;
    c.raw = Some(String::from_utf8_lossy(bytes).into_owned());
    Some(c)
}

pub(crate) fn chart_from(root: &El) -> Option<Chart> {
    let ch = root.child("chart")?;
    let pa = ch.child("plotArea")?;
    let mut c = Chart { categories: vec![], series: vec![], title: None, legend: None, gridlines: false, ..Default::default() };
    c.style = root.child_val("style").and_then(|v| v.parse().ok()).unwrap_or(2);
    if let Some(t) = ch.child("title") {
        c.title = Some(rich_text(t).unwrap_or_else(|| "Chart Title".into()));
    }
    if ch.child("autoTitleDeleted").and_then(|a| a.bool("val")) == Some(true) {
        c.title = None;
    }
    c.legend = ch.child("legend").map(|l| l.child_val("legendPos").unwrap_or("r").to_string());
    c.legend_overlay = ch.child("legend").and_then(|l| l.child("overlay")).and_then(|o| o.bool("val")).unwrap_or(false);
    c.chart_fill = simple_fill(root.child("spPr"));
    c.plot_fill = simple_fill(pa.child("spPr"));

    let mut kinds: Vec<ChartType> = vec![];
    // The value axis the first plot group uses is the primary one; a group on another value axis
    // (a combo chart's line on the right) is on the secondary axis.
    let val_ids: Vec<&str> = pa.children_named("valAx").filter_map(|a| a.child_val("axId")).collect();
    let group_val_axis = |g: &El| g.children_named("axId").filter_map(|a| a.attr("val")).find(|id| val_ids.contains(id)).map(str::to_string);
    let mut primary_val: Option<String> = None;
    for g in pa.elements() {
        let name = g.local();
        if !name.ends_with("Chart") {
            continue;
        }
        let grouping = g.child_val("grouping").unwrap_or("clustered");
        let kind = match name {
            "barChart" | "bar3DChart" => {
                let bar = g.child_val("barDir") == Some("bar");
                match (bar, grouping) {
                    (false, "stacked") => ChartType::StackedColumn,
                    (false, "percentStacked") => ChartType::PercentColumn,
                    (false, _) => ChartType::Column,
                    (true, "stacked") => ChartType::StackedBar,
                    (true, "percentStacked") => ChartType::PercentBar,
                    (true, _) => ChartType::Bar,
                }
            }
            "lineChart" | "line3DChart" => {
                let markers = g.child_val("marker").is_some_and(|v| v == "1" || v == "true")
                    && g.children_named("ser").all(|s| s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")) != Some("none"));
                match grouping {
                    "stacked" | "percentStacked" => ChartType::StackedLine,
                    _ if markers => ChartType::LineMarkers,
                    _ => ChartType::Line,
                }
            }
            "areaChart" | "area3DChart" => {
                if matches!(grouping, "stacked" | "percentStacked") {
                    ChartType::StackedArea
                } else {
                    ChartType::Area
                }
            }
            "pieChart" | "pie3DChart" | "ofPieChart" => ChartType::Pie,
            "doughnutChart" => ChartType::Doughnut,
            "scatterChart" => ChartType::Scatter,
            "bubbleChart" => ChartType::Bubble,
            "radarChart" => {
                if g.child_val("radarStyle") == Some("filled") {
                    ChartType::FilledRadar
                } else {
                    ChartType::Radar
                }
            }
            "stockChart" => ChartType::Stock,
            "surfaceChart" | "surface3DChart" => ChartType::Surface,
            _ => continue,
        };
        if let Some(v) = g.child_val("varyColors") {
            c.vary_colors = v == "1" || v == "true";
        }
        if let Some(v) = g.child_val("gapWidth").and_then(|v| v.parse::<f64>().ok()) {
            c.gap_width = (v / 100.0).clamp(0.0, 5.0);
        }
        if let Some(v) = g.child_val("overlap").and_then(|v| v.parse::<f64>().ok()) {
            c.overlap = (v / 100.0).clamp(-1.0, 1.0);
        }
        if let Some(v) = g.child_val("holeSize").and_then(|v| v.parse::<f64>().ok()) {
            c.hole_size = (v / 100.0).clamp(0.0, 0.9);
        }
        if g.path(&["dLbls", "showVal"]).and_then(|v| v.bool("val")) == Some(true) {
            c.data_labels = true;
        }
        kinds.push(kind);
        let val_axis = group_val_axis(g);
        if primary_val.is_none() {
            primary_val = val_axis.clone();
        }
        let secondary = val_axis.is_some() && val_axis != primary_val;
        for s in g.children_named("ser").take(1000) {
            let mut ser = Series {
                name: s.child("tx").map(strings).and_then(|v| v.into_iter().next()).unwrap_or_default(),
                fill: simple_fill(s.child("spPr")),
                line: simple_line(s.child("spPr")),
                smooth: s.child("smooth").and_then(|v| v.bool("val")).unwrap_or(false),
                marker: s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")).filter(|m| *m != "none").map(String::from),
                kind: Some(kind),
                secondary_axis: secondary,
                ..Default::default()
            };
            if let Some(d) = s.child("dLbls").and_then(|d| d.child("showVal")).and_then(|v| v.bool("val")) {
                ser.data_labels = Some(d);
            }
            for dp in s.children_named("dPt") {
                if let (Some(i), Some(f)) = (dp.child("idx").and_then(|i| i.u32("val")), simple_fill(dp.child("spPr"))) {
                    ser.point_fills.push((i as usize, f));
                }
            }
            if let Some(cat) = s.child("cat").or_else(|| s.child("xVal")) {
                let cats = strings(cat);
                if matches!(kind, ChartType::Scatter | ChartType::Bubble) {
                    ser.x_values = cats.iter().map(|v| v.trim().parse::<f64>().ok()).collect();
                }
                if c.categories.is_empty() {
                    c.categories = cats;
                }
            }
            if let Some(v) = s.child("val").or_else(|| s.child("yVal")) {
                ser.values = numbers(v);
            }
            if let Some(b) = s.child("bubbleSize") {
                ser.sizes = numbers(b);
            }
            c.series.push(ser);
        }
    }
    let first = *kinds.first()?;
    c.kind = if kinds.iter().any(|k| *k != first) { ChartType::Combo } else { first };
    if c.kind != ChartType::Combo {
        for s in &mut c.series {
            s.kind = None;
            s.secondary_axis = false;
        }
    }
    // Axes.
    let vertical = |a: &&El| matches!(a.child_val("axPos"), Some("l" | "r"));
    let val_ax = pa.children_named("valAx").find(vertical).or_else(|| pa.children_named("valAx").last());
    let cat_ax = pa.child("catAx").or_else(|| pa.child("dateAx")).or_else(|| pa.children_named("valAx").find(|a| !vertical(a)));
    if let Some(v) = val_ax {
        c.gridlines = v.child("majorGridlines").is_some();
        let sc = v.child("scaling");
        c.y_min = sc.and_then(|s| s.child_val("min")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.y_max = sc.and_then(|s| s.child_val("max")).and_then(|v| v.parse().ok()).filter(|v: &f64| v.is_finite());
        c.axis_titles.1 = v.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    if let Some(a) = cat_ax {
        c.axis_titles.0 = a.child("title").map(|t| rich_text(t).unwrap_or_else(|| "Axis Title".into()));
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_position_and_overlay_are_read() {
        let legend = r#"<c:legend><c:legendPos val="r"/><c:overlay val="1"/></c:legend>"#;
        let xml = format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea>{legend}</c:chart></c:chartSpace>"#);
        let c = read_chart_xml(xml.as_bytes()).unwrap();
        assert_eq!(c.legend.as_deref(), Some("r"));
        assert!(c.legend_overlay);
        let xml = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea><c:legend/></c:chart></c:chartSpace>"#;
        let c = read_chart_xml(xml).unwrap();
        assert_eq!(c.legend.as_deref(), Some("r"));
        assert!(!c.legend_overlay);
    }

    #[test]
    fn combo_chart_with_a_secondary_axis() {
        // Columns on axes 1/2, a line on axes 3/4 whose value axis is on the right.
        let xml = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea>
<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:tx><c:v>Spend</c:v></c:tx><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>W1</c:v></c:pt><c:pt idx="1"><c:v>W2</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>200000</c:v></c:pt><c:pt idx="1"><c:v>310000</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart>
<c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="1"/><c:tx><c:v>Clicks</c:v></c:tx><c:spPr><a:ln w="28575"><a:solidFill><a:srgbClr val="00B050"/></a:solidFill></a:ln></c:spPr><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>6000</c:v></c:pt><c:pt idx="1"><c:v>7400</c:v></c:pt></c:numLit></c:val></c:ser><c:marker val="1"/><c:axId val="3"/><c:axId val="4"/></c:lineChart>
<c:catAx><c:axId val="1"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx>
<c:valAx><c:axId val="4"/><c:axPos val="r"/><c:crossAx val="3"/></c:valAx><c:catAx><c:axId val="3"/><c:delete val="1"/><c:axPos val="b"/><c:crossAx val="4"/></c:catAx>
</c:plotArea></c:chart></c:chartSpace>"#;
        let c = read_chart_xml(xml).unwrap();
        assert_eq!(c.kind, ChartType::Combo);
        assert_eq!(c.series.len(), 2);
        assert!(!c.series[0].secondary_axis);
        assert!(c.series[1].secondary_axis);
        assert_eq!(c.series[1].kind, Some(ChartType::LineMarkers));
        assert!(c.series[1].line.as_ref().is_some_and(|l| l.width.is_some_and(|w| (w - 2.25).abs() < 0.01)));
        // A chart of one type on one axis has no secondary series.
        let single = String::from_utf8_lossy(xml)
            .replace("<c:axId val=\"3\"/><c:axId val=\"4\"/></c:lineChart>", "<c:axId val=\"1\"/><c:axId val=\"2\"/></c:lineChart>");
        let c = read_chart_xml(single.as_bytes()).unwrap();
        assert!(c.series.iter().all(|s| !s.secondary_axis));
    }
}
