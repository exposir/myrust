//! Previous-frame timing statistics.

use std::time::Duration;

use super::context::Context;

/// Layout, paint, and total timings for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameStats {
    /// Layout computation time in milliseconds.
    pub layout_ms: f32,
    /// Display-list/paint preparation time in milliseconds.
    pub paint_ms: f32,
    /// Total frame production time in milliseconds.
    pub total_ms: f32,
}

impl FrameStats {
    /// Creates stats from measured durations.
    pub fn from_durations(layout: Duration, paint: Duration, total: Duration) -> Self {
        Self {
            layout_ms: layout.as_secs_f64() as f32 * 1000.0,
            paint_ms: paint.as_secs_f64() as f32 * 1000.0,
            total_ms: total.as_secs_f64() as f32 * 1000.0,
        }
    }

    /// Adds another sample to this accumulator.
    pub fn accumulate(&mut self, sample: FrameStats) {
        self.layout_ms += sample.layout_ms;
        self.paint_ms += sample.paint_ms;
        self.total_ms += sample.total_ms;
    }

    /// Compact one-line rendering.
    pub fn summary(self) -> String {
        format!(
            "Frame {:.2} ms  layout {:.2}  paint {:.2}",
            self.total_ms, self.layout_ms, self.paint_ms
        )
    }
}

/// Result of rendering the optional stats widget.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameStatsResponse {
    /// Rendered text.
    pub text: String,
}

/// Draws a compact previous-frame statistics line.
pub fn frame_stats_panel(context: &mut Context, id: &str, stats: FrameStats) -> FrameStatsResponse {
    context.frame_stats_panel(id, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::text;
    use crate::layout::Size;

    #[test]
    fn frame_stats_accumulate_and_render() {
        let mut total = FrameStats::default();
        total.accumulate(FrameStats {
            layout_ms: 1.5,
            paint_ms: 2.5,
            total_ms: 4.0,
        });
        total.accumulate(FrameStats {
            layout_ms: 0.5,
            paint_ms: 1.0,
            total_ms: 1.5,
        });
        assert_eq!(total.layout_ms, 2.0);
        assert_eq!(total.paint_ms, 3.5);
        assert_eq!(total.total_ms, 5.5);
        assert!(total.summary().starts_with("Frame 5.50 ms"));

        let size = Size::new(320.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let response = frame_stats_panel(&mut context, "stats", total);
        text(&mut context, "body");
        let frame = context.produce().unwrap();
        assert!(response.text.contains("layout 2.00"));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.label == "frame stats"));
        let produced = context.frame_stats();
        assert!(produced.layout_ms >= 0.0);
        assert!(produced.paint_ms >= 0.0);
        assert!(produced.total_ms + 0.001 >= produced.layout_ms + produced.paint_ms);
    }
}
