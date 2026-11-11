`checkbox(c, "Accept terms", &mut checked)` 绘制带标签复选框，返回 `CheckboxState { changed, checked, focused, hovered }`。点击或 Space 切换。

```rust
let state = checkbox(c, "Accept terms", &mut app.accepted);
if state.changed {
    app.validate();
}
```

## Groups

没有 checkbox-group 或多选容器。每个 checkbox 拥有一个调用方 `bool`；分组只是视觉/应用状态。

`id` 同时是可见 label。没有独立 label/description/error API、disabled checkbox 或组级 AccessNode。

## Mixed

只支持 boolean。没有 mixed/indeterminate 三态、“select all” helper 或子项聚合；父 checkbox 需自行计算子树状态。

## Without a look

没有底层 checkbox builder。自定义视觉需 `container`/`draw` 与手动 hit 处理；标准控件才发 checkbox AccessNode 与焦点行为。

## Accessibility

checkbox 发出 `Role::CheckBox`，label 为固定 `"checkbox"`（不是可见 id），value 为 `"checked"`/`"unchecked"`，带 focused 布尔。没有 mixed、description、error、disabled 或 AT 桥。
