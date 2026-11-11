Input reaches the view through `NativeUi::input(UiEvent)`. A frame sees the pointer and keys accumulated since the previous frame; widgets use last frame's interaction bounds, so a click is reported after the pointer is released over the same region.

```rust
ui.input(UiEvent::PointerMoved { x, y })?;
ui.input(UiEvent::Key { key: Key::Tab })?;
let frame = ui.render(Size::new(640.0, 360.0))?;
```

## The pointer

`Context::pointer_position()` returns the latest logical point, `pointer_down()` reports the primary button, and `wheel_delta()` returns accumulated wheel movement. Standard controls register hover/press/click regions from the previous frame.

There is no pointer capture, drag threshold, cursor-shape API, right-click query or multi-touch model. `UiEvent::PointerPressed/Released` carry the primary button; custom raw-pointer widgets must combine these with their own last-frame bounds.

### A press the system cancels

There is no cancellation event distinct from `PointerReleased`. A press that leaves the region does not click; the control compares release with the previous frame's region. Gesture cancellation from the OS is not represented, so a caller must synthesize its own state if it needs that distinction.

## The keyboard focus

Tab and Shift+Tab traverse focusable controls in construction order; overlays trap focus on their top layer. `Context::focus(id)` queries ownership and `request_focus(id)` moves it.

Focusable widgets draw a ring and handle Enter/Space where appropriate. There is no public focus-order override, roving tab index or focus-scope object; the construction order and overlay layer are the order.

## Persistent focus

Focus is a stable string id stored in `Context`, so it survives frames while the control remains focusable. Removing the control or changing its id loses focus; `migrate_state` can move focus to a renamed id.

There is no per-window focus handle or API for restoring focus after an arbitrary external window. A caller can call `request_focus` on the next frame.

## Focus bound to data

Focus is not bound to a data model or collection index. It is keyed by the caller's stable id; a list that reorders data must keep stable row ids or focus will follow the old id.

There is no `Focus<T>` wrapper or data-keyed focus registry. The caller maps data to ids and may call `request_focus` after a structural change.

## Focus groups

There is no public focus-group container or group-level arrow navigation. Groups are just construction order; overlays form an internal top-layer focus trap.

Arrow-key navigation inside tabs and segmented controls is implemented by those controls, not by a general focus-group mechanism. Nested focus groups and escape-to-parent focus are not available.

## Shortcuts

No global shortcut registry, modifier matching or command table exists. Keys arrive as `UiEvent::Key`; a control consumes the keys it owns, and the application can inspect its own events before/after `NativeUi`.

`Context` does not expose a public key-pressed query, so application shortcuts should be handled outside the view or by a custom focusable control. Modifier state and shortcut priority are not modeled.

## Keys held

`UiEvent::Key` is a discrete press, not a down/up lifecycle. There is no held-key query, auto-repeat distinction or key-release event.

Held keys, key combinations and modifier chords cannot be observed through the current context API. An app can still maintain its own pressed set outside `NativeUi` if the platform sends raw events.

## Input methods

`UiEvent::TextInput { text }` inserts a completed string into a focused text field. There is no preedit, composition range, candidate window or IME commit/cancel event.

Text fields support cursor movement and selection, but IME composition is explicitly out of scope; a host IME can only deliver committed text. Bidi input and complex-script shaping are not implemented.

## Dropped files

Native UI windows do not accept operating-system file drops. File drop events belong to WebView windows (`FileDropEvent` / `FileDropHandler` in `myrust-core`), not to the immediate-mode native view.

There is no `NativeUi` drop callback. A caller can implement its own platform file-drop bridge and feed a payload into application state, but that is outside this crate.

## Scrolling and pinching

`scroll_view(c, id, viewport_height, content)` clips content and handles wheel, scrollbar drag and keyboard scrolling. `Context::wheel_delta()` exposes accumulated wheel input for custom controls.

Pinch/zoom, momentum scrolling, overscroll and two-axis scroll containers are not implemented. The scroll view is vertical; horizontal scrolling and touch gestures are absent.

## Every key, as it comes

`NativeUi::input(UiEvent::Key { key })` delivers named keys; `UiEvent::TextInput` delivers committed text separately. `Key` includes Tab/ShiftTab, Enter, Space, Backspace, Delete, arrows, Home, End, PageUp and PageDown.

The native window translator maps only a subset: Tab, Enter, Space, Backspace, Delete, Left/Right, Home and End. PageUp/PageDown exist in `Key` but are not yet mapped in `app/mod.rs`, so a native window cannot produce them until that mapping is added.

### Every key of the keyboard

There is no raw scancode, physical key, modifier bitmask or text-with-modifiers model. Character keys are delivered as text after the platform layout; named navigation keys use the limited `Key` enum.

Function keys, keypad distinctions, dead keys and composition data are not represented. A custom host can construct `UiEvent` values directly, but the built-in native adapter does not expose the full keyboard.

## Input inside a region

Hit-testing uses the previous frame's interaction regions. `scroll_view` and `split_view` adjust pointer coordinates for their active offset; `Context::transform` records an affine transform and inverse-transforms the pointer into local bounds.

Ordinary scissor clips do not change hit regions, and arbitrary `ScissorPush` commands do not create a coordinate space. Custom clipped content must keep its interaction bounds in the same local space as its layout.

## See also

- Layout for flow and clipping.
- Text for text measurement and inline limits.
- Styling and themes for focus rings and colors.
- Accessibility for the emitted nodes; no platform AT bridge is connected.
