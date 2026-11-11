`context_menu(c, "id", &mut state, area, &["Inspect"])` opens a themed menu when the secondary button is pressed inside `area`. Selection is an index; the menu anchors at the click point.

```rust
use myrust_core::ui::{context_menu, ContextMenuState, Rect};

let area = Rect::new(16.0, 16.0, 320.0, 120.0);
let state = context_menu(
    c, "item-menu", &mut app.context, area, &["Duplicate", "Delete"],
);
if let Some(index) = state.selected {
    app.run(index);
}
```

## Items

Items are string labels; `ContextMenuState` stores `open`, `highlight` and the click `anchor`. Selecting returns the index; Escape/outside click closes.

No icons, shortcuts, disabled/checked items, submenus or separators. A custom row/item requires a separate overlay.

## Which element

The caller supplies one rectangle; the menu opens only from a secondary press inside it. There is no per-element registry, right-click event on arbitrary drawing or hit-test through path geometry.

For multiple elements, call `context_menu` with each element's previous bounds and a separate or shared state. The area must be in the same logical space as pointer coordinates.

## Drawn in the window

The menu is drawn in-window by the overlay layer using theme colors. It is not a native OS context menu and does not use platform menu services.

### Richer items

Richer items (icons, checkmarks, submenus, shortcuts, destructive styling) are not implemented. Only string labels and index selection exist.

### Every menu of a window

There is no global context-menu router or “register all areas” API. Each `context_menu` call owns its rect/state; only the top overlay receives input.

## Without a look

There is no low-level context-menu item builder. The public primitive draws trigger-free menu content; for custom visuals use `overlay` and your own item controls.

### Accessibility

The menu surface is `Role::Popup` with its id; items are `Role::Option` with item text and value `"focused"`/`"normal"`. There is no context-menu or menu role and no AT bridge.
