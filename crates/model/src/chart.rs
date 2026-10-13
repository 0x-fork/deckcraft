//! Charts: an embedded data grid plus type and element options.

use serde::{Deserialize, Serialize};

use crate::style::{ColorRef, Fill, Line};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    #[default]
    Column,
    StackedColumn,
    PercentColumn,
    Bar,
    StackedBar,
    PercentBar,
    Line,
    LineMarkers,
    StackedLine,
    Area,
    StackedArea,
    Pie,
    Doughnut,
    Scatter,
    Bubble,
    Radar,
    FilledRadar,
    Stock,
    Surface,
    Histogram,
    Pareto,
    BoxWhisker,
    Waterfall,
    Funnel,
    Treemap,
    Sunburst,
    Combo,
}

impl ChartType {
    /// The Office 2016 (`chartEx`) types, offered after [`ChartType::MAIN`] in chart menus.
    pub const OFFICE_2016: [ChartType; 7] = [
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::Histogram,
        ChartType::Pareto,
        ChartType::BoxWhisker,
        ChartType::Treemap,
        ChartType::Sunburst,
    ];

    /// Whether the type is saved as a `chartEx` part (with a plain chart as the fallback).
    pub fn is_chartex(self) -> bool {
        Self::OFFICE_2016.contains(&self)
    }

