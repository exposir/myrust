`text_field(c, "id", &mut text)` 是单行输入，文本由调用方 `String` 持有。返回 `TextFieldState { changed, cursor, focused, hovered }`。

```rust
let state = text_field(c, "name", &mut app.name);
if state.changed {
    app.mark_dirty();
}
```

## Editing

字符输入在 cursor 插入；Backspace/Delete 删除前后字符，word/line 移动使用 shaped cluster 映射，支持鼠标与 Shift 选区，输入会替换选区。cut/copy/paste 使用可注入 `ClipboardProvider`；连续键入合并为一个 undo 事务，paste 与删除各自成组。多行编辑使用 `text_area`。

`UiEvent::CompositionStart/Update/Commit/Cancel` 驱动 IME preedit。preedit 以 accent 背景和下划线绘制，commit 替换当前选区，cancel 丢弃。Linux 的 tao 层只暴露已提交 IME 文本；浏览器 demo 从 hidden textarea 提供 start/update/end。autofill、dead key 与密码遮罩仍是独立问题。

native preedit 无法由 tao 0.37 提供：Linux 的 `IMContextSimple` 只暴露提交文本，macOS marked text 不转发，Windows 没有 preedit 事件。应用可直接输入 composition 事件；在补丁 tao 或更换事件循环前，内置 native preedit 是已知限制。

浏览器 demo 在可用时把 copy/cut/paste 映射到 `navigator.clipboard`，并把文本存入宿主注入的 `ClipboardProvider`。API 缺失、权限被拒或非安全上下文会回退到内存 provider，页面不报错。

## The caret

caret 是 shaped prefix 宽度处的 1px 线，会跟随 preedit 与水平滚动。`TextInputState` 暴露已提交文本的 `cursor`、`selection`、`bounds`、`cursor_x`、`cursor_y`；没有 blink timer 或 caret 动画。

## Beside the lines

字段单行，不换行、不显示行号。label、description、error 是独立控件；`form_field` 视觉分组，但不建立语义连接。

## Errors

`text_field` 没有 error/validation 参数。调用方在旁边绘制红/muted 消息，通常用 `form_field`；`AccessNode` 无 error/invalid 状态。

## Without a look

没有底层 editable-text builder。`TextInputState` 提供 cursor/selection/geometry，但自绘需自己实现输入与编辑；标准 `text_field` 是唯一完整可编辑控件。

## Accessibility

字段发出 `Role::TextField`，label 为 `"text field"`，value 为完整文本，带 focused 布尔。不发 hint text、description、error、multiline、readonly 或 password 标志；无 AT 桥。
