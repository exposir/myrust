`menu_button(c, "id", "Actions", &mut state, &["Cut", "Copy"])` 绘制触发器并在其上下打开主题菜单。选择是 item label 的索引。

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

item 是字符串 label；response `selected` 是索引。`MenuState` 拥有 `open` 与键盘 `highlight`。菜单项是 `Role::Option`，value 为 `"focused"` 或 `"normal"`。

没有 icon、shortcut、disabled item、checkmark、submenu 或 separator。更丰富的菜单需自绘或调用方 overlay。

## Any button

触发器始终是 `menu_button` 绘制的标准 button；没有“把菜单挂到任意 button”API。自定义 trigger 需另建 button + 调用方管理 `OverlayState`。

## Drawn in the window

菜单通过 overlay primitive 在窗口内用主题色绘制，不是 native OS menu。它锚在 trigger 下方，溢出时翻到上方，外部点击/Escape 关闭，并在层内限制焦点。

`myrust-ui` 不暴露系统菜单（macOS/Windows）、native context menu 或 platform menu bar；这些需要 `myrust-core`/平台集成。

## Without a look

没有暴露 item node、自定义 row 或 item focus 的底层 menu builder。`menu_button` 是唯一公开 menu-button primitive；`context_menu` 是独立右键路径。

## Accessibility

trigger 发出 `Role::Button`，label 为其文本；打开的菜单表面是 `Role::Popup`，label 为 overlay id。item 发出 `Role::Option`，label 为 item 文本，value 为 `"focused"`/`"normal"`。

没有 `Menu`/`MenuItem` role、checked/disabled 状态、shortcut 元数据或平台 AT 桥。
