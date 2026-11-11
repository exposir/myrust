`editable_text(c, "id", &mut state, &mut text)` shows a label and switches it into an inline text field when clicked. Enter or blur commits; Escape cancels an edit.

```rust
use myrust_core::ui::EditableTextState;

let response = editable_text(c, "title", &mut app.title_edit, &mut app.title);
if response.submitted {
    app.persist_title();
}
```

## In lists and tables

There is no built-in cell/row editor mode. One `id` edits one `String`; a list or table needs a state per row plus stable row ids, and must avoid constructing two editors with the same id.

`editable_text` does not integrate with `ListState`/`TableState` selection or scroll into view. Placement in a row is caller layout only.

## Accessibility

In view mode it emits `Role::Button`, label = the text, value `"editable"`, focused boolean. In edit mode it emits the underlying `Role::TextField`, label `"text field"`, value = buffer, focused boolean.

There is no inline-edit role, description/error, read-only state or AT bridge. Focus moves from the view button to the input when editing begins.
