`text_field(c, "id", &mut text)` 是单行输入，文本由调用方 `String` 持有。返回 `TextFieldState { changed, cursor, focused, hovered }`。

```rust
let state = text_field(c, "name", &mut app.name);
if state.changed {
    app.mark_dirty();
}
```

## Editing

字符输入在 cursor 插入；Backspace/Delete 删除前后字符，Home/End 跳转，Left/Right 按 UTF-8 字符边界移动。可用 `Context::set_text_input_selection` 设置选区；输入会替换选区。

没有 clipboard、undo/redo、IME composition、autofill、密码遮罩或多行编辑。`editable_text` 是就地标签替代。

## The caret

caret 是 measured prefix 宽度处的 1px 线。`TextInputState` 暴露 `cursor`、`selection`、`bounds`、`cursor_x`、`cursor_y`；没有 blink timer 或 caret 动画。

## Beside the lines

字段单行，不换行、不显示行号。label、description、error 是独立控件；`form_field` 视觉分组，但不建立语义连接。

## Errors

`text_field` 没有 error/validation 参数。调用方在旁边绘制红/muted 消息，通常用 `form_field`；`AccessNode` 无 error/invalid 状态。

## Without a look

没有底层 editable-text builder。`TextInputState` 提供 cursor/selection/geometry，但自绘需自己实现输入与编辑；标准 `text_field` 是唯一完整可编辑控件。

## Accessibility

字段发出 `Role::TextField`，label 为 `"text field"`，value 为完整文本，带 focused 布尔。不发 hint text、description、error、multiline、readonly 或 password 标志；无 AT 桥。
