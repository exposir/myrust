//! Bounds interpolation for layout changes.

use std::time::{Duration, Instant};

use super::animation::Animation;
use super::context::Context;
use super::paint::Rect;

/// Caller-owned transition state.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionState {
    /// Duration and easing used for retargets.
    pub animation: Animation,
    from: Rect,
    to: Rect,
    current: Rect,
    started_at: Option<Instant>,
    progress: f32,
}

impl Default for TransitionState {
    fn default() -> Self {
        Self {
            animation: Animation::default(),
            from: Rect::default(),
            to: Rect::default(),
            current: Rect::default(),
            started_at: None,
            progress: 0.0,
        }
    }
}

impl TransitionState {
    /// Creates a transition state with an explicit animation.
    pub fn new(animation: Animation) -> Self {
        Self {
            animation,
            ..Self::default()
        }
    }

    /// Last interpolated bounds.
    pub fn bounds(&self) -> Rect {
        self.current
    }

    /// Eased progress of the current transition.
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Whether the current transition reached its target.
    pub fn done(&self) -> bool {
        self.started_at.is_some() && self.progress >= 1.0
    }

    fn retarget(&mut self, target: Rect, now: Instant) {
        if self.started_at.is_none() {
            self.from = target;
            self.to = target;
            self.current = target;
            self.progress = 1.0;
            self.started_at = Some(now);
            return;
        }
        if self.to == target {
            return;
        }
        self.from = self.current;
        self.to = target;
        self.started_at = Some(now);
        self.progress = 0.0;
    }

    fn advance(&mut self, now: Instant) {
        let Some(started) = self.started_at else {
            return;
        };
        let duration = self.animation.duration;
        if duration.is_zero() {
            self.current = self.to;
            self.progress = 1.0;
            return;
        }
        let elapsed = now.saturating_duration_since(started);
        let fraction = elapsed.as_secs_f32() / duration.as_secs_f32();
        self.progress = self.animation.value(fraction);
        self.current = if fraction >= 1.0 {
            self.to
        } else {
            lerp_rect(self.from, self.to, self.progress)
        };
        self.progress = if fraction >= 1.0 { 1.0 } else { self.progress };
    }
}

/// Result of one transition frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransitionFrame {
    /// Interpolated bounds.
    pub bounds: Rect,
    /// Eased progress in `0.0..=1.0`.
    pub progress: f32,
    /// Whether the transition is at its target.
    pub done: bool,
    /// Whether this frame started a new retarget.
    pub retargeted: bool,
}

/// Advances a caller-owned bounds transition toward `target`.
///
/// The first call initializes directly at the target. A changed target starts
/// a new transition from the current interpolated bounds. While the transition
/// is running the context requests another animation frame.
pub fn transition(
    context: &mut Context,
    state: &mut TransitionState,
    target: Rect,
) -> TransitionFrame {
    let now = context.frame_time();
    let initialized = state.started_at.is_none();
    let mut retargeted = false;
    if initialized || state.to != target {
        state.retarget(target, now);
        retargeted = true;
    }
    if !initialized {
        state.advance(now);
    }
    if !state.done() {
        context.request_animation_frame();
    }
    TransitionFrame {
        bounds: state.current,
        progress: state.progress,
        done: state.done(),
        retargeted,
    }
}

fn lerp_rect(from: Rect, to: Rect, progress: f32) -> Rect {
    let progress = progress.clamp(0.0, 1.0);
    Rect::new(
        from.x + (to.x - from.x) * progress,
        from.y + (to.y - from.y) * progress,
        from.width + (to.width - from.width) * progress,
        from.height + (to.height - from.height) * progress,
    )
}

/// Creates a zero-duration state useful for tests or reduce-motion callers.
pub fn instant_state() -> TransitionState {
    TransitionState::new(Animation::new(
        Duration::ZERO,
        super::animation::Easing::Linear,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::animation::{Animation, Easing};
    use crate::ui::layout::Size;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(x, y, width, height)
    }

    #[test]
    fn transition_interpolates_bounds_and_reports_completion() {
        let size = Size::new(200.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let mut state =
            TransitionState::new(Animation::new(Duration::from_millis(100), Easing::Linear));
        let target = rect(10.0, 20.0, 30.0, 40.0);
        let first = transition(&mut context, &mut state, target);
        assert!(first.done);
        assert!(first.retargeted);
        assert_eq!(first.bounds, target);
        assert_eq!(first.progress, 1.0);

        let next = rect(110.0, 120.0, 50.0, 60.0);
        let second = transition(&mut context, &mut state, next);
        assert!(second.retargeted);
        assert!(!second.done);
        assert_eq!(second.bounds, target);
        assert_eq!(second.progress, 0.0);

        context.advance_frame_time_for_test(Duration::from_millis(50));
        let mid = transition(&mut context, &mut state, next);
        assert!(!mid.done);
        assert!((mid.progress - 0.5).abs() < 0.001);
        assert_eq!(mid.bounds, rect(60.0, 70.0, 40.0, 50.0));
        assert!(context.needs_animation_frame());

        context.advance_frame_time_for_test(Duration::from_millis(50));
        let done = transition(&mut context, &mut state, next);
        assert!(done.done);
        assert_eq!(done.bounds, next);
        assert_eq!(done.progress, 1.0);
    }
}
