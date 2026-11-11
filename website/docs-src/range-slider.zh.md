`range_slider(c, "id", &mut state, 0.0, 100.0)` 绘制 low/high 双 knob。`RangeSliderState { low, high, active }` 由调用方持有，bounds 会 clamp/order。

```rust
use myrust_core::ui::RangeSliderState;

let state = range_slider(c, "price", &mut app.range, 0.0, 100.0);
if state.changed {
    app.filter(state.low, state.high);
}
```

## Accessibility

控件发出一个 `Role::Slider`，label 为 `"range slider"`，value 为 `"low..high"`（格式化），带 focused 布尔。low/high knob 没有独立 node、value 或 label。

没有 min/max/step/orientation 元数据、disabled 或 AT 桥。键盘移动 active knob；内部 `RangeKnob` 不作为 accessible child 暴露。
