A transition interpolates one `Rect` toward a target. `TransitionState` is caller-owned; the first call initializes at the target, a changed target starts a new animation from the current interpolated rectangle.

```rust
use myrust_core::ui::{transition, Rect, TransitionState};

let target = Rect::new(if app.open { 180.0 } else { 0.0 }, 20.0, 120.0, 60.0);
let frame = transition(c, &mut app.panel_transition, target);
container(c).size(frame.bounds.width, frame.bounds.height);
```

The returned `TransitionFrame` has `bounds`, `progress`, `done` and `retargeted`. It does not position a child automatically; the caller applies the bounds to drawing or layout.

## What moves

Only bounds animate. Color, opacity, corner radius, border, shadow, text contents and z-order have no transition primitive. Animate those by reading `frame.progress` and rebuilding `Color`/commands each frame.

`TransitionState.animation` sets duration/easing for future retargets. `instant_state()` creates a zero-duration linear state for callers that want no animation.

## Coming and going

There is no enter/exit lifecycle, mount/unmount animation or presence tracking. A caller decides when content exists and must keep it constructed until its exit animation would finish; otherwise it disappears immediately.

### Exits decided as they go

No exit state is tracked by the UI. If removal depends on layout or scrolling, the caller must decide in application code and keep a transition value alive.

### One at a time

There is no queue or “one transition at a time” coordinator. One `TransitionState` interpolates one rectangle; multiple elements need multiple states.

### Timing the exit

There is no exit-duration separate from the main animation, no delay, and no cancellation callback. Set `TransitionState.animation` or use `instant_state`; all timing comes from that one `Animation`.

## Shared elements

There is no shared-element transition, keyed element matching or cross-page flying. The caller can animate one rectangle toward another component's bounds, but the UI does not discover or preserve identity.

### Flying

“Flying” requires the caller to compute start/end `Rect`s and run `transition`; there is no framework matcher or overlay clone.

### Everything else

Only geometry is interpolated. A caller crossing containers must draw the animated content at the interpolated rectangle itself; no source/target widget is cloned.

### Scaled content

Text and child layout do not scale as a group. A transition rect does not apply a `Transform`; the caller must wrap content in `Context::transform` if scale is desired.

### Text that morphs

No text morphing, cross-fade or glyph interpolation exists. Text is rebuilt each frame; changing strings snap unless the caller fades or animates color manually.

## When nothing moves

`transition` initializes directly at the target on first use, so a newly created state does not animate in. A target equal to `state.to` does not restart. Reduce-motion makes caller-side animations jump only if the caller checks `Context::reduce_motion`; `transition` itself still uses its animation duration.

For no motion at all, use `instant_state()` or zero-duration `Animation`; no global “disable transitions” switch exists.
