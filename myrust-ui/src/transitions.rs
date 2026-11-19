//! Bounds interpolation for layout changes.

use super::time::{Duration, Instant};

use super::animation::Animation;
use super::context::Context;
use super::layout::{Point, Size};
use super::paint::{Color, Rect};

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

    /// When the current transition reaches its target, if one is running.
    pub fn deadline(&self) -> Option<Instant> {
        if self.progress >= 1.0 {
            return None;
        }
        self.started_at
            .and_then(|started| started.checked_add(self.animation.duration))
    }

    /// Whether the current transition reached its target.
    pub fn done(&self) -> bool {
        self.started_at.is_some() && self.progress >= 1.0
    }

    /// Freezes at the current bounds without jumping to the target.
    pub fn cancel(&mut self) {
        self.from = self.current;
        self.to = self.current;
        self.progress = 1.0;
    }

    /// Jumps to the target and stops.
    pub fn complete(&mut self) {
        self.current = self.to;
        self.progress = 1.0;
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
    let initialized = state.started_at.is_some();
    let mut retargeted = false;
    if !initialized || state.to != target {
        state.retarget(target, now);
        retargeted = true;
    }
    if initialized && (retargeted || !state.done()) {
        state.advance(now);
    }
    if !state.done() {
        context.request_animation_frame();
        if let Some(deadline) = state.deadline() {
            context.request_animation_frame_at(deadline);
        }
    }
    TransitionFrame {
        bounds: state.current,
        progress: state.progress,
        done: state.done(),
        retargeted,
    }
}

/// A value that can be interpolated by [`ValueTransition`].
pub trait TransitionValue: Copy + PartialEq {
    /// Interpolates from `self` to `to` at `progress` in `0.0..=1.0`.
    fn lerp(self, to: Self, progress: f32) -> Self;
}

impl TransitionValue for f32 {
    fn lerp(self, to: Self, progress: f32) -> Self {
        self + (to - self) * progress
    }
}

impl TransitionValue for Color {
    fn lerp(self, to: Self, progress: f32) -> Self {
        let channel = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * progress)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Self::rgba(
            channel(self.r, to.r),
            channel(self.g, to.g),
            channel(self.b, to.b),
            channel(self.a, to.a),
        )
    }
}

impl TransitionValue for Size {
    fn lerp(self, to: Self, progress: f32) -> Self {
        Self::new(
            self.width + (to.width - self.width) * progress,
            self.height + (to.height - self.height) * progress,
        )
    }
}

impl TransitionValue for Point {
    fn lerp(self, to: Self, progress: f32) -> Self {
        Self::new(
            self.x + (to.x - self.x) * progress,
            self.y + (to.y - self.y) * progress,
        )
    }
}

impl TransitionValue for Rect {
    fn lerp(self, to: Self, progress: f32) -> Self {
        lerp_rect(self, to, progress)
    }
}

/// A caller-owned transition for any [`TransitionValue`].
///
/// Like [`TransitionState`], the first call initializes directly at the target.
/// A changed target starts a new animation from the current interpolated
/// value. There is no velocity carry-over; use a spring when interruption
/// should preserve momentum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueTransition<T: TransitionValue> {
    /// Duration and easing used for retargets.
    pub animation: Animation,
    from: T,
    to: T,
    current: T,
    started_at: Option<Instant>,
    elapsed: Duration,
    progress: f32,
}

impl<T: TransitionValue + Default> Default for ValueTransition<T> {
    fn default() -> Self {
        Self::new(Animation::default())
    }
}

impl<T: TransitionValue + Default> ValueTransition<T> {
    /// Creates an uninitialized transition.
    pub fn new(animation: Animation) -> Self {
        Self {
            animation,
            from: T::default(),
            to: T::default(),
            current: T::default(),
            started_at: None,
            elapsed: Duration::ZERO,
            progress: 0.0,
        }
    }
}

impl<T: TransitionValue> ValueTransition<T> {
    /// Last interpolated value.
    pub fn value(&self) -> T {
        self.current
    }

    /// Eased progress of the current transition.
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Whether the transition reached its target.
    pub fn done(&self) -> bool {
        self.started_at.is_some() && self.progress >= 1.0
    }

    /// When the current transition reaches its target, if one is running.
    pub fn deadline(&self) -> Option<Instant> {
        if self.progress >= 1.0 {
            return None;
        }
        self.started_at
            .and_then(|started| started.checked_add(self.animation.duration))
    }

    /// Freezes at the current value without jumping to the target.
    pub fn cancel(&mut self) {
        self.from = self.current;
        self.to = self.current;
        self.progress = 1.0;
    }

