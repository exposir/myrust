`slider(c, "volume", &mut value, 0.0, 100.0)` 在范围内设置连续 `f32`。点击/拖动轨道以及聚焦时方向键改变值。

```rust
let state = slider(c, "volume", &mut app.volume, 0.0, 100.0);
if state.changed {
    app.set_volume(state.value);
}
```

## Steps

没有 `step` 参数：指针在轨道上连续映射，键盘使用实现内增量。需要精确整数步进时，调用方 round 或使用 `stepper`/number input。

`SliderState` 暴露 `changed`、`dragging`、`focused`、`hovered`、`value`。没有 tick mark、label、tooltip value 或 disabled slider。

## Without a look

没有底层 slider/custom track API。自定义视觉需 `container`/`draw` 与手动 pointer 处理；不会获得内置 key/drag 状态。

## Accessibility

slider 发出 `Role::Slider`，label 为 `"slider"`，value 为格式化值，带 focused 布尔。不发 min/max、step、orientation 或 disabled；无 AT 桥。
