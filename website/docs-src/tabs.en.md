`tabs(c, "settings", &mut app.tab, &["General", "Appearance", "Advanced"])` draws a row of tabs. The selected index is caller-owned, and the returned `TabsState` reports whether input changed it.

```rust
let state = tabs(c, "settings", &mut app.tab, &["General", "Appearance", "Advanced"]);
if state.changed {
    // load the newly selected page
}

match app.tab {
    0 => general(c),
    1 => appearance(c),
    _ => advanced(c),
}
```

The tab row is one focus stop. Clicking a tab chooses it; while focused, Left and Right move the choice. There is no disabled-tab variant and no low-level tab-list builder in the current crate.

## Accessibility

The control emits a `Role::TabList` node with the stable id as its label and the tab list's focus state. Each tab emits `Role::Tab` with its visible label, and its value is `"selected"` or `"unselected"`; tabs themselves are not separately focusable. No platform accessibility bridge consumes these nodes yet.
