//! Minimal bar and line charts.

use super::context::Context;
use super::paint::Rect;

/// Chart drawing kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    /// Vertical bars.
    #[default]
    Bar,
    /// Connected line.
    Line,
}

/// Result of drawing a chart.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChartResponse {
    /// Plot rectangle in chart-local coordinates.
    pub plot: Rect,
    /// Previous-frame absolute bounds of the whole chart, when available.
    pub bounds: Option<Rect>,
    /// Number of finite data points.
    pub points: usize,
    /// Effective minimum.
    pub min: f32,
    /// Effective maximum.
    pub max: f32,
    /// Drawing kind.
    pub kind: ChartKind,
}

/// Draws a bar chart.
pub fn bar_chart(
    context: &mut Context,
    id: &str,
    values: &[f32],
    height: f32,
    min: Option<f32>,
    max: Option<f32>,
) -> ChartResponse {
    context.chart(id, values, ChartKind::Bar, height, min, max)
}

/// Draws a line chart.
pub fn line_chart(
    context: &mut Context,
    id: &str,
    values: &[f32],
    height: f32,
    min: Option<f32>,
    max: Option<f32>,
) -> ChartResponse {
    context.chart(id, values, ChartKind::Line, height, min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Size;
    use crate::paint::DrawCommand;

    #[test]
    fn bar_chart_maps_values_and_handles_empty_data() {
        let size = Size::new(320.0, 140.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let chart = bar_chart(&mut context, "bars", &[0.0, 50.0, 100.0], 100.0, None, None);
        let empty = bar_chart(&mut context, "bars-empty", &[], 100.0, None, None);
        let frame = context.produce().unwrap();
        assert_eq!(chart.min, 0.0);
        assert_eq!(chart.max, 100.0);
        assert_eq!(chart.points, 3);
        assert!(chart.plot.width > 0.0 && chart.plot.height > 0.0);
        let bars = frame
            .display
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::RoundedRect { .. }))
            .count();
        assert_eq!(bars, 2, "zero-height bar is omitted");
        assert!(frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::RoundedRect { y, height, .. }
                if (*y - chart.plot.y).abs() < 0.001 && (*height - chart.plot.height).abs() < 0.001)
        }));
        assert_eq!(empty.points, 0);
        assert_eq!(empty.min, 0.0);
        assert_eq!(empty.max, 1.0);
        assert_eq!(
            frame
                .display
                .commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::RoundedRect { .. }))
                .count(),
            bars,
            "empty chart adds no bars"
        );
    }

    #[test]
    fn line_chart_maps_points_and_returns_plot_bounds() {
        let size = Size::new(320.0, 140.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let chart = line_chart(
            &mut context,
            "line",
            &[0.0, 100.0],
            100.0,
            Some(0.0),
            Some(100.0),
        );
        let frame = context.produce().unwrap();
        assert_eq!(chart.kind, ChartKind::Line);
        assert_eq!(chart.points, 2);
        let line = frame
            .display
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    ..
                } if *width == 2.0 => Some((*x1, *y1, *x2, *y2)),
                _ => None,
            })
            .expect("data line");
        assert!((line.0 - chart.plot.x).abs() < 0.001);
        assert!((line.1 - chart.plot.bottom()).abs() < 0.001);
        assert!((line.2 - chart.plot.right()).abs() < 0.001);
        assert!((line.3 - chart.plot.y).abs() < 0.001);
        assert_eq!(
            frame
                .display
                .commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::Circle { .. }))
                .count(),
            2
        );
    }
}
