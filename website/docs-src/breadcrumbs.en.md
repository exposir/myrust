`breadcrumbs(c, "id", items)` draws a clickable path. Every item except the last is a button-like link; the final item is the current page label.

```rust
use myrust_core::ui::BreadcrumbItem;

let items = [BreadcrumbItem::new("home", "Home"), BreadcrumbItem::new("docs", "Docs")];
let response = breadcrumbs(c, "crumbs", &items);
if let Some(id) = response.clicked {
    app.go(id);
}
```

## Accessibility

The root emits `Role::Group` with the control id. Non-final items emit `Role::Button`, label = item label, value = item id; the last item is a `Role::Label` with no interaction.

There is no Breadcrumb/Navigation role, separator semantics, current-page flag or AT bridge. Keyboard Enter/Space activates the focusable link items; the final label is inert.
