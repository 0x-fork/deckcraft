//! Office 2016 chart layouts the bar renderer can't draw: funnel, treemap and sunburst
//! (waterfall bars are drawn by [`crate::chart`]).

use deckcraft_color::Rgba;
use deckcraft_model::chart::{Chart, ChartType};
use deckcraft_model::resolve::Ctx;
use deckcraft_model::text::{Align, Anchor};
use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use vello_cpu::RenderContext;

use crate::chart::{label, series_color};
use crate::color;

/// Draw `c` inside `area` (the space left by the title and legend).
pub fn draw(ctx: &mut RenderContext, rctx: &Ctx, c: &Chart, m: Affine, area: Rect, base: f64, text_col: Rgba) {
    let Some(s) = c.series.first() else { return };
    let vals: Vec<f64> = s.values.iter().map(|v| v.unwrap_or(0.0).max(0.0)).collect();
    if vals.iter().all(|v| *v <= 0.0) {
        return;
    }
    let value_label = |v: f64| c.data_label(s, v);
    let labels = c.data_labels || c.kind == ChartType::Funnel;
    match c.kind {
        ChartType::Funnel => funnel(ctx, rctx, c, m, area, base, text_col, &vals, &value_label),
        ChartType::Treemap => treemap(ctx, rctx, c, m, area, base, &vals, labels.then_some(&value_label)),
        _ => sunburst(ctx, rctx, c, m, area, base, &vals),
    }
}

/// Bars centred on one axis, as wide as their value, widest first as given: category labels on
/// the left, values inside.
#[allow(clippy::too_many_arguments)]
fn funnel(
    ctx: &mut RenderContext,
    rctx: &Ctx,
    c: &Chart,
    m: Affine,
    area: Rect,
    base: f64,
    text_col: Rgba,
    vals: &[f64],
    value_label: &dyn Fn(f64) -> String,
) {
    let n = vals.len().max(1);
    let gutter = c.categories.iter().map(|n| n.chars().count()).max().unwrap_or(0) as f64 * base * 0.55 + base;
    let plot = Rect::new(area.x0 + gutter.min(area.width() * 0.4), area.y0, area.x1, area.y1);
    let max = vals.iter().copied().fold(0.0, f64::max).max(1e-9);
    let band = plot.height() / n as f64;
    let cx = plot.center().x;
    for (i, v) in vals.iter().enumerate() {
        let w = v / max * plot.width();
        let y = plot.y0 + band * i as f64;
        let r = Rect::new(cx - w / 2.0, y + band * 0.05, cx + w / 2.0, y + band * 0.95);
        ctx.set_transform(m);
        ctx.set_paint(color(series_color(rctx, c, 0), 1.0));
        ctx.fill_rect(&r);
        if let Some(name) = c.categories.get(i) {
            label(ctx, rctx, m, name, Rect::new(area.x0, y, plot.x0 - base * 0.4, y + band), base, Align::Right, Anchor::Middle, text_col, false);
        }
        label(ctx, rctx, m, &value_label(*v), Rect::new(cx - 60.0, y, cx + 60.0, y + band), base, Align::Center, Anchor::Middle, Rgba::WHITE, false);
    }
}

