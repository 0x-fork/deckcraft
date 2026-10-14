//! Slide Show: full-screen playback with transitions and animations (`deckcraft-anim`),
//! keyboard/mouse navigation, pen, black/white screens, See All Slides, zoom, presenter view
//! and rehearsed timings.

use deckcraft_anim::{Layer, LinkJump, ShowAction, ShowState, Source, Timeline};
use deckcraft_model::text::Hyperlink;
use deckcraft_model::{Presentation, ShapeId};
use deckcraft_render::{ParaState, RenderOpts, ShapeState};
use egui::{Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, pos2, vec2};
use serde_json::json;

use crate::SlideApp;
use crate::theme;

pub struct Show {
    pub state: ShowState,
    pub reading: bool,
    pub presenter: bool,
    /// Preview from the Transitions/Animations tab: exits after the current slide.
    pub preview_only: bool,
    /// Play every step automatically (animation preview).
    pub autoplay: bool,
    pub rehearse: bool,
    timeline: Option<(usize, Timeline)>,
    step_start: f64,
    slide_start: f64,
    step_end: f64,
    trans: Option<Transition>,
    tex: Option<(Key, TextureHandle)>,
    next_tex: Option<(usize, TextureHandle)>,
    blank: Option<Color32>,
    /// See All Slides covers the show until a slide is chosen.
    pub grid: bool,
    /// The next click on the slide zooms in there.
    pub zoom_arm: bool,
    zoom: Option<Zoom>,
    thumbs: Vec<(usize, TextureHandle)>,
    pen: bool,
    strokes: Vec<Vec<Pos2>>,
    goto: String,
    started: bool,
    show_start: f64,
    /// Elapsed time in presenter view. Pausing it does not pause the show.
    timer: PresenterTimer,
    /// Speaker-notes size in presenter view, in points.
    notes_px: f32,
    timings: Vec<(usize, f64)>,
    last_move: f64,
    pub ended: bool,
    /// The slide whose media was last started (auto-play runs once per visit).
    media_slide: Option<usize>,
    /// The slide shown before the current one (the Last Slide Viewed action).
    last_viewed: Option<usize>,
    /// Click areas of hyperlinks and shape actions on slide `.0`, in slide points.
    links: Option<(usize, Vec<deckcraft_render::PlacedLink>)>,
}

#[derive(Clone, PartialEq)]
struct Key {
    slide: usize,
    step: usize,
    size: (u32, u32),
    doc: usize,
    /// Media hidden while not playing.
    hidden: Vec<ShapeId>,
    /// The frames animated GIFs show.
    gif: u64,
}

struct Transition {
    old: Option<TextureHandle>,
    /// The slide left behind (Morph matches its shapes) and the Morph frame texture.
    from: Option<usize>,
    morph: Option<TextureHandle>,
    /// Backgrounds and contents of the old and new slides, rendered once when the transition
    /// moves contents over backgrounds (Pan, Ferris Wheel, Reveal…): old back, old content, new
    /// back, new content.
    parts: Option<[Option<TextureHandle>; 4]>,
    start: f64,
    kind: String,
    option: String,
    dur: f64,
}

impl Show {
    pub fn new(doc: &Presentation, from: usize, reading: bool, presenter: bool) -> Self {
        Show {
            state: ShowState::start_at(doc, from),
            reading,
            presenter,
            preview_only: false,
            autoplay: false,
            rehearse: false,
            timeline: None,
            step_start: 0.0,
            slide_start: 0.0,
            step_end: 0.0,
            trans: None,
            tex: None,
            next_tex: None,
            blank: None,
            grid: false,
            zoom_arm: false,
            zoom: None,
            thumbs: vec![],
            pen: false,
            strokes: vec![],
            goto: String::new(),
            started: false,
            show_start: 0.0,
            timer: PresenterTimer::start(0.0),
            notes_px: NOTES_DEFAULT,
            timings: vec![],
            last_move: 0.0,
            ended: false,
            media_slide: None,
            last_viewed: None,
            links: None,
        }
    }

    fn timeline(&mut self, doc: &Presentation) -> &Timeline {
        let i = self.state.slide;
        if self.timeline.as_ref().is_none_or(|(k, _)| *k != i) {
            self.timeline = Some((i, ShowState::timeline(doc, i)));
        }
        match &self.timeline {
            Some((_, t)) => t,
            None => {
                static EMPTY: std::sync::OnceLock<Timeline> = std::sync::OnceLock::new();
                EMPTY.get_or_init(Timeline::default)
            }
        }
    }

    fn apply(&mut self, a: ShowAction, doc: &Presentation, now: f64) -> bool {
        match a {
            ShowAction::PlayStep(_) => {
                self.step_start = now;
            }
            ShowAction::FinishStep(_) | ShowAction::StepBack(_) => {
                self.step_end = now;
            }
            ShowAction::Slide(i) => {
                self.zoom = None;
                self.zoom_arm = false;
                if self.rehearse {
                    self.timings.push((self.timeline.as_ref().map(|t| t.0).unwrap_or(i), now - self.slide_start));
                }
                let (kind, option, dur) = doc
                    .slides
                    .get(i)
                    .and_then(|s| s.transition.as_ref())
                    .map(|t| (t.kind.clone(), t.option.clone(), t.duration_ms as f64 / 1000.0))
                    .unwrap_or(("none".into(), String::new(), 0.0));
                // Take the old slide's texture rather than cloning its handle: the new slide is
                // otherwise rendered into the same texture, and the transition blends the new
                // slide with itself (#33).
                let (from, old) = self.tex.take().map(|(k, t)| (k.slide, t)).unzip();
                if from != Some(i) {
                    self.last_viewed = from;
                }
                self.trans = if kind != "none" && dur > 0.0 && !doc.show.without_animation {
                    Some(Transition { old, from, morph: None, parts: None, start: now, kind, option, dur })
                } else {
                    None
                };
                self.slide_start = now;
                self.step_start = now;
                self.step_end = now;
                self.strokes.clear();
                if self.preview_only {
                    return false;
                }
                self.after_enter(doc, now);
            }
            ShowAction::End => {
                self.ended = true;
                if self.preview_only {
                    return false;
                }
            }
            ShowAction::Exit => return false,
            ShowAction::None => {}
        }
        true
    }

    /// On entering a slide: auto-play its first step when it begins With/After Previous.
    fn after_enter(&mut self, doc: &Presentation, now: f64) {
        let auto = self.timeline(doc).step_is_auto(0);
        if auto || self.autoplay {
            let a = self.state.next(doc);
            if matches!(a, ShowAction::PlayStep(_)) {
                self.step_start = now;
            }
        }
    }
}

fn state_for(tl: &Timeline, id: ShapeId, step: usize, t: f64) -> ShapeState {
    let a = tl.state(id, None, step, t);
    ShapeState {
        visible: a.visible,
        opacity: a.opacity,
        offset: deckcraft_geom::Vec2::new(a.offset_x, a.offset_y),
        scale: (a.scale_x, a.scale_y),
        rotate: a.rotate,
        clip: a.clip,
        tint: a.tint,
        paras: tl
            .paragraph_targets(id)
            .into_iter()
            .map(|p| {
                let a = tl.state(id, Some(p), step, t);
                (p, ParaState { visible: a.visible, opacity: a.opacity, offset: deckcraft_geom::Vec2::new(a.offset_x, a.offset_y) })
            })
            .collect(),
    }
}

fn fit(avail: Rect, doc: &Presentation) -> Rect {
    let (sw, sh) = (doc.slide_size.width as f32, doc.slide_size.height as f32);
    let s = (avail.width() / sw).min(avail.height() / sh);
    Rect::from_center_size(avail.center(), vec2(sw * s, sh * s))
}

