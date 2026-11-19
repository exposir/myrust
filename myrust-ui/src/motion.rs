//! Deterministic motion primitives and host-independent frame scheduling.
//!
//! The types in this module never read a clock or start a timer. Callers pass
//! either an explicit `dt` or an explicit [`Instant`], which keeps springs,
//! keyframes and timelines reproducible in tests. Hosts translate
//! [`FrameScheduler::next_wake`] into their own wait policy; the native event
//! loop uses tao's `ControlFlow::WaitUntil`.

use std::fmt;

use super::animation::Easing;
use super::time::{Duration, Instant};

/// Default cadence for a running animation, roughly 60 frames per second.
pub const DEFAULT_FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

/// What a host should do after producing one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDemand {
    /// Whether at least one animation still has work to do.
    pub needs_animation_frame: bool,
    /// Earliest instant at which the host should produce another frame.
    ///
    /// `None` means the host may wait indefinitely. A deadline may already be
    /// in the past, in which case the next frame is due immediately.
    pub next_wake: Option<Instant>,
}

impl FrameDemand {
    /// Nothing is animating.
    pub const IDLE: Self = Self {
        needs_animation_frame: false,
        next_wake: None,
    };
}

/// Collects animation deadlines for one frame.
///
/// `FrameScheduler` is immediate-mode: [`Self::reset`] starts a frame, the
/// animation code calls [`Self::request_frame`], [`Self::request_at`] or
/// [`Self::request_frame_now`], and the host reads [`Self::next_wake`]. It is
/// deliberately independent of any windowing system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameScheduler {
    frame_interval: Duration,
    active: bool,
    deadline: Option<Instant>,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self::new(DEFAULT_FRAME_INTERVAL)
    }
}

impl FrameScheduler {
    /// Creates a scheduler with an explicit animation cadence.
    ///
    /// A zero interval is promoted to one microsecond so a running animation
    /// can never accidentally turn into an unthrottled spin.
    pub fn new(frame_interval: Duration) -> Self {
        Self {
            frame_interval: if frame_interval.is_zero() {
                Duration::from_micros(1)
            } else {
                frame_interval
            },
            active: false,
            deadline: None,
        }
    }

    /// Returns the cadence used by [`Self::request_frame`].
    pub fn frame_interval(&self) -> Duration {
        self.frame_interval
    }

    /// Clears all deadlines at the start of a frame.
    pub fn reset(&mut self) {
        self.active = false;
        self.deadline = None;
    }

    /// Requests another frame at the configured cadence.
    pub fn request_frame(&mut self, now: Instant) {
        let deadline = now.checked_add(self.frame_interval).unwrap_or(now);
        self.request_at(deadline);
    }

    /// Requests another frame as soon as the host can produce one.
    pub fn request_frame_now(&mut self, now: Instant) {
        self.request_at(now);
    }

    /// Requests another frame no earlier than `deadline`.
    pub fn request_at(&mut self, deadline: Instant) {
        self.active = true;
        self.deadline = Some(match self.deadline {
            Some(current) => current.min(deadline),
            None => deadline,
        });
    }

    /// Requests another frame after `delay`.
    pub fn request_after(&mut self, now: Instant, delay: Duration) {
        let deadline = now.checked_add(delay).unwrap_or(now);
        self.request_at(deadline);
    }

    /// Cancels every pending frame request.
    pub fn cancel(&mut self) {
        self.reset();
    }

    /// Whether an animation still needs frames.
    pub fn needs_animation_frame(&self) -> bool {
        self.active
    }

    /// Earliest wake-up, or `None` when the scheduler is idle.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        if !self.active {
            return None;
        }
        Some(
            self.deadline
                .unwrap_or_else(|| now.checked_add(self.frame_interval).unwrap_or(now)),
        )
    }

    /// Whether the current deadline has been reached.
    pub fn is_due(&self, now: Instant) -> bool {
        self.next_wake(now)
            .map(|deadline| deadline <= now)
            .unwrap_or(false)
    }

    /// Snapshot for the host's event loop.
    pub fn demand(&self, now: Instant) -> FrameDemand {
        FrameDemand {
            needs_animation_frame: self.active,
            next_wake: self.next_wake(now),
        }
    }
}

