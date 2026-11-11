`decimal_input(c, "id", &mut text, options)` 用 scaled `i64` 保存精确 fixed-point 值；可见字符串经过过滤，绝不按 float 四舍五入。

```rust
use myrust_core::ui::DecimalInputOptions;

let state = decimal_input(
    c,
    "amount",
    &mut app.amount_text,
    DecimalInputOptions { allow_negative: true, decimals: 2 },
);
if let Some(units) = state.units {
    app.amount_units = units;
}
```

## Nothing is rounded

多余小数位由输入过滤器丢弃，不是四舍五入。`DecimalInputState::units` 是精确 scaled integer（如两位小数时 `"12.34"` -> `1234`）；没有 float `value`。

没有 currency symbol/locale 格式化、分组、min/max、decimal step，或字符过滤之外的粘贴归一化。

## Accessibility

字段发出 `Role::TextField`，label 为 `"text field"`，value 为可见 decimal 字符串，带 focused 布尔。不发 `SpinButton`、数值范围、currency 或 invalid/error 元数据；无 AT 桥。
