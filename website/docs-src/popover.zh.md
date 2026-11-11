`popover(c, "id", &mut state, anchor, content)` 是锚定 overlay。绘制在普通内容之上，打开时限制焦点，Escape/外部点击关闭。

```rust
use myrust_core::ui::OverlayState;

let state = popover(c, "menu", &mut app.popover, anchor, |c| {
    text(c, "Popover content");
});
```

## Without a look

`popover` 是 `overlay` 别名：它拥有 panel 背景与锚定，内容由调用方构建。没有带标题 popover、arrow、动画、嵌套 popover stack 或自定义 placement。

anchor 必须是上一/当前 `Rect`；popover 宽度跟随 anchor，溢出时翻到上方。自定义 panel chrome 可在 content 闭包内构建。

## Accessibility

popover surface 发出 `Role::Popup`，label 为 overlay id；child 控件/文本保留自己的 node。没有 Popover/Dialog role、expanded 状态、anchor 关系或 AT 桥。

打开时焦点进入 popup layer，关闭恢复；没有平台 accessibility 桥消费。
