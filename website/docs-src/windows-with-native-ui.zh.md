native UI 窗口由 `WindowConfig` 与 `WindowContent::NativeUi` 配置。`NativeUi` 持有逐帧视图；`App::on_ready` 通过 `AppContext::create_window` 创建窗口。

```rust
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{column, text, NativeUi};

let view = NativeUi::new(|c| {
    column(c).padding(16.0).children(|c| text(c, "Native window"));
});

let mut app = App::new("myui")?;
app.on_ready(move |context| {
    context.create_window(WindowConfig {
        title: "MyRust UI".to_string(),
        width: 640.0,
        height: 360.0,
        content: Some(WindowContent::NativeUi(view.clone())),
        ..WindowConfig::default()
    })?;
    Ok(())
});
app.run()
```

## Title bars

`WindowConfig::decorations` 选择原生边框/标题栏；`TitleButtons` 设置 close/minimize/maximize 可见性。`set_window_title(id, title)` 运行时改原生标题。

自定义标题栏可在 `decorations(false)` 后画进 native UI，但窗口拖动/resize 命中区与 traffic-light 行为由平台拥有。选择性 title-button 控制在 macOS 外是 no-op；没有自定义标题栏布局支持。

## Zoom

`WindowConfig::fullscreen` 启动即全屏。没有公开的运行时 zoom/maximize/restore API，也没有逐窗口 zoom 回调；`WindowInfo` 只报告配置的 `fullscreen`。

自定义 zoom 按钮可关闭/创建新窗口，但当前 `myrust-core` 不能程序化 maximize 已有窗口。

## Vibrancy

`WindowConfig::vibrancy: Option<Vibrancy>` 请求平台材质：`Blur`、`UnderWindowBackground`、`Sidebar`、`Popover`。`transparent` 与 `always_on_top` 是独立选项。

Vibrancy 是窗口级平台效果，不是 native UI command。内容仍自绘；没有实时 backdrop 采样或逐区域 vibrancy。

### Liquid Glass over other apps

没有 Liquid Glass 或任意 behind-window material API。`Vibrancy` 枚举是唯一材质表面；不支持的平台视为 no-op。需要其他应用之上的玻璃效果时，使用本 crate 外的平台 effect。

## Flyouts that fit their content

native 窗口没有内容自适应 flyout/panel API。`panel` 被记录但当前是 no-op，也没有 resize-to-content 或锚定窗口定位 helper。

窗口内 flyout 用 `overlay`/popover，在现有窗口内定位。独立 native panel 窗口需要平台代码或未来 config 字段。

## Active and inactive windows

`App::on_activate` 接收应用 activation/focus。`WindowConfig`/`WindowInfo` 没有逐窗口 active/inactive 状态、focus 查询或回调。

视图可根据应用状态自绘 active chrome，但无法通过公开 API 观察 native 窗口焦点变化。

## Hidden windows

`WindowConfig::visible` 可创建隐藏窗口。没有 `show_window`/`hide_window`；当前公开表面可创建、关闭、列出和改标题。

`skip_taskbar` 是创建选项；运行时切换未暴露。隐藏的 native UI 仍需宿主驱动帧循环重绘。

## Closing

`AppContext::close_window(id)` 请求关闭；`App::exit_when_last_window_closes` 控制退出。`on_all_closed` 在最后一个窗口关闭时通知。

没有逐窗口 close-request veto、关闭动画或“隐藏而非关闭”API。native UI 窗口不向视图暴露 window handle。

## No webview

`WindowContent::NativeUi(NativeUi)` 不启动 WebView；内容事件驱动，由 runtime CPU 光栅化/呈现。`WindowContent::WebView` 仍是 Web 内容替代。

同时设置 legacy `webview` 与统一 `content` 会报错。native UI 窗口是普通平台窗口，但没有 HTML/JS 层，也没有 WebView 文件投放/context menu 服务。

runtime 为窗口内容发出 `AccessNode` 元数据；native 标题栏/traffic lights 是平台 chrome，不在该列表。未连接平台 AT 桥。