/// A transition layer as an egui mesh: [`deckcraft_anim::layer_mesh`]'s triangles (perspective
/// cards subdivided so their pictures stay straight) with per-vertex opacity.
fn quad_mesh(tex: Option<egui::TextureId>, rect: Rect, l: &Layer, color: Color32) -> Mesh {
    let mut m = match tex {
        Some(id) => Mesh::with_texture(id),
        None => Mesh::default(),
    };
    let (verts, tris) = deckcraft_anim::layer_mesh(l);
    for v in &verts {
        let a = if v.a.is_finite() { v.a.clamp(0.0, 1.0) } else { 0.0 };
        let col = if tex.is_some() { Color32::from_white_alpha((a * 255.0).round() as u8) } else { color.gamma_multiply(a as f32) };
        let p = pos2(rect.min.x + v.pos[0] as f32 * rect.width(), rect.min.y + v.pos[1] as f32 * rect.height());
        m.vertices.push(egui::epaint::Vertex { pos: p, uv: pos2(v.uv[0] as f32, v.uv[1] as f32), color: col });
    }
    for t in tris {
        m.add_triangle(t[0], t[1], t[2]);
    }
    m
}

/// Render the backgrounds and contents of a transition's two slides into textures, once.
fn part_textures(ctx: &egui::Context, doc: &Presentation, from: Option<usize>, to: usize, size: (u32, u32)) -> [Option<TextureHandle>; 4] {
    use deckcraft_render::SlidePart;
    let threads = if cfg!(target_arch = "wasm32") { 0 } else { std::thread::available_parallelism().map(|n| n.get().min(8) as u16).unwrap_or(0) };
    let opts = RenderOpts { scale: size.0 as f64 / doc.slide_size.width.max(1.0), size: Some(size), threads, ..Default::default() };
    let tex = |i: usize, part: SlidePart, name: &str| {
        let img = deckcraft_render::render_slide_part(doc, i, part, &opts);
        ctx.load_texture(name, crate::textures::to_color_image(&img, false), egui::TextureOptions::LINEAR)
    };
    [
        from.map(|i| tex(i, SlidePart::Backdrop, "trans-old-back")),
        from.map(|i| tex(i, SlidePart::Content, "trans-old-content")),
        Some(tex(to, SlidePart::Backdrop, "trans-new-back")),
        Some(tex(to, SlidePart::Content, "trans-new-content")),
    ]
}