    pub const MAIN: [ChartType; 12] = [
        ChartType::Column,
        ChartType::Bar,
        ChartType::Line,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Doughnut,
        ChartType::Scatter,
        ChartType::Radar,
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::Histogram,
        ChartType::Treemap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ChartType::Column => "Clustered Column",
            ChartType::StackedColumn => "Stacked Column",
            ChartType::PercentColumn => "100% Stacked Column",
            ChartType::Bar => "Clustered Bar",
            ChartType::StackedBar => "Stacked Bar",
            ChartType::PercentBar => "100% Stacked Bar",
            ChartType::Line => "Line",
            ChartType::LineMarkers => "Line with Markers",
            ChartType::StackedLine => "Stacked Line",
            ChartType::Area => "Area",
            ChartType::StackedArea => "Stacked Area",
            ChartType::Pie => "Pie",
            ChartType::Doughnut => "Doughnut",
            ChartType::Scatter => "Scatter",
            ChartType::Bubble => "Bubble",
            ChartType::Radar => "Radar",
            ChartType::FilledRadar => "Filled Radar",
            ChartType::Stock => "Stock",
            ChartType::Surface => "Surface",
            ChartType::Histogram => "Histogram",
            ChartType::Pareto => "Pareto",
            ChartType::BoxWhisker => "Box and Whisker",
            ChartType::Waterfall => "Waterfall",
            ChartType::Funnel => "Funnel",
            ChartType::Treemap => "Treemap",
            ChartType::Sunburst => "Sunburst",
            ChartType::Combo => "Combo",
        }
    }
    pub fn is_bar_like(self) -> bool {
        matches!(
            self,
            ChartType::Column
                | ChartType::StackedColumn
                | ChartType::PercentColumn
                | ChartType::Bar
                | ChartType::StackedBar
                | ChartType::PercentBar
                | ChartType::Histogram
                | ChartType::Pareto
                | ChartType::Waterfall
        )
    }
    pub fn horizontal(self) -> bool {
        matches!(self, ChartType::Bar | ChartType::StackedBar | ChartType::PercentBar | ChartType::Funnel)
    }
    pub fn stacked(self) -> bool {
        matches!(
            self,
            ChartType::StackedColumn
                | ChartType::StackedBar
                | ChartType::StackedLine
                | ChartType::StackedArea
                | ChartType::PercentColumn
                | ChartType::PercentBar
        )
    }
    pub fn percent(self) -> bool {
        matches!(self, ChartType::PercentColumn | ChartType::PercentBar)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
    /// X values for scatter/bubble.
    pub x_values: Vec<Option<f64>>,
    pub sizes: Vec<Option<f64>>,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    /// Per-point fill overrides (pie slices).
    pub point_fills: Vec<(usize, Fill)>,
    pub data_labels: Option<bool>,
    pub smooth: bool,
    pub marker: Option<String>,
    /// Combo charts: this series' own type.
    pub kind: Option<ChartType>,
    pub secondary_axis: bool,
    /// The number format of the series' data labels (Excel code): the labels' own, else the
    /// source data's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartType,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub legend: Option<String>,
    /// `c:legend/c:overlay`: the legend is drawn over the plot instead of shrinking it.
    pub legend_overlay: bool,
    /// Text size in points of the axis labels and legend (from their `c:txPr`, else the chart's);
    /// `None`: the file gives none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f64>,
    /// Title text size in points (its rich text or `c:txPr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_size: Option<f64>,
    pub data_labels: bool,
    pub gridlines: bool,
    pub axis_titles: (Option<String>, Option<String>),
    /// Chart style number (1–48) and colour palette (`colorful1`, `monochrome3`…).
    pub style: u32,
    pub palette: String,
    pub plot_fill: Option<Fill>,
    pub chart_fill: Option<Fill>,
    pub text_color: Option<ColorRef>,
    pub hole_size: f64,
    pub gap_width: f64,
    pub overlap: f64,
    pub vary_colors: bool,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    /// The value axis' major unit, when the chart fixes it.
    pub y_major: Option<f64>,
    /// The value axis' number format (an Excel format code such as `#,##0` or `0.0%`).
    pub y_format: Option<String>,
    /// The value axis is deleted (`c:delete`): no labels, though gridlines may stay.
    pub y_hidden: bool,
    /// A combo chart's secondary value axis: bounds, major unit and number format.
    pub y2_min: Option<f64>,
    pub y2_max: Option<f64>,
    pub y2_major: Option<f64>,
    pub y2_format: Option<String>,
    pub y2_hidden: bool,
    /// The chart's language (`c:lang`, e.g. `ru-RU`): digit grouping and decimal mark.
    pub lang: Option<String>,
    /// A waterfall's total points (`cx:subtotals`): bars that stand on the axis and restart
    /// the running total, instead of floating.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subtotals: Vec<usize>,
    /// A histogram's bin count (`cx:binCount`); with neither this nor `bin_size` the bins are
    /// automatic (Scott's rule, as Excel does).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_count: Option<u32>,
    /// A histogram's bin width (`cx:binSize`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_size: Option<f64>,
    /// Original chart XML part kept for round-trip of unmodelled options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

impl Default for Chart {
    fn default() -> Self {
        Chart {
            kind: ChartType::Column,
            title: Some("Chart Title".into()),
            categories: vec![],
            series: vec![],
            legend: Some("b".into()),
            legend_overlay: false,
            text_size: None,
            title_size: None,
            data_labels: false,
            gridlines: true,
            axis_titles: (None, None),
            style: 2,
            palette: "colorful1".into(),
            plot_fill: None,
            chart_fill: None,
            text_color: None,
            hole_size: 0.5,
            gap_width: 2.19,
            overlap: -0.27,
            vary_colors: false,
            y_min: None,
            y_max: None,
            y_major: None,
            y_format: None,
            y_hidden: false,
            y2_min: None,
            y2_max: None,
            y2_major: None,
            y2_format: None,
            y2_hidden: false,
            lang: None,
            subtotals: vec![],
            bin_count: None,
            bin_size: None,
            raw: None,
        }
    }
}

impl Chart {
    /// The sample data a new chart starts with (our own numbers).
    pub fn sample(kind: ChartType) -> Self {
        let cats = ["Q1", "Q2", "Q3", "Q4"];
        let data = [[4.2, 2.6, 2.1], [2.8, 4.1, 2.4], [3.6, 1.9, 3.3], [4.7, 3.0, 5.1]];
        let mut c = Chart { kind, categories: cats.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        if matches!(kind, ChartType::Histogram | ChartType::Pareto | ChartType::BoxWhisker) {
            // These plot raw observations: a histogram bins one series, a box chart summarises
            // each category's points.
            let obs = [12.0, 15.0, 17.0, 18.0, 21.0, 22.0, 22.0, 24.0, 25.0, 27.0, 28.0, 31.0, 33.0, 36.0, 41.0, 48.0];
            let n_series = if kind == ChartType::BoxWhisker { 2 } else { 1 };
            c.categories = if kind == ChartType::BoxWhisker {
                (0..obs.len()).map(|i| if i % 2 == 0 { "Group A" } else { "Group B" }.to_string()).collect()
            } else {
                vec![]
            };
            for s in 0..n_series {
                c.series.push(Series {
                    name: format!("Series {}", s + 1),
                    values: obs.iter().map(|v| Some(v + s as f64 * 6.0)).collect(),
                    ..Default::default()
                });
            }
            c.gridlines = true;
            c.legend = (kind == ChartType::BoxWhisker).then(|| "b".to_string());
            return c;
        }
        let n_series =
            if matches!(kind, ChartType::Pie | ChartType::Doughnut | ChartType::Funnel | ChartType::Waterfall | ChartType::Treemap) { 1 } else { 3 };
        for s in 0..n_series {
            c.series.push(Series {
                name: format!("Series {}", s + 1),
                values: data.iter().map(|row| row.get(s).copied()).collect(),
                x_values: if kind == ChartType::Scatter { (1..=4).map(|x| Some(x as f64)).collect() } else { vec![] },
                ..Default::default()
            });
        }
        c.vary_colors = n_series == 1;
        c
    }
    /// A series' own chart type: its own in a combo chart (columns when it names none), else the
    /// chart's.
    pub fn series_kind(&self, s: &Series) -> ChartType {
        match (self.kind, s.kind) {
            (ChartType::Combo, Some(k)) if k != ChartType::Combo => k,
            (ChartType::Combo, _) => ChartType::Column,
            (k, _) => k,
        }
    }

    /// Whether a series is plotted against the secondary value axis (combo charts only).
    pub fn on_secondary(&self, s: &Series) -> bool {
        self.kind == ChartType::Combo && s.secondary_axis
    }

    /// The value range of one value axis: the primary axis (`y_min`/`y_max` apply), or a combo
    /// chart's secondary axis. A combo chart stacks only its stacked series. `None` when no series
    /// uses that axis.
    pub fn axis_range(&self, secondary: bool) -> Option<(f64, f64)> {
        if self.kind != ChartType::Combo {
            return (!secondary).then(|| self.value_range());
        }
        let ser: Vec<&Series> = self.series.iter().filter(|s| self.on_secondary(s) == secondary).collect();
        if ser.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = (0.0_f64, 0.0_f64);
        let n = ser.iter().map(|s| s.values.len()).max().unwrap_or(0);
        let mut percent = false;
        for i in 0..n {
            let (mut p, mut m) = (0.0, 0.0);
            for s in &ser {
                let k = self.series_kind(s);
                let Some(v) = s.values.get(i).copied().flatten().filter(|v| v.is_finite()) else { continue };
                if k.percent() {
                    percent = true;
                } else if k.stacked() {
                    if v >= 0.0 { p += v } else { m += v }
                } else {
                    hi = hi.max(v);
                    lo = lo.min(v);
                }
            }
            hi = hi.max(p);
            lo = lo.min(m);
        }
        if percent {
            hi = hi.max(1.0);
        }
        Some(if secondary { (lo, hi) } else { (self.y_min.unwrap_or(lo), self.y_max.unwrap_or(hi)) })
    }

    /// A value axis' scale — (min, max, major unit) — with at most `max_intervals` intervals:
    /// the bounds and unit the chart fixes, else PowerPoint's automatic ones (the data plus 5 %
    /// headroom, rounded out to a 1-2-5 unit). `None` when no series uses the axis.
    /// A waterfall's bars as (from, to) on the value axis: each point floats from the running
    /// total, a total point stands on zero and sets it.
    pub fn waterfall_bars(&self) -> Vec<(f64, f64)> {
        let Some(s) = self.series.first() else { return vec![] };
        let mut run = 0.0;
        s.values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let v = v.unwrap_or(0.0);
                if self.subtotals.contains(&i) {
                    run = v;
                    (0.0, v)
                } else {
                    let from = run;
                    run += v;
                    (from, run)
                }
            })
            .collect()
    }

    pub fn axis_scale(&self, secondary: bool, max_intervals: usize) -> Option<(f64, f64, f64)> {
        let (lo, hi) = self.axis_range(secondary)?;
        let (fmin, fmax, major) = if secondary { (self.y2_min, self.y2_max, self.y2_major) } else { (self.y_min, self.y_max, self.y_major) };
        let full_percent = !secondary && self.kind.percent();
        Some(axis_scale(lo, hi, fmin, fmax, major, max_intervals, !full_percent))
    }

    /// A value axis label in the axis' number format and the chart's language.
    pub fn axis_label(&self, secondary: bool, v: f64) -> String {
        let code = if secondary { &self.y2_format } else { &self.y_format };
        crate::numfmt::format(code.as_deref().unwrap_or("General"), v, self.lang.as_deref())
    }

    /// A data label of series `s`: its number format, else its axis', in the chart's language.
    pub fn data_label(&self, s: &Series, v: f64) -> String {
        let axis = if self.on_secondary(s) { &self.y2_format } else { &self.y_format };
        let code = s.number_format.as_ref().or(axis.as_ref());
        crate::numfmt::format(code.map_or("General", String::as_str), v, self.lang.as_deref())
    }

    /// Value range across series (stacked sums for stacked types).
    pub fn value_range(&self) -> (f64, f64) {
        let mut lo: f64 = 0.0;
        let mut hi: f64 = 0.0;
        if self.kind == ChartType::Waterfall {
            for (a, b) in self.waterfall_bars() {
                lo = lo.min(a.min(b));
                hi = hi.max(a.max(b));
            }
            return (lo, hi);
        }
        let n = self.categories.len().max(self.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
        if self.kind.stacked() {
            for i in 0..n {
                let (mut p, mut m) = (0.0, 0.0);
                for s in &self.series {
                    let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
                    if v >= 0.0 { p += v } else { m += v }
                }
                hi = hi.max(p);
                lo = lo.min(m);
            }
        } else {
            for s in &self.series {
                for v in s.values.iter().flatten() {
                    if v.is_finite() {
                        hi = hi.max(*v);
                        lo = lo.min(*v);
                    }
                }
            }
        }
        if self.kind.percent() {
            return (0.0, 1.0);
        }
        (self.y_min.unwrap_or(lo), self.y_max.unwrap_or(hi))
    }
}

/// The (min, max, major unit) of an axis over the data range [lo, hi]: `fmin`/`fmax`/`major`
/// where the chart fixes them; otherwise the range takes in zero, gets 5 % headroom (`headroom`)
/// and rounds out to the smallest 1-2-5 unit giving at most `max_intervals` intervals.
pub fn axis_scale(
    lo: f64,
    hi: f64,
    fmin: Option<f64>,
    fmax: Option<f64>,
    major: Option<f64>,
    max_intervals: usize,
    headroom: bool,
) -> (f64, f64, f64) {
    // Values past ±1e100 aren't axis values; clamping keeps the arithmetic finite.
    let fin = |v: Option<f64>| v.filter(|v| v.is_finite()).map(|v| v.clamp(-1e100, 1e100));
    let (fmin, fmax) = (fin(fmin), fin(fmax));
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi).clamp(-1e100, 1e100), lo.max(hi).clamp(-1e100, 1e100)) } else { (0.0, 1.0) };
    let mut lo = fmin.unwrap_or(lo.min(0.0));
    let mut hi = fmax.unwrap_or(hi.max(lo));
    if hi - lo < 1e-12 {
        hi = lo + lo.abs().max(1.0);
    }
    let span = hi - lo;
    if headroom && fmax.is_none() && hi > 0.0 {
        hi += span * 0.05;
    }
    if headroom && fmin.is_none() && lo < 0.0 {
        lo -= span * 0.05;
    }
    let n = max_intervals.clamp(1, 50) as f64;
    let step = match fin(major).filter(|m| *m > 0.0 && (hi - lo) / m <= 1000.0) {
        Some(m) => m,
        None => {
            let mut step = 10f64.powf(((hi - lo) / n).log10().floor());
            let mut k = 0;
            // 1, 2, 5, 10, 20… × the magnitude until the intervals fit.
            while ((hi / step).ceil() - (lo / step).floor()) > n && k < 30 {
                step *= if matches!(k % 3, 1) { 2.5 } else { 2.0 };
                k += 1;
            }
            step
        }
    };
    let min = fmin.unwrap_or((lo / step + 1e-9).floor() * step);
    let max = fmax.unwrap_or((hi / step - 1e-9).ceil() * step).max(min + step);
    (min, max, step)
}