/// A deterministic animation clock paired with a [`FrameScheduler`].
///
/// This is the convenience API for hosts and tests that want to advance time
/// explicitly. Native windows use [`FrameScheduler`] directly because their
/// clock comes from the event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationDriver {
    now: Instant,
    scheduler: FrameScheduler,
}

impl AnimationDriver {
    /// Creates a driver at `now` with the default 60 Hz cadence.
    pub fn new(now: Instant) -> Self {
        Self::with_frame_interval(now, DEFAULT_FRAME_INTERVAL)
    }

    /// Creates a driver at `now` with an explicit cadence.
    pub fn with_frame_interval(now: Instant, frame_interval: Duration) -> Self {
        Self {
            now,
            scheduler: FrameScheduler::new(frame_interval),
        }
    }

    /// Returns the driver's current deterministic time.
    pub fn now(&self) -> Instant {
        self.now
    }

    /// Moves the clock forward by `dt` and returns the new time.
    pub fn advance(&mut self, dt: Duration) -> Instant {
        self.now = self.now.checked_add(dt).unwrap_or(self.now);
        self.now
    }

    /// Sets the clock explicitly.
    pub fn set_now(&mut self, now: Instant) {
        self.now = now;
    }

    /// Starts a new immediate-mode frame.
    pub fn begin_frame(&mut self) {
        self.scheduler.reset();
    }

    /// Requests another frame at the configured cadence.
    pub fn request_frame(&mut self) {
        self.scheduler.request_frame(self.now);
    }

    /// Requests a frame immediately.
    pub fn request_frame_now(&mut self) {
        self.scheduler.request_frame_now(self.now);
    }

    /// Requests a frame after `delay`.
    pub fn request_after(&mut self, delay: Duration) {
        self.scheduler.request_after(self.now, delay);
    }

    /// Requests a frame no earlier than `deadline`.
    pub fn request_at(&mut self, deadline: Instant) {
        self.scheduler.request_at(deadline);
    }

    /// Earliest wake-up in terms of the driver's clock.
    pub fn next_wake(&self) -> Option<Instant> {
        self.scheduler.next_wake(self.now)
    }

    /// Whether the driver has pending animation work.
    pub fn needs_animation_frame(&self) -> bool {
        self.scheduler.needs_animation_frame()
    }

    /// Snapshot for the host.
    pub fn demand(&self) -> FrameDemand {
        self.scheduler.demand(self.now)
    }

    /// Returns the current demand and resets the per-frame deadline set.
    pub fn finish_frame(&mut self) -> FrameDemand {
        let demand = self.demand();
        self.begin_frame();
        demand
    }
}

/// Damped spring integrated with semi-implicit Euler.
///
/// Every call to [`Self::step`] is deterministic: the same position, velocity,
/// target and `dt` always produce the same next state. Large deltas are split
/// into substeps for stability, and deltas beyond one second are capped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Spring stiffness, in mass-normalized force per unit displacement.
    pub stiffness: f32,
    /// Damping coefficient. Lower values overshoot more.
    pub damping: f32,
    target: f32,
    position: f32,
    velocity: f32,
    rest_distance: f32,
    rest_velocity: f32,
    active: bool,
}

impl Default for Spring {
    fn default() -> Self {
        Self::new(170.0, 26.0)
    }
}

impl Spring {
    /// Creates a spring resting at zero.
    pub fn new(stiffness: f32, damping: f32) -> Self {
        Self {
            stiffness: finite_non_negative(stiffness),
            damping: finite_non_negative(damping),
            target: 0.0,
            position: 0.0,
            velocity: 0.0,
            rest_distance: 0.001,
            rest_velocity: 0.001,
            active: false,
        }
    }

    /// Creates a spring resting at `position` with `target` as its target.
    pub fn at(position: f32, target: f32) -> Self {
        let position = finite_or_zero(position);
        let mut spring = Self::default();
        spring.position = position;
        spring.target = finite_or_zero(target);
        spring.active = (spring.target - position).abs() > spring.rest_distance;
        spring
    }

