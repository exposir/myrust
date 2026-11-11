绘制是逐帧 `DisplayList` 命令。container 可通过 `.draw(...)` 接收原始命令；CPU rasterizer 用于 native 呈现，可选 wgpu renderer 支持其中一部分。

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

`Rect`、`RoundedRect`、`Circle`、`Line`、`LinearGradient`、`SvgPath`、`Text` 与 `Image` 是可绘制命令。SVG path 由 CPU presenter 用 even-odd 填充，并按缩放后的线段描边。

命中测试独立：使用上一帧矩形 `InteractiveRegion` bounds，不是 path 几何。自定义圆形视觉仍需矩形交互区域；没有 shape/path hit test 或 pointer-event command。

## Paths from SVG data

`DrawCommand::SvgPath` 携带 path 数据、可选 fill/stroke 与 stroke width。`parse_svg_path` 展平支持子集：M/m、L/l、H/h、V/v、Q/q、C/c、A/a、Z/z。内置 icon 通过 `icon` 使用同一路径数据。

没有通用 SVG document parser、path gradient、dash pattern、even-odd/nonzero 切换或 text-on-path。曲线在光栅化时展平为线段。

## Clipping to a path

没有 path clip。`ScissorPush { rect }` 裁剪到轴对齐矩形，`scroll_view` 使用矩形 viewport。自定义 container 可发出 scissor 命令，但不支持任意 path mask。

## Parts of bitmaps

`image(c, id, data)` 通过 `ImageState` 句柄接受调用方 RGBA8 `ImageData`。它绘制整图；没有 source rectangle、atlas/atlas index、nine-slice 或 lazy provider。

CPU presenter 目前把 `DrawCommand::Image` 计为 unsupported，因为 display list 只有 source handle；native UI 需要调用方/平台图像桥。可选 GPU renderer 也不增加图像解码。

## Canvases

没有 retained canvas、drawing context 或 immediate 2D API。绘制命令每帧重建并挂到 container；rasterizer 对每次 `CpuRasterizer::rasterize` 调用无状态。

自定义动画必须每帧重建命令。没有 canvas save/restore、layer list、offscreen render target 或通过 `myrust-ui` 暴露的 GPU canvas。

## Transforms

`Context::transform(transform, content)` 为构造节点限定 translation/scale/rotation；`DisplayList` 也支持 `TransformPush`/`TransformPop`。`Transform` 只有 translation、scale、rotation 字段。

指针命中会对记录的 `Context::transform` 作用域做逆变换。没有 skew、perspective、transform-origin 或任意 4x4 矩阵。transform 作用域逐节点应用，不是围绕所有 children 的整组 transform。

## Shaping text

文字测量与光栅化不做 shaping、kerning、ligature、bidi 重排或字体回退。`FontRegistry` 加载单个 TTF/OpenType；`CpuRasterizer::rasterize_with_font` 可接收。缺字会计数。

text command 只有一个字号/字重；没有 rich shaping run、OpenType feature、variable-font axis 或 per-glyph positioning。

## Animation

`Animation` 与 `Easing` 通过 `Context::animate` / `animate_with` 驱动调用方标量。帧时钟（`Context::frame_time`）提供时间；`needs_animation_frame` 请求宿主再画一帧。`spinner` 与 `transition` 基于此路径。

没有 keyframes、spring physics、timeline、declarative transition 或 animation event。Reduce-motion（`MYRUST_REDUCE_MOTION`）直接把动画跳到目标；没有 OS motion preference 回调。
