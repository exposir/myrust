Drawing is a per-frame `DisplayList` of commands. A container can accept raw commands with `.draw(...)`; the CPU rasterizer consumes them for native presentation and the optional wgpu renderer supports a subset.

```rust
use myrust_core::ui::{Color, DrawCommand};

container(c).size(80.0, 80.0).draw([
    DrawCommand::Circle {
        x: 40.0,
        y: 40.0,
        radius: 28.0,
        color: Color::rgb(34, 211, 238),
    },
]);
```

## Fill rules and hits

`Rect`, `RoundedRect`, `Circle`, `Line`, `LinearGradient`, `SvgPath`, `Text` and `Image` are the drawable command kinds. SVG paths are filled with the even-odd rule and stroked as scaled line segments by the CPU presenter.

Hit-testing is separate: it uses rectangular `InteractiveRegion` bounds from the previous frame, not path geometry. A custom circular visual still needs a rectangular interaction region; there is no shape/path hit test or pointer-event command.

## Paths from SVG data

`DrawCommand::SvgPath` carries path data, optional fill/stroke and a stroke width. `parse_svg_path` flattens the supported subset: M/m, L/l, H/h, V/v, Q/q, C/c, A/a and Z/z. Built-in icons use the same path data through `icon`.

There is no general SVG document parser, gradient-on-path, dash pattern, even-odd/nonzero switch or text-on-path. Curves are flattened into line segments at rasterization.

## Clipping to a path

No path clipping exists. `ScissorPush { rect }` clips to an axis-aligned rectangle, and `scroll_view` uses a rectangular viewport. A custom container can emit scissor commands, but arbitrary path masks are not supported.

## Parts of bitmaps

`image(c, id, data)` accepts caller-owned RGBA8 `ImageData` and emits one shared `DrawCommand::Image`. The command carries a normalized `ImageSourceRect`, Fit placement, corner radius and grayscale; `ImageState.width`/`height` describe the box. The CPU and optional GPU presenters both sample the decoded pixels, so no per-output-pixel rectangle stream is generated.

There is no atlas index, nine-slice or lazy provider API. With the optional `image-decode` feature, PNG/JPEG/WebP bytes can be decoded into shared `ImagePixels`; otherwise callers decode them.

## Canvases

There is no retained canvas, drawing context or immediate 2D API. Draw commands are rebuilt each frame and attached to a container; the rasterizer is stateless per `CpuRasterizer::rasterize` call.

Custom animation must therefore rebuild commands each frame. There is no canvas save/restore, layer list, offscreen render target or GPU canvas exposed through `myrust-ui`.

## Transforms

`Context::transform(transform, content)` scopes translation/scale/rotation for constructed nodes; `DisplayList` also supports `TransformPush` and `TransformPop`. `Transform` has translation, scale and rotation fields only.

Pointer hit testing inverse-transforms the pointer for recorded `Context::transform` scopes. There is no skew, perspective, transform-origin or arbitrary 4x4 matrix. A transform scope applies per node, not as one group around all children.

## Shaping text

Text is shaped with OpenType kerning and ligatures, reordered by the Unicode bidi algorithm, wrapped by UAX #14 and mapped through an ordered sans/mono/fallback font stack. The resulting `TextLayout` drives CPU rasterization, the GPU glyph atlas and damage bounds. `FontRegistry` still loads individual TTF/OpenType faces and `CpuRasterizer::rasterize_with_font` remains available for explicit-face tests; missing glyphs are counted.

A text command still carries one font size/weight; callers cannot choose per-run OpenType features, variable-font axes or named families.

## Animation

`Animation` and `Easing` drive caller-owned scalar values through `Context::animate` / `animate_with`. The frame clock (`Context::frame_time`) supplies timing; `FrameScheduler` turns active animation deadlines into `next_wake`, and `needs_animation_frame` still reports whether work remains. `spinner` and `transition` are built on this path.

`Spring`, `Keyframes` and `Timeline` add deterministic physics, multi-stop and multi-track motion; each is stepped with an explicit `dt`, so drawing code should sample it and rebuild the same display list as before. `ColorTransition`, `OpacityTransition`, `SizeTransition` and `PositionTransition` extend transitions beyond bounds. Reduce-motion (`MYRUST_REDUCE_MOTION`) jumps built-in animations to their target; no OS motion preference callback exists.

See [Motion](motion) for `Context::animate_spring`, spring stiffness/damping, timeline stepping and how active motion feeds `next_animation_frame()`.
