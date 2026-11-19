WebView 有两种模式：整窗 `WindowContent::WebView`，或在 `WindowContent::NativeUi` 窗口里通过 `Context::web_view` 创建的嵌入区域。

```rust
use myrust_core::app::{WindowConfig, WindowContent};
use myrust_core::webview::{PageSource, WebViewConfig};

let config = WindowConfig {
    title: "Docs".to_string(),
    content: Some(WindowContent::WebView(Box::new(WebViewConfig {
        source: PageSource::Url("https://example.com".to_string()),
        ..WebViewConfig::default()
    }))),
    ..WindowConfig::default()
};
```

## Making web views

通过 `AppContext::create_window` + `WindowContent::WebView` 创建整窗 WebView。`PageSource::Url`/`Html`、session、page options、download、protocol、file-drop handler 在 `myrust-core::webview`。

通过 `myrust_core::ui` 创建嵌入区域：

```rust
use myrust_core::ui::{NativeUi, WebViewPermissionPolicy};

let view = NativeUi::new(|c| {
    c.column().padding(16.0).children(|c| {
        c.web_view("docs", 480.0, 320.0)
            .url("about:blank")
            .initialization_script("window.embedded = true;")
            .permissions(WebViewPermissionPolicy::DenyAll);
    });
});
```

区域 id 跨帧稳定。可配置 `.html(...)`、`.transparent(true)`、`.navigation(...)`、`.permissions(...)`、`.size(...)`、`.flex_grow(...)` 与 `.visible(false)`。

## Its box

整窗 WebView 填满 native 窗口内容框。嵌入区域是普通 flex 节点：按声明尺寸或内容尺寸占位，遵守 gap/padding，并随祖先滚动视图移动。

每帧在 `ViewFrame::web_views` 返回区域：逻辑 bounds、有效 clip 矩形、物理像素 bounds/clip、`visible` 与 `z_order`。区域本身不产生绘制命令。

### Scaled with the native UI around it

整窗 WebView 按窗口 DPI 作为一个平台视图缩放。嵌入区域按 NativeUi 窗口 scale factor 以物理像素定位/缩放，因此跟随窗口 resize 与显示器 DPI 变化。

DisplayList transform 不缩放两种 surface：WebView 是平台子视图，不是 UI renderer 合成的纹理。

## What shows over it

Native UI overlay/popover 不能盖在整窗 WebView 或嵌入区域之上；native 子视图在 GPU/CPU 画面之上。必须覆盖页面的 chrome 用独立 native/panel 窗口。

## Keys the page leaves

WebView 键盘处理留在页面/平台 WebView；NativeUi key 事件独立。没有共享焦点或“未处理键冒泡到 native UI”的桥。

嵌入区域里点击会把焦点交给 WebView，点击 native UI 内容会把焦点还给父窗口。这是宿主级焦点转移，不是共享按键路由。

## Scrolling the native UI around it

整窗 WebView 填满窗口；native UI 无法围绕它滚动。

嵌入区域参与 native 滚动：逻辑与物理 bounds 叠加祖先滚动偏移，clip 是窗口与祖先滚动/裁剪矩形的交集。wry 没有逐视图 scissor，部分裁剪会让 native view 变小，而不是对网页做像素裁剪；完全裁掉时隐藏。

## Security

嵌入区域在 `WebViewConfig` 之上使用 `WebViewNavigationPolicy::AllowAll`/`DenyAll`/`AllowHosts` 与 `WebViewPermissionPolicy::PlatformDefault`/`DenyAll`/`AllowAll`。`AllowHosts` 匹配 host 与子域名；`about:blank` 放行。平台会把初始文档加载也送进 navigation handler，因此策略必须放行起始 URL。

`untrusted` 仍只是记录标记，不构成 trusted/untrusted 分区。

## Its page

Page source、history、IPC、session 与 file drop 属于 `myrust-core` WebView。阅读 `WebViewConfig`、`PageOptions`、`Session`、`PageHandle` 与 `ipc`。

嵌入区域安装同一 IPC bridge：页面请求到达 `IpcRouter`，回复会 resolve promise。本批没有公开的逐区域 `PageHandle`、文件投放接线或 Rust→JS push API；`App::emit`、`App::evaluate_script`、`App::reload` 与 `App::page` 仍只面向整窗 `WindowContent::WebView`。

### The typed client

`myrust-core::ipc` 提供 Rust 与 WebView JS 之间的 typed commands/events。它与 NativeUi `Context`/`AccessNode` 是独立桥；UI crate 不自动生成 typed bridge。

## Limits

没有把网页快照进 wgpu 纹理、WebView 上的 native overlay、共享焦点/按键路由、DisplayList transform/旋转，也没有已有区域视图之间的运行时 restack。多个区域按 `z_order` 创建；该顺序变化不会改变平台堆叠。

创建失败或不支持的 target 会通过 `App::native_ui_last_error` 显式暴露；窗口继续渲染 native UI 内容，不会假装页面存在。WASM/browser 构建只记录区域几何，不创建 native view。

## Testing

区域布局、裁剪、物理换算与同步计划都有无显示器单测。Native UI 用 `NativeUi::render` 测试。真实 wry 视图需要桌面显示与系统 WebView，该 smoke test 默认 ignored，需在真机显式运行。
