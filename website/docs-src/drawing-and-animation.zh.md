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

`image(c, id, data)` 接受调用方 RGBA8 `ImageData`，并发出单条共享 `DrawCommand::Image`。命令携带归一化 `ImageSourceRect`、Fit 放置、圆角与灰度；`ImageState.width`/`height` 描述目标盒。CPU 与可选 GPU presenter 都直接采样解码像素，不再生成逐输出像素 Rect 流。

没有 atlas index、nine-slice 或 lazy provider API。启用可选 `image-decode` feature 时，PNG/JPEG/WebP 字节可解码为共享 `ImagePixels`；否则由调用方解码。

## Canvases

没有 retained canvas、drawing context 或 immediate 2D API。绘制命令每帧重建并挂到 container；rasterizer 对每次 `CpuRasterizer::rasterize` 调用无状态。

自定义动画必须每帧重建命令。没有 canvas save/restore、layer list、offscreen render target 或通过 `myrust-ui` 暴露的 GPU canvas。

## Transforms

`Context::transform(transform, content)` 为构造节点限定 translation/scale/rotation；`DisplayList` 也支持 `TransformPush`/`TransformPop`。`Transform` 只有 translation、scale、rotation 字段。

指针命中会对记录的 `Context::transform` 作用域做逆变换。没有 skew、perspective、transform-origin 或任意 4x4 矩阵。transform 作用域逐节点应用，不是围绕所有 children 的整组 transform。

## Shaping text

文字由 OpenType kerning/ligature shaping、Unicode bidi 重排、UAX #14 换行以及有序 sans/mono/回退字体栈完成布局。生成的 `TextLayout` 驱动 CPU 光栅、GPU glyph atlas 与 damage bounds。`FontRegistry` 仍负责加载单个 TTF/OpenType，`CpuRasterizer::rasterize_with_font` 仍供显式字体测试使用；缺字会计数。

text command 仍只有一个字号/字重；调用方不能选择逐 run 的 OpenType feature、variable-font axis 或命名字体。

## Animation

`Animation` 与 `Easing` 通过 `Context::animate` / `animate_with` 驱动调用方标量。帧时钟（`Context::frame_time`）提供时间；`FrameScheduler` 把活动动画的 deadline 转成 `next_wake`，`needs_animation_frame` 仍报告是否还有工作。`spinner` 与 `transition` 基于此路径。

`Spring`、`Keyframes`、`Timeline` 增加确定性物理、多停靠点与多轨运动；它们都用显式 `dt` 步进，绘制代码只需采样并像以前一样重建 display list。`ColorTransition`、`OpacityTransition`、`SizeTransition`、`PositionTransition` 把 transition 扩展到 bounds 之外。Reduce-motion（`MYRUST_REDUCE_MOTION`）把内置动画跳到目标；没有 OS motion preference 回调。

`Context::animate_spring`、stiffness/damping、timeline 推进以及活跃动效如何驱动 `next_animation_frame()` 见 [动效](motion)。
