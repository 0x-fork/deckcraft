//! Spelling squiggles: misspelt words on the current slide get a red wavy underline, checked on a
//! worker thread so typing never waits for a dictionary. Results are for one version of the
//! document; while a newer check runs, nothing stale is drawn. The word being typed (the caret at
//! or inside it) isn't marked, as in PowerPoint. The web build has no worker and shows none.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use deckcraft_engine::DocState;
use deckcraft_geom::Point;
use deckcraft_model::{Paragraph, ShapeId};

/// What a check is for: the document version, the slide and the user dictionary version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    doc: usize,
    rev: u64,
    slide: usize,
    user: u64,
}

struct Job {
    key: Key,
    default_lang: String,
    paras: Vec<(ShapeId, usize, Paragraph)>,
    repaint: Option<egui::Context>,
}

/// Misspelt ranges for one key: `(shape, para, start, end)`.
struct Done {
    key: Key,
    ranges: Vec<(ShapeId, usize, usize, usize)>,
}

#[derive(Default)]
pub struct Squiggles {
    tx: Option<Sender<Job>>,
    rx: Option<Receiver<Done>>,
    /// The check in flight, if any.
    pending: Option<Key>,
    done: Option<Done>,
    /// Underlines in slide coordinates for `done`, with each one's word: `(shape, para, start, end)`.
    lines: Vec<((ShapeId, usize, usize, usize), Point, Point)>,
    /// Off (tests or the setting).
    pub disabled: bool,
    /// The misspelt word a context menu is open on.
    pub menu_word: Option<MenuWord>,
    /// Words skipped with Ignore All this session (lower-case).
    ignored: Vec<String>,
}

/// A misspelt word under the pointer, with corrections.
#[derive(Clone, Debug)]
pub struct MenuWord {
    pub shape: ShapeId,
    pub para: usize,
    pub start: usize,
    pub word: String,
    pub suggestions: Vec<String>,
}

fn worker(rx: Receiver<Job>, tx: Sender<Done>) {
    // Paragraph results by content, so typing in one paragraph re-checks only that one.
    let mut cache: HashMap<(String, u64), Vec<(usize, usize)>> = HashMap::new();
    let mut user = None;
    while let Ok(mut job) = rx.recv() {
        // Only the newest job matters.
        while let Ok(j) = rx.try_recv() {
            job = j;
        }
        if user != Some(job.key.user) {
            cache.clear();
            user = Some(job.key.user);
        }
        let mut ranges = vec![];
        for (id, pi, p) in &job.paras {
            let k = (serde_json::to_string(&p.runs).unwrap_or_default(), job.key.user);
            let k = (format!("{}\u{0}{}", job.default_lang, k.0), k.1);
            let r = cache
                .entry(k)
                .or_insert_with(|| deckcraft_engine::spell::misspelt(p, &job.default_lang, &[]).into_iter().map(|w| (w.start, w.end)).collect());
            ranges.extend(r.iter().map(|(a, b)| (*id, *pi, *a, *b)));
        }
        if cache.len() > 20_000 {
            cache.clear();
        }
        if tx.send(Done { key: job.key, ranges }).is_err() {
            return;
        }
        if let Some(c) = &job.repaint {
            c.request_repaint();
        }
    }
}

fn default_lang(st: &DocState) -> String {
    for sl in &st.doc.slides {
        let mut found = None;
        deckcraft_model::walk(&sl.shapes, &mut |sh, _| {
            if found.is_none()
                && let Some(t) = &sh.text
            {
                found = t.paragraphs.iter().flat_map(|p| &p.runs).find_map(|r| r.props.lang.clone());
            }
        });
        if let Some(l) = found {
            return l;
        }
    }
    "en-US".into()
}

impl Squiggles {
    fn start(&mut self) -> bool {
        if self.tx.is_some() {
            return true;
        }
        if cfg!(target_arch = "wasm32") {
            return false;
        }
        let (jtx, jrx) = channel();
        let (dtx, drx) = channel();
        let ok = std::thread::Builder::new().name("spelling".into()).spawn(move || worker(jrx, dtx)).is_ok();
        if ok {
            self.tx = Some(jtx);
            self.rx = Some(drx);
        }
        ok
    }

