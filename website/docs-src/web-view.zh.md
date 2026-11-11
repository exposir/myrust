WebView 是独立的 `myrust-core` 窗口内容，不是 NativeUi 内的一层。一个窗口的 `WindowContent::WebView` 与 `WindowContent::NativeUi` 互斥。

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

通过 `AppContext::create_window` + `WindowContent::WebView` 创建。`PageSource::Url`/`Html`、session、page options、download、protocol、file-drop handler 在 `myrust-core::webview`。

`myrust-ui` 没有 `WebView` 控件；没有 native UI API 创建网页表面。

## Its box

WebView 填满其 native 窗口内容框；不由 NativeUi 窗口的 flex 布局定位。没有在同一窗口混排一个 WebView child 与 native 控件的模式。

### Scaled with the native UI around it

没有 native-UI 缩放的 WebView。窗口 DPI 按平台 chrome 缩放 WebView；DisplayList transform 不能缩放 WebView surface，因为它们是不同窗口/compositor。

## What shows over it

Native UI overlay/popover 不能在同一窗口绘制在 WebView OS surface 之上。用独立 native/panel 窗口放 chrome，或保持纯 WebView 内容。

## Keys the page leaves

WebView 键盘处理留在页面/平台 WebView；NativeUi key 事件独立。没有共享焦点或“未处理键冒泡到 native UI”的桥。

## Scrolling the native UI around it

Native UI 不能在同一窗口围绕 live WebView surface 滚动；native 滚动与 WebView 滚动独立。WebView 窗口滚动页面；native 窗口滚动 native 内容。

## Its page

Page source、history、IPC、session 与 file drop 属于 `myrust-core` WebView。阅读 `WebViewConfig`、`PageOptions`、`Session`、`PageHandle` 与 `ipc`；本页只记录合成边界。

### The typed client

`myrust-core::ipc` 提供 Rust 与 WebView JS 之间的 typed commands/events。它与 NativeUi `Context`/`AccessNode` 是独立桥；UI crate 不自动生成 typed bridge。

## Limits

没有同窗 native+web 合成、WebView 上的 native overlay、共享焦点/按键路由、DisplayList transform 缩放 WebView 或 WASM/browser target。`/docs-demo` 没有该 demo；wasm UI demo 不是 WebView。

## Testing

WebView 行为在 `myrust-core`/宿主集成测试，不在 headless NativeUi 测试。Native UI 用 `NativeUi::render` 测试；WebView 需要平台 WebView 与 window handle。`myrust-ui` 没有 unit-level WebView surface。
