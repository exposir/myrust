//! Chart scales and plot layout, as MyGo's `ui/chart` package builds them.
//!
//! Positions along a plot are fractions of it, from 0 to 1: along x from
//! its left edge, and up y from its bottom, the way values grow.

use super::context::Context;
use super::layout::{AlignItems, JustifyContent, Style};
use super::paint::{Color, DrawCommand, Radius, Rect, TextAlignment};

/// A stretch of a plot along one axis, in fractions of it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Span {
    /// Lower end.
    pub lo: f32,
    /// Upper end.
    pub hi: f32,
}

impl Span {
    /// Returns the middle of the span.
    pub fn mid(self) -> f32 {
        (self.lo + self.hi) / 2.0
    }
}

/// A value a scale marks along an axis: where it is in fractions of the
/// plot, and its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    /// Position along the plot.
    pub at: f32,
    /// Value the tick marks.
    pub value: f64,
    /// Label the axis shows.
    pub label: String,
}

/// Maps numbers from `lo` to `hi` onto the plot, as d3's `scaleLinear`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Linear {
    /// Start of the domain.
    pub lo: f64,
    /// End of the domain.
    pub hi: f64,
}

impl Linear {
    /// Creates a linear scale from `lo` to `hi`.
    pub fn new(lo: f64, hi: f64) -> Self {
        Self { lo, hi }
    }

    /// Extends the domain to round values, those of about `count` ticks,
    /// as d3's nice.
    pub fn nice(mut self, count: i32) -> Self {
        let (mut lo, mut hi) = (self.lo, self.hi);
        let reverse = hi < lo;
        if reverse {
            std::mem::swap(&mut lo, &mut hi);
        }
        let mut previous = f64::NAN;
        for _ in 0..10 {
            let step = tick_increment(lo, hi, f64::from(count));
            if step == previous {
                break;
            }
            if step > 0.0 {
                lo = (lo / step).floor() * step;
                hi = (hi / step).ceil() * step;
            } else if step < 0.0 {
                lo = (lo * -step).ceil() / -step;
                hi = (hi * -step).floor() / -step;
            } else {
                return self;
            }
            previous = step;
        }
        if reverse {
            std::mem::swap(&mut lo, &mut hi);
        }
        self.lo = lo;
        self.hi = hi;
        self
    }

    /// Returns where `value` is along the plot.
    pub fn at(&self, value: f64) -> f32 {
        if self.hi == self.lo {
            return 0.5;
        }
        ((value - self.lo) / (self.hi - self.lo)) as f32
    }

    /// Returns the stretch from `a` to `b` along the plot.
    pub fn span(&self, a: f64, b: f64) -> Span {
        Span {
            lo: self.at(a),
            hi: self.at(b),
        }
    }

    /// Returns about `count` round values within the domain, as d3's ticks.
    pub fn ticks(&self, count: i32) -> Vec<Tick> {
        let values = tick_values(self.lo, self.hi, f64::from(count));
        let step = tick_step(self.lo, self.hi, f64::from(count)).abs();
        let mut decimals = 0;
        if step > 0.0 && step < 1.0 {
            decimals = (-(step.log10() + 1e-9).floor()).max(0.0) as i32;
            let scaled = step * 10_f64.powi(decimals);
            if scaled != scaled.round() {
                decimals += 1;
            }
        }
        values
            .into_iter()
            .map(|value| Tick {
                at: self.at(value),
                value,
                label: format!("{value:.decimals$}", decimals = decimals.max(0) as usize),
            })
            .collect()
    }
}

/// Returns the smallest and largest of `values`, or 0 and 1 for none.
pub fn extent(values: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for value in values.iter().copied().filter(|value| !value.is_nan()) {
        lo = lo.min(value);
        hi = hi.max(value);
    }
    if lo > hi {
        return (0.0, 1.0);
    }
    (lo, hi)
}

/// Divides the plot into a band for each label, as d3's `scaleBand`.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    /// Labels of the bands, in their order.
    pub labels: Vec<String>,
    /// Room between bands, in fractions of a band's step.
    pub inner: f32,
    /// Room before the first and after the last band.
    pub outer: f32,
    /// Where the bands sit in the room the outer padding leaves.
    pub align: f32,
}

impl Band {
    /// Creates a band scale of labels, without padding.
    pub fn new(labels: &[&str]) -> Self {
        Self {
            labels: labels.iter().map(|label| label.to_string()).collect(),
            inner: 0.0,
            outer: 0.0,
            align: 0.5,
        }
    }

    /// Sets the room between bands and before and after them.
    pub fn padding(mut self, inner: f32, outer: f32) -> Self {
        self.inner = inner.clamp(0.0, 1.0);
        self.outer = outer.max(0.0);
        self
    }