    /// Jumps to the target and stops.
    pub fn complete(&mut self) {
        self.current = self.to;
        self.progress = 1.0;
    }

    /// Advances by an explicit `dt` and returns the new value.
    ///
    /// This is the deterministic alternative to [`Self::deadline`]-driven
    /// frame advances. It is a no-op before the first retarget.
    pub fn step(&mut self, dt: Duration) -> T {
        if self.started_at.is_none() || self.progress >= 1.0 {
            return self.current;
        }
        self.elapsed = self.elapsed.saturating_add(dt);
        self.apply_elapsed();
        self.current
    }

    fn retarget(&mut self, target: T, now: Instant) -> bool {
        if self.started_at.is_none() {
            self.from = target;
            self.to = target;
            self.current = target;
            self.elapsed = Duration::ZERO;
            self.progress = 1.0;
            self.started_at = Some(now);
            return true;
        }
        if self.to == target {
            return false;
        }
        self.from = self.current;
        self.to = target;
        self.started_at = Some(now);
        self.elapsed = Duration::ZERO;
        self.progress = 0.0;
        true
    }

    fn advance(&mut self, now: Instant) {
        let Some(started) = self.started_at else {
            return;
        };
        self.elapsed = now.saturating_duration_since(started);
        self.apply_elapsed();
    }

    fn apply_elapsed(&mut self) {
        let duration = self.animation.duration;
        if duration.is_zero() {
            self.current = self.to;
            self.progress = 1.0;
            return;
        }
        let fraction = self.elapsed.as_secs_f32() / duration.as_secs_f32();
        self.progress = self.animation.value(fraction);
        self.current = if fraction >= 1.0 {
            self.to
        } else {
            self.from.lerp(self.to, self.progress)
        };
        self.progress = if fraction >= 1.0 { 1.0 } else { self.progress };
    }
}

/// Result of one value-transition frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionValueFrame<T> {
    /// Interpolated value.
    pub value: T,
    /// Eased progress in `0.0..=1.0`.
    pub progress: f32,
    /// Whether the transition is at its target.
    pub done: bool,
    /// Whether this frame started a new retarget.
    pub retargeted: bool,
}

/// Scalar transition, also useful for opacity values.
pub type ScalarTransition = ValueTransition<f32>;
/// Opacity transition in the conventional `0.0..=1.0` range.
pub type OpacityTransition = ValueTransition<f32>;
/// Color transition including the alpha channel.
pub type ColorTransition = ValueTransition<Color>;
/// Size transition.
pub type SizeTransition = ValueTransition<Size>;
/// Position transition.
pub type PositionTransition = ValueTransition<Point>;

/// Advances a generic value transition toward `target`.
pub fn value_transition<T: TransitionValue>(
    context: &mut Context,
    state: &mut ValueTransition<T>,
    target: T,
) -> TransitionValueFrame<T> {
    let now = context.frame_time();
    let initialized = state.started_at.is_some();
    let mut retargeted = false;
    if !initialized || state.to != target {
        state.retarget(target, now);
        retargeted = true;
    }
    if initialized && (retargeted || !state.done()) {
        state.advance(now);
    }
    if !state.done() {
        context.request_animation_frame();
        if let Some(deadline) = state.deadline() {
            context.request_animation_frame_at(deadline);
        }
    }
    TransitionValueFrame {
        value: state.current,
        progress: state.progress,
        done: state.done(),
        retargeted,
    }
}

/// Advances a scalar transition toward `target`.
pub fn scalar_transition(
    context: &mut Context,
    state: &mut ScalarTransition,
    target: f32,
) -> TransitionValueFrame<f32> {
    value_transition(context, state, target)
}

/// Advances an opacity transition, clamping the target to `0.0..=1.0`.
pub fn opacity_transition(
    context: &mut Context,
    state: &mut OpacityTransition,
    target: f32,
) -> TransitionValueFrame<f32> {
    let target = if target.is_finite() {
        target.clamp(0.0, 1.0)
    } else {
        0.0
    };
    value_transition(context, state, target)
}

/// Advances a color transition toward `target`.
pub fn color_transition(
    context: &mut Context,
    state: &mut ColorTransition,
    target: Color,
) -> TransitionValueFrame<Color> {
    value_transition(context, state, target)
}

/// Advances a size transition toward `target`.
pub fn size_transition(
    context: &mut Context,
    state: &mut SizeTransition,
    target: Size,
) -> TransitionValueFrame<Size> {
    value_transition(context, state, target)
}

