`text_field(c, "id", &mut text)` is a single-line input with a caller-owned `String`. It returns `TextFieldState { changed, cursor, focused, hovered }`.

```rust
let state = text_field(c, "name", &mut app.name);
if state.changed {
    app.mark_dirty();
}
```

## Editing

Character input inserts at the cursor; Backspace/Delete remove before/after, Home/End jump, and Left/Right move by UTF-8 char boundary. Selection can be set with `Context::set_text_input_selection`; typing replaces it.

There is no clipboard, undo/redo, IME composition, autofill, password masking or multi-line editing. `editable_text` is the in-place label alternative.

## The caret

The caret is a 1px line at the measured prefix width. `TextInputState` exposes `cursor`, `selection`, `bounds`, `cursor_x` and `cursor_y`; no blink timer or caret animation exists.

## Beside the lines

The field is one line and does not wrap or show line numbers. Labels, descriptions and errors are separate widgets; `form_field` groups them visually but does not connect them semantically.

## Errors

There is no error/validation argument on `text_field`. Callers draw a red/muted message beside it, usually via `form_field`; `AccessNode` carries no error or invalid state.

## Without a look

There is no low-level editable-text builder. `TextInputState` gives cursor/selection/geometry, but custom drawing must implement its own input and editing; the standard `text_field` is the only complete editable control.

## Accessibility

The field emits `Role::TextField`, label `"text field"`, value = full text, and focused boolean. It does not emit hint text, description, error, multiline, readonly or password flags; no AT bridge exists.
