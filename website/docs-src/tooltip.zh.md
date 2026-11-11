`tooltip_for(c, "id", anchor, target_id, text)` 在指针 hover target 或 target 聚焦并经过 delay 后显示小 label。`tooltip` 是仅 anchor 的变体。

```rust
let tip = tooltip_for(c, "hint", anchor, "save-button", "Save changes");
if tip.visible {
    app.tooltip_seen = true;
}
```

## The delay

默认 delay 是 `TOOLTIP_DELAY`；`tooltip_with_delay` 接收自定义 `Duration`。hover/focus 启动计时，离开清除。

tooltip 不跟随 pointer、不自动翻转、不支持富内容或 close 按钮。delay 是 wall-clock，测试需冻结帧时钟或等待。

## Without a look

没有底层 tooltip body builder、arrow、动画或 collision engine。`tooltip_for` 绘制圆角 label box；自定义内容需 overlay（但就不再是 tooltip）。

## Accessibility

tooltip 文本是 `Role::Label`，无 value；anchor target 发出自己的 node。没有 Tooltip role、`aria-describedby` 关系、delay 元数据或 AT 桥。

此实现中 screen reader 不会宣告 tooltip。
