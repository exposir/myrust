`rating(c, "stars", &mut value, 5)` 绘制五个星标按钮，把 `u32` 设为 0..max。点击第 `n` 颗星设为 `n`；hover 预览该值。

```rust
let state = rating(c, "quality", &mut app.rating, 5);
if state.changed {
    app.review(state.value);
}
```

## Accessibility

根发出 `Role::Slider`，label 为 `"rating"`，value 为 `"n/max"`，带 focused 布尔。每颗星发出 `Role::Button`，label 为 `"star 1"`…，value 为 `"filled"`/`"empty"`；星不能单独聚焦。

没有 rating/label role、半星值、description、disabled 或 AT 桥。键盘方向键改变根值。
