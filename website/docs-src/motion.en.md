Motion is caller-owned scalar animation plus a frame clock. `Context::animate` and `animate_with` advance values toward targets; `spinner` and `transition` are the built-in users of that path.

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

The animation is keyed by a stable id and stored in `Context`. A target change retargets from the current value. There is no declarative `Animation` widget and no callback when it finishes.

## Poses that only paint

There are no pose/paint-only animation objects. A caller can animate a color, radius, opacity-like blend or draw-command position by storing a scalar and rebuilding commands; layout is not automatically excluded.

## Delays and staggering

No delay, stagger, sequence or orchestration API exists. To stagger, the caller starts separate animations at different times or offsets their targets; `Animation` has only duration/easing.

## Gestures

There is no gesture recognizer, velocity tracking, drag/scroll physics or interruptible spring. Controls handle their own pointer drags; custom gestures must be built from `UiEvent` pointer values and application state.

### By hand

A custom gesture can track `Context::pointer_position()` / `pointer_down()` and previous regions, or receive raw `UiEvent`s through `NativeUi`. There is no gesture arena, pinch recognizer or inertia helper.

## When it is done

`animate_with` returns the current scalar, not a completion flag. Compare it with the target or use `TransitionFrame::done` for bounds transitions. There is no `on_complete` callback.

## Without animation

Use `Duration::ZERO` with `Easing::Linear`, or a direct assignment; `instant_state()` does this for `TransitionState`. `Context::reduce_motion` reports the environment flag and callers can jump values themselves.

## Keyframes

No keyframe, tween-sequence, multi-stop or path animation API exists. A caller can chain targets over time in application state; each step is still one scalar/transition.

## Timelines

No timeline object, playback control, time remapping or group exists. `TransitionState` is a single animated rect; multiple values need separate states or `animate_with` calls.

### Labels and stagger

No labels, markers, stagger groups or timeline callbacks are available. Staggering is application scheduling.

### Playing it

No play/pause/seek/rate API exists. Animation time is the frame clock; a caller that wants pause must freeze its own elapsed time.

## Scrolling

No scroll-linked or scroll-position animation API exists. `scroll_view` reports offset via `ScrollState`; `Context::wheel_delta()` exposes wheel input for custom effects. Parallax/sticky transitions must be built by the caller.

## Content that changes

There is no implicit transition when content or layout changes. Rebuilding a view at a new size snaps unless the caller animates a rectangle/value. There is no diffing or layout animation cache.

## Loops and your own motion

`spinner` is the built-in loop and requests animation frames. `Context::request_animation_frame()` lets a custom view request another frame; the app/host rerenders. `needs_animation_frame` is exposed on `ViewFrame`.

There is no loop manager, frame budget, tween scheduler or retained animation graph.

## Less motion and frames

`MYRUST_REDUCE_MOTION` sets `Context::reduce_motion`; built-in spinner/transitions skip or settle when it is true. There is no OS callback for live preference changes, and no FPS/vsync API.

`needs_animation_frame` is best-effort. The native window loop decides how to wake; a hidden window or slow host can stop frames, so animation timing must not assume a fixed frame rate.

## Out of sight

There is no visibility tracking or automatic pause when offscreen. `scroll_view` constructs only visible rows for lists, but arbitrary animations continue if the caller keeps updating them.

To stop work, the caller skips `animate_with`/`request_animation_frame` or drops the animated widget. No intersection observer or occlusion API exists.
