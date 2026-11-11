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

`image(c, id, data)` accepts caller-owned RGBA8 `ImageData` through a `ImageState` handle. It can draw a whole image; there is no source rectangle, atlas/atlas index, nine-slice or lazy provider.

The CPU presenter currently counts `DrawCommand::Image` as unsupported because the display list carries only a source handle; native UI needs a caller/platform image bridge. The optional GPU renderer does not add image decoding.

## Canvases

There is no retained canvas, drawing context or immediate 2D API. Draw commands are rebuilt each frame and attached to a container; the rasterizer is stateless per `CpuRasterizer::rasterize` call.

Custom animation must therefore rebuild commands each frame. There is no canvas save/restore, layer list, offscreen render target or GPU canvas exposed through `myrust-ui`.

## Transforms

`Context::transform(transform, content)` scopes translation/scale/rotation for constructed nodes; `DisplayList` also supports `TransformPush` and `TransformPop`. `Transform` has translation, scale and rotation fields only.

Pointer hit testing inverse-transforms the pointer for recorded `Context::transform` scopes. There is no skew, perspective, transform-origin or arbitrary 4x4 matrix. A transform scope applies per node, not as one group around all children.

## Shaping text

Text is measured and rasterized without shaping, kerning, ligatures, bidi reordering or font fallback. `FontRegistry` loads one TTF/OpenType face; `CpuRasterizer::rasterize_with_font` can receive it. Missing glyphs are counted.

A text command carries one font size/weight; no rich shaping run, OpenType features, variable-font axis or per-glyph positioning is exposed.

## Animation

`Animation` and `Easing` drive caller-owned scalar values through `Context::animate` / `animate_with`. The frame clock (`Context::frame_time`) supplies timing; `needs_animation_frame` asks the host for another frame. `spinner` and `transition` are built on this path.

There are no keyframes, spring physics, timelines, declarative transitions or animation events. Reduce-motion (`MYRUST_REDUCE_MOTION`) jumps animations to their target; no OS motion preference callback exists.
