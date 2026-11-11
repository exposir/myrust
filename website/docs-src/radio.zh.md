`radio_group(c, "size", &mut selected, &["Small", "Medium"])` 绘制纵向单选列表并保持一个 selected 索引。

```rust
let state = radio_group(c, "size", &mut app.size, &["Small", "Medium", "Large"]);
if state.changed {
    app.apply_size(state.selected);
}
```

## Without a look

没有底层 radio builder 或自定义 radio row。`radio_group` 是公开 primitive；自定义视觉需要独立 container 与手动选择/按键处理。

## Accessibility

根发出 `Role::RadioGroup`，label 为控件 id，带 focused 布尔。每个 option 发出 `Role::Radio`，label 为文本，value 为 `"selected"`/`"unselected"`；option 不能单独聚焦。

没有 disabled option、multi-select、description/error 或 AT 桥。
