Shaders 未实现。MyRust 不暴露 shading language、shader compiler、GPU pipeline 或自定义逐像素效果。绘制仍是 `DisplayList`；CPU rasterizer 绘制几何，可选 wgpu backend 消费固定命令子集。

最接近的替代是 `DrawCommand` 的 gradient、rounded rect、circle、line、SVG path、text，以及调用方组合的 `Color` 混合。逐像素效果需要调用方图像缓冲和 `myrust-ui` 之外的 renderer。

## Text, paths and icons

文字、SVG path 与 icon glyph 没有 shader 路径。`text`、`DrawCommand::SvgPath`、`icon` 走 CPU glyph/path 光栅化；颜色已写入命令。

## Effects on elements

没有元素效果 shader：blur、glow、drop shadow、refraction、distortion、noise 或自定义 mask 都不存在。替代是分层 `DrawCommand`、gradient、预渲染 image，或 crate 之外的平台 compositor effect。

## The backdrop

没有 backdrop filter 或实时 backdrop texture。窗口可通过 `WindowConfig::vibrancy` 请求平台 vibrancy，但 native UI 内容不能采样或着色桌面背景。

## The language

MyRust 没有 shader language、GLSL/WGSL 编写、函数库、include resolver 或 hot reload。可选 wgpu renderer 是内部实现细节，不是公开 shader surface。

## Uniforms

没有 uniform、uniform buffer、binding、texture 或 `DrawCommand` 固定字段之外的逐 draw 参数。动画标量的调用方必须每帧重建颜色/几何。

## Images

`ImageData`/`image` 把解码 RGBA8 放进 draw command，但 CPU presenter 目前报告 image unsupported；没有 image shader 或 sampler。GPU image sampling、texture atlas 与 render target 缺失。

## Errors and cost

没有 shader compiler、reflection、pipeline cache、compile error 类型或 GPU timing API。成本是 CPU 光栅化加可选 wgpu backend 内部工作；失败表现为 `RasterError`/`RenderError`，不是 shader 诊断。

真正 shader API 需要公开 pipeline/effect 抽象、GPU 资源所有权、shader 错误报告、跨后端 binding 与 wasm/WebGL 对齐。这些都未提供。
