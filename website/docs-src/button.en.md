`button(c, "Save")` draws a themed push button and returns `ButtonState` for the frame. The label is a plain string; the click is consumed inline by the caller.

```rust
if button(c, "Save").clicked {
    app.save();
}
```

## Content

The button content is one text label. There is no child/content closure, leading icon, keyboard shortcut label or multi-line button; compose those with `row`/`icon` around a custom control or use a separate label.

## States

`ButtonState` exposes `clicked`, `pressed`, `focused`, `hovered` and `id`/`bounds`. Hover and press animate through theme colors; a click is reported after a release over the previous region, and Enter/Space activates while focused.

There is no disabled state, loading state, toggle state, long-press or double-click. A disabled-looking button must be simulated by not constructing it or by drawing/custom behavior.

## Style

Button fill, border, radius, text and focus ring come from the current `Theme`; there is no per-button style builder or color override. Set `NativeUi::with_theme` or build a custom container with `.draw(...)` for a different look.

## Accessibility

The button emits `Role::Button` with its visible label, no `value`, and the current `focused` boolean. It does not emit a disabled state or shortcut; no platform AT bridge consumes it.

## Click actions

`ButtonState::clicked` is the action hook. There is no deferred `OnClick`, command object, action id or click callback; call the action immediately after constructing the button. `c.id("save").button("Save")` sets a stable id for focus/state when needed.