/// "Nice" axis scale: (min, max, major step) covering [lo, hi].
pub fn nice_scale(lo: f64, hi: f64) -> (f64, f64, f64) {
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 1.0) };
    let span = if hi - lo < 1e-12 { hi.abs().max(1.0) } else { hi - lo };
    let raw = span / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;
    let min = (lo / step).floor() * step;
    let max = (hi / step).ceil() * step;
    let max = if (max - min).abs() < 1e-12 { min + step } else { max };
    (min, max, step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_scale_covers() {
        let (a, b, s) = nice_scale(0.0, 5.1);
        assert!(a <= 0.0 && b >= 5.1 && s > 0.0);
        let (a, b, _) = nice_scale(f64::NAN, 3.0);
        assert!(a.is_finite() && b.is_finite());
        let (a, b, _) = nice_scale(7.0, 7.0);
        assert!(b > a);
    }

    #[test]
    fn automatic_axis_scales_like_powerpoint() {
        // Weekly spend up to ~715 000 on a plot with room for 9 intervals: 0–800 000 by 100 000.
        assert_eq!(axis_scale(0.0, 715_000.0, None, None, None, 9, true), (0.0, 800_000.0, 100_000.0));
        // Clicks up to 6 600: 0–7 000 by 1 000.
        assert_eq!(axis_scale(0.0, 6_600.0, None, None, None, 9, true), (0.0, 7_000.0, 1_000.0));
        // Fewer intervals fit on a short axis.
        assert_eq!(axis_scale(0.0, 6_600.0, None, None, None, 4, true), (0.0, 8_000.0, 2_000.0));
        // Negative data gets room below; fixed bounds and unit are kept.
        let (min, max, _) = axis_scale(-30.0, 100.0, None, None, None, 8, true);
        assert!(min <= -30.0 && max >= 105.0);
        assert_eq!(axis_scale(0.0, 715_000.0, Some(100_000.0), Some(750_000.0), Some(50_000.0), 9, true), (100_000.0, 750_000.0, 50_000.0));
        // 100 % stacked stays at 0–1 without headroom.
        assert_eq!(axis_scale(0.0, 1.0, None, None, None, 5, false), (0.0, 1.0, 0.2));
        // Nonsense never loops or divides by zero.
        for (lo, hi) in [(f64::NAN, 1.0), (5.0, 5.0), (0.0, 0.0), (-1e300, 1e300)] {
            let (a, b, s) = axis_scale(lo, hi, None, None, Some(0.0), 0, true);
            assert!(a.is_finite() && b.is_finite() && s > 0.0 && b > a, "{lo} {hi}: {a} {b} {s}");
        }
    }

    #[test]
    fn axis_labels_use_the_format_and_language() {
        let mut c = Chart::sample(ChartType::Column);
        c.y_format = Some(r#"_-* #\ ##0\ "₽"_-;\-* #\ ##0\ "₽"_-;_-* "-"??\ "₽"_-;_-@_-"#.into());
        c.lang = Some("ru-RU".into());
        assert_eq!(c.axis_label(false, 800_000.0), "800\u{a0}000 ₽");
        assert_eq!(c.axis_label(false, 0.0), "- ₽");
        // No format: General.
        assert_eq!(c.axis_label(true, 2.5), "2,5");
        c.lang = None;
        assert_eq!(c.axis_label(true, 2.5), "2.5");
        // Data labels: the series' format, else the axis'.
        c.lang = Some("ru-RU".into());
        c.series[0].number_format = Some(r#"#,##0\ "₽""#.into());
        assert_eq!(c.data_label(&c.series[0], 516_600_000.0), "516\u{a0}600\u{a0}000 ₽");
        assert_eq!(c.data_label(&c.series[1], 0.0), "- ₽");
    }

    #[test]
    fn combo_axis_ranges() {
        // Spend in columns on the primary axis, clicks as a line on the secondary one.
        let mut c = Chart::sample(ChartType::Combo);
        c.series.truncate(2);
        c.series[0].kind = Some(ChartType::Column);
        c.series[0].values = vec![Some(200_000.0), Some(310_000.0)];
        c.series[1].kind = Some(ChartType::Line);
        c.series[1].values = vec![Some(6_000.0), Some(7_400.0)];
        c.series[1].secondary_axis = true;
        assert_eq!(c.axis_range(false), Some((0.0, 310_000.0)));
        assert_eq!(c.axis_range(true), Some((0.0, 7_400.0)));
        assert_eq!(c.series_kind(&c.series[1]), ChartType::Line);
        // Without a secondary series there is no secondary axis; a non-combo chart never has one.
        c.series[1].secondary_axis = false;
        assert_eq!(c.axis_range(true), None);
        let col = Chart::sample(ChartType::Column);
        assert_eq!(col.axis_range(false), Some(col.value_range()));
        assert_eq!(col.axis_range(true), None);
        assert!(!col.on_secondary(&col.series[0]));
    }

    #[test]
    fn sample_ranges() {
        let c = Chart::sample(ChartType::StackedColumn);
        let (_, hi) = c.value_range();
        assert!(hi > 10.0);
        assert_eq!(Chart::sample(ChartType::Pie).series.len(), 1);
    }
}

/// One histogram bin: `[lo, hi]` (the first bin is closed on both sides, later ones open on the
/// left, as Excel bins) and how many observations fall in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bin {
    pub lo: f64,
    pub hi: f64,
    pub count: usize,
}

/// Bins a histogram has at most (a hostile bin size cannot make millions).
pub const MAX_BINS: usize = 1000;

/// A box and whisker summary of one group: quartiles by Excel's exclusive method, whiskers at
/// the furthest points within 1.5 IQR of the box, and the points beyond them.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxStats {
    pub low: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub high: f64,
    pub mean: f64,
    pub outliers: Vec<f64>,
}