pub fn ui(app: &mut SlideApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let Some(doc) = app.session.active().map(|d| d.doc.clone()) else {
        app.show = None;
        return;
    };
    let now = ctx.input(|i| i.time);
    let Some(mut show) = app.show.take() else { return };
    if !show.started {
        show.started = true;
        show.slide_start = now;
        show.step_start = now;
        show.show_start = now;
        show.timer = PresenterTimer::start(now);
        show.last_move = now;
        if !show.reading {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        show.after_enter(&doc, now);
    }
    let mut keep = true;
    // Input.
    let (keys, clicked, secondary, moved) = ctx.input(|i| {
        let keys: Vec<(egui::Key, egui::Modifiers)> = i
            .events
            .iter()
            .filter_map(|e| if let egui::Event::Key { key, pressed: true, modifiers, .. } = e { Some((*key, *modifiers)) } else { None })
            .collect();
        (keys, i.pointer.primary_clicked(), i.pointer.secondary_clicked(), i.pointer.delta().length() > 0.0)
    });
    if moved {
        show.last_move = now;
    }
    let mut actions: Vec<ShowAction> = vec![];
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, &doc);
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    for (k, m) in keys {
        use egui::Key::*;
        if matches!(k, Escape | Minus) {
            if !show.back_out() {
                keep = false;
            }
            continue;
        }
        if show.grid {
            if k == G && plain(m) {
                show.grid = false;
            }
            continue;
        }
        match k {
            ArrowRight | ArrowDown | Space | PageDown | N => {
                if show.ended {
                    keep = false;
                } else {
                    actions.push(show.state.next(&doc));
                }
            }
            Enter => {
                if let Ok(n) = show.goto.parse::<usize>() {
                    actions.push(show.state.goto(&doc, n.saturating_sub(1)));
                    show.goto.clear();
                } else if show.ended {
                    keep = false;
                } else {
                    actions.push(show.state.next(&doc));
                }
            }
            ArrowLeft | ArrowUp | Backspace | PageUp | P if !(k == P && m.command) => {
                show.ended = false;
                actions.push(show.state.prev(&doc));
            }
            P if m.command => show.pen = !show.pen,
            A if m.command => show.pen = false,
            Home => actions.push(show.state.goto(&doc, 0)),
            End => actions.push(show.state.goto(&doc, doc.slides.len().saturating_sub(1))),
            B | Period => toggle_blank(&mut show, Color32::BLACK),
            W | Comma => toggle_blank(&mut show, Color32::WHITE),
            E => show.strokes.clear(),
            G if plain(m) => show.grid = true,
            Z if plain(m) => show.zoom_key(),
            T if plain(m) => show.timer.toggle(now),
            R if plain(m) => show.timer.reset(now),
            OpenBracket if plain(m) => show.nudge_notes(-NOTES_STEP),
            CloseBracket if plain(m) => show.nudge_notes(NOTES_STEP),
            Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9 => {
                show.goto.push_str(k.name());
            }
            _ => {}
        }
    }
    let on_slide =
        !show.grid && !show.pen && !show.ended && show.blank.is_none() && show.zoom.is_none() && !show.zoom_arm && !show.reading_bar_hit(&ctx);
    let gif_click = if clicked && on_slide { gif_hit(&ctx, ui, &show, &doc, app) } else { None };
    let media_click = if clicked && on_slide && gif_click.is_none() { media_hit(&ctx, ui, &show, &doc) } else { None };
    let link = if on_slide && gif_click.is_none() && media_click.is_none() { link_hit(&ctx, ui, &mut show, &doc) } else { None };
    if let Some(id) = gif_click {
        let _ = app.run("media.gifPlay", json!({"id": id}));
    } else if let Some((id, clip)) = media_click {
        app.media.toggle(&doc, id, &clip, crate::media::Owner::Show);
    } else if clicked && let Some(l) = &link {
        let last = show.last_viewed;
        match show.state.follow(&doc, &l.action, last) {
            LinkJump::Show(a) => actions.push(a),
            LinkJump::Open(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            LinkJump::Exit => keep = false,
            LinkJump::None => {}
        }
    } else if show.grid && clicked {
        let aspect = slide_aspect(&doc);
        if let Some(p) = pointer
            && let Some(n) = see_all_hit(full, grid_indices(doc.slides.len()).len(), aspect, p)
            && let Some(&i) = grid_indices(doc.slides.len()).get(n)
        {
            show.grid = false;
            show.blank = None;
            show.zoom = None;
            show.zoom_arm = false;
            actions.push(show.state.goto(&doc, i));
        }
    } else if clicked && show.zoom_arm && !show.reading_bar_hit(&ctx) {
        if let Some(p) = pointer
            && let Some((nx, ny)) = norm_on_slide(srect, unit_uv(), p)
        {
            show.zoom = Some(Zoom::at(nx, ny));
            show.zoom_arm = false;
        }
    } else if clicked && show.zoom.is_some() && !show.pen && !show.reading_bar_hit(&ctx) {
        show.dismiss_zoom();
    } else if clicked && !show.pen && !show.reading_bar_hit(&ctx) {
        if show.ended {
            keep = false;
        } else if show.blank.is_some() {
            show.blank = None;
        } else {
            actions.push(show.state.next(&doc));
        }
    }
    if secondary && !show.ended && !show.grid && show.zoom.is_none() && !show.zoom_arm {
        actions.push(show.state.prev(&doc));
    }
    // Apply input first: a step started by this click must restart the clock before it is read.
    for a in actions.drain(..) {
        if !show.apply(a, &doc, now) {
            keep = false;
        }
    }
    // Step clock.
    let (step, playing) = (show.state.step, show.state.playing);
    let t_step = if playing { now - show.step_start } else { 0.0 };
    if playing {
        let dur = show.timeline(&doc).step_duration(step);
        if t_step >= dur {
            actions.push(show.state.step_done(&doc));
            show.step_end = now;
        }
    } else if show.trans.is_none() {
        // Auto-advance (After: timings) or autoplay preview.
        let idle = now - show.step_end.max(show.slide_start);
        let cur_step = show.state.step;
        let next_auto = show.timeline(&doc).step_is_auto(cur_step);
        let n_steps = show.timeline(&doc).steps();
        let step_due = cur_step < n_steps && ((show.autoplay && idle > 0.3) || next_auto);
        let holding = show.grid || show.zoom.is_some() || show.zoom_arm;
        if !holding && (step_due || (!show.rehearse && show.state.auto_advance_due(&doc, idle) && !show.ended)) {
            actions.push(show.state.next(&doc));
        } else if show.preview_only && cur_step >= n_steps && idle > 0.8 {
            keep = false;
        }
    }
    for a in actions {
        if !show.apply(a, &doc, now) {
            keep = false;
        }
    }
    show_media(app, &mut show, &doc);
    // Draw.
    let painter = ui.painter_at(full);
    painter.rect_filled(full, CornerRadius::ZERO, Color32::BLACK);
    let ppp = ctx.pixels_per_point();
    let size = ((srect.width() * ppp).round().max(1.0) as u32, (srect.height() * ppp).round().max(1.0) as u32);
    if show.ended {
        painter.text(
            pos2(full.center().x, full.min.y + 40.0),
            Align2::CENTER_CENTER,
            "End of slide show, click to exit.",
            theme::font(18.0),
            Color32::from_gray(200),
        );
    } else {
        let idx = show.state.slide;
        let step = show.state.step;
        let animating = show.state.playing;
        let t = if animating { now - show.step_start } else { 0.0 };
        let media = crate::media::slide_media(&doc, idx);
        let hidden: Vec<ShapeId> = media
            .iter()
            .filter(|(id, _, clip)| {
                clip.hide_while_not_playing && !app.media.status(*id).is_some_and(|s| s.state != deckcraft_media::PlayState::Ended)
            })
            .map(|m| m.0)
            .collect();
        let gifs = app.media.gifs(&doc, idx, now, &app.session.gif_paused);
        let times: Vec<_> = gifs.iter().map(|g| (g.id, g.t)).collect();
        crate::media::gif_repaint(&ctx, &gifs);
        let gif = crate::media::gif_key(&gifs);
        let key = Key { slide: idx, step, size, doc: std::sync::Arc::as_ptr(&doc) as usize, hidden: hidden.clone(), gif };
        let tex = if !animating && show.tex.as_ref().is_some_and(|(k, _)| *k == key) {
            show.tex.as_ref().map(|(_, t)| t.clone())
        } else {
            let tl = show.timeline(&doc).clone();
            let hide = hidden.clone();
            let f = move |id: ShapeId| {
                let mut st = state_for(&tl, id, step, t);
                if hide.contains(&id) {
                    st.visible = false;
                }
                st
            };
            let threads =
                if cfg!(target_arch = "wasm32") { 0 } else { std::thread::available_parallelism().map(|n| n.get().min(8) as u16).unwrap_or(0) };
            let img = deckcraft_render::render_slide(
                &doc,
                idx,
                &RenderOpts {
                    scale: size.0 as f64 / doc.slide_size.width.max(1.0),
                    size: Some(size),
                    state: Some(&f),
                    gif_times: &times,
                    threads,
                    ..Default::default()
                },
            );
            let ci = crate::textures::to_color_image(&img, false);
            let tex = match show.tex.take() {
                Some((_, mut h)) => {
                    h.set(ci, egui::TextureOptions::LINEAR);
                    h
                }
                None => ctx.load_texture("show", ci, egui::TextureOptions::LINEAR),
            };
            show.tex = Some((key, tex.clone()));
            Some(tex)
        };
        let mut drew = false;
        let morphing = show.trans.as_ref().is_some_and(|tr| tr.kind == "morph" && tr.from.is_some() && now - tr.start < tr.dur);
        let tl = if morphing { Some(show.timeline(&doc).clone()) } else { None };
        let morph = match (&show.trans, tl) {
            (Some(tr), Some(tl)) => {
                let p = ((now - tr.start) / tr.dur.max(0.01)).clamp(0.0, 1.0);
                let hidden_b = |id: ShapeId| {
                    let st = state_for(&tl, id, 0, 0.0);
                    !st.visible || st.opacity <= 0.0 || hidden.contains(&id)
                };
                tr.from
                    .and_then(|i| doc.slides.get(i).map(|a| (i, a)))
                    .zip(doc.slides.get(idx))
                    .map(|((i, a), b)| (i, deckcraft_anim::morph_frame(&doc, a, b, p, &hidden_b)))
            }
            _ => None,
        };
        if let (Some((from, m)), Some(tr)) = (morph, show.trans.as_mut()) {
            let f = |id: ShapeId| ShapeState { opacity: m.opacity_of(id), ..Default::default() };
            let threads =
                if cfg!(target_arch = "wasm32") { 0 } else { std::thread::available_parallelism().map(|n| n.get().min(8) as u16).unwrap_or(0) };
            let opts =
                RenderOpts { scale: size.0 as f64 / doc.slide_size.width.max(1.0), size: Some(size), state: Some(&f), threads, ..Default::default() };
            let text: Vec<deckcraft_render::TextMorph> = m
                .text
                .iter()
                .map(|x| deckcraft_render::TextMorph { old: x.old, new: x.new, from: x.from, to: x.to, chars: x.chars, t: m.mix })
                .collect();
            let paths: Vec<deckcraft_render::PathMorph> = m
                .paths
                .iter()
                .map(|x| deckcraft_render::PathMorph { id: x.id, from: x.from.clone(), from_box: x.from_box, to_box: x.to_box, t: m.mix })
                .collect();
            let img = deckcraft_render::render_blend(&doc, from, idx, m.mix, &m.shapes, &text, &paths, &opts);
            let ci = crate::textures::to_color_image(&img, false);
            let frame = match tr.morph.take() {
                Some(mut h) => {
                    h.set(ci, egui::TextureOptions::LINEAR);
                    h
                }
                None => ctx.load_texture("morph", ci, egui::TextureOptions::LINEAR),
            };
            painter.image(frame.id(), srect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            tr.morph = Some(frame);
            drew = true;
            ctx.request_repaint();
        } else if let (Some(tr), Some(new)) = (show.trans.as_mut(), &tex) {
            let p = ((now - tr.start) / tr.dur.max(0.01)).clamp(0.0, 1.0);
            if p < 1.0 {
                // Slides split into background and content are rendered once per transition;
                // every frame only composites cached textures.
                if tr.parts.is_none() && deckcraft_anim::uses_slide_parts(&tr.kind, &tr.option) {
                    tr.parts = Some(part_textures(&ctx, &doc, tr.from, idx, size));
                }
                let aspect = doc.slide_size.width / doc.slide_size.height.max(1.0);
                let part = |k: usize| tr.parts.as_ref().and_then(|p| p.get(k).cloned().flatten());
                let parts = [part(0), part(1), part(2), part(3)];
                for l in deckcraft_anim::transition_frame(&tr.kind, &tr.option, p, aspect) {
                    let old_or_black = |t: Option<&TextureHandle>| match t {
                        Some(o) => quad_mesh(Some(o.id()), srect, &l, Color32::WHITE),
                        None => quad_mesh(None, srect, &l, Color32::BLACK),
                    };
                    let mesh = match l.source {
                        Source::Old => old_or_black(tr.old.as_ref()),
                        Source::New => quad_mesh(Some(new.id()), srect, &l, Color32::WHITE),
                        Source::Black => quad_mesh(None, srect, &l, Color32::BLACK),
                        Source::White => quad_mesh(None, srect, &l, Color32::WHITE),
                        Source::OldBack => old_or_black(parts[0].as_ref().or(tr.old.as_ref())),
                        Source::NewBack => quad_mesh(Some(parts[2].as_ref().unwrap_or(new).id()), srect, &l, Color32::WHITE),
                        Source::OldContent => match &parts[1] {
                            Some(t) => quad_mesh(Some(t.id()), srect, &l, Color32::WHITE),
                            None => continue,
                        },
                        Source::NewContent => match &parts[3] {
                            Some(t) => quad_mesh(Some(t.id()), srect, &l, Color32::WHITE),
                            None => continue,
                        },
                    };
                    painter.add(egui::Shape::mesh(mesh));
                }
                drew = true;
                ctx.request_repaint();
            }
        }
        if !drew {
            show.trans = None;
            if let Some(tex) = &tex {
                painter.image(tex.id(), srect, show.zoom_uv(), Color32::WHITE);
            }
            // Playing video: the current frame in its box, or full screen.
            let (sw, sh) = (doc.slide_size.width as f32, doc.slide_size.height as f32);
            let uv = show.zoom_uv();
            for (id, x, clip) in &media {
                if !clip.video || hidden.contains(id) {
                    continue;
                }
                let r = if clip.full_screen && app.media.is_playing(*id) {
                    full
                } else {
                    slide_box(srect, uv, sw, sh, x.x as f32, x.y as f32, x.w as f32, x.h as f32)
                };
                crate::media::paint_frame(&painter, &app.media, *id, r);
            }
            // Play/pause on the animated GIF under the pointer.
            if let Some(p) =
                ctx.input(|i| i.pointer.hover_pos()).filter(|_| !show.pen && !show.zoom_arm && show.zoom.is_none() && now - show.last_move < 2.5)
            {
                let k = srect.width() / sw.max(1.0);
                for g in &gifs {
                    let r = on_screen(srect, k, &g.rect);
                    if r.contains(p) {
                        let b = crate::media::gif_button_rect(r);
                        crate::media::paint_gif_button(&painter, b, g.paused, b.contains(p));
                    }
                }
            }
        }
        if animating {
            ctx.request_repaint();
        }
    }
    if let Some(c) = show.blank {
        painter.rect_filled(full, CornerRadius::ZERO, c);
    }
    // Pen.
    let resp = ui.interact(full, ui.id().with("show_pen"), if show.pen { Sense::drag() } else { Sense::hover() });
    if show.pen {
        if resp.drag_started() {
            show.strokes.push(vec![]);
        }
        let uv = show.zoom_uv();
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some((nx, ny)) = norm_on_slide(srect, uv, p)
            && let Some(s) = show.strokes.last_mut()
        {
            s.push(pos2(nx, ny));
        }
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    } else if show.zoom_arm || show.grid {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    } else if link.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if now - show.last_move > 2.5 && !show.reading {
        ctx.set_cursor_icon(egui::CursorIcon::None);
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
    let pen_col = theme::to_color32(doc.show.pen_color);
    let uv = show.zoom_uv();
    for s in &show.strokes {
        let pts: Vec<Pos2> = s.iter().map(|q| slide_to_screen(srect, uv, q.x, q.y)).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(3.0, pen_col)));
    }
    if show.zoom_arm && !show.grid {
        painter.text(
            pos2(full.center().x, full.min.y + 36.0),
            Align2::CENTER_CENTER,
            "Click the slide to zoom in. Esc cancels.",
            theme::font(16.0),
            Color32::WHITE,
        );
    }
    // Reading view bar / on-screen controls.
    if show.reading {
        let bar = Rect::from_min_max(pos2(full.min.x, full.max.y - 34.0), full.max);
        painter.rect_filled(bar, CornerRadius::ZERO, Color32::from_gray(32));
        painter.text(
            pos2(bar.min.x + 14.0, bar.center().y),
            Align2::LEFT_CENTER,
            format!("Slide {} of {}", show.state.slide + 1, doc.slides.len()),
            theme::font(12.0),
            Color32::from_gray(200),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(bar.max.x - 260.0, bar.min.y), bar.max))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        if child.button("Close").clicked() {
            keep = false;
        }
        if child.button("▶").clicked() {
            let a = show.state.next(&doc);
            if !show.apply(a, &doc, now) {
                keep = false;
            }
        }
        if child.button("◀").clicked() {
            let a = show.state.prev(&doc);
            show.apply(a, &doc, now);
        }
    } else if !show.grid && (now - show.last_move < 2.5 || show.zoom_arm) {
        let bar = tool_bar_rect(full);
        painter.rect_filled(bar, CornerRadius::same(17), Color32::from_black_alpha(140));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(10.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let b = |ui: &mut Ui, s: &str| ui.add(egui::Button::new(egui::RichText::new(s).color(Color32::WHITE).size(15.0)).frame(false));
        if b(&mut child, "◀").clicked() {
            let a = show.state.prev(&doc);
            show.apply(a, &doc, now);
        }
        if b(&mut child, "▶").clicked() {
            let a = show.state.next(&doc);
            if !show.apply(a, &doc, now) {
                keep = false;
            }
        }
        if b(&mut child, if show.pen { "✏ on" } else { "✏" }).clicked() {
            show.pen = !show.pen;
        }
        if b(&mut child, "Black").clicked() {
            toggle_blank(&mut show, Color32::BLACK);
        }
        if b(&mut child, "White").clicked() {
            toggle_blank(&mut show, Color32::WHITE);
        }
        if b(&mut child, "All").clicked() {
            show.grid = !show.grid;
        }
        if b(&mut child, show.zoom_label()).clicked() {
            show.zoom_key();
        }
        if b(&mut child, "End").clicked() {
            keep = false;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
    }
    if show.grid && !show.ended {
        paint_grid(&painter, &ctx, &mut show, &doc, full);
    }
    if show.rehearse {
        let t = now - show.slide_start;
        let total = now - show.show_start;
        let r = Rect::from_min_size(pos2(full.min.x + 12.0, full.min.y + 12.0), vec2(220.0, 30.0));
        painter.rect_filled(r, CornerRadius::same(6), Color32::from_black_alpha(160));
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            format!("Slide {:02}:{:04.1}   Total {:02}:{:02}", (t / 60.0) as u32, t % 60.0, (total / 60.0) as u32, (total % 60.0) as u32),
            theme::font(13.0),
            Color32::WHITE,
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    // Presenter view in a second window.
    if show.presenter && !show.reading && !show.ended {
        presenter_window(&ctx, &mut show, &doc, now, &mut keep);
    }
    if keep {
        app.show = Some(show);
    } else {
        app.media.stop_owned(crate::media::Owner::Show);
        if !show.reading {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        if show.rehearse {
            if let Some((_, tl)) = &show.timeline {
                let _ = tl;
            }
            show.timings.push((show.state.slide, now - show.slide_start));
            for (i, secs) in show.timings.clone() {
                let _ = app.run("transition.timing", json!({"index": i, "after": (secs * 1000.0).round() as u64}));
            }
            app.set_status("Slide timings recorded.");
        }
        let last = show.state.slide;
        let _ = app.run("slide.go", json!({"index": last}));
    }
}

/// Start the media of a newly shown slide: media from the previous slide stops unless it plays
/// across slides; media set to start automatically plays. Everything stops at the end.
fn show_media(app: &mut SlideApp, show: &mut Show, doc: &Presentation) {
    let cur = if show.ended { None } else { Some(show.state.slide) };
    if show.media_slide == cur {
        return;
    }
    show.media_slide = cur;
    let keep: Vec<ShapeId> = match cur {
        Some(_) => app.media.ids().into_iter().filter(|id| crate::media::find_clip(doc, *id).is_some_and(|c| c.play_across_slides)).collect(),
        None => vec![],
    };
    for id in app.media.ids() {
        if !keep.contains(&id) {
            app.media.stop(id);
        }
    }
    if let Some(i) = cur {
        for (id, _, clip) in crate::media::slide_media(doc, i) {
            if clip.autoplay && !keep.contains(&id) {
                app.media.play(doc, id, &clip, None, crate::media::Owner::Show);
            }
        }
    }
}

/// The media shape under the pointer on the shown slide.
fn media_hit(ctx: &egui::Context, ui: &Ui, show: &Show, doc: &Presentation) -> Option<(ShapeId, deckcraft_model::MediaClip)> {
    let p = ctx.input(|i| i.pointer.interact_pos())?;
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let k = srect.width() / doc.slide_size.width.max(1.0) as f32;
    crate::media::slide_media(doc, show.state.slide).into_iter().rev().find_map(|(id, x, clip)| {
        let r = Rect::from_min_size(pos2(srect.min.x + x.x as f32 * k, srect.min.y + x.y as f32 * k), vec2(x.w as f32 * k, x.h as f32 * k));
        r.contains(p).then_some((id, clip))
    })
}

/// The link under the pointer: the topmost shape action or text hyperlink on the current slide
/// (a text link wins over its shape's action).
fn link_hit(ctx: &egui::Context, ui: &Ui, show: &mut Show, doc: &Presentation) -> Option<Hyperlink> {
    let p = ctx.input(|i| i.pointer.hover_pos())?;
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let k = (srect.width() / doc.slide_size.width.max(1.0) as f32) as f64;
    let (x, y) = ((p.x - srect.min.x) as f64 / k, (p.y - srect.min.y) as f64 / k);
    let i = show.state.slide;
    if show.links.as_ref().is_none_or(|(s, _)| *s != i) {
        show.links = Some((i, deckcraft_render::place_slide(doc, i).links));
    }
    let (_, links) = show.links.as_ref()?;
    links.iter().rev().find(|l| l.rect.x0 <= x && x <= l.rect.x1 && l.rect.y0 <= y && y <= l.rect.y1).map(|l| l.link.clone())
}

/// A slide-space box on screen, the slide being drawn in `srect` at `k` pixels per point.
fn on_screen(srect: Rect, k: f32, x: &deckcraft_geom::Xfrm) -> Rect {
    Rect::from_min_size(pos2(srect.min.x + x.x as f32 * k, srect.min.y + x.y as f32 * k), vec2(x.w as f32 * k, x.h as f32 * k))
}

/// The animated GIF whose play/pause button is under the pointer on the shown slide.
fn gif_hit(ctx: &egui::Context, ui: &Ui, show: &Show, doc: &Presentation, app: &mut SlideApp) -> Option<ShapeId> {
    let p = ctx.input(|i| i.pointer.interact_pos())?;
    let full = ui.max_rect();
    let avail = if show.reading { Rect::from_min_max(full.min, pos2(full.max.x, full.max.y - 34.0)) } else { full };
    let srect = fit(avail, doc);
    let k = srect.width() / doc.slide_size.width.max(1.0) as f32;
    let gifs = app.media.gifs(doc, show.state.slide, ctx.input(|i| i.time), &app.session.gif_paused);
    gifs.iter().rev().find(|g| crate::media::gif_button_rect(on_screen(srect, k, &g.rect)).contains(p)).map(|g| g.id)
}

impl Show {
    fn zoom_uv(&self) -> Rect {
        self.zoom.map(Zoom::uv).unwrap_or_else(unit_uv)
    }

    fn zoom_label(&self) -> &'static str {
        if self.zoom.is_some() {
            "Zoom on"
        } else if self.zoom_arm {
            "Zoom…"
        } else {
            "Zoom"
        }
    }

    /// Z, or the Zoom button: arm a click-to-zoom, or leave a zoom that is already on.
    fn zoom_key(&mut self) {
        if self.zoom.is_some() || self.zoom_arm {
            self.dismiss_zoom();
        } else {
            self.zoom_arm = true;
            self.pen = false;
        }
    }

    /// Esc closes See All Slides, then a zoom, and only then leaves the show.
    fn back_out(&mut self) -> bool {
        if self.grid {
            self.grid = false;
            true
        } else {
            self.dismiss_zoom()
        }
    }

    /// Esc backs out of a zoom before it leaves the show. True when it did.
    fn dismiss_zoom(&mut self) -> bool {
        if self.zoom.is_some() || self.zoom_arm {
            self.zoom = None;
            self.zoom_arm = false;
            true
        } else {
            false
        }
    }

    pub fn blank_name(&self) -> Option<&'static str> {
        match self.blank {
            Some(c) if c == Color32::BLACK => Some("black"),
            Some(c) if c == Color32::WHITE => Some("white"),
            Some(_) => Some("blank"),
            None => None,
        }
    }

    pub fn zoom_view(&self) -> Option<(f32, f32, f32)> {
        self.zoom.map(|z| (z.nx, z.ny, z.scale))
    }

    fn nudge_notes(&mut self, delta: f32) {
        let next = self.notes_px + delta;
        if next.is_finite() {
            self.notes_px = next.clamp(NOTES_MIN, NOTES_MAX);
        }
    }

    pub fn showing_value(&self, n_slides: usize, aspect: f32, full: Rect, now: f64) -> serde_json::Value {
        let mut v = json!({
            "slide": self.state.slide,
            "step": self.state.step,
            "ended": self.ended,
            "grid": self.grid,
            "zoomArm": self.zoom_arm,
            "blank": self.blank_name(),
            "zoom": self.zoom_view().map(|(x, y, scale)| json!({"x": x, "y": y, "scale": scale})),
            "presenterPaused": self.timer.paused(),
            "presenterSecs": self.timer.elapsed(now),
            "notesPx": self.notes_px,
        });
        if self.grid {
            let slides = grid_indices(n_slides);
            let cells = see_all_cells(full, slides.len(), aspect);
            v["cells"] =
                serde_json::Value::Array(slides.iter().zip(cells).map(|(i, r)| json!({"slide": i, "x": r.center().x, "y": r.center().y})).collect());
        }
        v
    }

    fn reading_bar_hit(&self, ctx: &egui::Context) -> bool {
        // Clicks on the on-screen controls must not also advance.
        ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
            let r = ctx.content_rect();
            (self.reading && p.y > r.max.y - 34.0) || (!self.reading && tool_bar_rect(r).expand(8.0).contains(p))
        })
    }
}

