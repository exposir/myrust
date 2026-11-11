`TextInputState` exposes the existing text field's state to custom controls. Query it with `Context::text_input_state(id)`, and move a selection with `set_text_input_selection(id, start, end)`.

```rust
if let Some(state) = c.text_input_state("editor") {
    let point = state.cursor_position();
    let range = state.selection_range();
}

c.set_text_input_selection("editor", 2, 5);
```

## Client contract

`TextInputState` has `cursor` (byte offset), `selection: Option<(start, end)>`, previous/current `bounds`, and absolute `cursor_x`/`cursor_y`. Selection is normalized; equal anchors yield `None`.

The caller still owns the `String` and must call `text_field`/`text_field_impl` each frame. There is no callback stream, composition event, clipboard, undo or IME; custom controls cannot intercept keystrokes through this API.

## Retained text geometry

Geometry is retained from the last rendered frame; `cursor_x/y` use measured prefix width and padding, `bounds` comes from interaction regions. Querying before the field is constructed/rendered returns `None` or stale bounds.

There is no line-wrapping geometry, multi-line caret rect, glyph hit-testing or selection handle geometry. Only the single-line cursor point and normalized byte selection are exposed.