impl Chart {
    /// The first series' finite observations.
    fn observations(&self) -> Vec<f64> {
        self.series.first().map(|s| s.values.iter().flatten().copied().filter(|v| v.is_finite()).collect()).unwrap_or_default()
    }

    /// The histogram's bins over the first series: by `bin_size`, else `bin_count`, else
    /// Scott's rule (3.49·σ·n^(-1/3)). Empty without observations.
    pub fn histogram_bins(&self) -> Vec<Bin> {
        let mut v = self.observations();
        if v.is_empty() {
            return vec![];
        }
        v.sort_by(f64::total_cmp);
        let (lo, hi) = (v[0], v[v.len() - 1]);
        let span = hi - lo;
        let n = v.len() as f64;
        let width = if let Some(w) = self.bin_size.filter(|w| w.is_finite() && *w > 0.0) {
            w
        } else if let Some(k) = self.bin_count.filter(|k| *k > 0) {
            span / f64::from(k)
        } else {
            let mean = v.iter().sum::<f64>() / n;
            let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
            3.49 * sd / n.cbrt()
        };
        let count = if span <= 0.0 || !width.is_finite() || width <= 0.0 { 1 } else { ((span / width).ceil() as usize).clamp(1, MAX_BINS) };
        let width = if count == 1 && (span <= 0.0 || !width.is_finite() || width <= 0.0) { span.max(1.0) } else { width.max(span / MAX_BINS as f64) };
        let mut bins: Vec<Bin> = (0..count).map(|i| Bin { lo: lo + width * i as f64, hi: lo + width * (i + 1) as f64, count: 0 }).collect();
        for x in v {
            // (lo, hi] bins, the first one also holding the minimum.
            let i = (((x - lo) / width).ceil() as usize).saturating_sub(1).min(count - 1);
            if let Some(b) = bins.get_mut(i) {
                b.count += 1;
            }
        }
        bins
    }