    fn step(&self) -> (f32, f32) {
        let count = self.labels.len() as f32;
        let step = 1.0 / (count - self.inner + 2.0 * self.outer).max(1.0);
        let start = (1.0 - step * (count - self.inner)) * self.align;
        (step, start)
    }

    /// Returns the width of a band in fractions of the plot.
    pub fn width(&self) -> f32 {
        let (step, _) = self.step();
        step * (1.0 - self.inner)
    }

    /// Returns the stretch of the `index`-th band.
    pub fn band(&self, index: usize) -> Span {
        let (step, start) = self.step();
        let lo = start + step * index as f32;
        Span {
            lo,
            hi: lo + self.width(),
        }
    }

    /// Returns the middle of the `index`-th band.
    pub fn at(&self, index: usize) -> f32 {
        self.band(index).mid()
    }

    /// Returns a tick in the middle of each band, labeled with its label.
    pub fn ticks(&self) -> Vec<Tick> {
        self.labels
            .iter()
            .enumerate()
            .map(|(index, label)| Tick {
                at: self.at(index),
                value: index as f64,
                label: label.clone(),
            })
            .collect()
    }
}

/// Tick spacing d3-array uses: steps of 1, 2 or 5 times a power of ten.
/// The thresholds are sqrt(50), sqrt(10) and sqrt(2), as in d3-array's
/// `e10`, `e5` and `e2`.
const E10: f64 = 7.071_067_811_865_475_5;
const E5: f64 = 3.162_277_660_168_379_5;
const E2: f64 = std::f64::consts::SQRT_2;

fn tick_spec(lo: f64, hi: f64, count: f64) -> (f64, f64, f64) {
    let step = (hi - lo) / count.max(0.0);
    let power = step.log10().floor();
    let error = step / 10_f64.powf(power);
    let factor = if error >= E10 {
        10.0
    } else if error >= E5 {
        5.0
    } else if error >= E2 {
        2.0
    } else {
        1.0
    };
    if power < 0.0 {
        let inc = 10_f64.powf(-power) / factor;
        let mut first = (lo * inc).round();
        let mut last = (hi * inc).round();
        if first / inc < lo {
            first += 1.0;
        }
        if last / inc > hi {
            last -= 1.0;
        }
        (first, last, -inc)
    } else {
        let inc = 10_f64.powf(power) * factor;
        let mut first = (lo / inc).round();
        let mut last = (hi / inc).round();
        if first * inc < lo {
            first += 1.0;
        }
        if last * inc > hi {
            last -= 1.0;
        }
        (first, last, inc)
    }
}

fn tick_increment(lo: f64, hi: f64, count: f64) -> f64 {
    tick_spec(lo, hi, count).2
}

fn tick_step(lo: f64, hi: f64, count: f64) -> f64 {
    let reverse = hi < lo;
    let (lo, hi) = if reverse { (hi, lo) } else { (lo, hi) };
    let mut inc = tick_increment(lo, hi, count);
    if inc < 0.0 {
        inc = 1.0 / -inc;
    }
    if reverse {
        -inc
    } else {
        inc
    }
}

fn tick_values(lo: f64, hi: f64, count: f64) -> Vec<f64> {
    if !(count > 0.0) || lo.is_nan() || hi.is_nan() || lo.is_infinite() || hi.is_infinite() {
        return Vec::new();
    }
    if lo == hi {
        return vec![lo];
    }
    let reverse = hi < lo;
    let (lo, hi) = if reverse { (hi, lo) } else { (lo, hi) };
    let (first, last, inc) = tick_spec(lo, hi, count);
    if !(last >= first) || inc.is_infinite() || inc.is_nan() {
        return Vec::new();
    }
    let n = (last - first) as i64 + 1;
    (0..n)
        .map(|i| {
            let k = if reverse { n - 1 - i } else { i } as f64;
            if inc < 0.0 {
                (first + k) / -inc
            } else {
                (first + k) * inc
            }
        })
        .collect()
}

/// How the labels of a plot's axes look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisStyle {
    /// Label font size.
    pub font_size: f32,
    /// Label color.
    pub color: Color,
    /// Room between the plot and its labels across.
    pub gap_x: f32,
    /// Room between the plot and its labels down.
    pub gap_y: f32,
}

impl Default for AxisStyle {
    fn default() -> Self {
        Self {
            font_size: 11.0,
            color: Color::rgb(113, 113, 122),
            gap_x: 8.0,
            gap_y: 6.0,
        }
    }
}

/// Result of laying out a plot.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlotResponse {
    /// The plot's box, in coordinates local to the plot's root.
    pub area: Rect,
}

