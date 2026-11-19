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

## 嵌入 WebView

`Context::web_view(id, width, height)` 为系统 WebView 预留一个 flex 布局盒。区域占用布局空间、参与滚动与裁剪，并可逐帧显隐。它不是网页快照，也不是 wgpu 纹理：宿主会在 GPU/CPU native UI 像素之上创建一个真实的 wry 子视图。

```rust
use myrust_core::ui::{
    NativeUi, WebViewNavigationPolicy, WebViewPermissionPolicy,
};

let view = NativeUi::new(|c| {
    c.column().padding(16.0).gap(12.0).children(|c| {
        c.text("Native controls");
        c.web_view("docs", 420.0, 260.0)
            .url("about:blank")
            .initialization_script("document.title = 'embedded';")
            .transparent(true)
            .navigation(WebViewNavigationPolicy::AllowHosts(vec![
                "example.com".to_string(),
            ]))
            .permissions(WebViewPermissionPolicy::DenyAll);
        c.text("Native controls below the page");
    });
});
```

builder 还支持 `.html(...)`、`.size(width, height)`、`.width(...)`、`.height(...)`、`.flex_grow(...)` 与 `.visible(false)`。id 跨帧稳定；宿主按 id 保持一个 native view。

### 每帧区域表

UI crate 从不创建平台视图。每帧返回 `ViewFrame::web_views`：

- `id` 是应用稳定 view id。
- `bounds` 是叠加祖先滚动偏移后的逻辑布局矩形。
- `clip` 是窗口/滚动/裁剪容器求交后的逻辑矩形。
- `physical_bounds` 与 `physical_clip` 是同一矩形按宿主提供的窗口 scale factor 换算出的设备像素。
- `visible` 在隐藏或完全滚出视口时为 false。
- `z_order` 是帧内区域顺序；native 子视图始终位于 GPU/CPU 画面之上。
- 区域不产生 `DrawCommand`，不会重复绘制。

`Context::web_view` 是纯布局/记录操作。它可在测试与 WASM target 使用；那边只记录几何，不创建 native view。

### 宿主同步

`WindowContent::NativeUi` 在 `NativeUiWindow` 中把区域表同步到 wry 子视图：

- 新 id 创建 WKWebView（macOS）、WebView2（Windows）或 WebKitGTK view（Linux）。
- bounds 变化按物理像素移动/缩放 native view；窗口 resize 与 scale factor 变化走同一条路径。
- 零尺寸或完全滚出视口的区域只隐藏、不销毁，滚回来不会重新加载页面。
- 点击区域会把焦点交给它的 WebView；点击 native UI 内容会把焦点还给父窗口。
- 区域消失或 `WebViewOptions` 改变时，销毁并重建 native view。
- Linux 上视图放在 Tao 窗口 default box 里的 `gtk::Fixed` 中，X11 与 Wayland 都能精确摆放子视图。

创建失败与不支持 target 都是显式错误：native UI 继续渲染，`App::native_ui_last_error(window)` 返回平台错误，失败配置不会每帧重试。

### 安全与页面配置

区域选项会转换成 `myrust_core::webview::WebViewConfig` 加一份区域策略。`WebViewNavigationPolicy::DenyAll`、`AllowHosts`（匹配 host 与子域名）以及 `WebViewPermissionPolicy::DenyAll`/`AllowAll` 使用 wry 的 navigation/permission handler。平台会把初始文档加载也送进 navigation handler，因此策略必须放行起始 URL。

旧的 `WebViewConfig::untrusted` 仍只是元数据，不是 trusted/untrusted 分区。

区域视图同样安装 IPC bridge：请求到达应用的 `IpcRouter`，回复会 resolve 页面 promise。没有公开的逐区域 `PageHandle`；`App::emit`、`App::evaluate_script`、`App::reload` 与 `App::page` 仍只面向整窗 `WindowContent::WebView`，不是区域视图。

### 合成限制

wry 没有逐视图 scissor。区域被滚动视口或裁剪容器部分裁掉时，宿主用 bounds 与 clip 求交，native view 会变小，而不是对网页做像素裁剪；完全裁掉时隐藏。

Native UI overlay/popover 画在 native 子视图下方的 GPU/CPU 画面里，不能覆盖 WebView。必须盖在页面上方的 chrome 用独立窗口。DisplayList transform 与旋转不会作用于 WebView surface。

多个区域按 `z_order` 创建；wry 没有运行时 restack API，已有视图的相对顺序变化不会改变平台堆叠。native 视图仍在 GPU 画面之上。

### 文件与无障碍

区域视图不继承 `WindowContent::WebView` 的文件投放与窗口 context menu。`WindowConfig::file_drop` 只启用整窗 WebView 路径。

区域无障碍由平台 WebView 引擎负责，不会合并进 native UI 的 `AccessNode` 树。

runtime 为窗口内容发出 `AccessNode` 元数据；native 标题栏/traffic lights 是平台 chrome，不在该列表。默认开启 `a11y` feature 时，桌面运行时通过 AccessKit 的 UIA 适配器在 Windows 发布这些节点。

## System appearance

`myrust_core::desktop::system::AppearancePreferences::detect()` 读取原生外观设置。Windows 上 reduced motion 来自 `UISettings::AnimationsEnabled`，accent color 来自 `UISettings::GetColorValue(UIColorType::Accent)`，high contrast 来自 `SPI_GETHIGHCONTRAST`。每个字段都是 `Option`：API 失败或不可用时保留 `None`。

`AppearancePreferences::theme()` 选择内置明/暗主题并应用检测到的 accent，同时生成 hover/pressed 色阶与对比色标签。`myrust_core::system_theme()` 组合检测与主题构建。`Theme::system()` 本身仍只处理明暗。reduced motion 与 high contrast 留在 `AppearancePreferences`，因为 `Theme` 没有对应字段。

## Desktop platform services

这些服务独立于 native UI 渲染，`WindowContent::NativeUi` 与 `WindowContent::WebView` 都可使用。

`SingleInstance::acquire(name)` 会把第二次启动的参数与工作目录转发给第一个进程。Unix 使用每用户文件系统 socket，Windows 使用 namespaced named pipe。回调注册前到达的 activation 会排队。Unix 与 Windows 之外的平台保留 detection-only 行为，可用 `supports_activation_forwarding()` 判断差异。

`WindowsJumpList::new(app_id)` 通过 Shell COM API 发布最近文档。`app_id` 必须与安装器和快捷方式使用的 AppUserModelID 一致，不能为空，也不能包含空白字符。当前实现会重新加入 `BeginList` 返回的路径，尚未尊重用户从 jump list 中移除的目标；也没有 macOS 或 Linux 等价实现。

`PowerAssertion::prevent_sleep` 与 `PowerAssertion::prevent_display_sleep` 持有 RAII 睡眠断言。macOS 使用 `IOPMAssertion`，Windows 使用 `SetThreadExecutionState`，Linux 使用 `systemd-inhibit` 阻止系统睡眠。Linux 阻止显示休眠会返回显式错误。Windows 断言是线程本地的，应在同一线程创建和释放。

`SecretStore` 把二进制 secret 保存到 Keychain、Credential Manager 或 Linux Secret Service。后端缺失、锁定或不可用会返回错误；不存在内存 fallback。同一个 store 内的操作会串行执行，因为 Secret Service 对并发请求敏感。

`ClipboardWatcher` 轮询文本与 RGBA 图像，做去抖并抑制重复值。采用轮询是因为目标平台没有统一的原生剪贴板变更事件。打开剪贴板发生在 worker 线程启动之前，因此 headless 环境会在 `watch()` 处直接失败，不会静默回退。