/// Squarified treemap (Bruls, Huizing, van Wijk): rows of tiles laid along the short side,
/// each row kept while it improves the worst aspect ratio.
pub fn squarify(vals: &[f64], r: Rect) -> Vec<Rect> {
    let total: f64 = vals.iter().sum();
    let mut out = vec![Rect::ZERO; vals.len()];
    if total <= 0.0 || r.area() <= 0.0 {
        return out;
    }
    let scale = r.area() / total;
    let mut order: Vec<usize> = (0..vals.len()).filter(|&i| vals.get(i).is_some_and(|v| *v > 0.0)).collect();
    order.sort_by(|a, b| vals[*b].total_cmp(&vals[*a]));
    let mut free = r;
    let mut i = 0;
    while i < order.len() {
        let short = free.width().min(free.height());
        let worst = |row: &[usize]| {
            let areas: Vec<f64> = row.iter().map(|&k| vals[k] * scale).collect();
            let sum: f64 = areas.iter().sum();
            let (mx, mn) = (areas.iter().copied().fold(0.0, f64::max), areas.iter().copied().fold(f64::MAX, f64::min));
            ((short * short * mx) / (sum * sum)).max((sum * sum) / (short * short * mn))
        };
        let mut end = i + 1;
        while end < order.len() && worst(&order[i..=end]) <= worst(&order[i..end]) {
            end += 1;
        }
        let row = &order[i..end];
        let sum: f64 = row.iter().map(|&k| vals[k] * scale).sum();
        let thick = sum / short;
        let mut at = 0.0;
        for &k in row {
            let len = vals[k] * scale / thick;
            out[k] = if free.width() >= free.height() {
                Rect::new(free.x0, free.y0 + at, free.x0 + thick, free.y0 + at + len)
            } else {
                Rect::new(free.x0 + at, free.y0, free.x0 + at + len, free.y0 + thick)
            };
            at += len;
        }
        free = if free.width() >= free.height() {
            Rect::new(free.x0 + thick, free.y0, free.x1, free.y1)
        } else {
            Rect::new(free.x0, free.y0 + thick, free.x1, free.y1)
        };
        i = end;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn treemap(
    ctx: &mut RenderContext,
    rctx: &Ctx,
    c: &Chart,
    m: Affine,
    area: Rect,
    base: f64,
    vals: &[f64],
    value_label: Option<&dyn Fn(f64) -> String>,
) {
    for (i, t) in squarify(vals, area).into_iter().enumerate() {
        if t.area() <= 0.0 {
            continue;
        }
        ctx.set_transform(m);
        ctx.set_paint(color(series_color(rctx, c, i), 1.0));
        ctx.fill_rect(&t);
        ctx.set_paint(color(Rgba::WHITE, 1.0));
        ctx.set_stroke(kurbo::Stroke::new(1.5));
        ctx.stroke_path(&t.to_path(0.1));
        let mut text = c.categories.get(i).cloned().unwrap_or_default();
        if let (Some(f), Some(v)) = (value_label, vals.get(i)) {
            text = format!("{text}\n{}", f(*v));
        }
        let inner = t.inset(-base * 0.3);
        if inner.width() > base && inner.height() > base {
            label(ctx, rctx, m, &text, inner, base, Align::Left, Anchor::Top, Rgba::WHITE, false);
        }
    }
}

/// One ring of the innermost level, with the centre left open.
fn sunburst(ctx: &mut RenderContext, rctx: &Ctx, c: &Chart, m: Affine, area: Rect, base: f64, vals: &[f64]) {
    let total: f64 = vals.iter().sum();
    let r = area.width().min(area.height()) / 2.0 * 0.95;
    let hole = r * 0.35;
    let cen = area.center();
    let mut a0 = -std::f64::consts::FRAC_PI_2;
    for (i, v) in vals.iter().enumerate() {
        let sweep = v / total * std::f64::consts::TAU;
        if sweep <= 0.0 {
            continue;
        }
        let mut p = BezPath::new();
        p.move_to(Point::new(cen.x + hole * a0.cos(), cen.y + hole * a0.sin()));
        p.line_to(Point::new(cen.x + r * a0.cos(), cen.y + r * a0.sin()));
        p.extend(kurbo::Arc::new(cen, (r, r), a0, sweep, 0.0).append_iter(0.1));
        let a1 = a0 + sweep;
        p.line_to(Point::new(cen.x + hole * a1.cos(), cen.y + hole * a1.sin()));
        p.extend(kurbo::Arc::new(cen, (hole, hole), a1, -sweep, 0.0).append_iter(0.1));
        p.close_path();
        ctx.set_transform(m);
        ctx.set_paint(color(series_color(rctx, c, i), 1.0));
        ctx.fill_path(&p);
        ctx.set_paint(color(Rgba::WHITE, 1.0));
        ctx.set_stroke(kurbo::Stroke::new(1.5));
        ctx.stroke_path(&p);
        if let Some(name) = c.categories.get(i).filter(|_| sweep > 0.25) {
            let mid = a0 + sweep / 2.0;
            let rr = (r + hole) / 2.0;
            let lp = Point::new(cen.x + rr * mid.cos(), cen.y + rr * mid.sin());
            label(
                ctx,
                rctx,
                m,
                name,
                Rect::new(lp.x - 50.0, lp.y - base, lp.x + 50.0, lp.y + base),
                base,
                Align::Center,
                Anchor::Middle,
                Rgba::WHITE,
                false,
            );
        }
        a0 = a1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squarify_tiles_the_area_in_proportion() {
        let vals = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let r = Rect::new(0.0, 0.0, 600.0, 400.0);
        let tiles = squarify(&vals, r);
        let total: f64 = vals.iter().sum();
        for (t, v) in tiles.iter().zip(vals) {
            assert!((t.area() - v / total * r.area()).abs() < 1e-6, "{t:?} for {v}");
            assert!(t.x0 >= -1e-9 && t.y0 >= -1e-9 && t.x1 <= 600.0 + 1e-6 && t.y1 <= 400.0 + 1e-6, "{t:?}");
            // Squarified: no sliver tiles.
            let aspect = t.width().max(t.height()) / t.width().min(t.height());
            assert!(aspect < 4.0, "{t:?}");
        }
        for (i, a) in tiles.iter().enumerate() {
            for b in &tiles[i + 1..] {
                assert!(a.intersect(*b).area() < 1e-6, "{a:?} overlaps {b:?}");
            }
        }
        assert!(squarify(&[0.0, 0.0], r).iter().all(|t| t.area() == 0.0));
    }

    #[test]
    fn waterfall_bars_float_and_totals_stand() {
        let c = Chart {
            kind: ChartType::Waterfall,
            subtotals: vec![3],
            series: vec![deckcraft_model::chart::Series { values: vec![Some(100.0), Some(40.0), Some(-25.0), Some(115.0)], ..Default::default() }],
            ..Default::default()
        };
        assert_eq!(c.waterfall_bars(), [(0.0, 100.0), (100.0, 140.0), (140.0, 115.0), (0.0, 115.0)]);
        assert_eq!(c.value_range(), (0.0, 140.0));
    }
}
