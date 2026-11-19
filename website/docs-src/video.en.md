MyRust provides host-driven frame playback. It does not decode files, play audio, synchronize A/V clocks or manage a media surface.

## Player

`VideoPlayer` stores timestamped RGBA frames as shared `ImageHandle` values. A host decodes each frame and pushes it with a presentation timestamp, then calls `advance(dt)` from its own clock. The player is deterministic: `current_frame()` chooses the last frame whose `pts` is at or before the current position.

`play`, `pause`, `seek`, looping and end state are available without a system clock. `set_duration` is optional; without it the last frame timestamp is used. When looping wraps, frame selection still follows timestamps.

## Widget

`video(context, id, &player, width, height, VideoStyle)` participates in layout and emits the existing `DrawCommand::Image`. Fit, corner radius and grayscale reuse the image pipeline. Frame changes alter the image content version, so CPU/GPU damage repaints the video box.

An empty player draws a one-pixel background placeholder. Missing or malformed frames never panic the widget.

## Host responsibilities

The host owns demuxing, decoding, frame pacing, audio, subtitles and platform hardware surfaces. MyRust links no ffmpeg, gstreamer or codec dependency. Native and wasm builds use the same primitive.

## Accessibility

There is no Video/Media role, playback value, caption track or AT bridge. Applications that need platform media accessibility should use a WebView `<video>` surface.