/// Lays out a plot with labels down its left side and under it, then hands
/// the area to `build` for its marks.
///
/// The area is the room the labels and gaps leave, as MyGo's `chart.Plot`
/// grid gives its middle cell.
pub fn plot(
    context: &mut Context,
    id: &str,
    left: &[Tick],
    bottom: &[Tick],
    style: AxisStyle,
    build: impl FnOnce(&mut Context, Rect),
) -> PlotResponse {
    plot_sized(context, id, left, bottom, style, 0.0, build)
}

/// Lays out a plot `height` tall, or as tall as the room it is given when
/// that is 0, then hands the area to `build` for its marks.
pub fn plot_sized(
    context: &mut Context,
    id: &str,
    left: &[Tick],
    bottom: &[Tick],
    style: AxisStyle,
    height: f32,
    build: impl FnOnce(&mut Context, Rect),
) -> PlotResponse {
    let width = context.current_content_width();
    let height = if height > 0.0 {
        height
    } else {
        context.current_content_height()
    };
    let left_width = left
        .iter()
        .map(|tick| crate::context::measure_text_width(&tick.label, style.font_size))
        .fold(0.0_f32, f32::max);
    // MyGo lays a chart label out with the font's own metrics, one line
    // box of about 1.2 times the size (`internal/text` metricsOf), which is
    // a pixel shorter than body text at 11px.
    let line_height = (style.font_size * 1.2).round();
    let area = Rect::new(
        if left.is_empty() {
            0.0
        } else {
            left_width + style.gap_x
        },
        if bottom.is_empty() { 0.0 } else { 0.0 },
        (width - left_width - if left.is_empty() { 0.0 } else { style.gap_x }).max(1.0),
        (height - line_height - if bottom.is_empty() { 0.0 } else { style.gap_y }).max(1.0),
    );
    let root = context.new_node(Style {
        width: Some(width),
        height: Some(height),
        ..Style::default()
    });
    context.set_node_key(root, id);
    context.push_parent(root);

    // Left labels: right-aligned beside the area, centered on their ticks.
    for (index, tick) in left.iter().enumerate() {
        let y = area.y + (1.0 - tick.at) * area.height;
        plot_label(
            context,
            &format!("{id}:left:{index}"),
            &tick.label,
            style,
            Rect::new(0.0, y - line_height / 2.0, left_width, line_height),
            TextAlignment::End,
        );
    }
    // Bottom labels: centered on their ticks, under the area.
    for (index, tick) in bottom.iter().enumerate() {
        let center = area.x + tick.at * area.width;
        let label_width = crate::context::measure_text_width(&tick.label, style.font_size);
        plot_label(
            context,
            &format!("{id}:bottom:{index}"),
            &tick.label,
            style,
            Rect::new(
                center - label_width / 2.0,
                area.y + area.height + style.gap_y,
                label_width,
                line_height,
            ),
            TextAlignment::Start,
        );
    }

    let area_node = context.new_node(Style {
        position: super::layout::Position::Absolute,
        left: Some(area.x),
        top: Some(area.y),
        width: Some(area.width),
        height: Some(area.height),
        ..Style::default()
    });
    context.push_parent(area_node);
    // The area's own box starts at its own origin, so marks place
    // themselves in fractions of the area, as MyGo's marks do.
    build(context, Rect::new(0.0, 0.0, area.width, area.height));
    context.pop_parent();
    context.pop_parent();
    PlotResponse { area }
}

/// Draws a bar, a band or a cell of the plot, as MyGo's `chart.Rect`.
pub fn rect(context: &mut Context, id: &str, x: Span, y: Span, color: Color, radius: f32) {
    let lo = x.lo.min(x.hi);
    let hi = x.lo.max(x.hi);
    let bottom = y.lo.min(y.hi);
    let top = y.lo.max(y.hi);
    plot_mark(
        context,
        id,
        Rect::new(lo, 1.0 - top, hi - lo, top - bottom),
        color,
        radius,
    );
}

/// Draws a mark of `w` by `h` DIPs centered on a point of the plot, as
/// MyGo's `chart.Point`.
pub fn point(context: &mut Context, id: &str, x: f32, y: f32, w: f32, h: f32, color: Color) {
    plot_mark(
        context,
        id,
        Rect::new(x - w / 2.0, 1.0 - y - h / 2.0, w, h),
        color,
        w.min(h) / 2.0,
    );
}

/// Draws a vertical rule across the plot at `x`, as MyGo's `chart.RuleX`.
pub fn rule_x(context: &mut Context, id: &str, x: f32, width: f32, color: Color) {
    plot_mark(
        context,
        id,
        Rect::new(x - width / 2.0, 0.0, width, 1.0),
        color,
        0.0,
    );
}