const ZOOM_SCALE: f32 = 2.0;
const GRID_CAP: usize = 48;
const TOOL_BAR_W: f32 = 520.0;
const NOTES_DEFAULT: f32 = 18.0;
const NOTES_MIN: f32 = 12.0;
const NOTES_MAX: f32 = 48.0;
const NOTES_STEP: f32 = 4.0;

/// Presenter-view elapsed time. Pausing freezes the number; the show keeps playing.
#[derive(Clone, Copy)]
struct PresenterTimer {
    origin: f64,
    frozen: f64,
    paused: bool,
}

impl PresenterTimer {
    fn start(now: f64) -> Self {
        PresenterTimer { origin: finite_time(now), frozen: 0.0, paused: false }
    }

    fn elapsed(self, now: f64) -> f64 {
        if self.paused {
            return finite_secs(self.frozen);
        }
        let dt = now - self.origin;
        finite_secs(self.frozen + if dt.is_finite() && dt > 0.0 { dt } else { 0.0 })
    }

    fn paused(self) -> bool {
        self.paused
    }

    fn toggle(&mut self, now: f64) {
        if self.paused {
            self.origin = finite_time(now);
            self.paused = false;
        } else {
            self.frozen = self.elapsed(now);
            self.paused = true;
        }
    }

    /// Zero the elapsed time. A paused timer stays paused at zero.
    fn reset(&mut self, now: f64) {
        self.frozen = 0.0;
        self.origin = finite_time(now);
    }
}

