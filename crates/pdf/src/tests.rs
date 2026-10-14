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

/// Notes pages and handouts print on the chosen paper, landscape included, with every slide box
/// on the page and room left for the notes.
#[test]
fn notes_and_handouts_fit_any_paper_and_orientation() {
    let p = deck(3);
    for (id, _, _) in PAPERS {
        for landscape in [false, true] {
            let paper = paper_size(id, landscape).expect("known paper");
            assert_eq!(paper.0 > paper.1, landscape, "{id}");
            let b = notes_slide_box(paper, 16.0 / 9.0);
            assert!(b.x0 >= 0.0 && b.x1 <= paper.0 && b.y1 <= paper.1 * 0.5, "{id} {landscape}: {b:?}");
            assert!((b.width() / b.height() - 16.0 / 9.0).abs() < 1e-6);
            for n in [1, 3, 6, 9] {
                for r in handout_boxes(n, paper, 16.0 / 9.0) {
                    assert!(r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= paper.0 && r.y1 <= paper.1, "{id} {landscape} {n}: {r:?}");
                }
            }
            for layout in [PageLayout::Notes, PageLayout::Handouts { per_page: 6 }] {
                let bytes = export(&p, &PdfOptions { paper, ..opts(layout) }).expect("export");
                let text = String::from_utf8_lossy(&bytes);
                let media = format!("/MediaBox[0 0 {} {}]", fmt(paper.0), fmt(paper.1));
                assert!(text.contains(&media), "{id} {landscape} {layout:?}: no {media}");
            }
        }
    }
    assert_eq!(paper_size("bogus", false), None);
}

fn fmt(v: f64) -> String {
    let s = format!("{:.2}", v as f32);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