    /// Sets the initial position without changing the target.
    pub fn with_position(mut self, position: f32) -> Self {
        self.position = finite_or_zero(position);
        self.active = (self.target - self.position).abs() > self.rest_distance
            || self.velocity.abs() > self.rest_velocity;
        self
    }

    /// Sets the initial velocity.
    pub fn with_velocity(mut self, velocity: f32) -> Self {
        self.velocity = finite_or_zero(velocity);
        if self.velocity.abs() > self.rest_velocity {
            self.active = true;
        }
        self
    }

    /// Sets the target. An invalid target is ignored.
    pub fn with_target(mut self, target: f32) -> Self {
        self.set_target(target);
        self
    }

    /// Overrides the distance and speed at which the spring is considered at
    /// rest.
    pub fn with_rest_thresholds(mut self, distance: f32, velocity: f32) -> Self {
        self.rest_distance = finite_non_negative(distance).max(f32::EPSILON);
        self.rest_velocity = finite_non_negative(velocity).max(f32::EPSILON);
        self
    }

    /// Current position.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Current velocity in units per second.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// Current target.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Whether the spring is still moving.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the spring is at rest on its target.
    pub fn is_complete(&self) -> bool {
        !self.active && (self.position - self.target).abs() <= self.rest_distance
    }

    /// Redirects the target while preserving position and velocity.
    ///
    /// This is what makes an interrupted spring feel continuous: changing the
    /// target never zeros the velocity or jumps the position. Returns whether
    /// the target changed.
    pub fn set_target(&mut self, target: f32) -> bool {
        if !target.is_finite() {
            return false;
        }
        let changed = (self.target - target).abs() > f32::EPSILON;
        self.target = target;
        if changed
            || (self.position - target).abs() > self.rest_distance
            || self.velocity.abs() > self.rest_velocity
        {
            self.active = true;
        }
        changed
    }

    /// Stops at the current position without snapping to the target.
    ///
    /// The target is left unchanged, so [`Self::set_target`] with a new target
    /// starts a fresh motion from here. [`Self::is_complete`] stays false while
    /// the position differs from the target.
    pub fn cancel(&mut self) {
        self.velocity = 0.0;
        self.active = false;
    }

    /// Snaps to the target and stops.
    pub fn complete(&mut self) {
        self.position = self.target;
        self.velocity = 0.0;
        self.active = false;
    }

    /// Snaps both position and target to `value` and stops.
    pub fn reset_to(&mut self, value: f32) {
        let value = finite_or_zero(value);
        self.position = value;
        self.target = value;
        self.velocity = 0.0;
        self.active = false;
    }

    /// Advances the spring by `dt` and returns the new position.
    pub fn step(&mut self, dt: Duration) -> f32 {
        if !self.active {
            return self.position;
        }
        let mut remaining = dt.as_secs_f32();
        if !remaining.is_finite() || remaining <= 0.0 {
            return self.position;
        }
        remaining = remaining.min(1.0);
        while remaining > f32::EPSILON && self.active {
            let step = remaining.min(1.0 / 120.0);
            let acceleration =
                -self.stiffness * (self.position - self.target) - self.damping * self.velocity;
            self.velocity += acceleration * step;
            self.position += self.velocity * step;
            remaining -= step;
            if !self.position.is_finite() || !self.velocity.is_finite() {
                self.complete();
                break;
            }
            if (self.position - self.target).abs() <= self.rest_distance
                && self.velocity.abs() <= self.rest_velocity
            {
                self.complete();
            }
        }
        self.position
    }
}

/// One stop in a [`Keyframes`] track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    /// Time from the start of the track.
    pub time: Duration,
    /// Value at this stop.
    pub value: f32,
    /// Easing used for the segment that ends at this stop.
    pub easing: Easing,
}

impl Keyframe {
    /// Creates a linear keyframe.
    pub fn new(time: Duration, value: f32) -> Self {
        Self {
            time,
            value: finite_or_zero(value),
            easing: Easing::Linear,
        }
    }

    /// Creates a keyframe with an explicit easing curve.
    pub fn eased(time: Duration, value: f32, easing: Easing) -> Self {
        Self {
            time,
            value: finite_or_zero(value),
            easing,
        }
    }
}