fn finite_time(now: f64) -> f64 {
    if now.is_finite() { now } else { 0.0 }
}

fn finite_secs(n: f64) -> f64 {
    if n.is_finite() && n >= 0.0 { n } else { 0.0 }
}

fn format_elapsed(secs: f64) -> String {
    let s = finite_secs(secs) as u64;
    let s = s.min(99 * 3600 + 59 * 60 + 59);
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Local wall clock for presenter view. `--:--` where there is no clock (wasm).
fn wall_clock() -> String {
    match deckcraft_text::datetime::DateTime::now_local() {
        Some(t) if t.hour < 24 && t.minute < 60 => format!("{:02}:{:02}", t.hour, t.minute),
        _ => "--:--".to_string(),
    }
}

#[derive(Clone, Copy)]
struct Zoom {
    nx: f32,
    ny: f32,
    scale: f32,
}

impl Zoom {
    fn at(nx: f32, ny: f32) -> Self {
        Zoom { nx: finite_unit(nx), ny: finite_unit(ny), scale: ZOOM_SCALE }
    }

    fn uv(self) -> Rect {
        zoom_uv(self.nx, self.ny, self.scale)
    }
}

fn finite_unit(n: f32) -> f32 {
    if n.is_finite() { n.clamp(0.0, 1.0) } else { 0.5 }
}

fn plain(m: egui::Modifiers) -> bool {
    !m.command && !m.ctrl && !m.alt && !m.shift
}

fn unit_uv() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

fn slide_aspect(doc: &Presentation) -> f32 {
    let a = doc.slide_size.width as f32 / doc.slide_size.height.max(1.0) as f32;
    if a.is_finite() && a > 0.05 { a } else { 16.0 / 9.0 }
}

/// The part of the slide a zoom shows, in 0–1 texture space. The window stays on the slide.
fn zoom_uv(nx: f32, ny: f32, scale: f32) -> Rect {
    let scale = if scale.is_finite() && scale > 1.0 { scale } else { ZOOM_SCALE };
    let w = 1.0 / scale;
    if w >= 1.0 {
        return unit_uv();
    }
    let x = (nx - w * 0.5).clamp(0.0, 1.0 - w);
    let y = (ny - w * 0.5).clamp(0.0, 1.0 - w);
    Rect::from_min_size(pos2(x, y), vec2(w, w))
}

fn zoom_frame(slide: Rect, uv: Rect) -> Rect {
    Rect::from_min_size(
        pos2(slide.min.x + uv.min.x * slide.width(), slide.min.y + uv.min.y * slide.height()),
        vec2(uv.width() * slide.width(), uv.height() * slide.height()),
    )
}

fn toggle_blank(show: &mut Show, color: Color32) {
    show.blank = if show.blank == Some(color) { None } else { Some(color) };
}

fn tool_bar_rect(full: Rect) -> Rect {
    let w = TOOL_BAR_W.min((full.width() - 32.0).max(32.0));
    Rect::from_min_size(pos2(full.min.x + 16.0, full.max.y - 50.0), vec2(w, 34.0))
}

fn grid_indices(n: usize) -> Vec<usize> {
    (0..n.min(GRID_CAP)).collect()
}

/// Thumbnail rectangles for `n` slides inside `full`, largest 16:9-style thumbs that fit.
fn see_all_cells(full: Rect, n: usize, aspect: f32) -> Vec<Rect> {
    if n == 0 || full.width() < 16.0 || full.height() < 16.0 {
        return vec![];
    }
    let aspect = if aspect.is_finite() && aspect > 0.05 { aspect } else { 16.0 / 9.0 };
    let margin = 28.0;
    let gap = 16.0;
    let label_h = 22.0;
    let inner = full.shrink(margin);
    let inner = Rect::from_min_max(pos2(inner.min.x, inner.min.y + 36.0), inner.max);
    if inner.width() < 8.0 || inner.height() < 8.0 {
        return vec![];
    }
    let mut best: Vec<Rect> = vec![];
    let mut best_area = 0.0_f32;
    for cols in 1..=n.min(8) {
        let rows = n.div_ceil(cols);
        let cell_w = (inner.width() - gap * (cols.saturating_sub(1) as f32)) / cols as f32;
        let cell_h = (inner.height() - gap * (rows.saturating_sub(1) as f32)) / rows as f32;
        if cell_w < 8.0 || cell_h < label_h + 8.0 {
            continue;
        }
        let thumb_max_h = cell_h - label_h;
        let mut tw = cell_w;
        let mut th = tw / aspect;
        if th > thumb_max_h {
            th = thumb_max_h;
            tw = th * aspect;
        }
        if tw < 8.0 || th < 8.0 {
            continue;
        }
        let area = tw * th;
        if area <= best_area {
            continue;
        }
        best_area = area;
        best = (0..n)
            .map(|i| {
                let c = i % cols;
                let r = i / cols;
                let ox = inner.min.x + c as f32 * (cell_w + gap) + (cell_w - tw) * 0.5;
                let oy = inner.min.y + r as f32 * (cell_h + gap);
                Rect::from_min_size(pos2(ox, oy), vec2(tw, th))
            })
            .collect();
    }
    best
}

fn see_all_hit(full: Rect, n: usize, aspect: f32, p: Pos2) -> Option<usize> {
    see_all_cells(full, n, aspect).into_iter().position(|r| r.expand2(vec2(4.0, 18.0)).contains(p))
}

fn norm_on_slide(srect: Rect, uv: Rect, p: Pos2) -> Option<(f32, f32)> {
    if srect.width() <= 1.0 || srect.height() <= 1.0 || !srect.contains(p) {
        return None;
    }
    let u = (p.x - srect.min.x) / srect.width();
    let v = (p.y - srect.min.y) / srect.height();
    let nx = uv.min.x + u * uv.width();
    let ny = uv.min.y + v * uv.height();
    (nx.is_finite() && ny.is_finite()).then_some((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)))
}

