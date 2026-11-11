`toolbar(c, "id", &mut state, &items, available_width, render)` 布局前导 item，并在声明宽度放不下时把尾部放进 menu button。

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

没有 toolbar 级方向键遍历或 roving focus。每个渲染控件是独立 Tab stop；overflow trigger 是标准 menu button，有自己的焦点/菜单键盘行为。

`ToolbarState` 没有 focus index。键盘用户用 Tab 在 item 间移动，而不是方向键；隐藏 item 通过菜单到达。

## Overflow

`overflow_split(items, available_width, gap)` 在预留固定菜单宽度后返回能放下多少前导 item。`toolbar` 使用它，然后用尾部 label 渲染 `…` menu button。

溢出只收尾部：没有 wrap、priority 排序、真实 child bounds 动态测量或逐 item 隐藏。宽度由调用方声明，可能与真实控件宽度不一致。overflow 菜单选择返回原始 item 索引。

## Accessibility

toolbar 根发出 `Role::Group`，label 为 id。可见 item 保留自己的 role；overflow trigger 是 `Button`，overflow item 是 `Popup` 中的 `Option` 节点。

没有 `Toolbar` role、position-in-set、overflow announcement 或平台 AT 桥。隐藏 item 只在菜单打开时以 label 存在。
