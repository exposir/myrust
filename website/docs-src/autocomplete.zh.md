`autocomplete(c, "id", &mut text, &mut state, candidates)` 在文本字段下方建议候选。Enter 选择高亮候选；Escape 关闭 overlay。

```rust
use myrust_core::ui::AutocompleteState;

let state = autocomplete(
    c, "city", &mut app.city, &mut app.auto,
    &["Berlin", "Bern", "Boston"],
);
if let Some(index) = state.selected {
    app.choose(index);
}
```

## Accessibility

input 发出 `Role::TextField`，label 为 `"text field"`，value 为文本，带 focused 布尔。suggestion row 是普通 `Role::Label` 文本节点，不是 `Option`；没有 ComboBox/listbox role、active descendant 或 expanded 状态。

没有 async suggestion、loading、disabled candidate 或 AT 桥。popup 只是视觉元数据。
