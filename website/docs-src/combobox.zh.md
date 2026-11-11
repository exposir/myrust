`combobox(c, "id", &mut query, &mut state, &["Apple", "Apricot"])` 是可编辑文本输入 + 过滤 option overlay。输入按大小写不敏感过滤；Enter 选择；Escape 关闭。

```rust
use myrust_core::ui::ComboboxState;

let state = combobox(
    c, "fruit", &mut app.query, &mut app.combo,
    &["Apple", "Apricot", "Banana"],
);
if let Some(index) = state.selected {
    app.choose(index);
}
```

## Without a look

没有底层 combobox/listbox builder 或自定义 option row。trigger 就是 text input；option 用共享 overlay primitive 绘制。

没有 disabled option、分组、多选、异步加载或 option icon。`select` 是不可编辑索引选择器；`autocomplete` 只建议文本。

## Accessibility

input 发出 `Role::TextField`，label 为 `"text field"`，value 为 query，带 focused 布尔。option row 是普通 `Role::Label` 文本节点，不是 `Option`；没有 ComboBox role、expanded 状态、active descendant 或 AT 桥。
