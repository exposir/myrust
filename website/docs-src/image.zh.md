`image(c, "id", data, width, height)` 把调用方解码的 RGBA8 `ImageData { pixels, width, height }` 画到目标矩形。`myrust-ui` 不做 PNG/JPEG/WebP 解码。

```rust
let pixels = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
image(c, "swatch", ImageData::new(&pixels, 2, 2), 64.0, 64.0);
```

## Bitmaps

`ImageData::new` 接收 row-major RGBA8 slice 与尺寸；`is_valid` 检查字节数。解码、颜色转换与生命周期归调用方。

CPU presenter 目前报告 `DrawCommand::Image` unsupported；display list 没有把解码像素交给 presenter。GPU/host image bridge 未实现。

## Fitting

目标 width/height 拉伸 source；没有 aspect-ratio 保持、object-fit/crop、source rectangle、tiling 或 background fill。调用方需预缩放像素/target。

## SVGs

没有 SVG document 或 rasterizer。内置 `icon` path 与 `DrawCommand::SvgPath` 覆盖小子集；不支持任意 SVG 文件。

## Pictures of any format

只接受 raw RGBA8。JPEG/PNG/WebP/HEIC/GIF 解码、EXIF orientation、动画与 color profile 是调用方/平台职责。浏览器级解码用 WebView。

## Accessibility

image node 发出 `Role::Label`，label 为 image id，value 为 `"image"`（avatar 则用 avatar label/value）。没有 Image role、alt-text 参数、loading/error 状态或 AT 桥。
