use std::sync::Arc;

use super::*;
use deckcraft_model::text::TextBody;

fn deck(n: usize) -> Presentation {
    let mut p = deckcraft_model::defaults::new_presentation(None);
    let layout = p.slides[0].layout;
    while p.slides.len() < n {
        let s = deckcraft_model::defaults::new_slide(&mut p, layout);
        p.slides.push(Arc::new(s));
    }
    for (i, s) in p.slides.iter_mut().enumerate() {
        let s = Arc::make_mut(s);
        if let Some(sh) = s.shapes.first_mut() {
            sh.text = Some(TextBody::from_text(&format!("Slide title {i}")));
        }
        s.notes = TextBody::from_text("Speaker notes that wrap across the page width to test line breaking of long notes.");
    }
    p
}

fn opts(layout: PageLayout) -> PdfOptions {
    PdfOptions { layout, dpi: 48.0, ..Default::default() }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn exports_slides_notes_and_handouts() {
    let p = deck(5);
    for layout in [PageLayout::Slides, PageLayout::Notes, PageLayout::Handouts { per_page: 3 }, PageLayout::Handouts { per_page: 6 }] {
        let bytes = export(&p, &opts(layout)).expect("export");
        assert!(bytes.starts_with(b"%PDF-"), "{layout:?}");
        assert!(bytes.len() > 1000);
        let pdf = hayro_syntax::Pdf::new(bytes.clone()).unwrap();
        let (pages, size) = match layout {
            PageLayout::Slides => (5, (960.0, 540.0)),
            PageLayout::Notes => (5, (612.0, 792.0)),
            PageLayout::Handouts { per_page: 3 } => (2, (612.0, 792.0)),
            _ => (1, (612.0, 792.0)),
        };
        assert_eq!(pdf.pages().len(), pages);
        assert!(pdf.pages().iter().all(|page| page.render_dimensions() == size));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn empty_range_is_an_error() {
    let p = deck(1);
    let o = PdfOptions { slides: Some(vec![7]), ..opts(PageLayout::Slides) };
    assert!(matches!(export(&p, &o), Err(PdfError::NoPages)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn hidden_slides_skipped_unless_asked() {
    let mut p = deck(2);
    Arc::make_mut(&mut p.slides[1]).hidden = true;
    let a = export(&p, &opts(PageLayout::Slides)).unwrap();
    let b = export(&p, &PdfOptions { include_hidden: true, ..opts(PageLayout::Slides) }).unwrap();
    assert_eq!(hayro_syntax::Pdf::new(a.clone()).unwrap().pages().len(), 1);
    assert_eq!(hayro_syntax::Pdf::new(b.clone()).unwrap().pages().len(), 2);
    let ranged = export(&p, &PdfOptions { slides: Some(vec![1, 0, 9]), ..opts(PageLayout::Slides) }).unwrap();
    assert_eq!(hayro_syntax::Pdf::new(ranged.clone()).unwrap().pages().len(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn handout_boxes_fit_the_paper() {
    for n in [1, 2, 3, 4, 6, 9] {
        let v = handout_boxes(n, (612.0, 792.0), 16.0 / 9.0);
        assert_eq!(v.len(), n as usize);
        for r in &v {
            assert!(r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= 612.0 && r.y1 <= 792.0, "{n}: {r:?}");
            assert!((r.width() / r.height() - 16.0 / 9.0).abs() < 1e-6);
        }
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                assert!(a.intersect(*b).area() <= 0.0, "{n}: overlap");
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn metadata_retains_values_and_omits_blank_fields() {
    let mut p = deck(1);
    p.props.title = "  Deck title  ".into();
    p.props.author = "  Author  ".into();
    let bytes = export(&p, &opts(PageLayout::Slides)).unwrap();
    let pdf = hayro_syntax::Pdf::new(bytes.clone()).unwrap();
    assert_eq!(pdf.metadata().title.as_deref(), Some(b"  Deck title  ".as_slice()));
    assert_eq!(pdf.metadata().author.as_deref(), Some(b"  Author  ".as_slice()));
    assert_eq!(pdf.metadata().creator.as_deref(), Some(b"DeckCraft".as_slice()));
    assert_eq!(pdf.metadata().producer.as_deref(), Some(b"DeckCraft".as_slice()));
    assert!(pdf.metadata().creation_date.is_none());
    p.props.author = " ".into();
    let bytes = export(&p, &PdfOptions { title: Some(" ".into()), ..opts(PageLayout::Slides) }).unwrap();
    let pdf = hayro_syntax::Pdf::new(bytes.clone()).unwrap();
    assert!(pdf.metadata().title.is_none());
    assert!(pdf.metadata().author.is_none());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn text_layer_links_and_bookmarks_survive_serialization() {
    use deckcraft_model::text::{Action, Hyperlink};
    use hayro_syntax::object::{Array, Dict};
    let mut p = deck(2);
    let last = p.slides[1].id;
    let slide = Arc::make_mut(&mut p.slides[0]);
    for (shape, action) in slide.shapes.iter_mut().zip([Action::Url { url: "https://example.com/deck".into() }, Action::Slide { slide: last }]) {
        shape.click = Some(Hyperlink { action, tooltip: "Link".into(), highlight_click: false });
    }
    for text_layer in [true, false] {
        let bytes = export(&p, &PdfOptions { text_layer, ..opts(PageLayout::Slides) }).unwrap();
        let pdf = hayro_syntax::Pdf::new(bytes.clone()).unwrap();
        let page = &pdf.pages()[0];
        let annotations = page.raw().get::<Array<'_>>(b"Annots").unwrap();
        assert_eq!(annotations.iter::<Dict<'_>>().count(), 2);
        let resources = page.raw().get::<Dict<'_>>(b"Resources").unwrap();
        let fonts = resources.get::<Dict<'_>>(b"Font");
        assert_eq!(fonts.is_some_and(|fonts| !fonts.is_empty()), text_layer);
        let catalog = pdf.xref().get::<Dict<'_>>(pdf.xref().root_id()).unwrap();
        let outlines = catalog.get::<Dict<'_>>(b"Outlines").unwrap();
        let first = outlines.get::<Dict<'_>>(b"First").unwrap();
        assert!(first.get::<Dict<'_>>(b"Next").is_some());
    }
}

/// Build a small damaged derivative of our openly licensed last-resort font entirely in
/// memory. Its naming/shaping tables still parse, but its missing head table prevents PDF
/// embedding. No font bytes or vendor assets are stored as test fixtures.
fn font_without_head() -> Vec<u8> {
    let mut bytes = FontDb::global().face("Ubuntu", "Regular").data().to_vec();
    for (old, new) in [(b"Ubuntu".as_slice(), b"BadPDF".as_slice()), (b"\0U\0b\0u\0n\0t\0u".as_slice(), b"\0B\0a\0d\0P\0D\0F".as_slice())] {
        let mut offset = 0;
        while offset + old.len() <= bytes.len() {
            if bytes[offset..offset + old.len()] == *old {
                bytes[offset..offset + old.len()].copy_from_slice(new);
                offset += old.len();
            } else {
                offset += 1;
            }
        }
    }
    let tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    for i in 0..tables {
        let offset = 12 + i * 16;
        if &bytes[offset..offset + 4] == b"head" {
            bytes[offset..offset + 4].copy_from_slice(b"zzzz");
        }
    }
    bytes
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unembeddable_font_returns_error_instead_of_a_pdf_missing_text() {
    use deckcraft_model::{Shape, ShapeId, Xfrm};
    let db = FontDb::global();
    assert_eq!(db.add_font(font_without_head()), 1);
    assert_eq!(db.face("BadPDF", "Regular").family, "BadPDF");
    let mut p = deck(1);
    let slide = Arc::make_mut(&mut p.slides[0]);
    let mut body = TextBody::from_text("Text that must not disappear silently");
    for paragraph in &mut body.paragraphs {
        for run in &mut paragraph.runs {
            run.props.font = Some("BadPDF".into());
            run.props.size = Some(12.0);
        }
    }
    slide.shapes = vec![Shape { id: ShapeId(9000), xfrm: Some(Xfrm::new(20.0, 20.0, 500.0, 200.0)), text: Some(body), ..Default::default() }];
    let error = export(&p, &opts(PageLayout::Slides)).unwrap_err();
    assert!(matches!(error, PdfError::Write(_)));
    assert!(error.to_string().contains("font could not be read for embedding"));
    // The failed export leaves no poisoned writer state behind.
    assert!(export(&deck(1), &opts(PageLayout::Slides)).is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn supported_unicode_is_shaped_and_recovered_from_the_page_text_layer() {
    use deckcraft_model::{Shape, ShapeId, Xfrm};
    use hayro_syntax::content::ops::TypedInstruction;
    use hayro_syntax::object::{Dict, Object, Stream};

    let text = "Café Ελληνικά Привет";
    let face = FontDb::global().face("Ubuntu", "Regular");
    assert_eq!(face.family, "Ubuntu");
    assert!(text.chars().all(|c| face.glyph_for(c) != 0), "missing: {:?}", text.chars().filter(|c| face.glyph_for(*c) == 0).collect::<Vec<_>>());
    let mut body = TextBody::from_text(text);
    for paragraph in &mut body.paragraphs {
        for run in &mut paragraph.runs {
            run.props.font = Some("Ubuntu".into());
            run.props.size = Some(20.0);
        }
    }
    let mut p = deck(1);
    Arc::make_mut(&mut p.slides[0]).shapes =
        vec![Shape { id: ShapeId(9001), xfrm: Some(Xfrm::new(20.0, 20.0, 920.0, 100.0)), text: Some(body), ..Default::default() }];
    let placed = deckcraft_render::place_slide(&p, 0);
    let runs: Vec<_> = placed.texts.iter().flat_map(|placed| &placed.layout.runs).collect();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| run.face.family == "Ubuntu" && !run.glyphs.is_empty() && run.glyphs.iter().all(|glyph| glyph.0 != 0)));

    let bytes = export(&p, &opts(PageLayout::Slides)).unwrap();
    let pdf = hayro_syntax::Pdf::new(bytes.clone()).unwrap();
    let page = &pdf.pages()[0];
    let resources = page.raw().get::<Dict<'_>>(b"Resources").unwrap();
    let fonts = resources.get::<Dict<'_>>(b"Font").unwrap();
    assert_eq!(fonts.len(), 1);
    let font = fonts.get::<Dict<'_>>(fonts.keys().next().unwrap()).unwrap();
    let cmap = font.get::<Stream<'_>>(b"ToUnicode").unwrap().decoded().unwrap().into_owned();
    let cmap = std::str::from_utf8(&cmap).unwrap();
    // This fixture uses the writer's two-byte CIDs and bfchar mappings. Decode the
    // content's text-showing operators through that map, not document metadata.
    let mut mapping = HashMap::<u16, String>::new();
    let mut in_char_map = false;
    for line in cmap.lines() {
        if line.ends_with("beginbfchar") {
            in_char_map = true;
        } else if line == "endbfchar" {
            in_char_map = false;
        } else if in_char_map {
            let parts: Vec<_> = line.split_whitespace().collect();
            let cid = u16::from_str_radix(parts[0].trim_matches(['<', '>']), 16).unwrap();
            let value = parts[1].trim_matches(['<', '>']);
            let units: Vec<_> =
                value.as_bytes().chunks_exact(4).map(|chunk| u16::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap()).collect();
            mapping.insert(cid, String::from_utf16(&units).unwrap());
        }
    }
    let mut extracted = String::new();
    let mut append = |bytes: &[u8]| {
        assert!(bytes.len().is_multiple_of(2));
        for pair in bytes.chunks_exact(2) {
            extracted.push_str(mapping.get(&u16::from_be_bytes([pair[0], pair[1]])).unwrap());
        }
    };
    let mut operations = page.typed_operations();
    while let Some(operation) = operations.next() {
        match operation {
            TypedInstruction::ShowText(value) => append(value.0.as_bytes()),
            TypedInstruction::ShowTexts(values) => {
                for value in values.0.iter::<Object<'_>>() {
                    if let Object::String(value) = value {
                        append(value.as_bytes());
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(extracted, text);
}
