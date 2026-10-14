//! An unchanged chart keeps the embedded workbook its `c:externalData` points at (#335).

use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use deckcraft_geom::Xfrm;
use deckcraft_model::chart::{Chart, ChartType};
use deckcraft_model::{LayoutType, Shape, ShapeId, ShapeKind, defaults};

const WORKBOOK: &[u8] = b"chart-workbook-bytes-v1";
const EMBED: &str = "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx";

fn chart_pptx() -> Vec<u8> {
    let mut p = defaults::new_presentation(None);
    p.slides.clear();
    let layout = defaults::layout_of_kind(&p, LayoutType::Blank).unwrap_or_default();
    let mut slide = defaults::new_slide(&mut p, layout);
    let mut shape = Shape { id: ShapeId(p.alloc_id()), name: "Pie".into(), xfrm: Some(Xfrm::new(40.0, 40.0, 400.0, 300.0)), ..Default::default() };
    shape.kind = ShapeKind::Chart(Box::new(Chart::sample(ChartType::Pie)));
    slide.shapes.push(shape);
    p.slides.push(Arc::new(slide));
    deckcraft_pptx::export(&p).expect("export")
}

/// Add an embedded workbook and point the chart part at it, the way PowerPoint writes a chart.
fn with_workbook(bytes: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let mut chart_part = None;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).expect("entry");
        let name = f.name().to_string();
        let mut data = vec![];
        f.read_to_end(&mut data).expect("read");
        if name.starts_with("ppt/charts/chart") && name.ends_with(".xml") && !name.contains("_rels") {
            let xml = String::from_utf8(data).expect("chart xml");
            assert!(xml.contains("</c:chartSpace>"), "{name}");
            data = xml
                .replacen("</c:chartSpace>", r#"<c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#, 1)
                .into_bytes();
            chart_part = Some(name.clone());
        } else if name == "[Content_Types].xml" {
            let xml = String::from_utf8(data).expect("content types");
            data = xml
                .replacen(
                    "</Types>",
                    r#"<Override PartName="/ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/></Types>"#,
                    1,
                )
                .into_bytes();
        }
        out.start_file(&name, opts).expect("start");
        out.write_all(&data).expect("write");
    }
    let chart_part = chart_part.expect("chart part");
    let rels = format!("ppt/charts/_rels/{}.rels", chart_part.rsplit('/').next().unwrap_or("chart1.xml"));
    let rels_xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/Microsoft_Excel_Worksheet1.xlsx"/><Relationship Id="rIdBad" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide1.xml"/></Relationships>"#;
    out.start_file(rels, opts).expect("rels");
    out.write_all(rels_xml).expect("rels bytes");
    out.start_file(EMBED, opts).expect("embed");
    out.write_all(WORKBOOK).expect("workbook");
    out.finish().expect("finish").into_inner()
}

fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut out = vec![];
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).expect("entry");
        let name = f.name().to_string();
        let mut data = vec![];
        f.read_to_end(&mut data).expect("read");
        out.push((name, data));
    }
    out
}

fn chart_of(bytes: &[u8]) -> deckcraft_model::Chart {
    let p = deckcraft_pptx::import(bytes).expect("import");
    p.slides
        .iter()
        .flat_map(|s| s.shapes.iter())
        .find_map(|s| match &s.kind {
            ShapeKind::Chart(c) if s.name == "Pie" => Some((**c).clone()),
            _ => None,
        })
        .expect("pie chart")
}

#[test]
fn an_unchanged_chart_keeps_its_embedded_workbook() {
    let src = with_workbook(&chart_pptx());
    let chart = chart_of(&src);
    assert_eq!(chart.kept.parts.iter().filter(|p| p.name == EMBED).count(), 1);
    assert_eq!(chart.kept.parts.iter().find(|p| p.name == EMBED).map(|p| p.bytes.as_slice()), Some(WORKBOOK));

    let saved = deckcraft_pptx::export(&deckcraft_pptx::import(&src).expect("import")).expect("export");
    let saved_parts = parts(&saved);
    let workbook = saved_parts.iter().find(|(n, _)| n == EMBED).map(|(_, d)| d.as_slice());
    assert_eq!(workbook, Some(WORKBOOK), "the embedding part is dropped on save");
    let rels = saved_parts
        .iter()
        .find(|(n, _)| n.starts_with("ppt/charts/_rels/chart") && n.ends_with(".xml.rels"))
        .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
        .expect("chart relationships");
    assert!(rels.contains("../embeddings/Microsoft_Excel_Worksheet1.xlsx"), "{rels}");
    assert!(!rels.contains("slide1.xml"), "a chart relationship must not replace the slide: {rels}");
    let slide = saved_parts.iter().find(|(n, _)| n == "ppt/slides/slide1.xml").map(|(_, d)| String::from_utf8_lossy(d).into_owned()).expect("slide");
    assert!(slide.contains("sld"), "{slide}");
    assert!(!slide.as_bytes().windows(WORKBOOK.len()).any(|w| w == WORKBOOK));
    let xml = saved_parts
        .iter()
        .find(|(n, _)| n.starts_with("ppt/charts/chart") && n.ends_with(".xml"))
        .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
        .expect("chart xml");
    assert!(xml.contains("externalData"), "{xml}");
}

#[test]
fn an_edited_chart_does_not_write_a_stale_workbook() {
    let src = with_workbook(&chart_pptx());
    let mut p = deckcraft_pptx::import(&src).expect("import");
    let slide = Arc::make_mut(p.slides.get_mut(0).expect("slide"));
    let chart = slide.shapes.iter_mut().find_map(|s| match &mut s.kind {
        ShapeKind::Chart(c) if s.name == "Pie" => Some(c),
        _ => None,
    });
    let chart = chart.expect("pie");
    let value = chart.series.get_mut(0).and_then(|s| s.values.get_mut(0)).expect("value");
    *value = Some(99.0);
    let saved = parts(&deckcraft_pptx::export(&p).expect("export"));
    assert!(saved.iter().all(|(n, _)| n != EMBED), "a stale workbook was written after the chart changed");
    let xml = saved
        .iter()
        .find(|(n, _)| n.starts_with("ppt/charts/chart") && n.ends_with(".xml"))
        .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
        .expect("chart xml");
    assert!(!xml.contains("externalData"), "{xml}");
}
