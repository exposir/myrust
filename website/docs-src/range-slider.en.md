`range_slider(c, "id", &mut state, 0.0, 100.0)` draws two knobs for a low/high range. `RangeSliderState { low, high, active }` is caller-owned and clamps/orders the bounds.

```rust
use myrust_core::ui::RangeSliderState;

let state = range_slider(c, "price", &mut app.range, 0.0, 100.0);
if state.changed {
    app.filter(state.low, state.high);
}
```

## Accessibility

The control emits one `Role::Slider`, label `"range slider"`, value = `"low..high"` (formatted), and focused boolean. Individual low/high knobs have no separate nodes, values or labels.

There is no min/max/step/orientation metadata, disabled state or AT bridge. Keyboard moves the active knob; internal `RangeKnob` is not exposed as an accessible child.
