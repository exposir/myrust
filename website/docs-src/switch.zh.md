`switch(c, "id", &mut on)` 绘制开关，返回 `SwitchState { changed, checked, focused, hovered }`。点击或 Space 切换调用方 boolean。

```rust
let state = switch(c, "wifi", &mut app.wifi);
if state.changed {
    app.apply_network();
}
```

## Without a look

没有底层 track/thumb builder。自定义视觉需 `container`/`draw` 与自己的交互；标准 switch 是唯一发出 Switch AccessNode 的控件。

## Accessibility

switch 发出 `Role::Switch`，label 为 `"switch"`，value 为 `"on"`/`"off"`，带 focused 布尔。除 id 参数外没有可见 label，也没有 description/error/disabled；无 AT 桥。
