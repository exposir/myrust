overlay 是锚定到上一帧矩形的 popup。它在普通根之后绘制，拥有顶层，并在打开时阻止下层指针输入。

```rust
use myrust_core::ui::{overlay, text, OverlayState};

let anchor = myrust_core::ui::Rect::new(16.0, 16.0, 160.0, 28.0);
overlay(c, "popover", &mut app.popover, anchor, |c| {
    text(c, "Anchored content");
});
```

`OverlayState::open` 由调用方持有。overlay 在会溢出窗口时翻到锚点上方，并按锚点宽度（带最小值）确定尺寸。

## The keyboard

打开 overlay 会给它分配聚焦层；Tab 与 Shift+Tab 留在该层内。Escape 关闭它，指针在上一帧 overlay bounds 外释放也会关闭。`OverlayResponse` 报告 `closed`、`escape_pressed`、`outside_clicked` 与上一帧 `bounds`。

关闭后焦点恢复到打开前控件。打开期间下层不接收指针输入。primitive 本身没有 modal scrim；dialog 自己绘制 dimming 层。

## Your own overlays

`context.overlay(id, &mut state, anchor, content)` 是公开 primitive；`popup` 是别名。dialog 基于同一层，增加居中 panel、scrim 与 `DialogAction` 结果。

自定义 overlay 内容就是普通控件。没有任意窗口栈、z-index、嵌套 overlay 深度 API 或非锚定 floating panel。overlay 注册为 `Role::Popup`，label 为 id；不发出 live-region 或 alert role。
