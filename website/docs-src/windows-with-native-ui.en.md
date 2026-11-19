A native UI window is configured with `WindowConfig` and `WindowContent::NativeUi`. `NativeUi` holds the per-frame view; `App::on_ready` creates the window through `AppContext::create_window`.

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

`WindowConfig::decorations` chooses native borders/title bar; `TitleButtons` sets close/minimize/maximize visibility. `set_window_title(id, title)` changes the native title at runtime.

Custom title bars are drawn inside the native UI after `decorations(false)`, but the window drag/resize hit zones and traffic-light behavior are platform-owned. Selective title-button control is no-op outside macOS; custom title bar layout is not provided.

## Zoom

`WindowConfig::fullscreen` starts full screen. There is no public runtime zoom/maximize/restore API and no per-window zoom callback; `WindowInfo` reports configured `fullscreen` only.

A custom zoom button can call `close_window`/create a new window, but cannot programmatically maximize the existing window through `myrust-core` today.

## Vibrancy

`WindowConfig::vibrancy: Option<Vibrancy>` requests platform material: `Blur`, `UnderWindowBackground`, `Sidebar` or `Popover`. `transparent` and `always_on_top` are separate options.

Vibrancy is a window-level platform effect, not a native UI command. Content remains self-drawn; there is no live backdrop sampling and no per-region vibrancy.

### Liquid Glass over other apps

No Liquid Glass or arbitrary behind-window material API is exposed. The `Vibrancy` enum is the only material surface; unsupported platforms treat it as a no-op. For glass over other apps, use a platform effect outside this crate.

## Flyouts that fit their content

Native windows do not have a content-sized flyout/panel API. `panel` is recorded but currently a no-op, and there is no auto-resize-to-content or anchored window positioning helper.

For in-window flyouts, use `overlay`/popover, which positions inside the existing window. A separate native panel window needs platform code or a future config field.

## Active and inactive windows

`App::on_activate` receives application activation/focus. There is no per-window active/inactive state, focus query or callback in `WindowConfig`/`WindowInfo`.

A view can draw its own active chrome from application state, but it cannot observe native window focus changes through the public API.

## Hidden windows

`WindowConfig::visible` can create a hidden window. There is no `show_window`/`hide_window` method; the current public surface can create, close, list and retitle windows.

`skip_taskbar` is a creation option; toggling it at runtime is not exposed. Hidden native UI still needs a host-driven frame loop to repaint.

## Closing

`AppContext::close_window(id)` requests close; `App::exit_when_last_window_closes` controls app exit. `on_all_closed` notifies when the last window closes.

There is no per-window close-request veto, close animation or “hide instead of close” API. Native UI windows do not expose a window handle to the view.

## Embedded WebViews

`Context::web_view(id, width, height)` reserves a flex-layout box for a system WebView. The region takes layout space, participates in scrolling and clipping, and can be shown or hidden per frame. It is not an HTML snapshot and not a wgpu texture: the host creates a real wry child view above the GPU/CPU native UI pixels.

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

The builder also accepts `.html(...)`, `.size(width, height)`, `.width(...)`, `.height(...)`, `.flex_grow(...)` and `.visible(false)`. The id is stable across frames; the host keeps one native view per id.

### The per-frame region table

The UI crate never creates a platform view. Each frame returns `ViewFrame::web_views`:

- `id` is the application-stable view id.
- `bounds` is the logical layout rectangle after ancestor scroll offsets.
- `clip` is the logical window/scroll/clip intersection.
- `physical_bounds` and `physical_clip` are the same rectangles in device pixels, using the window scale factor supplied by the host.
- `visible` is false for hidden or fully clipped regions.
- `z_order` orders regions within the frame; native child views always sit above the GPU/CPU picture.
- The region emits no `DrawCommand`, so the repaint is not duplicated.

`Context::web_view` is a pure layout/recording operation. It works in tests and on the WASM target, where it records geometry but no native view is created.

### Host synchronization

`WindowContent::NativeUi` syncs the frame table to wry child views in `NativeUiWindow`:

- A new id creates a WKWebView (macOS), WebView2 (Windows) or WebKitGTK view (Linux).
- Bounds changes move and resize the native view in physical pixels; window resizes and scale-factor changes flow through the same path.
- Zero-size or fully scrolled-out regions are hidden, not destroyed, so scrolling back does not reload the page.
- A click inside a region focuses its WebView; a click on native UI content returns focus to the parent window.
- Removing a region, or changing its `WebViewOptions`, destroys and recreates the native view.
- On Linux the views live in a `gtk::Fixed` packed into the Tao window's default box, which is what makes exact child positioning work on both X11 and Wayland.

Creation failures and unsupported targets are explicit: native UI rendering continues, `App::native_ui_last_error(window)` returns the platform error, and a failing configuration is not retried on every frame.

### Security and page configuration

Region options are converted to a `myrust_core::webview::WebViewConfig` plus a region policy. `WebViewNavigationPolicy::DenyAll`, `AllowHosts` (host and subdomain matching) and `WebViewPermissionPolicy::DenyAll`/`AllowAll` use wry's navigation and permission handlers. Backends report the initial document load through the navigation handler too, so the policy must admit the starting URL.

The older `WebViewConfig::untrusted` flag remains metadata only; it is not a trusted/untrusted partition.

The IPC bridge is installed in region views too. Requests reach the app's `IpcRouter` and replies resolve the page promise. There is no public per-region `PageHandle`, and `App::emit`, `App::evaluate_script`, `App::reload` and `App::page` still address a full-window `WindowContent::WebView`, not region views.

### Composition limits

wry exposes no per-view scissor rectangle. When a region is partially clipped by a scroll viewport or clipping container, the host intersects its bounds with the clip and the native view becomes smaller; the page is not pixel-cropped. Fully clipped regions are hidden.

Native UI overlays and popovers paint in the GPU/CPU picture below the native child views, so they cannot cover a WebView. Use a separate window for chrome that must sit above a page. Display-list transforms and rotations do not apply to a WebView surface.

Multiple regions are created in `z_order`; wry has no runtime restack API, so changing the relative order of existing views does not move them in the platform stack. Native views still stay above the GPU picture.

### Files and accessibility

Region views do not inherit `WindowContent::WebView` file-drop wiring or the window context menu. `WindowConfig::file_drop` only enables the full-window WebView path.

Region accessibility belongs to the platform WebView engine. The WebView is not merged into the native UI `AccessNode` tree.

The runtime emits `AccessNode` metadata for window content; native title bar/traffic lights are platform chrome and not in that list. With the default `a11y` feature, the desktop runtime publishes those nodes through AccessKit's UIA adapter on Windows.

## System appearance

`myrust_core::desktop::system::AppearancePreferences::detect()` reads native appearance settings. On Windows, reduced motion comes from `UISettings::AnimationsEnabled`, the accent color from `UISettings::GetColorValue(UIColorType::Accent)`, and high contrast from `SPI_GETHIGHCONTRAST`. Each field is an `Option`: a failed or unavailable API leaves it `None`.

`AppearancePreferences::theme()` chooses the light or dark built-in theme and applies the detected accent, including hover/pressed shades and a contrasting label color. `myrust_core::system_theme()` combines detection and theme construction. `Theme::system()` itself still only handles light/dark. Reduced motion and high contrast remain on `AppearancePreferences`, because `Theme` has no fields for them.
