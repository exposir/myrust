`menu_button(c, "id", "Actions", &mut state, &["Cut", "Copy"])` draws a trigger and opens a themed menu below/above it. The selection is an index into the item labels.

```rust
use myrust_core::ui::MenuState;

let state = menu_button(
    c, "actions", "Actions", &mut app.menu, &["Cut", "Copy"],
)
.selected;
if let Some(index) = state {
    app.command(index);
}
```

## Items

Items are string labels; the response `selected` is their index. `MenuState` owns `open` and keyboard `highlight`. Menu items are `Role::Option` with value `"focused"` or `"normal"`.

No icons, shortcuts, disabled items, checkmarks, submenus or separators exist. Richer menus need custom drawing or a caller-owned overlay.

## Any button

The trigger is always the standard button drawn by `menu_button`; there is no “attach this menu to an arbitrary button” API. A custom trigger must be built as a separate button plus a caller-managed `OverlayState`.

## Drawn in the window

The menu is drawn in-window with theme colors via the overlay primitive, not as a native OS menu. It anchors below the trigger, flips above on overflow, closes on outside click/Escape and traps focus on its layer.

System menus (macOS/Windows), native context menus and platform menu bars are not exposed by `myrust-ui`; those require `myrust-core`/platform integration.

## Without a look

There is no low-level menu builder exposing item nodes, custom rows or item focus. `menu_button` is the only public menu-button primitive; `context_menu` is the separate right-click path.

## Accessibility

The trigger emits `Role::Button` with its label; the open menu surface is `Role::Popup` with the overlay id. Items emit `Role::Option`, label = item text, value = `"focused"`/`"normal"`.

There is no `Menu`/`MenuItem` role, checked/disabled state, shortcut metadata or platform AT bridge.
