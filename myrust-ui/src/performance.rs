//! Previous-frame timing statistics and runtime reporting.

use std::time::Duration;

use super::context::Context;

/// Build, layout, paint, present, and total timings for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameStats {
    /// View callback time in milliseconds.
    pub build_ms: f32,
    /// Layout computation time in milliseconds.
    pub layout_ms: f32,
    /// Display-list/paint preparation time in milliseconds.
    pub paint_ms: f32,
    /// CPU rasterization or GPU presentation time in milliseconds.
    pub present_ms: f32,
    /// Total frame production time in milliseconds.
    pub total_ms: f32,
}

impl FrameStats {
    /// Creates stats from measured durations.
    ///
    /// This keeps the original three-sample API. Use
    /// [`Self::from_breakdown`] when build and present time are available.
    pub fn from_durations(layout: Duration, paint: Duration, total: Duration) -> Self {
        Self::from_breakdown(Duration::ZERO, layout, paint, Duration::ZERO, total)
    }

    /// Creates stats from the complete frame breakdown.
    pub fn from_breakdown(
        build: Duration,
        layout: Duration,
        paint: Duration,
        present: Duration,
        total: Duration,
    ) -> Self {
        Self {
            build_ms: build.as_secs_f64() as f32 * 1000.0,
            layout_ms: layout.as_secs_f64() as f32 * 1000.0,
            paint_ms: paint.as_secs_f64() as f32 * 1000.0,
            present_ms: present.as_secs_f64() as f32 * 1000.0,
            total_ms: total.as_secs_f64() as f32 * 1000.0,
        }
    }

    /// Adds another sample to this accumulator.
    pub fn accumulate(&mut self, sample: FrameStats) {
        self.build_ms += sample.build_ms;
        self.layout_ms += sample.layout_ms;
        self.paint_ms += sample.paint_ms;
        self.present_ms += sample.present_ms;
        self.total_ms += sample.total_ms;
    }

    /// Records post-production presentation time.
    ///
    /// The time is added to `present_ms` and `total_ms` so callers can add
    /// separate surface-present or readback costs.
    pub fn record_present(&mut self, present: Duration) {
        let present_ms = present.as_secs_f64() as f32 * 1000.0;
        self.present_ms += present_ms;
        self.total_ms += present_ms;
    }

    /// Replaces `present_ms` and adjusts `total_ms` by the delta.
    pub fn set_present_ms(&mut self, present_ms: f32) {
        let present_ms = if present_ms.is_finite() {
            present_ms.max(0.0)
        } else {
            0.0
        };
        self.total_ms += present_ms - self.present_ms;
        self.present_ms = present_ms;
    }

    /// Compact one-line rendering.
    pub fn summary(self) -> String {
        let glyph = crate::present::glyph_cache_stats();
        let text = crate::text::text_layout_cache_stats();
        format!(
            "Frame {:.2} ms  build {:.2}  layout {:.2}  paint {:.2}  present {:.2}  glyph {}/{}  text {}/{}",
            self.total_ms,
            self.build_ms,
            self.layout_ms,
            self.paint_ms,
            self.present_ms,
            glyph.hits,
            glyph.misses,
            text.hits,
            text.misses
        )
    }
}

/// Parsed `MYRUST_FRAME_STATS` setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameStatsFilter {
    /// Disable frame statistics output.
    Off,
    /// Emit every frame.
    All,
    /// Emit frames whose total time is at least this many milliseconds.
    Threshold(f32),
}

impl FrameStatsFilter {
    /// Parses `all`, `off`, a bare millisecond value, or a value ending in `ms`.
    ///
    /// An unset, empty, invalid or negative value disables output. Parsing is
    /// intentionally total so a typo cannot change rendering or abort the
    /// application.
    pub fn parse(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Off;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("off") {
            return Self::Off;
        }
        if trimmed.eq_ignore_ascii_case("all") {
            return Self::All;
        }
        let number = trimmed
            .strip_suffix("ms")
            .or_else(|| trimmed.strip_suffix("MS"))
            .unwrap_or(trimmed)
            .trim();
        match number.parse::<f32>() {
            Ok(milliseconds) if milliseconds.is_finite() && milliseconds >= 0.0 => {
                Self::Threshold(milliseconds)
            }
            _ => Self::Off,
        }
    }

    /// Reads the current process environment.
    pub fn from_env() -> Self {
        Self::parse(std::env::var("MYRUST_FRAME_STATS").ok().as_deref())
    }

    /// Returns whether this frame should be written.
    pub fn should_emit(self, stats: FrameStats) -> bool {
        match self {
            Self::Off => false,
            Self::All => true,
            Self::Threshold(milliseconds) => stats.total_ms >= milliseconds,
        }
    }

    /// Writes a one-line sample to stderr when the filter accepts it.
    pub fn emit(self, stats: FrameStats) {
        if self.should_emit(stats) {
            eprintln!("myrust: {}", stats.summary());
        }
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
            build_ms: 1.0,
            layout_ms: 1.5,
            paint_ms: 2.5,
            present_ms: 0.5,
            total_ms: 5.5,
        });
        total.accumulate(FrameStats {
            build_ms: 0.25,
            layout_ms: 0.5,
            paint_ms: 1.0,
            present_ms: 0.25,
            total_ms: 2.0,
        });
        assert_eq!(total.build_ms, 1.25);
        assert_eq!(total.layout_ms, 2.0);
        assert_eq!(total.paint_ms, 3.5);
        assert_eq!(total.present_ms, 0.75);
        assert_eq!(total.total_ms, 7.5);
        assert!(total.summary().starts_with("Frame 7.50 ms"));

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
        assert!(produced.build_ms >= 0.0);
        assert!(produced.layout_ms >= 0.0);
        assert!(produced.paint_ms >= 0.0);
        assert!(produced.present_ms >= 0.0);
        assert!(
            produced.total_ms + 0.001
                >= produced.build_ms + produced.layout_ms + produced.paint_ms
        );
    }

    #[test]
    fn frame_stats_environment_filter_parses_thresholds_and_typos() {
        assert_eq!(FrameStatsFilter::parse(None), FrameStatsFilter::Off);
        assert_eq!(FrameStatsFilter::parse(Some("")), FrameStatsFilter::Off);
        assert_eq!(FrameStatsFilter::parse(Some("all")), FrameStatsFilter::All);
        assert_eq!(
            FrameStatsFilter::parse(Some(" 4.5ms ")),
            FrameStatsFilter::Threshold(4.5)
        );
        assert_eq!(
            FrameStatsFilter::parse(Some("12")),
            FrameStatsFilter::Threshold(12.0)
        );
        assert_eq!(
            FrameStatsFilter::parse(Some("nonsense")),
            FrameStatsFilter::Off
        );
        assert!(!FrameStatsFilter::parse(Some("4.0")).should_emit(FrameStats {
            total_ms: 3.9,
            ..FrameStats::default()
        }));
        assert!(FrameStatsFilter::All.should_emit(FrameStats::default()));
    }

    #[test]
    fn recording_present_time_keeps_total_in_sync() {
        let mut stats = FrameStats::from_breakdown(
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
            Duration::from_millis(4),
            Duration::from_millis(10),
        );
        assert_eq!(stats.total_ms, 10.0);
        stats.set_present_ms(6.0);
        assert_eq!(stats.total_ms, 12.0);
        stats.record_present(Duration::from_millis(2));
        assert_eq!(stats.present_ms, 8.0);
        assert_eq!(stats.total_ms, 14.0);
    }
}