/// A sorted, piecewise interpolation track.
///
/// The track starts at its first keyframe's value; there is no implicit zero
/// keyframe. Duplicate timestamps keep the last supplied value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Keyframes {
    frames: Vec<Keyframe>,
}

impl Keyframes {
    /// Builds a track from keyframes, sorting them by time.
    pub fn new(frames: impl IntoIterator<Item = Keyframe>) -> Self {
        let mut frames = frames
            .into_iter()
            .map(|frame| Keyframe {
                time: frame.time,
                value: finite_or_zero(frame.value),
                easing: frame.easing,
            })
            .collect::<Vec<_>>();
        frames.sort_by_key(|frame| frame.time);
        frames.dedup_by(|later, earlier| {
            if later.time == earlier.time {
                *earlier = *later;
                true
            } else {
                false
            }
        });
        Self { frames }
    }

    /// Builds a linear track with evenly spaced values.
    pub fn from_values(values: impl IntoIterator<Item = f32>, duration: Duration) -> Self {
        let values = values.into_iter().collect::<Vec<_>>();
        let last = values.len().saturating_sub(1);
        let frames = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let fraction = if last == 0 {
                    0.0
                } else {
                    index as f32 / last as f32
                };
                let time = Duration::from_secs_f32(duration.as_secs_f32() * fraction);
                Keyframe::new(time, value)
            })
            .collect::<Vec<_>>();
        Self::new(frames)
    }

    /// Returns the keyframes in time order.
    pub fn frames(&self) -> &[Keyframe] {
        &self.frames
    }

    /// Time of the last keyframe.
    pub fn duration(&self) -> Duration {
        self.frames
            .last()
            .map(|frame| frame.time)
            .unwrap_or_default()
    }

    /// Samples the track at `elapsed`.
    pub fn value(&self, elapsed: Duration) -> f32 {
        let Some(first) = self.frames.first() else {
            return 0.0;
        };
        if elapsed <= first.time {
            return first.value;
        }
        let Some(last) = self.frames.last() else {
            return 0.0;
        };
        if elapsed >= last.time {
            return last.value;
        }
        for pair in self.frames.windows(2) {
            let from = pair[0];
            let to = pair[1];
            if elapsed < to.time {
                let span = to.time.saturating_sub(from.time).as_secs_f32();
                let fraction = if span <= f32::EPSILON {
                    1.0
                } else {
                    elapsed.saturating_sub(from.time).as_secs_f32() / span
                };
                let eased = to.easing.value(fraction);
                return from.value + (to.value - from.value) * eased;
            }
        }
        last.value
    }
}

/// One scalar track inside a [`Timeline`].
#[derive(Debug, Clone, PartialEq)]
pub struct Tween {
    /// Stable identifier used in [`TimelineFrame::values`].
    pub id: String,
    /// Value before the track starts.
    pub from: f32,
    /// Value after the track ends.
    pub to: f32,
    /// Delay inside each timeline cycle.
    pub delay: Duration,
    /// Track duration.
    pub duration: Duration,
    /// Easing curve.
    pub easing: Easing,
}

impl Tween {
    /// Creates a tween with ease-in-out timing.
    pub fn new(id: impl Into<String>, from: f32, to: f32, duration: Duration) -> Self {
        Self {
            id: id.into(),
            from: finite_or_zero(from),
            to: finite_or_zero(to),
            delay: Duration::ZERO,
            duration,
            easing: Easing::EaseInOutCubic,
        }
    }

    /// Adds a delay inside each timeline cycle.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Changes the easing curve.
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    fn end(&self) -> Duration {
        self.delay.saturating_add(self.duration)
    }

    fn value_at(&self, cycle_time: Duration) -> f32 {
        if cycle_time < self.delay {
            return self.from;
        }
        if self.duration.is_zero() {
            return self.to;
        }
        let fraction =
            cycle_time.saturating_sub(self.delay).as_secs_f32() / self.duration.as_secs_f32();
        if fraction >= 1.0 {
            return self.to;
        }
        let eased = self.easing.value(fraction);
        self.from + (self.to - self.from) * eased
    }
}

