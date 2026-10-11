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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Chart {
    pub kind: ChartType,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub legend: Option<String>,
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
    /// The value axis scale `(min, max, major unit)` the chart is drawn with: the chart's fixed
    /// limits, otherwise Office's automatic scale ([`auto_scale`]).
    pub fn axis_scale(&self) -> (f64, f64, f64) {
        if self.kind.percent() {
            return auto_scale(0.0, 1.0, Some(0.0), Some(1.0));
        }
        let (lo, hi) = self.value_range();
        auto_scale(lo, hi, self.y_min, self.y_max)
    }

    /// Range of the plotted values (stacked sums for stacked types, which start at zero);
    /// `(0, 0)` without values.
    pub fn value_range(&self) -> (f64, f64) {
        let n = self.categories.len().max(self.series.iter().map(|s| s.values.len()).max().unwrap_or(0));
        if self.kind.stacked() {
            let (mut lo, mut hi) = (0.0f64, 0.0f64);
            for i in 0..n {
                let (mut p, mut m) = (0.0, 0.0);
                for s in &self.series {
                    let v = s.values.get(i).copied().flatten().filter(|v| v.is_finite()).unwrap_or(0.0);
                    if v >= 0.0 { p += v } else { m += v }
                }
                hi = hi.max(p);
                lo = lo.min(m);
            }
            return (lo, hi);
        }
        let mut values = self.series.iter().flat_map(|s| s.values.iter().flatten()).filter(|v| v.is_finite());
        let Some(first) = values.next() else { return (0.0, 0.0) };
        values.fold((*first, *first), |(lo, hi), v| (lo.min(*v), hi.max(*v)))
    }
}