    /// A histogram (or Pareto) as the chart that draws it: a column chart of the bin counts with
    /// touching columns, labelled by bin range. A Pareto sorts the bins by count and adds the
    /// cumulative percentage as a line on a 0–100 % secondary axis.
    pub fn histogram_view(&self) -> Chart {
        let mut bins = self.histogram_bins();
        let pareto = self.kind == ChartType::Pareto;
        if pareto {
            bins.sort_by_key(|b| std::cmp::Reverse(b.count));
        }
        let fmt = |v: f64| {
            let r = (v * 100.0).round() / 100.0;
            if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r}") }
        };
        let first_lo = bins.iter().map(|b| b.lo).fold(f64::INFINITY, f64::min);
        let mut v = Chart {
            kind: if pareto { ChartType::Combo } else { ChartType::Column },
            categories: bins
                .iter()
                .map(|b| if b.lo <= first_lo { format!("[{}, {}]", fmt(b.lo), fmt(b.hi)) } else { format!("({}, {}]", fmt(b.lo), fmt(b.hi)) })
                .collect(),
            series: vec![],
            gap_width: 0.0,
            raw: None,
            subtotals: vec![],
            ..self.clone()
        };
        let name = self.series.first().map(|s| s.name.clone()).unwrap_or_default();
        let base = self.series.first().cloned().unwrap_or_default();
        v.series.push(Series {
            name,
            values: bins.iter().map(|b| Some(b.count as f64)).collect(),
            kind: pareto.then_some(ChartType::Column),
            number_format: None,
            ..base
        });
        if pareto {
            let total = bins.iter().map(|b| b.count).sum::<usize>().max(1) as f64;
            let mut run = 0.0;
            v.series.push(Series {
                name: "Cumulative %".into(),
                values: bins
                    .iter()
                    .map(|b| {
                        run += b.count as f64;
                        Some(run / total)
                    })
                    .collect(),
                kind: Some(ChartType::Line),
                secondary_axis: true,
                ..Default::default()
            });
            v.y2_min = Some(0.0);
            v.y2_max = Some(1.0);
            v.y2_major = Some(0.2);
            v.y2_format = Some("0%".into());
        }
        v.vary_colors = false;
        v
    }

    /// The box chart's groups (distinct category labels in order of appearance; one unnamed
    /// group without categories) and, per series, each group's summary (None when it has no
    /// points).
    pub fn box_groups(&self) -> (Vec<String>, Vec<Vec<Option<BoxStats>>>) {
        let mut groups: Vec<String> = Vec::new();
        for c in &self.categories {
            if !groups.contains(c) {
                groups.push(c.clone());
            }
        }
        if groups.is_empty() {
            groups.push(String::new());
        }
        let stats = self
            .series
            .iter()
            .map(|s| {
                groups
                    .iter()
                    .map(|g| {
                        let pts: Vec<f64> = s
                            .values
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| self.categories.get(*i).map_or(g.is_empty() || self.categories.is_empty(), |c| c == g))
                            .filter_map(|(_, v)| *v)
                            .filter(|v| v.is_finite())
                            .collect();
                        box_stats(pts)
                    })
                    .collect()
            })
            .collect();
        (groups, stats)
    }
}

