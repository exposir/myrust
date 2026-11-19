A transition interpolates one value toward a target. `TransitionState` handles bounds; `ColorTransition`, `OpacityTransition`, `SizeTransition` and `PositionTransition` cover the other common drawing and layout values. All are caller-owned: the first call initializes at the target, a changed target starts a new animation from the current interpolated value.

```rust
use myrust_core::ui::{transition, Rect, TransitionState};

let target = Rect::new(if app.open { 180.0 } else { 0.0 }, 20.0, 120.0, 60.0);
let frame = transition(c, &mut app.panel_transition, target);
container(c).size(frame.bounds.width, frame.bounds.height);
```

The returned `TransitionFrame` has `bounds`, `progress`, `done` and `retargeted`. It does not position a child automatically; the caller applies the bounds to drawing or layout.

## What moves

Bounds, color, opacity, size and position have transition primitives. Corner radius, border, shadow, text contents and z-order do not; animate those by rebuilding commands from a caller-owned value.

```rust
use myrust_core::ui::{color_transition, Animation, Color, ColorTransition};

let mut fade = ColorTransition::new(Animation::default());
let frame = color_transition(c, &mut fade, Color::rgb(34, 211, 238));
let color = frame.value;
```

`OpacityTransition` is a scalar clamped to `0.0..=1.0` by `opacity_transition`. `SizeTransition` and `PositionTransition` interpolate `Size` and `Point`. All use the same `Animation` duration/easing and expose `progress`, `done` and `retargeted` through `TransitionValueFrame`.

`TransitionState.animation` and `ValueTransition.animation` set duration/easing for future retargets. `instant_state()` creates a zero-duration linear bounds state for callers that want no animation.

Transitions retarget from the current value but do not carry velocity. Use `Spring` when an interrupted motion should preserve momentum. `ValueTransition::step(dt)` advances without a clock, which keeps tests deterministic.

For keyed `Context` scalars, `animate_spring` / `animate_spring_with` preserve velocity across retargets and participate in `next_animation_frame()`. Use transitions for bounds/color/size/position records and springs for momentum.

## Coming and going

There is no enter/exit lifecycle, mount/unmount animation or presence tracking. A caller decides when content exists and must keep it constructed until its exit animation would finish; otherwise it disappears immediately.

### Exits decided as they go

No exit state is tracked by the UI. If removal depends on layout or scrolling, the caller must decide in application code and keep a transition value alive.

### One at a time

There is no queue or “one transition at a time” coordinator. One state interpolates one value; multiple elements need multiple states.

### Timing the exit

There is no exit-duration separate from the main animation, no delay, and no cancellation callback. Set the state's `animation` or use `instant_state`; all timing comes from that one `Animation`.

## Shared elements

There is no shared-element transition, keyed element matching or cross-page flying. The caller can animate one rectangle toward another component's bounds, but the UI does not discover or preserve identity.

### Flying

“Flying” requires the caller to compute start/end `Rect`s and run `transition`; there is no framework matcher or overlay clone.

### Everything else

Only the selected value is interpolated. A caller crossing containers must draw the animated content at the interpolated rectangle itself; no source/target widget is cloned.

### Scaled content

Text and child layout do not scale as a group. A transition rect does not apply a `Transform`; the caller must wrap content in `Context::transform` if scale is desired.

### Text that morphs

No text morphing, cross-fade or glyph interpolation exists. Text is rebuilt each frame; changing strings snap unless the caller fades or animates color manually.

## When nothing moves

`transition` and `value_transition` initialize directly at the target on first use, so a newly created state does not animate in. A target equal to the current target does not restart. Reduce-motion makes caller-side animations jump only if the caller checks `Context::reduce_motion`; transitions themselves still use their animation duration.

`complete()` snaps a bounds or value transition to its target and `cancel()` freezes it at the current value. For no motion at all, use `instant_state()` or zero-duration `Animation`; no global “disable transitions” switch exists.
