`sidebar(c, "id", &mut selected, sections)` draws grouped single-selection rows. The sidebar owns no state struct; `selected` is a caller `String`, and sections/items are caller data.

```rust
use myrust_core::ui::{SidebarItem, SidebarSection};

let items = [SidebarItem::new("home", "Home"), SidebarItem::new("settings", "Settings")];
let sections = [SidebarSection::new("General", &items)];
let response = sidebar(c, "nav", &mut app.selected, &sections);
```

## Choosing

Clicking an item selects it and focuses the list. ArrowUp/Left, ArrowDown/Right, Home and End move the focused selection. If `selected` is empty, the first item id is filled in automatically. Only one id is selected; no multi-select or checkbox.

No collapsible sidebar sections, drag reorder, search or custom row content exists.

## Accessibility

The root emits `Role::Group`, label = sidebar id, value = selected id, focused boolean. Items emit `Role::Option`, label = item label, value `"selected"`/`"unselected"`.

There is no Tree/Navigation role, level, section header semantics, position-in-set or AT bridge.