/// How often a timeline repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Play exactly once.
    #[default]
    Once,
    /// Play a fixed number of times. Values below one are treated as one.
    Times(u32),
    /// Play until cancelled or completed explicitly.
    Forever,
}

/// One sampled value from a timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TweenValue {
    /// Identifier of the track that produced the value.
    pub id: String,
    /// Current scalar value.
    pub value: f32,
}

/// A snapshot of a timeline after an advance or seek.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineFrame {
    /// Playhead position.
    pub time: Duration,
    /// Overall progress in `0.0..=1.0`.
    pub progress: f32,
    /// Zero-based repeat iteration.
    pub iteration: u32,
    /// Whether the timeline reached its final frame.
    pub completed: bool,
    /// Values in track insertion order.
    pub values: Vec<TweenValue>,
}

/// A single playhead driving multiple scalar tweens.
///
/// A new timeline starts playing immediately. `advance(dt)` is deterministic;
/// `complete()` and `cancel()` give callers explicit terminal behaviour.
/// The completion callback fires at most once per playing cycle and is reset by
/// [`Self::restart`].
pub struct Timeline {
    tweens: Vec<Tween>,
    delay: Duration,
    repeat: Repeat,
    playhead: Duration,
    playing: bool,
    completed: bool,
    callback_fired: bool,
    on_complete: Option<Box<dyn FnMut()>>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Timeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Timeline")
            .field("tweens", &self.tweens)
            .field("delay", &self.delay)
            .field("repeat", &self.repeat)
            .field("playhead", &self.playhead)
            .field("playing", &self.playing)
            .field("completed", &self.completed)
            .field("has_complete_callback", &self.on_complete.is_some())
            .finish()
    }
}

impl Timeline {
    /// Creates an empty, playing timeline.
    pub fn new() -> Self {
        Self {
            tweens: Vec::new(),
            delay: Duration::ZERO,
            repeat: Repeat::Once,
            playhead: Duration::ZERO,
            playing: true,
            completed: false,
            callback_fired: false,
            on_complete: None,
        }
    }

    /// Adds a tween and returns the timeline for chaining.
    pub fn add_tween(mut self, tween: Tween) -> Self {
        self.tweens.push(tween);
        self
    }

    /// Adds a tween built from its parts.
    pub fn tween(self, id: impl Into<String>, from: f32, to: f32, duration: Duration) -> Self {
        self.add_tween(Tween::new(id, from, to, duration))
    }

    /// Adds a tween at runtime.
    pub fn push_tween(&mut self, tween: Tween) {
        self.tweens.push(tween);
    }

    /// Sets a delay before the first cycle starts.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Sets how many total times the timeline plays. Zero means once.
    pub fn repeat(mut self, times: u32) -> Self {
        self.repeat = Repeat::Times(times.max(1));
        self
    }

    /// Repeats forever until cancelled or explicitly completed.
    pub fn repeat_forever(mut self) -> Self {
        self.repeat = Repeat::Forever;
        self
    }

    /// Installs a callback invoked once when the timeline completes.
    pub fn on_complete(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_complete = Some(Box::new(callback));
        self
    }

    /// Replaces the completion callback.
    pub fn set_on_complete(&mut self, callback: impl FnMut() + 'static) {
        self.on_complete = Some(Box::new(callback));
    }

    /// Playhead position.
    pub fn time(&self) -> Duration {
        self.playhead
    }

    /// Whether the timeline is currently advancing.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether the timeline reached its final frame.
    pub fn is_complete(&self) -> bool {
        self.completed
    }

    /// Number of configured tracks.
    pub fn len(&self) -> usize {
        self.tweens.len()
    }

    /// Whether no tracks are configured.
    pub fn is_empty(&self) -> bool {
        self.tweens.is_empty()
    }

    /// Resumes playback. A completed timeline restarts from zero.
    pub fn play(&mut self) {
        if self.completed {
            self.restart();
        } else {
            self.playing = true;
        }
    }

