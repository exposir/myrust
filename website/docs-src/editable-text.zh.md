`editable_text(c, "id", &mut state, &mut text)` 显示 label，点击后切换为内联文本字段。Enter 或失焦提交；Escape 取消编辑。

```rust
use myrust_core::ui::EditableTextState;

let response = editable_text(c, "title", &mut app.title_edit, &mut app.title);
if response.submitted {
    app.persist_title();
}
```

## In lists and tables

没有内置 cell/row editor mode。一个 `id` 编辑一个 `String`；列表/表格需每行一个 state 与稳定 row id，并避免构造两个相同 id 的编辑器。

`editable_text` 不与 `ListState`/`TableState` 选择或 scroll-into-view 集成。放入 row 只是调用方布局。

## Accessibility

view 模式发出 `Role::Button`，label 为文本，value 为 `"editable"`，带 focused 布尔。edit 模式发出底层 `Role::TextField`，label 为 `"text field"`，value 为 buffer，带 focused 布尔。

没有 inline-edit role、description/error、read-only 或 AT 桥。开始编辑时焦点从 view button 移到 input。
