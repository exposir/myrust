MyRust renders on demand: a frame is built, laid out, painted into a `DisplayList`, then rasterized/presented by the host. There is no retained render graph.

```rust
let frame = ui.render(Size::new(960.0, 540.0))?;
let pixels = myrust_core::ui::CpuRasterizer::rasterize(
    &frame.display,
    myrust_core::ui::RenderSize::new(960, 540),
    myrust_core::ui::Theme::dark().background,
)?;
```

## Frame pacing

Frames are driven by host input and `needs_animation_frame`. `NativeUi::render` may run up to two build passes when input is consumed, so a click can update state and be painted in one host frame. It does not start a timer or own a vsync loop.

`FrameStats` reports `layout_ms`, `paint_ms` and `total_ms` for the last production; it does not include softbuffer/GPU presentation or the host compositor. `needs_animation_frame` is best-effort: the native window loop decides when to wake, and a hidden window may stop.

The CPU rasterizer is deterministic for a command list and font but has no retained layers, glyph cache, partial redraw or damage tracking. The optional wgpu backend is a thin offscreen renderer for supported geometry, not a public render graph. Images are unsupported by the CPU presenter; GPU API/feature availability is host-dependent.

There is no public FPS counter, vsync control, triple buffering policy or frame-deadline API. Use `FrameStats` plus host measurements; animation timing must not assume a fixed refresh rate.
