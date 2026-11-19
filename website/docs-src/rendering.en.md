MyRust renders on demand: a frame is built, laid out, painted into a `DisplayList`, then presented by the host. There is no retained render graph.

## Native presentation

Native windows present through a wgpu surface: Vulkan on Linux, Metal on macOS, DX12 on Windows. Shapes, gradients, text, SVG paths, shadows and decoded images are drawn on the GPU; glyphs are rasterized at device pixels and cached in a mask atlas, while images live in a content-versioned texture cache. When no compatible adapter exists, surface creation fails, or the surface is lost at runtime, the window falls back to CPU rasterization plus softbuffer; set `NO_GPU=1` to force the CPU path. The `gpu` feature is on by default in `myrust-core`; disabling it, or `myrust-ui`'s `gpu` feature, builds only the CPU path.

Text is shaped once into a shared `TextLayout`: OpenType kerning/ligatures, bidi runs and per-glyph fallback produce positioned glyph ids that both the CPU rasterizer and GPU mask atlas draw. Wrapping and alignment are part of the same result, and the damage differ uses its visual content width and line count rather than a parallel character approximation. The layout cache is bounded; changing the installed font stack changes the cache key.

The CPU fallback remains usable on its own:

```rust
let frame = ui.render(Size::new(960.0, 540.0))?;
let pixels = myrust_core::ui::CpuRasterizer::rasterize(
    &frame.display,
    myrust_core::ui::RenderSize::new(960, 540),
    myrust_core::ui::Theme::dark().background,
)?;
```

## Damage

Native GPU presentation compares adjacent `DisplayList`s (commands, transforms and scissors must all match): only the regions that differ from the previous frame are redrawn, merged into at most eight disjoint rectangles. Changes covering more than half the window, or a resize, redraw the whole frame. An unchanged display list skips drawing but is still presented again, because the platform window may have painted over the last frame. Recycling the mask atlas, gradient LUT or byte-bounded image cache within a frame re-emits the whole frame once; a frame that still exceeds the caches fails explicitly and the window falls back to CPU presentation. Image cache keys include the source id and content version, so changed pixels are damaged even if the id is stable. `MYRUST_GPU_DEBUG=1` prints the selected adapter and each frame's damage.

## Frame pacing

Frames are driven by host input and `needs_animation_frame`. `NativeUi::render` may run up to two build passes when input is consumed, so a click can update state and be painted in one host frame. It does not start a timer or own a vsync loop.

`FrameStats` reports `layout_ms`, `paint_ms` and `total_ms` for the last production; it does not include softbuffer/GPU presentation or the host compositor. `needs_animation_frame` is best-effort: the native window loop decides when to wake, and a hidden window may stop.

The CPU rasterizer is deterministic for a command list and font but has no glyph cache, partial redraw or damage tracking; damage lives in the native GPU presenter. It draws the same shaped glyph runs as the GPU path, while the GPU atlas keys each glyph by face id, glyph id, device size and synthetic weight. Decoded images use bilinear sampling and alpha blending on CPU, and a content-versioned texture cache on GPU; PNG/JPEG/WebP decoding is opt-in through `image-decode`. The optional wgpu offscreen renderer is for tests and tools, not a public render graph. GPU API/feature availability is host-dependent.

There is no public FPS counter, vsync control, triple buffering policy or frame-deadline API. Use `FrameStats` plus host measurements; animation timing must not assume a fixed refresh rate.
