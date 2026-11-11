`back_forward_buttons(c, "id", &mut RouterState)` 绘制两个 history 按钮，并在点击时移动 router index。它是 `RouterState` 的 UI 配套。

```rust
let state = back_forward_buttons(c, "history", &mut app.router);
if state.back_clicked {
    app.leave_page();
}
```

## Accessibility

两个控件都发出 `Role::Button`：label 为 `"back"`/`"forward"`，value 为 `"enabled"`/`"disabled"`，disabled 按钮无 focused flag。

没有 History/Navigation role、shortcut 元数据、page title 宣告或 AT 桥。disabled 按钮移除 interaction，但仍发 nodes。
