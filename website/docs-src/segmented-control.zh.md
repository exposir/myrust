`segmented(c, "id", &mut selected, &["Small", "Medium", "Large"])` 在当前宽度内绘制等宽选择器。点击 segment 选中；聚焦时方向键移动。

```rust
let state = segmented(c, "size", &mut app.size, &["Small", "Medium", "Large"]);
if state.changed {
    app.apply_size(state.selected);
}
```

## Full width

控件占满可用宽度并等分 label，只留一个 gap。不支持 intrinsic/two-segment 宽度或逐 segment 宽度。

## Icons and other content

segment 只有文本 label。没有 icon、badge、任意 child widget 或逐 segment tooltip。自定义 segment 内容需自绘 container；公开控件只接受 labels。

## Accessibility

根发出 `Role::TabList`，label 为控件 id；每个 segment 发出 `Role::Tab`，label 为文本，value 为 `"selected"`/`"unselected"`。焦点在根 TabList；segment 不能单独聚焦。

没有 disabled segment、roving tab index 或 AT 桥。根聚焦时方向键/Home/End 改变选择；选择 clamp 不循环。