    /// Stops advancing without changing the playhead or values.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stops advancing and stays at the current frame.
    ///
    /// Cancelling does not fire the completion callback and does not mark the
    /// timeline complete. A later [`Self::play`] resumes from the playhead.
    pub fn cancel(&mut self) {
        self.playing = false;
    }

    /// Returns to zero and starts playing again.
    pub fn restart(&mut self) {
        self.playhead = Duration::ZERO;
        self.completed = false;
        self.callback_fired = false;
        self.playing = true;
    }

    /// Moves the playhead without invoking the completion callback.
    pub fn seek(&mut self, time: Duration) {
        self.playhead = match self.total_duration() {
            Some(total) => time.min(total),
            None => time,
        };
        self.completed = self
            .total_duration()
            .map(|total| self.playhead >= total)
            .unwrap_or(false);
    }

    /// Jumps to the final frame, stops, and fires the completion callback once.
    ///
    /// For a forever-repeating timeline the final frame is the end of one
    /// cycle.
    pub fn complete(&mut self) {
        self.playhead = self
            .total_duration()
            .unwrap_or_else(|| self.delay.saturating_add(self.cycle_duration()));
        self.playing = false;
        self.mark_complete_and_fire();
    }

    /// Advances the playhead by `dt` and returns a snapshot.
    pub fn advance(&mut self, dt: Duration) -> TimelineFrame {
        if !self.playing || self.completed || dt.is_zero() {
            return self.frame();
        }
        self.playhead = self.playhead.saturating_add(dt);
        if self.cycle_duration().is_zero() && self.playhead >= self.delay {
            self.mark_complete_and_fire();
            return self.frame();
        }
        if let Some(total) = self.total_duration() {
            if self.playhead >= total {
                self.playhead = total;
                self.mark_complete_and_fire();
            }
        }
        self.frame()
    }

    /// Current value for one track id.
    pub fn value(&self, id: &str) -> Option<f32> {
        self.frame()
            .values
            .into_iter()
            .find(|value| value.id == id)
            .map(|value| value.value)
    }