/// Excel's QUARTILE.EXC position `p·(n+1)` (1-based) on sorted data, clamped to the ends.
fn quantile_exc(v: &[f64], p: f64) -> f64 {
    let n = v.len();
    let pos = (p * (n as f64 + 1.0)).clamp(1.0, n as f64);
    let i = pos.floor() as usize;
    let frac = pos - i as f64;
    let a = v[i.saturating_sub(1).min(n - 1)];
    let b = v[i.min(n - 1)];
    a + (b - a) * frac
}

/// The summary of one group's points.
pub fn box_stats(mut pts: Vec<f64>) -> Option<BoxStats> {
    if pts.is_empty() {
        return None;
    }
    pts.sort_by(f64::total_cmp);
    let (q1, median, q3) = (quantile_exc(&pts, 0.25), quantile_exc(&pts, 0.5), quantile_exc(&pts, 0.75));
    let iqr = q3 - q1;
    let (lo_fence, hi_fence) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    let inside: Vec<f64> = pts.iter().copied().filter(|v| *v >= lo_fence && *v <= hi_fence).collect();
    let low = inside.first().copied().unwrap_or(q1).min(q1);
    let high = inside.last().copied().unwrap_or(q3).max(q3);
    let mean = pts.iter().sum::<f64>() / pts.len() as f64;
    let outliers = pts.iter().copied().filter(|v| *v < lo_fence || *v > hi_fence).collect();
    Some(BoxStats { low, q1, median, q3, high, mean, outliers })
}

