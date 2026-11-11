Video is not implemented. MyRust has no media backend, decoder, audio output or video surface; NativeUi cannot play a video file or stream.

## Players

No video player widget, controls, playlist, subtitles or fullscreen player exists. A custom clock and frame swapping would be application code.

## Playing

No play/pause/seek/rate/volume API, buffering state, A/V sync or hardware surface. System media playback is outside `myrust-ui`.

## Decoders

No H.264/HEVC/VP9/AV1/GIF decoder is linked, and no audio callback/device is managed. Callers can decode externally and pass RGBA8 frames to `image` (CPU presenter still doesn't render Image commands).

## System video

For real playback, use a platform WebView with HTML `<video>` in a separate WebView window. NativeUi and a live WebView cannot be composited in the same window.

## Accessibility

There is no Video/Media role, playback value, captions track or AT bridge. If using WebView `<video>`, accessibility comes from the platform browser, not `myrust-ui`.

No demo is registered for video; `/docs-demo` has no media surface.
