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

## No webview

`WindowContent::NativeUi(NativeUi)` starts no WebView; content is event-driven and CPU-rasterized/presented by the runtime. `WindowContent::WebView` remains the alternative for web content.

Setting both legacy `webview` and unified `content` is an error. A native UI window is a normal platform window, but it has no HTML/JS layer and no WebView file-drop/context-menu services.

The runtime emits `AccessNode` metadata for window content; native title bar/traffic lights are platform chrome and not in that list. No platform AT bridge is connected.