/// Points of a line through `xs` and `ys`, joined as `curve` says, in the
/// plot's box.
///
/// A `NaN` value breaks the line there, as d3's `line.defined`.
pub fn line_path(area: Rect, xs: &[f32], ys: &[f32], curve: Curve) -> String {
    let mut path = String::new();
    let count = xs.len().min(ys.len());
    for (from, to) in runs(count, |index| defined(&[xs[index], ys[index]])) {
        let points: Vec<(f32, f32)> = (from..to)
            .map(|index| {
                (
                    area.x + xs[index] * area.width,
                    area.y + (1.0 - ys[index]) * area.height,
                )
            })
            .collect();
        push_points(&mut path, &points, curve);
    }
    path
}

/// Path of the area between two lines through `xs`, as MyGo's
/// `chart.Area`; `None` puts the bottom line at the plot's bottom.
pub fn area_path(area: Rect, xs: &[f32], lo: Option<&[f32]>, hi: &[f32], curve: Curve) -> String {
    let mut path = String::new();
    let count = match lo {
        Some(lo) => xs.len().min(hi.len()).min(lo.len()),
        None => xs.len().min(hi.len()),
    };
    for (from, to) in runs(count, |index| {
        defined(&[xs[index], hi[index], lo.map_or(0.0, |lo| lo[index])])
    }) {
        let top: Vec<(f32, f32)> = (from..to)
            .map(|index| {
                (
                    area.x + xs[index] * area.width,
                    area.y + (1.0 - hi[index]) * area.height,
                )
            })
            .collect();
        let bottom: Vec<(f32, f32)> = (from..to)
            .map(|index| {
                let low = lo.map_or(0.0, |lo| lo[index]);
                (
                    area.x + xs[index] * area.width,
                    area.y + (1.0 - low) * area.height,
                )
            })
            .collect();
        push_points(&mut path, &top, curve);
        // Back along the bottom line, the same curve the other way.
        let segments = segments(&bottom, curve);
        if let Some(last) = bottom.last() {
            path.push_str(&format!(" L{:.2} {:.2}", last.0, last.1));
        }
        for (index, segment) in segments.iter().enumerate().rev() {
            let from = if index > 0 {
                segments[index - 1].to
            } else {
                bottom[0]
            };
            if segment.cubic {
                path.push_str(&format!(
                    " C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
                    segment.c2.0, segment.c2.1, segment.c1.0, segment.c1.1, from.0, from.1
                ));
            } else {
                path.push_str(&format!(" L{:.2} {:.2}", from.0, from.1));
            }
        }
        path.push_str(" Z");
    }
    path
}

/// How a line goes from a point to the next, as d3's curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curve {
    /// Straight lines between points.
    #[default]
    Linear,
    /// A smooth curve that never overshoots the points' values, as d3's
    /// `curveMonotoneX` (Steffen's method).
    Monotone,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    cubic: bool,
    c1: (f32, f32),
    c2: (f32, f32),
    to: (f32, f32),
}

fn segments(points: &[(f32, f32)], curve: Curve) -> Vec<Segment> {
    let count = points.len();
    if count < 2 {
        return Vec::new();
    }
    match curve {
        Curve::Monotone if count >= 3 => {
            let mut slopes = vec![0.0_f32; count];
            for index in 1..count - 1 {
                slopes[index] =
                    monotone_slope(points[index - 1], points[index], points[index + 1]);
            }
            slopes[0] = end_slope(points[0], points[1], slopes[1]);
            slopes[count - 1] = end_slope(points[count - 2], points[count - 1], slopes[count - 2]);
            (1..count)
                .map(|index| {
                    let (p0, p1) = (points[index - 1], points[index]);
                    let dx = (p1.0 - p0.0) / 3.0;
                    Segment {
                        cubic: true,
                        c1: (p0.0 + dx, p0.1 + dx * slopes[index - 1]),
                        c2: (p1.0 - dx, p1.1 - dx * slopes[index]),
                        to: p1,
                    }
                })
                .collect()
        }
        _ => points[1..]
            .iter()
            .map(|point| Segment {
                cubic: false,
                c1: (0.0, 0.0),
                c2: (0.0, 0.0),
                to: *point,
            })
            .collect(),
    }
}

fn push_points(path: &mut String, points: &[(f32, f32)], curve: Curve) {
    let Some(first) = points.first() else {
        return;
    };
    path.push_str(&format!("M{:.2} {:.2}", first.0, first.1));
    for segment in segments(points, curve) {
        if segment.cubic {
            path.push_str(&format!(
                " C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}",
                segment.c1.0, segment.c1.1, segment.c2.0, segment.c2.1, segment.to.0, segment.to.1
            ));
        } else {
            path.push_str(&format!(" L{:.2} {:.2}", segment.to.0, segment.to.1));
        }
    }
}

