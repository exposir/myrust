`image(c, "id", data, width, height)` draws caller-decoded RGBA8 `ImageData { pixels, width, height }` into a target rectangle. No PNG/JPEG/WebP decoding happens in `myrust-ui`.

```rust
let pixels = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
image(c, "swatch", ImageData::new(&pixels, 2, 2), 64.0, 64.0);
```

## Bitmaps

`ImageData::new` takes a row-major RGBA8 slice and dimensions. `is_valid` checks the byte count. The caller owns decoding, color conversion and lifetime.

The CPU presenter currently reports `DrawCommand::Image` unsupported; the display list carries no decoded pixels to the presenter. GPU/host image bridges are not implemented.

## Fitting

The target width/height stretch the source; there is no aspect-ratio preservation, object-fit/crop, source rectangle, tiling or background fill. Pre-scale pixels/targets in the caller.

## SVGs

There is no SVG document or rasterizer. Built-in `icon` paths and `DrawCommand::SvgPath` cover a small path subset; arbitrary SVG files are not supported.

## Pictures of any format

Only raw RGBA8 is accepted. JPEG/PNG/WebP/HEIC/GIF decoding, EXIF orientation, animation and color profiles are caller/platform responsibilities. For browser-grade decoding, use a WebView.

## Accessibility

The image node emits `Role::Label`, label = the image id, value `"image"` (or the avatar label/value for avatars). There is no Image role, alt-text argument, loading/error state or AT bridge.
