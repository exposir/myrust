MyRust 提供由宿主驱动的帧播放，不解码文件、不播放音频、不管理 A/V 时钟或媒体 surface。

## Player

`VideoPlayer` 保存带时间戳的 RGBA 帧，像素复用 `ImageHandle`。宿主解码每一帧并按 pts 推入，然后用自身时钟调用 `advance(dt)`。`current_frame()` 是确定性的：选择 pts 小于等于当前位置的最后一帧。

`play`、`pause`、`seek`、循环与结束状态都不依赖系统时钟。`set_duration` 可选；未设置时使用最后一帧的 pts。循环回绕后仍按时间戳选帧。

## Widget

`video(context, id, &player, width, height, VideoStyle)` 参与布局，并发出已有的 `DrawCommand::Image`。Fit、圆角与灰度复用 image 管线。帧切换会改变 image 内容版本，因此 CPU/GPU damage 都会重绘视频框。

空播放器绘制一像素背景占位；缺失或畸形帧不会让 widget panic。

## 宿主职责

demux、解码、帧节奏、音频、字幕以及平台硬件 surface 都由宿主负责。MyRust 不链接 ffmpeg、gstreamer 或编解码器依赖。native 与 wasm 构建使用同一套原语。

## Accessibility

没有 Video/Media role、playback value、caption track 或 AT 桥。需要平台媒体无障碍能力时，应使用 WebView 的 `<video>` surface。