/// Slope at `b` between `a` and `c`, as d3's `slope3`.
fn monotone_slope(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let h0 = b.0 - a.0;
    let h1 = c.0 - b.0;
    if h0 == 0.0 || h1 == 0.0 {
        return 0.0;
    }
    let s0 = (b.1 - a.1) / h0;
    let s1 = (c.1 - b.1) / h1;
    let p = (s0 * h1 + s1 * h0) / (h0 + h1);
    if s0.is_sign_negative() != s1.is_sign_negative() || s0 == 0.0 || s1 == 0.0 {
        return 0.0;
    }
    let m = s0.abs().min(s1.abs()).min(0.5 * p.abs());
    if s0 < 0.0 {
        -2.0 * m
    } else {
        2.0 * m
    }
}

/// Slope at an end of a line, as d3's `slope2`.
fn end_slope(a: (f32, f32), b: (f32, f32), t: f32) -> f32 {
    let h = b.0 - a.0;
    if h == 0.0 {
        return t;
    }
    (3.0 * (b.1 - a.1) / h - t) / 2.0
}

fn defined(values: &[f32]) -> bool {
    values.iter().all(|value| !value.is_nan())
}

fn runs(count: usize, ok: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from: Option<usize> = None;
    for index in 0..=count {
        if index < count && ok(index) {
            from.get_or_insert(index);
            continue;
        }
        if let Some(start) = from.take() {
            out.push((start, index));
        }
    }
    out
}

/// A slice of a pie: from `start` to `end` radians clockwise from twelve
/// o'clock, with `pad` between it and the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slice {
    /// Start angle.
    pub start: f64,
    /// End angle.
    pub end: f64,
    /// Gap to the next slice.
    pub pad: f64,
}

/// Divides a circle among values, each slice as large as its share of
/// their sum, as MyGo's `chart.Pie`.
pub fn pie(values: &[f64], pad: f64) -> Vec<Slice> {
    let sum: f64 = values.iter().copied().filter(|value| *value > 0.0).sum();
    let count = values.len() as f64;
    let pad = pad.max(0.0).min(2.0 * std::f64::consts::PI / count.max(1.0));
    let scale = if sum > 0.0 {
        (2.0 * std::f64::consts::PI - count * pad) / sum
    } else {
        0.0
    };
    let mut angle = 0.0;
    values
        .iter()
        .map(|value| {
            let size = if *value > 0.0 { value * scale } else { 0.0 };
            let slice = Slice {
                start: angle,
                end: angle + size,
                pad,
            };
            angle += size + pad;
            slice
        })
        .collect()
}

/// Path of a pie slice's sector between the radii `inner` and `outer`,
/// centered on `(cx, cy)`, as MyGo's `chart.Sector`.
pub fn sector_path(cx: f32, cy: f32, inner: f32, outer: f32, slice: Slice) -> String {
    let point = |radius: f32, angle: f64| {
        (
            cx + radius * (angle - std::f64::consts::FRAC_PI_2).cos() as f32,
            cy + radius * (angle - std::f64::consts::FRAC_PI_2).sin() as f32,
        )
    };
    let (start, end) = (slice.start, slice.end);
    let full = (end - start) >= 2.0 * std::f64::consts::PI - 1e-9;
    let large = u8::from(end - start > std::f64::consts::PI);
    let (ax, ay) = point(outer, start);
    let (bx, by) = point(outer, end);
    let (cx2, cy2) = point(inner, end);
    let (dx, dy) = point(inner, start);
    if full {
        let mid = start + (end - start) / 2.0;
        let (mx, my) = point(outer, mid);
        let (nx, ny) = point(inner, mid);
        return format!(
            "M{ax:.2} {ay:.2} A{outer:.2} {outer:.2} 0 1 1 {mx:.2} {my:.2} A{outer:.2} {outer:.2} 0 1 1 {ax:.2} {ay:.2} Z \
             M{dx:.2} {dy:.2} A{inner:.2} {inner:.2} 0 1 0 {nx:.2} {ny:.2} A{inner:.2} {inner:.2} 0 1 0 {dx:.2} {dy:.2} Z"
        );
    }
    if inner <= 0.0 {
        return format!(
            "M{cx:.2} {cy:.2} L{ax:.2} {ay:.2} A{outer:.2} {outer:.2} 0 {large} 1 {bx:.2} {by:.2} Z"
        );
    }
    format!(
        "M{ax:.2} {ay:.2} A{outer:.2} {outer:.2} 0 {large} 1 {bx:.2} {by:.2} \
         L{cx2:.2} {cy2:.2} A{inner:.2} {inner:.2} 0 {large} 0 {dx:.2} {dy:.2} Z"
    )
}

/// Colors of a filled path, as a `DrawCommand`.
pub fn fill_path(path: String, color: Color) -> DrawCommand {
    DrawCommand::SvgPath {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        path,
        fill: Some(color),
        stroke: None,
        stroke_width: 0.0,
    }
}

