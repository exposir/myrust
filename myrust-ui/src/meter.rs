//! Three-tone quantity meter.

use super::context::Context;

/// Meter color band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeterTone {
    /// Value below the low threshold.
    #[default]
    Low,
    /// Value between thresholds.
    Medium,
    /// Value above the high threshold.
    High,
}

/// Result of constructing a meter.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeterState {
    /// Clamped value.
    pub value: f32,
    /// Normalized fill in `0.0..=1.0`.
    pub fraction: f32,
    /// Selected color band.
    pub tone: MeterTone,
    /// Effective range minimum.
    pub min: f32,
    /// Effective range maximum.
    pub max: f32,
}

/// Creates a three-tone quantity meter.
///
/// `value` is clamped to `min..=max`; `low_threshold` and `high_threshold`
/// are values in the same unit. The thresholds are ordered and clamped to the
/// range. The meter uses Theme muted/accent/warn colors and is a separate
/// control from `progress`.
pub fn meter(
    context: &mut Context,
    id: &str,
    value: f32,
    min: f32,
    max: f32,
    low_threshold: f32,
    high_threshold: f32,
) -> MeterState {
    context.meter(id, value, min, max, low_threshold, high_threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Size;

    #[test]
    fn meter_clamps_value_and_selects_tone() {
        let size = Size::new(300.0, 160.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let low = context.meter("meter-low", -10.0, 0.0, 100.0, 33.0, 66.0);
        let medium = context.meter("meter-medium", 50.0, 0.0, 100.0, 33.0, 66.0);
        let high = context.meter("meter-high", 80.0, 0.0, 100.0, 33.0, 66.0);
        let over = context.meter("meter-over", 150.0, 0.0, 100.0, 33.0, 66.0);
        let nan = context.meter("meter-nan", f32::NAN, 0.0, 100.0, 33.0, 66.0);
        let frame = context.produce().unwrap();
        assert_eq!(low.value, 0.0);
        assert_eq!(low.fraction, 0.0);
        assert_eq!(low.tone, MeterTone::Low);
        assert_eq!(medium.value, 50.0);
        assert_eq!(medium.fraction, 0.5);
        assert_eq!(medium.tone, MeterTone::Medium);
        assert_eq!(high.value, 80.0);
        assert_eq!(high.tone, MeterTone::High);
        assert_eq!(over.value, 100.0);
        assert_eq!(over.fraction, 1.0);
        assert_eq!(nan.value, 0.0);
        assert_eq!(nan.tone, MeterTone::Low);
        assert!(
            frame
                .access_nodes
                .iter()
                .any(|node| node.role == crate::access::Role::ProgressBar
                    && node.label == "meter")
        );
    }
}
