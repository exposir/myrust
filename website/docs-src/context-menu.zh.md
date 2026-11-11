`context_menu(c, "id", &mut state, area, &["Inspect"])` 在 `area` 内按下辅助键时打开主题菜单。选择是索引；菜单锚在点击点。

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

item 是字符串 label；`ContextMenuState` 存 `open`、`highlight` 与点击 `anchor`。选择返回索引；Escape/外部点击关闭。

没有 icon、shortcut、disabled/checked item、submenu 或 separator。自定义 row/item 需要独立 overlay。

## Which element

调用方提供一个矩形；只有其中辅助键按下才打开菜单。没有 per-element registry、任意自绘上的 right-click event 或 path 几何命中。

多个元素时，用各自上一帧 bounds 调用 `context_menu`，共享或分开状态。area 必须与指针坐标处于同一逻辑空间。

## Drawn in the window

菜单由 overlay 层在窗口内用主题色绘制。不是 native OS context menu，不使用平台菜单服务。

### Richer items

更丰富的 item（icon、checkmark、submenu、shortcut、destructive styling）未实现。只有字符串 label 与索引选择。

### Every menu of a window

没有全局 context-menu router 或“注册所有区域”API。每次 `context_menu` 调用拥有自己的 rect/state；只有顶层 overlay 接收输入。

## Without a look

没有底层 context-menu item builder。公开 primitive 绘制无 trigger 的菜单内容；自定义视觉用 `overlay` 与自己的 item 控件。

### Accessibility

菜单表面是 `Role::Popup`，label 为 id；item 是 `Role::Option`，label 为 item 文本，value 为 `"focused"`/`"normal"`。没有 context-menu/menu role，也没有 AT 桥。
