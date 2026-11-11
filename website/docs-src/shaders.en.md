Shaders are not implemented. MyRust does not expose a shading language, shader compiler, GPU pipeline or custom per-pixel effect. Drawing stays in `DisplayList`; the CPU rasterizer paints geometry and the optional wgpu backend consumes a fixed command subset.

The closest alternatives are `DrawCommand` gradients, rounded rects, circles, lines, SVG paths and text, plus caller-composed `Color` blends. A per-pixel effect needs a caller-side image buffer and a renderer outside `myrust-ui`.

## Text, paths and icons

There is no shader path for text, SVG paths or icon glyphs. `text`, `DrawCommand::SvgPath` and `icon` go through CPU glyph/path rasterization; colors are baked into commands.

## Effects on elements

No element effect shader exists: no blur, glow, drop shadow, refraction, distortion, noise or custom mask. Alternatives are layered `DrawCommand`s, gradients, a pre-rendered image, or a platform compositor effect outside the crate.

## The backdrop

There is no backdrop filter or live backdrop texture. Windows can request platform vibrancy via `WindowConfig::vibrancy`, but native UI content cannot sample or shade the desktop behind it.

## The language

MyRust has no shader language, GLSL/WGSL authoring, function library, include resolver or hot reload. The optional wgpu renderer is an internal implementation detail, not a public shader surface.

## Uniforms

There are no uniforms, uniform buffers, bindings, textures or per-draw parameters beyond the fixed fields on `DrawCommand`. A caller animating a value must rebuild colors/geometry per frame.

## Images

`ImageData`/`image` carries decoded RGBA8 into a draw command, but the CPU presenter currently reports images unsupported; there is no image shader or sampler. GPU image sampling, texture atlases and render targets are absent.

## Errors and cost

There is no shader compiler, reflection, pipeline cache, compile error type or GPU timing API. Cost is CPU rasterization plus whatever optional wgpu backend does internally; failures surface as `RasterError`/`RenderError`, not shader diagnostics.

A real shader API would need a public pipeline/effect abstraction, GPU resource ownership, shader error reporting, cross-backend bindings and wasm/WebGL parity. None of that is present.
