`select(c, "id", &mut selected, &["One", "Two"])` 是带 overlay option 列表的下拉触发器。selected 索引由调用方持有。

```rust
let state = select(c, "mode", &mut app.mode, &["Light", "Dark", "System"]);
if state.changed {
    app.apply_mode(state.selected);
}
```

## Without a look

没有底层 select/listbox builder 或自定义 option row。`select` 拥有 trigger、overlay 与 option 绘制；自定义内容需独立 overlay 与手动控件。

trigger 没有 hint text、disabled、搜索过滤或多选。`combobox` 处理过滤文本选择；`select` 是简单索引选择器。

## Accessibility

trigger 发出 `Role::ComboBox`，label 为 `"select"`，value 为选中 option label，带 focused 布尔。打开的 option 发出 `Role::Option`，label 为 option 文本，value 为 `"selected"`/`"unselected"`。

没有 expanded/collapsed 状态、active descendant、disabled option 或 AT 桥。