fn slide_to_screen(srect: Rect, uv: Rect, nx: f32, ny: f32) -> Pos2 {
    let u = (nx - uv.min.x) / uv.width().max(1e-4);
    let v = (ny - uv.min.y) / uv.height().max(1e-4);
    pos2(srect.min.x + u * srect.width(), srect.min.y + v * srect.height())
}

fn slide_box(srect: Rect, uv: Rect, slide_w: f32, slide_h: f32, x: f32, y: f32, w: f32, h: f32) -> Rect {
    let a = slide_to_screen(srect, uv, x / slide_w.max(1.0), y / slide_h.max(1.0));
    let b = slide_to_screen(srect, uv, (x + w) / slide_w.max(1.0), (y + h) / slide_h.max(1.0));
    Rect::from_min_max(a, b)
}

fn paint_grid(painter: &egui::Painter, ctx: &egui::Context, show: &mut Show, doc: &Presentation, full: Rect) {
    let slides = grid_indices(doc.slides.len());
    let aspect = slide_aspect(doc);
    let cells = see_all_cells(full, slides.len(), aspect);
    painter.rect_filled(full, CornerRadius::ZERO, Color32::from_gray(16));
    painter.text(pos2(full.center().x, full.min.y + 28.0), Align2::CENTER_CENTER, "All slides", theme::font(20.0), Color32::WHITE);
    if doc.slides.len() > GRID_CAP {
        painter.text(
            pos2(full.center().x, full.min.y + 50.0),
            Align2::CENTER_CENTER,
            format!("First {GRID_CAP} slides"),
            theme::font(13.0),
            Color32::from_gray(180),
        );
    }
    let budget = if slides.len() <= 12 { 12 } else { 4 };
    let mut made = 0;
    for (i, rect) in slides.iter().copied().zip(cells) {
        painter.rect_filled(rect, CornerRadius::same(4), Color32::from_gray(32));
        let tex = match show.thumbs.iter().find(|(k, _)| *k == i).map(|(_, t)| t.clone()) {
            Some(t) => Some(t),
            None if made < budget => {
                made += 1;
                render_thumb(ctx, show, doc, i)
            }
            None => {
                ctx.request_repaint();
                None
            }
        };
        if let Some(t) = tex {
            painter.image(t.id(), rect, unit_uv(), Color32::WHITE);
        }
        let current = i == show.state.slide;
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(if current { 3.0 } else { 1.0 }, if current { Color32::WHITE } else { Color32::from_gray(70) }),
            egui::StrokeKind::Inside,
        );
        let hidden = doc.slides.get(i).is_some_and(|s| s.hidden);
        let label = if hidden { format!("{}  Hidden", i + 1) } else { format!("{}", i + 1) };
        painter.text(
            pos2(rect.center().x, rect.max.y + 12.0),
            Align2::CENTER_CENTER,
            label,
            theme::font(13.0),
            if current { Color32::WHITE } else { Color32::from_gray(180) },
        );
    }
}

