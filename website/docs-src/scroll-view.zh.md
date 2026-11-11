`scroll_view(c, "id", viewport_height, content)` 裁剪纵向 viewport 并偏移内容。滚轮、滚动条拖动与键盘滚动是内部行为。

```rust
let state = scroll_view(c, "panel", 220.0, |c| {
    for row in 0..20 { text(c, &format!("Row {row}")); }
});
```

## Keeping the offset

offset 在 scroll id 下由 context 持有；`ScrollState` 返回 `scroll`、`max_scroll`、`changed`、`dragging`、`hovered`。重建内容会保留 offset，除非 scroll id 改变。

## Sticky elements

没有 sticky header/row。调用方可在 scroll view 外绘制固定 header；没有 scroll 内 sticky 行为。

## Scrolling into view

没有 `scroll_into_view` API。调用方不能通过公开 API 设置任意 scroll view 的 context offset；list/grid view 会自动计算 offset。

## Snapping

没有 snap point、paging、momentum 或 smooth wheel 动画。wheel delta 直接应用。

## Scroll bars

绘制一个内部纵向滚动条；没有水平条、corner、样式 hook 或 always-visible 选项。overlay scrollbar 不可配置。

## Accessibility

viewport 发出 `Role::ScrollView`，label 为 `"scroll view"`，无 value。children 保留自己的 role。没有 scroll position/range 元数据、focus-follow 或 AT 桥。
