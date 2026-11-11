`number_input(c, "id", &mut text, options)` 是只接受数字字符并可用 ArrowUp/ArrowDown 步进的文本字段。

```rust
use myrust_core::ui::NumberInputOptions;

let state = number_input(
    c,
    "count",
    &mut app.count_text,
    NumberInputOptions::default().allow_negative(true).decimals(0).step(1.0),
);
if state.changed {
    app.count = state.value;
}
```

## Accessibility

底层字段发出 `Role::TextField`，label 为 `"text field"`，value 为可见数字字符串，带 focused 布尔。没有 `SpinButton`/`NumberInput` role、min/max/step 元数据或 error/invalid 状态。

无 AT 桥消费。自定义 accessible 数字控件需要新 role/API；`number_input` 是带过滤的 text field，不是 spinbutton。
