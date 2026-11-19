`image(c, "id", data, width, height)` 把调用方解码的 RGBA8 `ImageData { pixels, width, height }` 画到目标矩形。display list 只保留一条 image 命令，不再按输出像素展开成大量 `Rect`。

```rust
let pixels = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
image(c, "swatch", ImageData::new(&pixels, 2, 2), 64.0, 64.0);
```

## Bitmaps

`ImageData::new` 接收 row-major RGBA8 slice 与尺寸；`is_valid` 检查字节数。`image` 与 `image_styled` 会把 slice 复制进可共享的 `ImageHandle`；调用方已经持有解码像素时，可用 `ImageHandle::from_pixels` 或 `image_handle` / `image_styled_handle` 避免逐帧复制。handle 的版本由尺寸和像素内容计算，因此相同像素可共享分配、damage 比较与 GPU cache entry。

CPU presenter 使用双线性采样，并处理 source alpha、圆角覆盖、灰度、transform 与矩形 scissor。只有不带解码像素的旧式 handle 会被判为 unresolved。

启用可选 `image-decode` feature 后，`decode_image(bytes)` 解码 PNG、JPEG 与 WebP，返回可共享的 RGBA8 `ImagePixels`。动画图片只取首帧。未启用该 feature 时，`myrust-ui` 不链接图像解码器，调用方仍需自行提供 RGBA8。

## Fitting

`ImageFit` 遵循 MyGo 的放置规则：`Fill` 拉伸铺满，`Contain` 完整放入，`Cover` 填满并裁剪，`ScaleDown` 等同 `Contain` 但不放大，`NaturalSize` 使用源像素尺寸。`ImageStyle` 同时携带圆角与灰度。底层命令保留归一化 `ImageSourceRect`；当前控件 API 使用完整源图。

## GPU textures

原生 GPU presenter 按 source id、内容版本与纹理格式缓存 RGBA8 图像。全不透明的灰度绘制使用单通道 `R8`，其余使用 `RGBA8`。shader 对纹理采样，并应用圆角遮罩、灰度与 source alpha；scissor 与 transform 与其他命令使用同一状态。

图像 cache 有字节上限。新图放不下时，cache 推进 generation 并丢弃旧纹理；presenter 随后针对新纹理整帧重发。重发后仍放不下则该帧显式失败，原生窗口回退到 CPU presenter。Damage 比较使用 image handle 的版本，source id 不变但像素变化时仍会重绘对应图像范围。

## SVGs

没有 SVG document 或 rasterizer。内置 `icon` path 与 `DrawCommand::SvgPath` 覆盖小子集；不支持任意 SVG 文件。

## Pictures of any format

可选解码器覆盖 PNG、JPEG 与 WebP。GIF 动画、EXIF orientation、ICC 色彩管理、HEIC 与视频解码未实现。需要这些语义时由调用方预旋转/预转换像素，或使用 WebView 获得浏览器级解码。

## Accessibility

image node 发出 `Role::Label`，label 为 image id，value 为 `"image"`（avatar 则用 avatar label/value）。没有 Image role、alt-text 参数、loading/error 状态或 AT 桥。
