`breadcrumbs(c, "id", items)` 绘制可点击路径。除最后一项外都是 button-like link；最后一项是当前页 label。

```rust
use myrust_core::ui::BreadcrumbItem;

let items = [BreadcrumbItem::new("home", "Home"), BreadcrumbItem::new("docs", "Docs")];
let response = breadcrumbs(c, "crumbs", &items);
if let Some(id) = response.clicked {
    app.go(id);
}
```

## Accessibility

根发出 `Role::Group`，label 为控件 id。非最后 item 发出 `Role::Button`，label 为 item label，value 为 item id；最后 item 是 `Role::Label`，无交互。

没有 Breadcrumb/Navigation role、separator 语义、current-page flag 或 AT 桥。键盘 Enter/Space 只激活获得焦点的 link item。
