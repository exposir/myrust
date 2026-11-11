MyRust 按需渲染：构建帧、布局、绘制到 `DisplayList`，再由宿主光栅化/呈现。没有 retained render graph。

```rust
let frame = ui.render(Size::new(960.0, 540.0))?;
let pixels = myrust_core::ui::CpuRasterizer::rasterize(
    &frame.display,
    myrust_core::ui::RenderSize::new(960, 540),
    myrust_core::ui::Theme::dark().background,
)?;
```

## Frame pacing

帧由宿主输入与 `needs_animation_frame` 驱动。消费输入时 `NativeUi::render` 最多运行两次 build pass，使一次点击可在同一个宿主帧内更新状态并绘制。它不启动 timer，也不拥有 vsync 循环。

`FrameStats` 报告上一帧生产的 `layout_ms`、`paint_ms`、`total_ms`；不包含 softbuffer/GPU 呈现或宿主 compositor。`needs_animation_frame` 是 best-effort：native window loop 决定唤醒，隐藏窗口可能停止。

CPU rasterizer 对固定命令列表与字体是确定性的，但没有 retained layer、glyph cache、partial redraw 或 damage tracking。可选 wgpu backend 是受支持几何的薄离屏 renderer，不是公开 render graph。CPU presenter 不支持 image；GPU API/feature 取决于宿主。

没有公开 FPS counter、vsync 控制、triple buffering 策略或 frame-deadline API。使用 `FrameStats` 加宿主测量；动画时间不能假设固定刷新率。