/// Colors of a stroked path, as a `DrawCommand`.
pub fn stroke_path(path: String, width: f32, color: Color) -> DrawCommand {
    DrawCommand::SvgPath {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        path,
        fill: None,
        stroke: Some(color),
        stroke_width: width,
    }
}

/// A rounded mark in the plot, as `DrawCommand`.
pub fn mark_commands(rect: Rect, color: Color, radius: f32) -> DrawCommand {
    DrawCommand::RoundedRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        radius: Radius::all(radius),
        color,
    }
}

/// Alignment of a plot's area children, kept for callers that lay out
/// their own marks.
pub const MARK_ALIGN: AlignItems = AlignItems::Stretch;
/// Justification of a plot's area children.
pub const MARK_JUSTIFY: JustifyContent = JustifyContent::FlexStart;

/// Places a label of a plot's axis in its own box, as MyGo's axis boxes
/// hold a zero-sized slot that centers the text on its tick.
fn plot_label(
    context: &mut Context,
    id: &str,
    text: &str,
    style: AxisStyle,
    rect: Rect,
    alignment: TextAlignment,
) {
    let node = context.new_node(Style {
        position: super::layout::Position::Absolute,
        left: Some(rect.x),
        top: Some(rect.y),
        width: Some(rect.width),
        height: Some(rect.height),
        ..Style::default()
    });
    context.set_node_key(node, id);
    let line_height = (style.font_size * 1.2).round();
    context.set_node_text(
        node,
        text,
        style.font_size,
        style.color,
        Some(rect.width),
        alignment,
        ((rect.height - line_height) / 2.0).max(0.0),
    );
}

/// Paints a mark of a plot in the plot's area: `rect` is in fractions of
/// the area, and the mark's box is placed in it.
fn plot_mark(context: &mut Context, id: &str, rect: Rect, color: Color, radius: f32) {
    let area_width = context.current_content_width();
    let area_height = context.current_content_height();
    let width = (rect.width * area_width).max(0.0);
    let height = (rect.height * area_height).max(0.0);
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let node = context.new_node(Style {
        position: super::layout::Position::Absolute,
        left: Some(rect.x * area_width),
        top: Some(rect.y * area_height),
        width: Some(width),
        height: Some(height),
        ..Style::default()
    });
    context.set_node_key(node, id);
    context.set_node_fill(node, color, radius);
}

/// A value's stretch in a stack, from `lo` up to `hi`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StackSegment {
    /// Bottom of the stretch.
    pub lo: f64,
    /// Top of the stretch.
    pub hi: f64,
}

/// Piles series up, as stacked bars and areas show them.
///
/// `series[s][i]` is the `i`-th value of the `s`-th series, and the
/// result's `[s][i]` is where it goes in the stack of the `i`-th values,
/// the `s`-th from the bottom. `NaN` values take no room, and negative
/// ones pile down from zero, as MyGo's `chart.Stack`.
pub fn stack(series: &[Vec<f64>]) -> Vec<Vec<StackSegment>> {
    let count = series.iter().map(|values| values.len()).max().unwrap_or(0);
    let mut out = vec![vec![StackSegment::default(); count]; series.len()];
    let value = |s: usize, i: usize| -> f64 {
        match series.get(s).and_then(|values| values.get(i)) {
            Some(value) if !value.is_nan() => *value,
            _ => 0.0,
        }
    };
    for i in 0..count {
        let mut up = 0.0;
        let mut down = 0.0;
        for s in 0..series.len() {
            let v = value(s, i);
            if v < 0.0 {
                out[s][i] = StackSegment {
                    lo: down + v,
                    hi: down,
                };
                down += v;
            } else {
                out[s][i] = StackSegment { lo: up, hi: up + v };
                up += v;
            }
        }
    }
    out
}

/// Paints a one-pixel rule across the plot, as the demo's `chartGrid` does.
pub fn grid_line(context: &mut Context, id: &str, x1: f32, y1: f32, x2: f32, y2: f32, color: Color) {
    let node = context.new_node(Style {
        position: super::layout::Position::Absolute,
        left: Some(0.0),
        top: Some(0.0),
        width: Some(0.0),
        height: Some(0.0),
        ..Style::default()
    });
    context.set_node_key(node, id);
    context.set_node_commands(
        node,
        vec![DrawCommand::Line {
            x1,
            // A one-pixel rule covers whole device pixels only where its
            // center sits on a pixel's middle, as MyGo's Painter.snap does.
            y1,
            x2,
            y2,
            width: 1.0,
            color,
        }],
    );
}

