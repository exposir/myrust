Motion is caller-owned scalar animation plus deterministic spring, keyframe and timeline primitives. A host-independent frame scheduler computes the next wake-up instead of polling.

```rust
use std::time::Duration;
use myrust_core::ui::{Animation, Easing};

let progress = c.animate_with(
    "panel",
    if open { 1.0 } else { 0.0 },
    Animation::new(Duration::from_millis(180), Easing::EaseOutCubic),
);
```

## Animations

`Animation` is `{ duration, easing }`; `Easing` is `Linear`, `EaseOutCubic` or `EaseInOutCubic`. `Context::animate(id, target)` uses a 120ms ease-out default; `animate_with` accepts an explicit `Animation`.

The animation is keyed by a stable id and stored in `Context`. A target change retargets from the current value. There is still no declarative animation widget for these scalars.

## Springs

`Spring` is a standalone scalar with `stiffness`, `damping`, position, velocity and target. It uses semi-implicit Euler integration and `step(dt)` is deterministic, so tests can drive it with a fixed delta.

```rust
use std::time::Duration;
use myrust_core::ui::Spring;

let mut spring = Spring::new(180.0, 20.0).with_target(1.0);
let value = spring.step(Duration::from_millis(16));
spring.set_target(0.0); // keeps position and velocity
```

Changing the target preserves momentum, which makes an interrupted motion continue smoothly. `cancel()` stops where it is without snapping; `complete()` snaps to the target. `is_active()` and `is_complete()` describe the two outcomes separately.

For keyed values owned by `Context`, `animate_spring(id, target)` uses the default spring and `animate_spring_with(id, target, spring)` takes explicit stiffness/damping. The first call establishes the resting target; later target changes preserve velocity. Timing comes from `frame_time`, reduced motion lands directly on target, and a converged spring stops requesting frames, so `next_animation_frame()` becomes `None`.

Stiffness controls return speed; damping controls how quickly oscillation decays. Low damping overshoots, high damping approaches without overshoot. Rest thresholds decide when integration stops.

## Keyframes

`Keyframes` is a sorted track of `{ time, value, easing }` stops. Time before the first stop holds the first value; time after the last stop holds the last value. Duplicate timestamps keep the last supplied value.

```rust
use std::time::Duration;
use myrust_core::ui::{Easing, Keyframe, Keyframes};

let track = Keyframes::new([
    Keyframe::new(Duration::ZERO, 0.0),
    Keyframe::eased(Duration::from_millis(120), 1.0, Easing::EaseOutCubic),
]);
let value = track.value(Duration::from_millis(60));
```

`Keyframes::from_values` creates an evenly spaced linear track. Sampling is pure and deterministic; no clock is read.

## Timelines

`Timeline` owns one playhead and any number of `Tween`s. Each tween has an id, from/to values, optional delay, duration and easing. `advance(dt)` returns a `TimelineFrame` whose `values` are in track insertion order, plus iteration, progress and completion state.

```rust
use std::time::Duration;
use myrust_core::ui::{Timeline, Tween};

let mut timeline = Timeline::new()
    .tween("opacity", 0.0, 1.0, Duration::from_millis(180))
    .add_tween(
        Tween::new("offset", 0.0, 24.0, Duration::from_millis(220))
            .with_delay(Duration::from_millis(40)),
    );
let frame = timeline.advance(Duration::from_millis(16));
for value in frame.values {
    let _ = (value.id, value.value);
}
```

A new timeline plays immediately. `pause()` stops without changing the playhead, `cancel()` is the same terminal-free stop, `seek()` moves without firing callbacks, and `complete()` jumps to the final values. `on_complete` fires once when playback actually completes; `restart()` resets it.

`repeat(n)` plays the whole timeline `n` times; `repeat_forever()` loops until cancelled or completed explicitly. Per-tween delays restart in every cycle.

## Poses that only paint