    /// Ask for a check of the current slide if it changed, take in finished checks, and return the
    /// underlines to draw (slide coordinates).
    pub fn update(&mut self, st: &DocState, repaint: Option<&egui::Context>) -> Vec<(Point, Point)> {
        if self.disabled || !self.start() {
            return vec![];
        }
        let key =
            Key { doc: Arc::as_ptr(&st.doc) as usize, rev: st.revision, slide: st.selection.slide, user: deckcraft_engine::spell::user_generation() };
        if let Some(rx) = &self.rx {
            while let Ok(d) = rx.try_recv() {
                if self.pending == Some(d.key) {
                    self.pending = None;
                }
                self.lines.clear();
                self.done = Some(d);
                if let Some(d) = &self.done
                    && d.key == key
                {
                    let mut by_shape: HashMap<ShapeId, Vec<(usize, usize, usize)>> = HashMap::new();
                    for (id, p, a, b) in &d.ranges {
                        by_shape.entry(*id).or_default().push((*p, *a, *b));
                    }
                    for (id, rs) in by_shape {
                        for r in rs {
                            for (a, b) in deckcraft_engine::cmd::text::range_underlines(st, id, &[r]) {
                                self.lines.push(((id, r.0, r.1, r.2), a, b));
                            }
                        }
                    }
                }
            }
        }
        let fresh = self.done.as_ref().is_some_and(|d| d.key == key);
        if !fresh && self.pending != Some(key) {
            let mut paras = vec![];
            if let Some(sl) = st.doc.slides.get(st.selection.slide) {
                deckcraft_model::walk(&sl.shapes, &mut |sh, _| {
                    if let Some(t) = &sh.text {
                        for (i, p) in t.paragraphs.iter().enumerate() {
                            paras.push((sh.id, i, p.clone()));
                        }
                    }
                });
            }
            let job = Job { key, default_lang: default_lang(st), paras, repaint: repaint.cloned() };
            if self.tx.as_ref().is_some_and(|tx| tx.send(job).is_ok()) {
                self.pending = Some(key);
            }
        }
        if !fresh {
            return vec![];
        }
        // Not under the word the caret is in.
        let typing = st.selection.text.as_ref().filter(|t| !t.notes && t.cell.is_none()).map(|t| (t.shape, t.caret));
        self.lines
            .iter()
            .filter(|((id, p, a, b), _, _)| !typing.is_some_and(|(s, (cp, cc))| s == *id && cp == *p && cc >= *a && cc <= *b))
            .filter(|((id, p, a, b), _, _)| {
                self.ignored.is_empty() || {
                    let w = st
                        .shape(*id)
                        .and_then(|sh| sh.text.as_ref()?.paragraphs.get(*p).map(|pp| pp.text().chars().skip(*a).take(b - a).collect::<String>()));
                    !w.is_some_and(|w| self.ignored.contains(&w.to_lowercase()))
                }
            })
            .map(|(_, a, b)| (*a, *b))
            .collect()
    }

    /// Skip `word` everywhere for the rest of the session.
    pub fn ignore(&mut self, word: &str) {
        self.ignored.push(word.to_lowercase());
    }

    /// The squiggled word at slide point `p` (within `tol` points of its underline).
    pub fn word_at(&self, st: &DocState, p: Point, tol: f64) -> Option<MenuWord> {
        let ((shape, para, start, end), _, _) = *self.lines.iter().find(|(_, a, b)| {
            let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
            // From the line's top (a word is about a line high above its underline) to just below it.
            p.x >= x0 - tol && p.x <= x1 + tol && p.y <= a.y.max(b.y) + tol && p.y >= a.y.min(b.y) - 24.0
        })?;
        let pp = st
            .doc
            .slides
            .get(st.selection.slide)
            .and_then(|sl| deckcraft_model::find_shape(&sl.shapes, shape))?
            .text
            .as_ref()?
            .paragraphs
            .get(para)?;
        let word: String = pp.text().chars().skip(start).take(end - start).collect();
        let lang = deckcraft_engine::spell::paragraph_words(pp, &default_lang(st)).into_iter().find(|w| w.start == start).map(|w| w.lang)?;
        let suggestions = deckcraft_engine::spell::dictionary(&lang).map(|d| d.suggest(&word, 5)).unwrap_or_default();
        Some(MenuWord { shape, para, start, word, suggestions })
    }

    /// Wait (tests) until the check for the current state is in.
    pub fn settle(&mut self, st: &DocState) -> Vec<(Point, Point)> {
        for _ in 0..500 {
            let v = self.update(st, None);
            if self.done.as_ref().is_some_and(|d| self.pending.is_none() && d.key.doc == Arc::as_ptr(&st.doc) as usize) {
                return v;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        vec![]
    }
}

/// A red wavy line from `a` to `b` (screen points).
pub fn paint(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, scale: f32) {
    let d = b - a;
    let len = d.length();
    if len < 1.0 {
        return;
    }
    let (u, n) = (d / len, egui::vec2(-d.y, d.x) / len);
    let step = (2.0 * scale).clamp(1.5, 3.0);
    let amp = step * 0.75;
    let mut pts = vec![];
    let mut t = 0.0;
    let mut up = false;
    while t <= len {
        pts.push(a + u * t + n * if up { -amp } else { amp });
        up = !up;
        t += step;
    }
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, egui::Color32::from_rgb(0xE0, 0x20, 0x20))));
}