/// Paints a path in the plot's own coordinates, as a line or an area.
pub fn path_mark(context: &mut Context, id: &str, command: DrawCommand) {
    let node = context.new_node(Style {
        position: super::layout::Position::Absolute,
        left: Some(0.0),
        top: Some(0.0),
        width: Some(0.0),
        height: Some(0.0),
        ..Style::default()
    });
    context.set_node_key(node, id);
    context.set_node_commands(node, vec![command]);
}

/// A `DrawCommand` that strokes a path in a color.
pub fn stroked_path(path: String, width: f32, color: Color) -> DrawCommand {
    stroke_path(path, width, color)
}

/// A `DrawCommand` that fills a path in a color.
pub fn filled_path(path: String, color: Color) -> DrawCommand {
    fill_path(path, color)
}

/// Maps times onto the plot, as d3's `scaleTime`.
///
/// Ticks fall on round times: a second, a quarter of an hour, a day, the
/// first of a month, chosen for the span, as MyGo's `chart.Time`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeScale {
    /// Start of the domain, in milliseconds since the Unix epoch.
    pub from: i64,
    /// End of the domain.
    pub to: i64,
}

impl TimeScale {
    /// Creates a time scale from `from` to `to`.
    pub fn new(from: i64, to: i64) -> Self {
        Self { from, to }
    }

    /// Returns where `time` is along the plot.
    pub fn at(&self, time: i64) -> f32 {
        let span = self.to - self.from;
        if span == 0 {
            return 0.5;
        }
        ((time - self.from) as f64 / span as f64) as f32
    }

    /// Returns about `count` round times within the domain.
    pub fn ticks(&self, count: i32) -> Vec<Tick> {
        let (lo, hi) = if self.to < self.from {
            (self.to, self.from)
        } else {
            (self.from, self.to)
        };
        if count <= 0 || lo >= hi {
            return Vec::new();
        }
        let interval = TimeInterval::for_span(lo, hi, count);
        let mut out = Vec::new();
        let mut time = interval.floor(lo);
        while time <= hi && out.len() < 1000 {
            if time >= lo {
                out.push(Tick {
                    at: self.at(time),
                    value: time as f64,
                    label: format_time(time),
                });
            }
            time = interval.next(time);
        }
        if self.to < self.from {
            out.reverse();
        }
        out
    }
}