/// Office's automatic value-axis scale `(min, max, major unit)` for values in `[lo, hi]`, keeping
/// the limits a chart fixes (`<c:min>`, `<c:max>`). The rule Office documents for automatic limits:
///
/// - **Minimum:** zero when every value is ≥ 0 and the smallest is below 5/6 of the largest;
///   otherwise the first major unit at or below `lo − (hi − lo)/20`.
/// - **Maximum:** the first major unit at or above `hi + (hi − lo)/20`.
/// - All-negative values mirror this: the axis ends at zero when the largest is above 5/6 of the
///   smallest.
///
/// The major unit is the smallest 1, 2 or 5 × 10ⁿ that gives at most 10 major intervals (#144).
pub fn auto_scale(lo: f64, hi: f64, fixed_min: Option<f64>, fixed_max: Option<f64>) -> (f64, f64, f64) {
    let fixed_min = fixed_min.filter(|v| v.is_finite());
    let fixed_max = fixed_max.filter(|v| v.is_finite());
    let (lo, hi) = if lo.is_finite() && hi.is_finite() { (lo.min(hi), lo.max(hi)) } else { (0.0, 0.0) };
    let (lo, hi) = (fixed_min.unwrap_or(lo), fixed_max.unwrap_or(hi));
    let pad = (hi - lo).abs() / 20.0;
    let flat = (hi - lo).abs() < 1e-12;
    // Where the axis has to reach, before rounding out to the major unit.
    let low = match fixed_min {
        Some(m) => m,
        None if lo >= 0.0 && (flat || lo < hi * 5.0 / 6.0) => 0.0,
        None => lo - pad,
    };
    let mut high = match fixed_max {
        Some(m) => m,
        None if hi <= 0.0 && (flat || hi > lo * 5.0 / 6.0) => 0.0,
        None => hi + pad,
    };
    if high - low < 1e-12 {
        high = low + low.abs().max(1.0);
    }
    // Smallest 1, 2 or 5 × 10ⁿ at or above a tenth of the span.
    let raw = (high - low) / 10.0;
    let mut exp = raw.log10().floor().clamp(-400.0, 400.0) as i32;
    let mut mantissa = [1.0, 2.0, 5.0, 10.0].into_iter().find(|m| m * 10f64.powi(exp) >= raw * (1.0 - 1e-9)).unwrap_or(10.0);
    let mut out = (low, high, raw);
    // A rounded-out axis can need one more interval than the span; then the next unit fits.
    for _ in 0..4 {
        if mantissa >= 10.0 {
            mantissa = 1.0;
            exp = exp.saturating_add(1);
        }
        let step = if exp >= 0 { mantissa * 10f64.powi(exp) } else { mantissa / 10f64.powi(-exp) };
        let min = fixed_min.unwrap_or_else(|| (low / step + 1e-9).floor() * step);
        let max = fixed_max.unwrap_or_else(|| (high / step - 1e-9).ceil() * step);
        let max = if max - min < 1e-12 { min + step } else { max };
        out = (min, max, step);
        if (max - min) / step <= 10.0 + 1e-9 {
            break;
        }
        mantissa = if mantissa < 2.0 {
            2.0
        } else if mantissa < 5.0 {
            5.0
        } else {
            10.0
        };
    }
    // Spans too large or too small for f64 steps.
    if [out.0, out.1, out.2].iter().all(|v| v.is_finite()) && out.2 > 0.0 && out.1 > out.0 { out } else { (0.0, 1.0, 0.1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_scale_covers() {
        let (a, b, s) = auto_scale(0.0, 5.1, None, None);
        assert!(a <= 0.0 && b >= 5.1 && s > 0.0);
        let (a, b, _) = auto_scale(f64::NAN, 3.0, None, None);
        assert!(a.is_finite() && b.is_finite() && b > a);
        let (a, b, _) = auto_scale(7.0, 7.0, None, None);
        assert!(b > a);
        for (lo, hi, min, max) in
            [(0.0, 1.0, Some(5.0), Some(5.0)), (-f64::MAX, f64::MAX, None, None), (0.0, 1e-320, None, None), (0.0, 1.0, Some(5.0), Some(1.0))]
        {
            let (a, b, s) = auto_scale(lo, hi, min, max);
            assert!(a.is_finite() && b.is_finite() && b > a && s > 0.0, "{lo} {hi}: {a} {b} {s}");
        }
    }

    fn column(values: &[f64]) -> Chart {
        let series = Series { name: "Sales".into(), values: values.iter().map(|v| Some(*v)).collect(), ..Default::default() };
        Chart { categories: (0..values.len()).map(|i| format!("C{i}")).collect(), series: vec![series], ..Default::default() }
    }

    /// #144: Office scales 42.1 / 18.7 / 6.3 as 0–45 in steps of 5, not 0–50 in steps of 10.
    #[test]
    fn automatic_axis_matches_office() {
        assert_eq!(column(&[42.1, 18.7, 6.3]).axis_scale(), (0.0, 45.0, 5.0));
        // PowerPoint's default chart data (max 5): 0–6 in steps of 1.
        let mut c = column(&[4.3, 2.5, 3.5, 4.5]);
        c.series.push(Series { values: [2.4, 4.4, 1.8, 2.8].map(Some).to_vec(), ..Default::default() });
        c.series.push(Series { values: [2.0, 2.0, 3.0, 5.0].map(Some).to_vec(), ..Default::default() });
        assert_eq!(c.axis_scale(), (0.0, 6.0, 1.0));
        // A maximum of 100 (or 10) gets 5% headroom, which rounds up to the next major unit.
        assert_eq!(column(&[100.0, 40.0]).axis_scale(), (0.0, 120.0, 20.0));
        assert_eq!(column(&[10.0, 3.0]).axis_scale(), (0.0, 12.0, 2.0));
        // Headroom 2.95 + 2.95/20 = 3.1 rounds up to 3.5.
        assert_eq!(column(&[2.95, 0.0]).axis_scale(), (0.0, 3.5, 0.5));
    }

    #[test]
    fn automatic_axis_minimum() {
        // Below 5/6 of the maximum, the axis starts at zero; at or above it, near the data.
        assert_eq!(column(&[59.0, 49.0]).axis_scale().0, 0.0);
        let (min, max, step) = column(&[59.0, 50.0]).axis_scale();
        assert!(min > 0.0 && min <= 50.0 - 0.45 && max >= 59.45 && (max - min) / step <= 10.0, "{min} {max} {step}");
        // Negative data mirrors it: the axis ends at zero, and has headroom below.
        assert_eq!(column(&[-42.1, -18.7, -6.3]).axis_scale(), (-45.0, 0.0, 5.0));
        // Mixed signs get headroom at both ends.
        assert_eq!(column(&[42.1, -18.7]).axis_scale(), (-30.0, 50.0, 10.0));
    }

    #[test]
    fn fixed_axis_limits_are_kept() {
        let mut c = column(&[42.1, 18.7, 6.3]);
        c.y_max = Some(50.0);
        assert_eq!(c.axis_scale(), (0.0, 50.0, 5.0));
        c.y_min = Some(10.0);
        c.y_max = None;
        assert_eq!(c.axis_scale(), (10.0, 45.0, 5.0));
        let mut p = column(&[42.1, 18.7, 6.3]);
        p.kind = ChartType::PercentColumn;
        assert_eq!(p.axis_scale(), (0.0, 1.0, 0.1));
        // Degenerate data still gives an axis.
        for (a, b) in [(0.0, 0.0), (7.0, 7.0), (f64::NAN, 3.0), (f64::INFINITY, 1.0)] {
            let (min, max, step) = column(&[a, b]).axis_scale();
            assert!(min.is_finite() && max > min && step > 0.0, "{a} {b}: {min} {max} {step}");
        }
    }

    #[test]
    fn sample_ranges() {
        let c = Chart::sample(ChartType::StackedColumn);
        let (_, hi) = c.value_range();
        assert!(hi > 10.0);
        assert_eq!(Chart::sample(ChartType::Pie).series.len(), 1);
    }
}