#[cfg(test)]
mod chartex_tests {
    use super::*;

    fn obs(v: &[f64]) -> Chart {
        Chart {
            kind: ChartType::Histogram,
            series: vec![Series { values: v.iter().map(|x| Some(*x)).collect(), ..Default::default() }],
            ..Default::default()
        }
    }

    #[test]
    fn histogram_bins_by_count_size_and_scott() {
        let mut c = obs(&[1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 9.0]);
        c.bin_count = Some(4);
        let b = c.histogram_bins();
        assert_eq!(b.len(), 4);
        assert_eq!((b[0].lo, b[0].hi), (1.0, 3.0));
        // (lo, hi] bins with the minimum in the first: 1, 2, 2, 3 | 4, 5 | - | 9.
        assert_eq!(b.iter().map(|b| b.count).collect::<Vec<_>>(), vec![4, 2, 0, 1]);
        c.bin_count = None;
        c.bin_size = Some(5.0);
        assert_eq!(c.histogram_bins().iter().map(|b| b.count).collect::<Vec<_>>(), vec![6, 1]);
        c.bin_size = None;
        let auto = c.histogram_bins();
        assert!(!auto.is_empty() && auto.iter().map(|b| b.count).sum::<usize>() == 7);
        // A tiny bin size is capped; one value gives one bin.
        c.bin_size = Some(1e-12);
        assert!(c.histogram_bins().len() <= MAX_BINS);
        assert_eq!(obs(&[3.0]).histogram_bins(), vec![Bin { lo: 3.0, hi: 4.0, count: 1 }]);
        assert!(obs(&[]).histogram_bins().is_empty());
    }

