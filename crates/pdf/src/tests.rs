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

#[test]
fn exports_slides_notes_and_handouts() {
    let p = deck(5);
    for layout in [PageLayout::Slides, PageLayout::Notes, PageLayout::Handouts { per_page: 3 }, PageLayout::Handouts { per_page: 6 }] {
        let bytes = export(&p, &opts(layout)).expect("export");
        assert!(bytes.starts_with(b"%PDF-"), "{layout:?}");
        assert!(bytes.len() > 1000);
    }
}

#[test]
fn empty_range_is_an_error() {
    let p = deck(1);
    let o = PdfOptions { slides: Some(vec![7]), ..opts(PageLayout::Slides) };
    assert!(matches!(export(&p, &o), Err(PdfError::NoPages)));
}

#[test]
fn hidden_slides_skipped_unless_asked() {
    let mut p = deck(2);
    Arc::make_mut(&mut p.slides[1]).hidden = true;
    let a = export(&p, &opts(PageLayout::Slides)).unwrap();
    let b = export(&p, &PdfOptions { include_hidden: true, ..opts(PageLayout::Slides) }).unwrap();
    assert!(b.len() > a.len());
}

#[test]
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

#[test]
fn glyph_ranges_follow_shaping_clusters() {
    // "fl" ligature: its glyph maps to both characters, the rest stay one to one (#164).
    assert_eq!(glyph_text_ranges("flash", (0, 5), &[(0, 'f'), (2, 'a'), (3, 's'), (4, 'h')], 4), vec![0..2, 2..3, 3..4, 4..5]);
    // Two glyphs in one cluster (base and mark) share its range.
    assert_eq!(glyph_text_ranges("ab", (0, 2), &[(0, 'a'), (0, 'a'), (1, 'b')], 3), vec![0..1, 0..1, 1..2]);
    // A run starting mid-paragraph, multi-byte characters in visual (right-to-left) order.
    assert_eq!(glyph_text_ranges("αβγ", (3, 6), &[(5, 'γ'), (4, 'β'), (3, 'α')], 3), vec![4..6, 2..4, 0..2]);
}
