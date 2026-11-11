`TextInputState` 把现有 text field 状态暴露给自定义控件。用 `Context::text_input_state(id)` 查询，用 `set_text_input_selection(id, start, end)` 移动选区。

```rust
if let Some(state) = c.text_input_state("editor") {
    let point = state.cursor_position();
    let range = state.selection_range();
}

c.set_text_input_selection("editor", 2, 5);
```

## Client contract

`TextInputState` 有 `cursor`（字节偏移）、`selection: Option<(start, end)>`、上一/当前 `bounds`、绝对 `cursor_x`/`cursor_y`。selection 会归一化；anchor 相等时 `None`。

调用方仍持有 `String`，每帧必须调用 `text_field`/`text_field_impl`。没有 callback stream、composition event、clipboard、undo 或 IME；自定义控件不能经此 API 拦截按键。

## Retained text geometry

geometry 来自上一渲染帧；`cursor_x/y` 使用 measured prefix 宽度与 padding，`bounds` 来自 interaction region。字段尚未构造/渲染时查询返回 `None` 或旧 bounds。

没有换行 geometry、多行 caret rect、glyph hit-testing 或 selection handle geometry。只暴露单行 cursor point 与归一化字节选区。
