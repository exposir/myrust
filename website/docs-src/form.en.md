`form_field(c, "id", "Label", Some("Description"), None, content)` lays out a label, optional description, caller content and optional error. It is a visual grouping, not a full form model.

```rust
let state = form_field(c, "name-field", "Name", Some("Display name"), None, |c| {
    text_field(c, "name-input", &mut app.name);
});
```

## Forms

There is no `Form` container, submit event, validation model or field registry. Each `form_field` is independent; the caller wires submission/validation.

## Fieldsets

No fieldset/legend or nested form grouping exists. Use a column with a label/header and multiple `form_field`s.

## Labels of your own

The `label` argument is rendered as a `Role::Label` text node; there is no explicit `for`/label association in AccessNode. A custom label can be drawn outside, but no semantic link is emitted.

## Hints and errors of your own

`description`/`error` strings are rendered as text and reflected only in the field root's value (`"valid"`/`"error"`). There is no per-control invalid flag, `aria-describedby`, live error announcement or error icon.

## Accessibility

The field root emits `Role::Group`, label = the visible label, value `"error"`/`"valid"`. Description/error/label strings are `Role::Label` text nodes. The wrapped control emits its own node.

There is no Form/Fieldset role, required/invalid flag on the control, label association or AT bridge.
