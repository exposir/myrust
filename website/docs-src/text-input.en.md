`text_field(c, "id", &mut text)` is a single-line input with a caller-owned `String`. It returns `TextFieldState { changed, cursor, focused, hovered }`.

```rust
let state = text_field(c, "name", &mut app.name);
if state.changed {
    app.mark_dirty();
}
```

## Editing

Character input inserts at the cursor; Backspace/Delete remove before/after, word/line navigation uses the shaped cluster map, mouse and Shift selection are supported, and typing replaces the selection. Cut/copy/paste use an injectable `ClipboardProvider`; consecutive typing is grouped into one undo transaction while paste and deletion are separate groups. Use `text_area` for multiline editing.

`UiEvent::CompositionStart/Update/Commit/Cancel` drive IME preedit. The preedit is drawn with an accent background and underline, commit replaces the current selection, and cancel discards it. Linux's tao layer exposes committed IME text only; the browser demo supplies start/update/end from its hidden textarea. Autofill, dead keys and password masking remain separate concerns.

Native preedit cannot be supplied by tao 0.37: Linux's `IMContextSimple` exposes commits only, macOS marked text is not forwarded, and Windows has no preedit event. Applications can feed composition events directly; built-in native preedit is a known limitation until tao is patched or the event loop is replaced.

The browser demo maps copy/cut/paste to `navigator.clipboard` when it is available and stores the text in a host-injected `ClipboardProvider`. A missing API, permission denial or non-secure context falls back to the in-memory provider without raising a page error.

## The caret

The caret is a 1px line at the shaped prefix width and follows preedit and horizontal scrolling. `TextInputState` exposes the committed `cursor`, `selection`, `bounds`, `cursor_x` and `cursor_y`; no blink timer or caret animation exists.

## Beside the lines

The field is one line and does not wrap or show line numbers. Labels, descriptions and errors are separate widgets; `form_field` groups them visually but does not connect them semantically.

## Errors

There is no error/validation argument on `text_field`. Callers draw a red/muted message beside it, usually via `form_field`; `AccessNode` carries no error or invalid state.

## Without a look

There is no low-level editable-text builder. `TextInputState` gives cursor/selection/geometry, but custom drawing must implement its own input and editing; the standard `text_field` is the only complete editable control.

## Accessibility

The field emits `Role::TextField`, label `"text field"`, value = full text, and focused boolean. It does not emit hint text, description, error, multiline, readonly or password flags; no AT bridge exists.
