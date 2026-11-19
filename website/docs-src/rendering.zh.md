MyRust 按需渲染：构建帧、布局、绘制到 `DisplayList`，再由宿主呈现。没有 retained render graph。

## Native presentation

原生窗口默认通过 wgpu 表面呈现：Linux 走 Vulkan，macOS 走 Metal，Windows 走 DX12。形状、渐变、文本、SVG 路径、阴影与解码图像由 GPU 绘制；字形按设备像素光栅化并缓存进 mask atlas，图像则进入按内容版本索引的纹理 cache。没有可用适配器、表面创建失败或运行中丢失表面时，窗口自动回退到 CPU 光栅 + softbuffer；设置 `NO_GPU=1` 可强制 CPU 路径。`myrust-core` 的 `gpu` 是默认特性；关闭它或 `myrust-ui` 的 `gpu` 特性时只编译 CPU 路径。

文本只整形一次成共享的 `TextLayout`：OpenType 字距/连字、bidi run 与逐 glyph 字体回退生成带位置的 glyph id，CPU rasterizer 与 GPU mask atlas 都画这一份结果。换行和对齐也在同一结果里，damage 差分使用它的视觉内容宽度和行数，不再维护另一套字符近似。布局缓存有上限；更换已安装字体栈会改变缓存 key。

CPU 回退路径仍可单独使用：

```rust
let frame = ui.render(Size::new(960.0, 540.0))?;
let pixels = myrust_core::ui::CpuRasterizer::rasterize(
    &frame.display,
    myrust_core::ui::RenderSize::new(960, 540),
    myrust_core::ui::Theme::dark().background,
)?;
```

## Damage

原生 GPU 呈现逐帧比较相邻 `DisplayList`（命令、transform 与 scissor 都相同才算未变）：只重绘与上一帧不同的区域，合并成最多 8 个互不相交的矩形；变化超过半帧或窗口尺寸变化时整帧重绘。display list 未变化时跳过绘制，但仍重新上屏，因为平台窗口可能覆盖上一帧。mask atlas、渐变 LUT 或按字节限额回收的图像 cache 在帧内重置时整帧重发一次；重发后仍不够用时该帧显式报错，窗口回退 CPU 呈现。图像 cache key 包含 source id 与内容版本，source id 不变而像素变化时仍会正确 damage。`MYRUST_GPU_DEBUG=1` 打印所选适配器与每帧 damage。

## Frame pacing

帧由宿主输入与 `needs_animation_frame` 驱动。消费输入时 `NativeUi::render` 最多运行两次 build pass，使一次点击可在同一个宿主帧内更新状态并绘制。它不启动 timer，也不拥有 vsync 循环。

`FrameStats` 报告上一帧生产的 `layout_ms`、`paint_ms`、`total_ms`；不包含 softbuffer/GPU 呈现或宿主 compositor。`needs_animation_frame` 是 best-effort：native window loop 决定唤醒，隐藏窗口可能停止。

CPU rasterizer 对固定命令列表与字体是确定性的，但没有 glyph cache、partial redraw 或 damage tracking；damage 由原生 GPU presenter 驱动。它画的是与 GPU 路径相同的 shaped glyph run；GPU atlas 以 face id、glyph id、设备字号与合成加粗档位作为 key。解码后的图像在 CPU 上用双线性采样与 alpha blending 绘制，在 GPU 上走内容版本化的纹理 cache；PNG/JPEG/WebP 解码通过 `image-decode` 按需启用。可选 wgpu 离屏 renderer 用于测试与工具，不是公开 render graph；GPU API/feature 取决于宿主。

没有公开 FPS counter、vsync 控制、triple buffering 策略或 frame-deadline API。使用 `FrameStats` 加宿主测量；动画时间不能假设固定刷新率。
