`toolbar(c, "id", &mut state, &items, available_width, render)` lays out leading items and puts the tail into a menu button when the declared widths do not fit.

```rust
use myrust_core::ui::{toolbar, ToolbarItem, ToolbarState};

let items = [
    ToolbarItem::new("cut", "Cut", 90.0),
    ToolbarItem::new("copy", "Copy", 90.0),
    ToolbarItem::new("paste", "Paste", 90.0),
];
let state = toolbar(c, "bar", &mut app.toolbar, &items, 260.0, |c, index| {
    button(c, items[index].label);
});
if let Some(index) = state.selected {
    app.run(index);
}
```

## Keyboard

There is no toolbar-level arrow traversal or roving focus. Each rendered control is its own Tab stop; the overflow trigger is a standard menu button with its own focus/menu keyboard behavior.

`ToolbarState` has no focus index. Keyboard users move between items with Tab, not arrows, and an item hidden in overflow is reached through the menu.

## Overflow

`overflow_split(items, available_width, gap)` returns how many leading items fit after reserving a fixed menu width. `toolbar` uses it, then renders a `…` menu button with the callsite labels for the tail.

Overflow is tail-only: no wrap, priority ranking, dynamic measurement of actual child bounds or per-item hide. Widths are caller-declared and may not match real control widths. The overflow menu selection returns the original item index.

## Accessibility

The toolbar root emits `Role::Group` labelled by its id. Visible items keep their own roles; the overflow trigger is a `Button`, and overflow items are `Option` nodes in a `Popup`.

There is no `Toolbar` role, position-in-set, overflow announcement or platform AT bridge. Hidden items exist only as menu labels while open.