There are still no pose or paint-only animation objects. A caller can animate a color, radius, opacity-like blend or draw-command position by storing a value and rebuilding commands; layout is not automatically excluded.

## Delays and staggering

Timeline delays and per-tween delays provide one-shot and staggered scheduling. For independent values driven by separate state machines, separate springs or scalar animations remain valid.

## Gestures

There is no gesture recognizer, pointer velocity tracker, drag/scroll physics or built-in interruptible gesture spring. `Spring` gives you the physics primitive, but controls still handle their own pointer drags; custom gestures must be built from `UiEvent` pointer values and application state.

### By hand

A custom gesture can track `Context::pointer_position()` / `pointer_down()` and previous regions, or receive raw `UiEvent`s through `NativeUi`. There is still no gesture arena or pinch recognizer.

## When it is done

`animate_with` returns the current scalar, not a completion flag. A `Spring` reports `is_active()` / `is_complete()`, and a `TimelineFrame` reports `completed`; timelines can also install an `on_complete` callback. Bounds transitions continue to use `TransitionFrame::done`.

## Without animation

Use `Duration::ZERO` with `Easing::Linear`, or a direct assignment; `instant_state()` does this for `TransitionState`. `Context::reduce_motion` reports the environment flag and callers can jump values or complete a spring themselves.

## Scheduling frames

`FrameScheduler` is a per-frame deadline set. `request_frame(now)` asks for another frame at the scheduler cadence (about 60 Hz), `request_frame_now(now)` makes it due immediately, and `request_at` / `request_after` supply an explicit deadline. `next_wake(now)` is `None` when idle.

```rust
use std::time::Duration;
use myrust_core::ui::FrameScheduler;

let mut scheduler = FrameScheduler::new(Duration::from_millis(16));
scheduler.request_frame(now);
if let Some(deadline) = scheduler.next_wake(now) {
    // host: ControlFlow::WaitUntil(deadline)
}
```

`AnimationDriver` combines the scheduler with an explicit clock for deterministic hosts and tests. `Context::next_animation_frame()` and `NativeUi::next_animation_frame()` expose the same deadline after a frame. The native app loop waits with tao's `ControlFlow::WaitUntil` and requests a redraw only when a deadline is due; idle windows do not poll.

Active scalar animations and springs contribute deadlines. A `Timeline` is caller-stepped with `advance(dt)`; its owner should call `request_animation_frame()` while the returned frame is not complete. Once every spring/tween converges or is cancelled, the scheduler becomes idle and no timer wake remains.

## Scrolling

No scroll-linked or scroll-position animation API exists. `scroll_view` reports offset via `ScrollState`; `Context::wheel_delta()` exposes wheel input for custom effects. Parallax/sticky transitions must be built by the caller.

## Content that changes

There is no implicit transition when content or layout changes. Rebuilding a view at a new size snaps unless the caller animates a rectangle/value or uses a `Timeline`. There is no diffing or layout animation cache.

## Loops and your own motion

`spinner` is the built-in loop and requests animation frames at the scheduler cadence. `Context::request_animation_frame()` lets a custom view request another frame; the app/host wakes at the computed deadline. `needs_animation_frame` remains available on `ViewFrame`.

## Less motion and frames

`MYRUST_REDUCE_MOTION` sets `Context::reduce_motion`; built-in spinner/transitions skip or settle when it is true. There is no OS callback for live preference changes, and no FPS/vsync API.

When `needs_animation_frame` is false, `next_animation_frame()` is `None`. A hidden window is excluded from the native wake calculation, so it stops animating until it is visible again.

## Out of sight

The native app loop does not schedule frames for a window while `is_visible()` is false. `scroll_view` constructs only visible rows for lists, but arbitrary animations continue if the caller keeps updating them in a visible window.

To stop work explicitly, the caller skips `animate_with`/`request_animation_frame`, cancels a spring or timeline, or drops the animated widget. There is no intersection observer or occlusion API.
