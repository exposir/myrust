`split_view(c, "id", &mut state, orientation, first, second)` 布置两个 panel 与可拖动 divider。`SplitViewState { ratio }` 由调用方持有；方向键每次移动 0.02。

```rust
use myrust_core::ui::{SplitOrientation, SplitViewState};

let response = split_view(c, "split", &mut app.split, SplitOrientation::Horizontal, |c| {
    text(c, "Left");
}, |c| {
    text(c, "Right");
});
```

## Accessibility

divider 发出 `Role::Slider`，label 为 `"split view"`，value 为 `"N%"`，带 focused 布尔。panel root 是 group；没有独立 panel role。

没有 min/max panel size、double-click reset、键盘 Home/End 或 AT 桥。response 暴露 ratio/changed/dragging/focused/hovered/divider_bounds。