    /// Samples the current playhead.
    pub fn frame(&self) -> TimelineFrame {
        let cycle_duration = self.cycle_duration();
        let total = self.total_duration();
        let progress = match total {
            Some(total) if !total.is_zero() => {
                (self.playhead.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
            }
            Some(_) => 1.0,
            None if !cycle_duration.is_zero() => {
                let after_delay = self.playhead.saturating_sub(self.delay);
                let cycle_time = duration_remainder(after_delay, cycle_duration);
                (cycle_time.as_secs_f32() / cycle_duration.as_secs_f32()).clamp(0.0, 1.0)
            }
            None => 0.0,
        };

        let (iteration, cycle_time) = if self.completed {
            let iterations = self.play_count();
            (iterations.saturating_sub(1), cycle_duration)
        } else if self.playhead <= self.delay || cycle_duration.is_zero() {
            (0, Duration::ZERO)
        } else {
            let after_delay = self.playhead.saturating_sub(self.delay);
            let index = after_delay.as_nanos() / cycle_duration.as_nanos();
            (
                (index.min(u128::from(u32::MAX))) as u32,
                duration_remainder(after_delay, cycle_duration),
            )
        };

        let before_start = self.playhead < self.delay;
        let values = self
            .tweens
            .iter()
            .map(|tween| TweenValue {
                id: tween.id.clone(),
                value: if self.completed {
                    tween.to
                } else if before_start {
                    tween.from
                } else {
                    tween.value_at(cycle_time)
                },
            })
            .collect();
        TimelineFrame {
            time: self.playhead,
            progress,
            iteration,
            completed: self.completed,
            values,
        }
    }

    fn play_count(&self) -> u32 {
        match self.repeat {
            Repeat::Once => 1,
            Repeat::Times(times) => times.max(1),
            Repeat::Forever => u32::MAX,
        }
    }

    fn cycle_duration(&self) -> Duration {
        self.tweens.iter().map(Tween::end).max().unwrap_or_default()
    }

    fn total_duration(&self) -> Option<Duration> {
        match self.repeat {
            Repeat::Forever => None,
            Repeat::Once => Some(self.delay.saturating_add(self.cycle_duration())),
            Repeat::Times(times) => Some(
                self.delay
                    .saturating_add(self.cycle_duration().saturating_mul(times.max(1))),
            ),
        }
    }

    fn mark_complete_and_fire(&mut self) {
        self.completed = true;
        self.playing = false;
        if !self.callback_fired {
            self.callback_fired = true;
            if let Some(callback) = self.on_complete.as_mut() {
                callback();
            }
        }
    }
}

fn duration_remainder(value: Duration, divisor: Duration) -> Duration {
    if divisor.is_zero() {
        return Duration::ZERO;
    }
    let nanos = value.as_nanos() % divisor.as_nanos();
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::animation::Easing;

    fn millis(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn scheduler_reports_idle_and_earliest_deadline() {
        let now = Instant::now();
        let mut scheduler = FrameScheduler::new(millis(16));
        assert!(!scheduler.needs_animation_frame());
        assert_eq!(scheduler.next_wake(now), None);
        assert_eq!(scheduler.demand(now), FrameDemand::IDLE);

        scheduler.request_frame(now);
        let cadence = now + millis(16);
        assert_eq!(scheduler.next_wake(now), Some(cadence));
        assert!(scheduler.needs_animation_frame());
        assert!(!scheduler.is_due(now));

        scheduler.request_after(now, millis(5));
        assert_eq!(scheduler.next_wake(now), Some(now + millis(5)));
        assert!(scheduler.is_due(now + millis(5)));

        scheduler.reset();
        assert_eq!(scheduler.next_wake(now), None);
    }

    #[test]
    fn scheduler_immediate_request_is_due_now() {
        let now = Instant::now();
        let mut scheduler = FrameScheduler::default();
        scheduler.request_frame_now(now);
        assert_eq!(scheduler.next_wake(now), Some(now));
        assert!(scheduler.is_due(now));
        assert!(scheduler.demand(now).needs_animation_frame);
    }

    #[test]
    fn animation_driver_steps_deterministically() {
        let start = Instant::now();
        let mut driver = AnimationDriver::with_frame_interval(start, millis(16));
        driver.begin_frame();
        driver.request_frame();
        assert_eq!(driver.next_wake(), Some(start + millis(16)));

        let now = driver.advance(millis(16));
        assert_eq!(now, start + millis(16));
        let demand = driver.finish_frame();
        assert!(demand.needs_animation_frame);
        assert_eq!(demand.next_wake, Some(start + millis(16)));
        assert!(!driver.needs_animation_frame());
    }

    #[test]
    fn spring_overshoots_then_converges() {
        let mut spring = Spring::new(100.0, 4.0).with_target(1.0);
        let dt = Duration::from_secs_f32(1.0 / 120.0);
        let mut overshot = false;
        for _ in 0..600 {
            let position = spring.step(dt);
            if position > 1.0 + 0.001 {
                overshot = true;
            }
        }
        assert!(overshot, "an underdamped spring must overshoot");
        assert!(!spring.is_active());
        assert!((spring.position() - 1.0).abs() < 0.01);
        assert!(spring.is_complete());
    }

    #[test]
    fn spring_step_is_deterministic() {
        let dt = Duration::from_secs_f32(1.0 / 60.0);
        let mut left = Spring::new(120.0, 18.0).with_target(2.0);
        let mut right = left;
        for _ in 0..240 {
            assert_eq!(left.step(dt), right.step(dt));
            assert_eq!(left.velocity(), right.velocity());
        }
        assert_eq!(left.position(), right.position());
    }

    #[test]
    fn spring_retarget_preserves_velocity() {
        let dt = Duration::from_secs_f32(1.0 / 120.0);
        let mut spring = Spring::new(100.0, 10.0).with_target(1.0);
        for _ in 0..12 {
            spring.step(dt);
        }
        let position = spring.position();
        let velocity = spring.velocity();
        assert!(velocity > 0.0);
        assert!(spring.set_target(2.0));
        assert_eq!(spring.position(), position);
        assert_eq!(spring.velocity(), velocity);
        assert!(spring.is_active());
    }

    #[test]
    fn spring_cancel_and_complete_are_distinct() {
        let mut spring = Spring::new(100.0, 10.0).with_target(1.0);
        spring.step(Duration::from_secs_f32(1.0 / 60.0));
        let position = spring.position();
        spring.cancel();
        assert!(!spring.is_active());
        assert_eq!(spring.position(), position);
        assert!(!spring.is_complete());
        spring.set_target(1.0);
        assert!(spring.is_active());

        spring.complete();
        assert_eq!(spring.position(), 1.0);
        assert!(spring.is_complete());
    }

    #[test]
    fn keyframes_interpolate_in_order() {
        let track = Keyframes::new([
            Keyframe::new(millis(0), 0.0),
            Keyframe::new(millis(100), 10.0),
            Keyframe::new(millis(200), 0.0),
        ]);
        assert_eq!(track.duration(), millis(200));
        assert!((track.value(millis(50)) - 5.0).abs() < 0.001);
        assert!((track.value(millis(100)) - 10.0).abs() < 0.001);
        assert!((track.value(millis(150)) - 5.0).abs() < 0.001);
        assert_eq!(track.value(millis(300)), 0.0);
    }

    #[test]
    fn timeline_runs_tracks_in_order_and_completes_once() {
        let completions = Rc::new(Cell::new(0));
        let callback_count = completions.clone();
        let mut timeline = Timeline::new()
            .tween("first", 0.0, 10.0, millis(100))
            .add_tween(
                Tween::new("second", 10.0, 20.0, millis(200))
                    .with_delay(millis(50))
                    .with_easing(Easing::Linear),
            )
            .on_complete(move || callback_count.set(callback_count.get() + 1));

        let frame = timeline.advance(millis(50));
        assert_eq!(
            frame
                .values
                .iter()
                .map(|value| value.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!((frame.values[0].value - 5.0).abs() < 0.001);
        assert_eq!(frame.values[1].value, 10.0);
        assert!(!frame.completed);

        let frame = timeline.advance(millis(100));
        assert!((frame.values[0].value - 10.0).abs() < 0.001);
        assert!((frame.values[1].value - 15.0).abs() < 0.001);
        assert!(!frame.completed);

        let frame = timeline.advance(millis(100));
        assert!(frame.completed);
        assert_eq!(frame.progress, 1.0);
        assert_eq!(frame.values[0].value, 10.0);
        assert_eq!(frame.values[1].value, 20.0);
        assert_eq!(completions.get(), 1);
        assert_eq!(timeline.value("second"), Some(20.0));
    }

    #[test]
    fn timeline_repeats_and_cancel_does_not_complete() {
        let completions = Rc::new(Cell::new(0));
        let callback_count = completions.clone();
        let mut timeline = Timeline::new()
            .tween("opacity", 0.0, 1.0, millis(100))
            .repeat(3)
            .on_complete(move || callback_count.set(callback_count.get() + 1));

        let frame = timeline.advance(millis(150));
        assert_eq!(frame.iteration, 1);
        assert!((frame.values[0].value - 0.5).abs() < 0.001);
        assert!(!frame.completed);
        assert_eq!(completions.get(), 0);

        let frame = timeline.advance(millis(200));
        assert!(frame.completed);
        assert_eq!(frame.values[0].value, 1.0);
        assert_eq!(completions.get(), 1);

        let mut cancelled = Timeline::new().tween("x", 0.0, 1.0, millis(100));
        cancelled.advance(millis(50));
        cancelled.cancel();
        let frame = cancelled.advance(millis(200));
        assert!(!frame.completed);
        assert!((frame.values[0].value - 0.5).abs() < 0.001);
    }

    #[test]
    fn timeline_complete_jumps_to_final_values() {
        let completions = Rc::new(Cell::new(0));
        let callback_count = completions.clone();
        let mut timeline = Timeline::new()
            .tween("x", 0.0, 8.0, millis(100))
            .with_delay(millis(20))
            .on_complete(move || callback_count.set(callback_count.get() + 1));
        timeline.complete();
        assert!(timeline.is_complete());
        assert_eq!(timeline.value("x"), Some(8.0));
        assert_eq!(completions.get(), 1);
        timeline.complete();
        assert_eq!(completions.get(), 1);
    }
}
