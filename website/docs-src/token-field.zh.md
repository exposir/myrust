`token_field(c, "id", &mut tokens, &mut input)` 以 chip 显示字符串并带内联输入。Enter 添加 buffer；逗号拆分；空输入 Backspace 删除最后一个；点击 chip 删除。

```rust
let state = token_field(c, "tags", &mut app.tags, &mut app.tag_input);
if state.added {
    app.sync_tags();
}
```

## Accessibility

根发出 `Role::Group`，label 为控件 id。每个 chip 是 `Role::Button`，label 为 `"remove token <text>"`，无 value。input 发出 `Role::TextField`，label 为 `"text field"`，value 为 buffer，带 focused 布尔。

没有 token/list role、独立 chip 文本 label、set position/size、disabled chip 或 AT 桥。键盘删除仅限空输入 Backspace。
