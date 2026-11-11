Video 未实现。MyRust 没有 media backend、decoder、audio output 或 video surface；NativeUi 不能播放视频文件/流。

## Players

没有 video player widget、controls、playlist、subtitle 或 fullscreen player。自定义时钟与帧交换属于应用代码。

## Playing

没有 play/pause/seek/rate/volume API、buffering 状态、A/V sync 或 hardware surface。系统媒体播放不在 `myrust-ui`。

## Decoders

未链接 H.264/HEVC/VP9/AV1/GIF decoder，也不管理 audio callback/device。调用方可外部解码并把 RGBA8 帧交给 `image`（CPU presenter 仍不渲染 Image 命令）。

## System video

真实播放用独立 WebView 窗口中的平台 HTML `<video>`。NativeUi 与 live WebView 不能在同一窗口合成。

## Accessibility

没有 Video/Media role、playback value、caption track 或 AT 桥。使用 WebView `<video>` 时 accessibility 来自平台浏览器，不来自 `myrust-ui`。

video 不注册 demo；`/docs-demo` 没有 media surface。
