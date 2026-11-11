`sidebar(c, "id", &mut selected, sections)` 绘制分组单选行。sidebar 没有 state struct；`selected` 是调用方 `String`，sections/items 是调用方数据。

```rust
use myrust_core::ui::{SidebarItem, SidebarSection};

let items = [SidebarItem::new("home", "Home"), SidebarItem::new("settings", "Settings")];
let sections = [SidebarSection::new("General", &items)];
let response = sidebar(c, "nav", &mut app.selected, &sections);
```

## Choosing

点击 item 选中并聚焦列表。聚焦时 ArrowUp/Left、ArrowDown/Right、Home、End 移动选择。`selected` 为空时自动填入第一个 item id。只选一个 id；没有多选或 checkbox。

没有可折叠 sidebar section、拖动重排、搜索或自定义行内容。

## Accessibility

根发出 `Role::Group`，label 为 sidebar id，value 为 selected id，带 focused 布尔。item 发出 `Role::Option`，label 为 item label，value 为 `"selected"`/`"unselected"`。

没有 Tree/Navigation role、level、section header 语义、position-in-set 或 AT 桥。
