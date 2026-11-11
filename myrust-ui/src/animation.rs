//! CPU-side animation timing and easing primitives.
//!
//! This module does not drive timers. The host event loop must request another
//! frame while [`crate::NativeUi`] reports `needs_animation_frame`.

use std::time::Duration;

/// Easing curve for scalar animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Constant-rate progress.
    Linear,
    /// Starts quickly and decelerates toward the target.
    #[default]
    EaseOutCubic,
    /// Smooth acceleration and deceleration.
    EaseInOutCubic,
}

/// Duration and easing for one scalar animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Animation {
    /// Total animation duration.
    pub duration: Duration,
    /// Easing curve.
    pub easing: Easing,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(120),
            easing: Easing::EaseOutCubic,
        }
    }
}

impl Animation {
    /// Creates an animation with a duration and easing curve.
    pub fn new(duration: Duration, easing: Easing) -> Self {
        Self { duration, easing }
    }

    /// Converts elapsed fraction to eased progress in `0.0..=1.0`.
    pub fn value(&self, elapsed_fraction: f32) -> f32 {
        let fraction = if elapsed_fraction.is_finite() {
            elapsed_fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        match self.easing {
            Easing::Linear => fraction,
            Easing::EaseOutCubic => 1.0 - (1.0 - fraction).powi(3),
            Easing::EaseInOutCubic => {
                if fraction < 0.5 {
                    4.0 * fraction.powi(3)
                } else {
                    1.0 - (-2.0 * fraction + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_hits_exact_endpoints() {
        for easing in [Easing::Linear, Easing::EaseOutCubic, Easing::EaseInOutCubic] {
            let animation = Animation::new(Duration::from_millis(100), easing);
            assert_eq!(animation.value(0.0), 0.0);
            assert_eq!(animation.value(1.0), 1.0);
        }
    }

    #[test]
    fn easing_is_monotonic_over_midpoints() {
        let animation = Animation::new(Duration::from_millis(100), Easing::EaseInOutCubic);
        let values = [0.0, 0.25, 0.5, 0.75, 1.0].map(|fraction| animation.value(fraction));
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!((values[2] - 0.5).abs() < 0.001);
    }
}
