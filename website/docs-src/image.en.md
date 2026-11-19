`image(c, "id", data, width, height)` draws caller-decoded RGBA8 `ImageData { pixels, width, height }` into a target rectangle. The display list carries one image command at any output size; it no longer expands the picture into one `Rect` per output pixel.

```rust
let pixels = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
image(c, "swatch", ImageData::new(&pixels, 2, 2), 64.0, 64.0);
```

## Bitmaps

`ImageData::new` takes a row-major RGBA8 slice and dimensions. `is_valid` checks the byte count. `image` and `image_styled` copy that slice into a shareable `ImageHandle`; use `ImageHandle::from_pixels` or `image_handle` / `image_styled_handle` when the caller already holds decoded pixels and wants to avoid a per-frame copy. A handle's version is derived from dimensions and bytes, so unchanged pixels share allocations, damage comparison, and GPU cache entries.

The CPU presenter samples the texture bilinearly, applies source alpha, corner coverage, grayscale, transforms and rectangular scissors. It rejects only an unresolved legacy handle that carries no decoded pixels.

With the optional `image-decode` feature, `decode_image(bytes)` accepts PNG, JPEG and WebP and returns shareable RGBA8 `ImagePixels`. Animated images use their first frame. Without that feature, `myrust-ui` links no image decoder and callers provide RGBA8 themselves.

## Fitting

`ImageFit` follows MyGo's placement rules: `Fill` stretches to the box, `Contain` fits the whole picture inside it, `Cover` fills and crops, `ScaleDown` behaves like `Contain` but never enlarges, and `NaturalSize` uses the source pixel size. `ImageStyle` also carries a corner radius and grayscale. The command keeps a normalized `ImageSourceRect` for low-level callers; the widget API currently uses the whole source.

## GPU textures

The native GPU presenter uploads RGBA8 image content into a texture cache keyed by source id, version and texture format. Opaque grayscale draws use one-channel `R8`; other images use `RGBA8`. The shader samples the texture, applies the rounded-rectangle mask, grayscale and source alpha, and respects the same scissor and transform state as other commands.

The image cache is byte-bounded. If a new image would overflow it, the cache advances a generation and drops its textures; the presenter re-emits the whole frame against fresh textures. A frame that still cannot fit the cache fails explicitly, and the native window falls back to the CPU presenter. Damage comparison uses the image handle version, so changed pixels damage the image bounds even when the source id stays the same.

## SVGs

There is no SVG document or rasterizer. Built-in `icon` paths and `DrawCommand::SvgPath` cover a small path subset; arbitrary SVG files are not supported.

## Pictures of any format

The optional decoder covers PNG, JPEG and WebP. GIF animation, EXIF orientation, ICC color management, HEIC and video decoding are not implemented. Pre-orient and pre-color-manage pixels in the caller when those matter, or use a WebView for browser-grade decoding.

## Accessibility

The image node emits `Role::Label`, label = the image id, value `"image"` (or the avatar label/value for avatars). There is no Image role, alt-text argument, loading/error state or AT bridge.