    #[test]
    fn pareto_view_sorts_bins_and_adds_the_cumulative_line() {
        let mut c = obs(&[1.0, 5.0, 5.0, 5.0, 9.0, 9.0]);
        c.kind = ChartType::Pareto;
        c.bin_count = Some(3);
        let v = c.histogram_view();
        assert_eq!(v.kind, ChartType::Combo);
        assert_eq!(v.series[0].values, vec![Some(3.0), Some(2.0), Some(1.0)]);
        let cum = &v.series[1];
        assert!(cum.secondary_axis && cum.kind == Some(ChartType::Line));
        assert_eq!(cum.values.last(), Some(&Some(1.0)));
        assert_eq!(v.categories[0], "(3.67, 6.33]");
    }

    #[test]
    fn box_stats_use_exclusive_quartiles_and_fence_outliers() {
        let b = box_stats(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 100.0]).unwrap();
        // QUARTILE.EXC on 8 points: positions 2.25, 4.5, 6.75.
        assert_eq!((b.q1, b.median, b.q3), (2.25, 4.5, 6.75));
        assert_eq!(b.outliers, vec![100.0]);
        assert_eq!((b.low, b.high), (1.0, 7.0));
        assert!(box_stats(vec![]).is_none());
        let one = box_stats(vec![5.0]).unwrap();
        assert_eq!((one.low, one.q1, one.median, one.q3, one.high), (5.0, 5.0, 5.0, 5.0, 5.0));
    }

    #[test]
    fn box_groups_follow_category_labels() {
        let c = Chart::sample(ChartType::BoxWhisker);
        let (groups, stats) = c.box_groups();
        assert_eq!(groups, vec!["Group A".to_string(), "Group B".to_string()]);
        assert_eq!(stats.len(), 2);
        assert!(stats.iter().all(|s| s.len() == 2 && s.iter().all(Option::is_some)));
    }
}