/// A step of time ticks: every `n` units of a calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeInterval {
    unit: TimeUnit,
    n: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TimeUnit {
    Milli,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

const MILLI: i64 = 1;
const SECOND: i64 = 1000;
const MINUTE: i64 = 60 * SECOND;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// d3-time's tick intervals, by their length.
const INTERVALS: [(TimeUnit, i64, i64); 18] = [
    (TimeUnit::Second, 1, SECOND),
    (TimeUnit::Second, 5, 5 * SECOND),
    (TimeUnit::Second, 15, 15 * SECOND),
    (TimeUnit::Second, 30, 30 * SECOND),
    (TimeUnit::Minute, 1, MINUTE),
    (TimeUnit::Minute, 5, 5 * MINUTE),
    (TimeUnit::Minute, 15, 15 * MINUTE),
    (TimeUnit::Minute, 30, 30 * MINUTE),
    (TimeUnit::Hour, 1, HOUR),
    (TimeUnit::Hour, 3, 3 * HOUR),
    (TimeUnit::Hour, 6, 6 * HOUR),
    (TimeUnit::Hour, 12, 12 * HOUR),
    (TimeUnit::Day, 1, DAY),
    (TimeUnit::Day, 2, 2 * DAY),
    (TimeUnit::Week, 1, 7 * DAY),
    (TimeUnit::Month, 1, 30 * DAY),
    (TimeUnit::Month, 3, 90 * DAY),
    (TimeUnit::Year, 1, 365 * DAY),
];

impl TimeInterval {
    /// Returns the interval of about `count` ticks from `lo` to `hi`, as
    /// d3-time's `tickInterval`.
    fn for_span(lo: i64, hi: i64, count: i32) -> Self {
        let target = (hi - lo) as f64 / f64::from(count.max(1));
        let index = INTERVALS
            .iter()
            .position(|(_, _, length)| *length as f64 > target);
        match index {
            None => {
                let years = hi.div_euclid(DAY) / 365 - lo.div_euclid(DAY) / 365;
                let step = tick_step(0.0, years as f64, f64::from(count)).abs().max(1.0);
                Self {
                    unit: TimeUnit::Year,
                    n: step.round() as i64,
                }
            }
            Some(0) => Self {
                unit: TimeUnit::Milli,
                n: tick_step(0.0, (hi - lo) as f64, f64::from(count))
                    .round()
                    .max(1.0) as i64,
            },
            Some(index) => {
                let previous = INTERVALS[index - 1];
                let current = INTERVALS[index];
                let chosen = if target / (previous.2 as f64) < (current.2 as f64) / target {
                    previous
                } else {
                    current
                };
                Self {
                    unit: chosen.0,
                    n: chosen.1,
                }
            }
        }
    }

    /// Returns the latest round time at or before `time`.
    fn floor(self, time: i64) -> i64 {
        let day_index = time.div_euclid(DAY);
        let (year, month, day) = civil_from_days(day_index);
        match self.unit {
            TimeUnit::Milli => time - time.rem_euclid(self.n),
            TimeUnit::Second => day_index * DAY + (time.rem_euclid(DAY) / (self.n * SECOND)) * (self.n * SECOND),
            TimeUnit::Minute => day_index * DAY + (time.rem_euclid(DAY) / (self.n * MINUTE)) * (self.n * MINUTE),
            TimeUnit::Hour => day_index * DAY + (time.rem_euclid(DAY) / (self.n * HOUR)) * (self.n * HOUR),
            TimeUnit::Day => days_from_civil((year, month, day - (day - 1) % self.n as i32)) * DAY,
            TimeUnit::Week => {
                // Weeks start on Sunday, as d3's timeWeek; the epoch was a
                // Thursday, so day 0 is three days before its Sunday.
                let weekday = (day_index + 4).rem_euclid(7);
                (day_index - weekday) * DAY
            }
            TimeUnit::Month => {
                let index = i64::from(month) - 1;
                let month = index - index.rem_euclid(self.n) + 1;
                days_from_civil((year, month as i32, 1)) * DAY
            }
            TimeUnit::Year => {
                let year = i64::from(year);
                days_from_civil(((year - year.rem_euclid(self.n)) as i32, 1, 1)) * DAY
            }
        }
    }

    /// Returns the round time after `time`.
    fn next(self, time: i64) -> i64 {
        match self.unit {
            TimeUnit::Milli => time + self.n * MILLI,
            TimeUnit::Second | TimeUnit::Minute => time + self.n * unit_length(self.unit),
            TimeUnit::Hour => time + self.n * HOUR,
            TimeUnit::Day => {
                let (year, month, day) = civil_from_days(time.div_euclid(DAY));
                let next = civil_from_days(time.div_euclid(DAY) + self.n);
                let days = days_from_civil((year, month, day));
                let _ = days;
                if next.1 != month {
                    days_from_civil((next.0, next.1, 1)) * DAY
                } else {
                    days_from_civil(next) * DAY
                }
            }
            TimeUnit::Week => time + 7 * self.n * DAY,
            TimeUnit::Month => {
                let (year, month, _) = civil_from_days(time.div_euclid(DAY));
                let total = year as i64 * 12 + (month as i64 - 1) + self.n;
                let (year, month) = (total.div_euclid(12) as i32, total.rem_euclid(12) as i32 + 1);
                days_from_civil((year, month, 1)) * DAY
            }
            TimeUnit::Year => {
                let (year, _, _) = civil_from_days(time.div_euclid(DAY));
                days_from_civil((year + self.n as i32, 1, 1)) * DAY
            }
        }
    }
}

fn unit_length(unit: TimeUnit) -> i64 {
    match unit {
        TimeUnit::Second => SECOND,
        TimeUnit::Minute => MINUTE,
        TimeUnit::Hour => HOUR,
        TimeUnit::Day => DAY,
        TimeUnit::Week => 7 * DAY,
        _ => MILLI,
    }
}

/// Days since 1970-01-01 of a civil date, as Howard Hinnant's `days_from_civil`.
fn days_from_civil((year, month, day): (i32, i32, i32)) -> i64 {
    let year = if month <= 2 { year - 1 } else { year } as i64;
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date of a day count, as Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i32, i32, i32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as i32;
    let month = (mp + if mp < 10 { 3 } else { -9 }) as i32;
    (
        (year + i64::from(month <= 2)) as i32,
        month,
        day,
    )
}

const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Labels a tick with the finest unit it is not round in, as d3's
/// multi-scale time format.
fn format_time(time: i64) -> String {
    let days = time.div_euclid(DAY);
    let (year, month, day) = civil_from_days(days);
    let millis = time.rem_euclid(DAY);
    let hour = millis / HOUR;
    let minute = (millis % HOUR) / MINUTE;
    let second = (millis % MINUTE) / SECOND;
    if millis % SECOND != 0 {
        return format!("{second}.{:03}", millis % SECOND);
    }
    if second != 0 {
        return format!("{minute:02}:{second:02}");
    }
    if minute != 0 {
        return format!("{hour:02}:{minute:02}");
    }
    if hour != 0 {
        return format!("{hour:02}:00");
    }
    if day != 1 {
        return format!("{} {day}", MONTH_NAMES[(month - 1) as usize]);
    }
    if month != 1 {
        return MONTH_NAMES[(month - 1) as usize].to_string();
    }
    format!("{year}")
}
