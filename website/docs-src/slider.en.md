`slider(c, "volume", &mut value, 0.0, 100.0)` sets a continuous `f32` in a range. Click/drag on the track and arrow keys while focused change the value.

```rust
let state = slider(c, "volume", &mut app.volume, 0.0, 100.0);
if state.changed {
    app.set_volume(state.value);
}
```

## Steps

There is no `step` argument: the pointer maps continuously across the track and keyboard uses the implementation's increment. Round the caller value or use a `stepper`/number input when exact integer steps are required.

`SliderState` exposes `changed`, `dragging`, `focused`, `hovered`, `value`. There are no tick marks, labels, tooltip value or disabled slider.

## Without a look

There is no low-level slider/custom track API. A custom visual must be composed with `container`/`draw` and manual pointer handling; it will not get the built-in key/drag state.

## Accessibility

The slider emits `Role::Slider`, label `"slider"`, value = the formatted value, and focused boolean. It does not emit min/max, step, orientation or disabled state; no AT bridge exists.