/// Advances a position transition toward `target`.
pub fn position_transition(
    context: &mut Context,
    state: &mut PositionTransition,
    target: Point,
) -> TransitionValueFrame<Point> {
    value_transition(context, state, target)
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
    use crate::animation::{Animation, Easing};
    use crate::layout::Size;

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

    #[test]
    fn color_transition_interpolates_all_channels() {
        let size = Size::new(200.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let mut state =
            ColorTransition::new(Animation::new(Duration::from_millis(100), Easing::Linear));
        let red = Color::rgb(255, 0, 0);
        let blue = Color::rgb(0, 0, 255);
        let first = color_transition(&mut context, &mut state, red);
        assert!(first.done);
        assert_eq!(first.value, red);

        let second = color_transition(&mut context, &mut state, blue);
        assert!(!second.done);
        assert_eq!(second.value, red);
        context.advance_frame_time_for_test(Duration::from_millis(50));
        let mid = color_transition(&mut context, &mut state, blue);
        assert!((mid.progress - 0.5).abs() < 0.001);
        assert!((i16::from(mid.value.r) - 128).abs() <= 1);
        assert_eq!(mid.value.g, 0);
        assert!((i16::from(mid.value.b) - 128).abs() <= 1);
        assert_eq!(mid.value.a, 255);
    }

    #[test]
    fn opacity_size_and_position_transitions_interpolate() {
        let frame_size = Size::new(200.0, 120.0);
        let animation = Animation::new(Duration::from_millis(100), Easing::Linear);
        let mut opacity = OpacityTransition::new(animation);
        let mut opacity_context = Context::new(frame_size);
        opacity_context.begin_frame(frame_size);
        opacity_transition(&mut opacity_context, &mut opacity, 0.0);
        opacity_transition(&mut opacity_context, &mut opacity, 1.0);
        opacity_context.advance_frame_time_for_test(Duration::from_millis(50));
        let opacity_frame = opacity_transition(&mut opacity_context, &mut opacity, 1.0);
        assert!((opacity_frame.value - 0.5).abs() < 0.001);

        let mut size = SizeTransition::new(animation);
        let mut size_context = Context::new(frame_size);
        size_context.begin_frame(frame_size);
        size_transition(&mut size_context, &mut size, Size::new(10.0, 20.0));
        size_transition(&mut size_context, &mut size, Size::new(30.0, 60.0));
        size_context.advance_frame_time_for_test(Duration::from_millis(50));
        let size_frame = size_transition(&mut size_context, &mut size, Size::new(30.0, 60.0));
        assert_eq!(size_frame.value, Size::new(20.0, 40.0));

        let mut position = PositionTransition::new(animation);
        let mut position_context = Context::new(frame_size);
        position_context.begin_frame(frame_size);
        position_transition(&mut position_context, &mut position, Point::new(10.0, 20.0));
        position_transition(&mut position_context, &mut position, Point::new(30.0, 60.0));
        position_context.advance_frame_time_for_test(Duration::from_millis(50));
        let position_frame =
            position_transition(&mut position_context, &mut position, Point::new(30.0, 60.0));
        assert_eq!(position_frame.value, Point::new(20.0, 40.0));
    }

    #[test]
    fn context_publishes_next_animation_frame_only_while_active() {
        let size = Size::new(200.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        assert_eq!(context.next_animation_frame(), None);

        let mut state =
            TransitionState::new(Animation::new(Duration::from_millis(100), Easing::Linear));
        let target = rect(10.0, 20.0, 30.0, 40.0);
        transition(&mut context, &mut state, target);
        assert_eq!(context.next_animation_frame(), None);

        context.begin_frame(size);
        let next = rect(110.0, 120.0, 50.0, 60.0);
        transition(&mut context, &mut state, next);
        assert!(context.next_animation_frame().is_some());

        context.advance_frame_time_for_test(Duration::from_millis(100));
        context.begin_frame(size);
        let done = transition(&mut context, &mut state, next);
        assert!(done.done);
        assert_eq!(context.next_animation_frame(), None);
    }

    #[test]
    fn value_transition_steps_with_an_explicit_dt() {
        let size = Size::new(200.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let mut state =
            ScalarTransition::new(Animation::new(Duration::from_millis(100), Easing::Linear));
        scalar_transition(&mut context, &mut state, 0.0);
        scalar_transition(&mut context, &mut state, 1.0);
        assert_eq!(state.step(Duration::from_millis(50)), 0.5);
        assert_eq!(state.step(Duration::from_millis(50)), 1.0);
        assert!(state.done());
    }
}
