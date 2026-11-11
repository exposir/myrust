`stepper(c, "count", &mut value, 0, 10, 1)` 绘制减/加按钮与居中值。整数 clamp 到范围；step 至少为 1。

```rust
let state = stepper(c, "count", &mut app.count, 0, 10, 1);
if state.changed {
    app.save_count();
}
```

## Accessibility

根发出 `Role::Slider`，label 为 `"stepper"`，value 为当前整数，带 focused 布尔。减/加按钮发出 `Role::Button`，label 为 `"-"`/`"+"`，value 为 `None`。

没有 spinbutton role、min/max/step 元数据、可见 label、disabled 或 AT 桥。根聚焦时 ArrowUp/ArrowDown 步进。