fn render_thumb(ctx: &egui::Context, show: &mut Show, doc: &Presentation, i: usize) -> Option<TextureHandle> {
    if show.thumbs.len() >= GRID_CAP {
        return None;
    }
    let w = 320.0_f64;
    let h = (w * doc.slide_size.height / doc.slide_size.width.max(1.0)).round().max(1.0);
    let img = deckcraft_render::render_slide(
        doc,
        i,
        &RenderOpts { scale: w / doc.slide_size.width.max(1.0), size: Some((w as u32, h as u32)), ..Default::default() },
    );
    let tex = ctx.load_texture(format!("show-grid-{i}"), crate::textures::to_color_image(&img, false), egui::TextureOptions::LINEAR);
    show.thumbs.push((i, tex.clone()));
    Some(tex)
}

fn presenter_window(ctx: &egui::Context, show: &mut Show, doc: &Presentation, now: f64, keep: &mut bool) {
    let cur = show.state.slide;
    let next = deckcraft_anim::show_order(doc).into_iter().find(|i| *i > cur);
    let cur_tex = show.tex.as_ref().map(|(_, t)| t.clone());
    if let Some(n) = next
        && show.next_tex.as_ref().is_none_or(|(k, _)| *k != n)
    {
        let img = deckcraft_render::render_slide(
            doc,
            n,
            &RenderOpts {
                scale: 480.0 / doc.slide_size.width.max(1.0),
                size: Some((480, (480.0 * doc.slide_size.height / doc.slide_size.width.max(1.0)) as u32)),
                ..Default::default()
            },
        );
        show.next_tex = Some((n, ctx.load_texture("presenter-next", crate::textures::to_color_image(&img, false), egui::TextureOptions::LINEAR)));
    }
    let next_tex = show.next_tex.as_ref().filter(|(k, _)| Some(*k) == next).map(|(_, t)| t.clone());
    let notes = doc.slides.get(cur).map(|s| s.notes_text()).unwrap_or_default();
    let clock = wall_clock();
    // The presenter clock only moves when this window is drawn again.
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
    let mut actions = vec![];
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("deckcraft_presenter"),
        egui::ViewportBuilder::default().with_title("Presenter View — DeckCraft").with_inner_size([1100.0, 700.0]),
        |ui, _| {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            let elapsed = show.timer.elapsed(now);
            let paused = show.timer.paused();
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_gray(24)).inner_margin(egui::Margin::same(16))).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format_elapsed(elapsed)).size(22.0).color(if paused {
                        Color32::from_gray(160)
                    } else {
                        Color32::WHITE
                    }));
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(clock.clone()).size(16.0).color(Color32::from_gray(200)));
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new(format!("Slide {} of {}", cur + 1, doc.slides.len())).size(16.0).color(Color32::from_gray(200)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("End Show").clicked() {
                            *keep = false;
                        }
                        if ui.button("Next ▶").clicked() {
                            actions.push(show.state.next(doc));
                        }
                        if ui.button("◀ Previous").clicked() {
                            actions.push(show.state.prev(doc));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button(if paused { "Resume" } else { "Pause" }).clicked() {
                        show.timer.toggle(now);
                    }
                    if ui.button("Reset").clicked() {
                        show.timer.reset(now);
                    }
                    if ui.button(if show.blank == Some(Color32::BLACK) { "Black on" } else { "Black" }).clicked() {
                        toggle_blank(show, Color32::BLACK);
                    }
                    if ui.button(if show.blank == Some(Color32::WHITE) { "White on" } else { "White" }).clicked() {
                        toggle_blank(show, Color32::WHITE);
                    }
                    if ui.button(if show.grid { "All slides on" } else { "All slides" }).clicked() {
                        show.grid = !show.grid;
                    }
                    if ui.button(show.zoom_label()).clicked() {
                        show.zoom_key();
                    }
                });
                ui.add_space(8.0);
                let avail = ui.available_rect_before_wrap();
                let left = Rect::from_min_max(avail.min, pos2(avail.min.x + avail.width() * 0.62, avail.max.y));
                let right = Rect::from_min_max(pos2(left.max.x + 16.0, avail.min.y), avail.max);
                let cr = fit(Rect::from_min_max(left.min, pos2(left.max.x, left.min.y + left.height() * 0.8)), doc);
                let cr = Rect::from_min_size(left.min, cr.size());
                if let Some(t) = &cur_tex {
                    ui.painter().image(t.id(), cr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                if let Some(z) = show.zoom {
                    let mark = zoom_frame(cr, z.uv());
                    ui.painter().rect_stroke(mark, CornerRadius::ZERO, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
                }
                if show.zoom_arm {
                    let hit = ui.interact(cr, ui.id().with("presenter_zoom"), Sense::click());
                    if hit.clicked()
                        && let Some(p) = hit.interact_pointer_pos()
                        && cr.width() > 1.0
                        && cr.height() > 1.0
                    {
                        show.zoom = Some(Zoom::at((p.x - cr.min.x) / cr.width(), (p.y - cr.min.y) / cr.height()));
                        show.zoom_arm = false;
                    }
                }
                if let Some(c) = show.blank {
                    ui.painter().rect_filled(cr, CornerRadius::ZERO, c);
                }
                let nr = Rect::from_min_size(
                    right.min + vec2(0.0, 22.0),
                    vec2(right.width(), right.width() * (doc.slide_size.height / doc.slide_size.width.max(1.0)) as f32),
                );
                ui.painter().text(right.min, Align2::LEFT_TOP, "Next", theme::font(13.0), Color32::from_gray(170));
                match &next_tex {
                    Some(t) => {
                        ui.painter().image(t.id(), nr, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        ui.painter().rect_filled(nr, CornerRadius::ZERO, Color32::from_gray(40));
                        ui.painter().text(nr.center(), Align2::CENTER_CENTER, "End of slide show", theme::font(13.0), Color32::from_gray(170));
                    }
                }
                let notes_head = Rect::from_min_size(pos2(right.min.x, nr.max.y + 12.0), vec2(right.width(), 28.0));
                let mut head = ui.new_child(egui::UiBuilder::new().max_rect(notes_head));
                head.horizontal(|ui| {
                    ui.label(egui::RichText::new("Notes").size(13.0).color(Color32::from_gray(170)));
                    if ui.button("A-").clicked() {
                        show.nudge_notes(-NOTES_STEP);
                    }
                    ui.label(egui::RichText::new(format!("{}", show.notes_px as i32)).size(13.0).color(Color32::from_gray(200)));
                    if ui.button("A+").clicked() {
                        show.nudge_notes(NOTES_STEP);
                    }
                });
                let notes_rect = Rect::from_min_max(pos2(right.min.x, notes_head.max.y + 4.0), right.max);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(notes_rect));
                egui::ScrollArea::vertical().show(&mut child, |ui| {
                    ui.label(
                        egui::RichText::new(if notes.is_empty() { "No notes." } else { &notes }).size(show.notes_px).color(Color32::from_gray(230)),
                    );
                });
            });
            if ui.input(|i| i.viewport().close_requested()) {
                show.presenter = false;
            }
        },
    );
    for a in actions {
        if !show.apply(a, doc, now) {
            *keep = false;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn a_transition_keeps_the_old_slide_in_its_own_texture() {
        // #33: the old slide's handle was cloned, so the new slide was rendered into the same
        // texture and every transition blended the new slide with itself.
        let mut s = deckcraft_engine::Session::with_new();
        s.execute("slide.new", &json!({})).unwrap();
        s.execute("transition.set", &json!({"kind": "fade", "index": 1})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        let ctx = egui::Context::default();
        let red = ctx.load_texture("show", egui::ColorImage::filled([1, 1], Color32::RED), egui::TextureOptions::LINEAR);
        let mut show = Show::new(&doc, 0, false, false);
        show.tex = Some((Key { slide: 0, step: 0, size: (1, 1), doc: 0, hidden: vec![], gif: 0 }, red.clone()));
        assert!(show.apply(ShowAction::Slide(1), &doc, 0.0));
        let tr = show.trans.as_ref().expect("the fade plays");
        assert_eq!(tr.old.as_ref().map(TextureHandle::id), Some(red.id()));
        assert_eq!(tr.from, Some(0));
        assert!(show.tex.is_none(), "the new slide gets a texture of its own instead of overwriting the old one");
    }

    #[test]
    fn zoom_stays_on_the_slide_and_the_grid_hits_a_thumbnail() {
        let uv = zoom_uv(0.5, 0.5, 2.0);
        assert!((uv.min.x - 0.25).abs() < 1e-4 && (uv.min.y - 0.25).abs() < 1e-4);
        assert!((uv.width() - 0.5).abs() < 1e-4);
        let corner = zoom_uv(0.0, 0.0, 2.0);
        assert_eq!(corner.min, pos2(0.0, 0.0));
        assert!(corner.max.x <= 1.0 && corner.max.y <= 1.0);
        let full = Rect::from_min_size(Pos2::ZERO, vec2(1728.0, 1084.0));
        let cells = see_all_cells(full, 7, 16.0 / 9.0);
        assert_eq!(cells.len(), 7);
        for (i, a) in cells.iter().enumerate() {
            assert!(full.contains_rect(*a), "cell {i} leaves the window");
            for b in cells.iter().skip(i + 1) {
                let overlap = a.intersect(*b);
                assert!(overlap.width() < 1.0 || overlap.height() < 1.0, "thumbnails overlap {a:?} {b:?}");
            }
        }
        let hit = see_all_hit(full, 7, 16.0 / 9.0, cells[3].center());
        assert_eq!(hit, Some(3));
        assert_eq!(see_all_hit(full, 7, 16.0 / 9.0, pos2(2.0, 2.0)), None);
        let srect = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0));
        assert_eq!(norm_on_slide(srect, unit_uv(), pos2(110.0, 70.0)), Some((0.5, 0.5)));
        let zoomed = zoom_uv(0.5, 0.5, 2.0);
        let back = norm_on_slide(srect, zoomed, pos2(110.0, 70.0)).expect("on the slide");
        assert!((back.0 - 0.5).abs() < 1e-3 && (back.1 - 0.5).abs() < 1e-3);
    }

    #[test]
    fn escape_leaves_the_grid_then_the_zoom_and_blank_toggles() {
        let mut s = deckcraft_engine::Session::with_new();
        s.execute("slide.new", &json!({})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        let mut show = Show::new(&doc, 0, false, true);
        show.grid = true;
        show.zoom = Some(Zoom::at(0.2, 0.3));
        assert!(show.back_out());
        assert!(!show.grid);
        assert!(show.zoom.is_some(), "closing the grid leaves the zoom");
        assert!(show.back_out());
        assert!(show.zoom.is_none());
        assert!(!show.back_out());
        show.zoom_key();
        assert!(show.zoom_arm);
        show.zoom_key();
        assert!(!show.zoom_arm);
        toggle_blank(&mut show, Color32::BLACK);
        assert_eq!(show.blank_name(), Some("black"));
        toggle_blank(&mut show, Color32::WHITE);
        assert_eq!(show.blank_name(), Some("white"));
        toggle_blank(&mut show, Color32::WHITE);
        assert_eq!(show.blank_name(), None);
        show.zoom = Some(Zoom::at(0.4, 0.6));
        assert!(show.apply(ShowAction::Slide(1), &doc, 1.0));
        assert!(show.zoom.is_none(), "a new slide is shown whole");
    }

    #[test]
    fn presenter_timer_pauses_resets_and_notes_size_clamps() {
        let mut t = PresenterTimer::start(10.0);
        assert!((t.elapsed(12.5) - 2.5).abs() < 1e-9);
        t.toggle(15.0);
        assert!(t.paused());
        assert!((t.elapsed(100.0) - 5.0).abs() < 1e-9);
        t.reset(100.0);
        assert!(t.paused());
        assert_eq!(t.elapsed(200.0), 0.0);
        t.toggle(200.0);
        assert!(!t.paused());
        assert!((t.elapsed(203.0) - 3.0).abs() < 1e-9);
        t.reset(203.0);
        assert!(!t.paused());
        assert!((t.elapsed(204.0) - 1.0).abs() < 1e-9);
        let bad = PresenterTimer::start(f64::NAN);
        assert_eq!(bad.elapsed(f64::INFINITY), 0.0);
        assert_eq!(format_elapsed(3661.4), "01:01:01");
        assert_eq!(format_elapsed(-1.0), "00:00:00");
        assert_eq!(format_elapsed(f64::NAN), "00:00:00");
        let clock = wall_clock();
        assert!(clock == "--:--" || (clock.len() == 5 && clock.as_bytes().get(2) == Some(&b':')));

        let mut s = deckcraft_engine::Session::with_new();
        s.execute("slide.new", &json!({})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        let mut show = Show::new(&doc, 0, false, true);
        assert_eq!(show.notes_px, NOTES_DEFAULT);
        show.nudge_notes(NOTES_STEP);
        assert_eq!(show.notes_px, 22.0);
        show.nudge_notes(1000.0);
        assert_eq!(show.notes_px, NOTES_MAX);
        show.nudge_notes(-1000.0);
        assert_eq!(show.notes_px, NOTES_MIN);
        show.nudge_notes(f32::NAN);
        assert_eq!(show.notes_px, NOTES_MIN);
        let v = show.showing_value(1, 16.0 / 9.0, Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0)), 0.0);
        assert_eq!(v["presenterPaused"], json!(false));
        assert_eq!(v["notesPx"], json!(NOTES_MIN));
    }
}
