<p align="center">
  <img src="docs/logo.svg" alt="MyRust" width="320">
</p>

# MyRust

**Desktop apps in Rust, with a web frontend or a native WebView — plus an immediate-mode native UI library with 87 live demo pages.**

The project site is static: [`website/index.html`](website/index.html) is the homepage, and
[`website/docs/ui/index.html`](website/docs/ui/index.html) is the native UI documentation (also at
**[myrust.pages.dev/docs/ui/](https://myrust.pages.dev/docs/ui/)** — each widget has a WASM-based live preview).
Serve the `website/` directory directly — no build step.

Each MyRust window loads a **web page** in the system's native WebView — WKWebView on macOS, WebKitGTK on Linux, WebView2 on Windows. Build the frontend with any web framework; it calls your Rust code through typed IPC with auto-generated TypeScript bindings.

- **No `unsafe` in the core** — `myrust-core` uses `#![forbid(unsafe_code)]` (platform WebView backends are provided by upstream crates).
- **Typed IPC for pages** — register Rust handlers and generate TypeScript clients from them; pages can also subscribe to Rust-pushed events.
- **System WebView** — uses the OS-native renderer instead of bundling a browser engine.
- **Dual mode** — the same source runs as a native desktop window or a standalone HTTP server.
- **Desktop APIs** — windows, native menus, system tray, notifications, clipboard, file dialogs, global shortcuts, and a single-instance lock.
- **File drag & drop** — WebView drop events reach Rust through typed callbacks.
- **Window state** — logical geometry, maximized/fullscreen state, and display scale persist to JSON.
- **Sessions & pages** — logical session identities, session-state hooks, page-level navigation, zoom, find, and content-rule metadata.
- **Window appearance** — transparency, always-on-top, taskbar visibility, title-button preferences, fit-content, and Linux input regions.
- **Documents & system hooks** — recent documents, file watching, deep-link dispatch, downloads, custom protocols, locale/appearance reads, media controls, and login items.
- **Hot reload** — event-driven file watching reloads all WebViews on change.
- **Ready to extend** — workspace structure with a core library, CLI tool, and examples.

## A window with a web page, whose frontend calls Rust
```rust
use myrust_core::app::{App, WindowConfig};
use myrust_core::ipc::{IpcRouter, IpcType, NoRequest};
use myrust_core::webview::{PageSource, WebViewConfig};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct GreetResponse { message: String }

fn main() -> Result<()> {
    let mut app = App::new("my-app")?;
    app.ipc_router().register::<NoRequest, GreetResponse, _>(
        "greet",
        "Returns a greeting.",
        |_| Ok(GreetResponse { message: "Hello from Rust!".into() }),
    )?;

    app.on_ready(|ctx| {
        ctx.create_window(WindowConfig {
            title: "Hello".into(),
            webview: Some(WebViewConfig {
                source: PageSource::Html(HTML.into()),
                devtools: false,
            }),
            ..Default::default()
        })?;
        Ok(())
    });

    app.run()
}
```

```typescript
// Generated — myrust.d.ts
export const greet: () => Promise<GreetResponse>;

// In your page
const res = await greet();
document.body.textContent = res.message;
```

## Getting started

With Rust 1.92+, build the example dashboard:

```bash
# Run in browser mode, then open http://0.0.0.0:8090
cargo run -p dashboard -- --web

# Run as a native desktop window (system WebView)
cargo run -p dashboard

# Run the native self-drawn UI demo
cargo run -p dashboard -- --native-ui

# Generate TypeScript IPC declarations
cargo run -p dashboard -- --generate-ts
```

More focused examples — WebView IPC, native UI, animation, drag and drop —
live in [`examples/`](examples/README.md).

### Native UI renderer

The default workspace build has no GPU dependency. `SoftwareRenderer` is
always available and analyzes/validates display lists without producing
pixels. Enable the real wgpu offscreen backend explicitly:

```bash
cargo build -p myrust-core --features gpu
```

HTTP, WebView, CLI, and Dashboard startup never initialize wgpu implicitly.
Applications must call `create_renderer` explicitly and select
`RendererKind::Wgpu` when a missing GPU must be treated as an error.

Developer tools follow the upstream `wry` gate: `WebView::open_devtools` and
`Page::open_devtools` exist in debug builds and when the `devtools` feature is
enabled. A release build without that feature does not compile those methods.

```bash
cargo build -p myrust-core --features devtools
```

### CLI

```bash
# Development server with auto-open browser
cargo run -p myrust-cli -- dev

# Release build
cargo run -p myrust-cli -- build
```

### Prerequisites

| Platform | Dependencies |
|----------|-------------|
| **macOS** | Xcode CLI tools (no additional setup) |
| **Linux** | GTK3, WebKitGTK 4.1, AppIndicator |
| **Windows** | WebView2 runtime (shipped with Windows 11) |

Debian / Ubuntu:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev
```

## Architecture

```
myrust/
├── myrust-core/            # Core library
│   └── src/
│       ├── app/
│       │   ├── mod.rs      # App lifecycle, multi-window manager
│       │   ├── recent_documents.rs # Recent files + jump-list interface
│       │   ├── file_watcher.rs # Public notify-based watcher
│       │   ├── deep_link.rs # Scheme registry + URL dispatch
│       │   └── window_state.rs # Logical window geometry persistence
│       ├── webview/
│       │   ├── mod.rs      # wry WebView wrapper
│       │   ├── page.rs     # Page-level navigation/zoom/find/rules
│       │   └── session.rs  # Logical sessions + state hooks
│       ├── ipc/
│       │   ├── mod.rs      # IPC router + JS bridge
│       │   └── gen.rs      # TypeScript declaration generator
│       └── desktop/
│           ├── menu.rs       # Native menu bar + context menu wrappers
│           ├── clipboard.rs  # Text + RGBA image clipboard
│           ├── tray.rs     # System tray icon + menu
│           ├── notify.rs   # Desktop notifications
│           ├── dialog.rs   # File open/save + message dialogs
│           ├── shortcut.rs # Global keyboard shortcuts
│           └── instance.rs # Single-instance lock
├── myrust-ui/              # Platform-agnostic native UI library (65+ widgets, WASM demos)
│   ├── src/context.rs     # Immediate-mode Context + widget builders
│   ├── src/native.rs      # NativeUi callback host, event routing
│   ├── src/layout.rs      # Flexbox-style CPU layout engine
│   ├── src/paint.rs       # DrawCommand display list, Color
│   ├── src/present.rs     # CPU rasterizer (ab_glyph TTF, SVG paths)
│   ├── src/markdown.rs    # Subset Markdown (headings, lists, code, tables)
│   ├── src/icon.rs        # 11 built-in SVG path icons
│   └── src/(30+ modules)  # Widgets: button, checkbox, radio, tabs, slider,
│                          #   scroll, tree, table, rating, color-picker,
│                          #   sidebar, breadcrumbs, toolbar, and more
├── myrust-cli/             # CLI (dev server, build)
├── examples/               # Dashboard plus focused example apps (see examples/README.md)
└── frontend/               # Web frontend assets
```

## Native UI (myrust-ui)

`myrust-ui` is an **immediate-mode native UI library** with 65+ widgets — think imgui
done right, with real TTF text, SVG paths, a CPU rasterizer, and a flexbox-style layout
engine. Every widget is a live WASM demo at
**[myrust.pages.dev/docs/ui/](https://myrust.pages.dev/docs/ui/)** (87 documented pages).

```rust
use myrust_ui::*;

NativeUi::new(|ctx| {
    column(ctx).width(380.0).gap(12.0).center().children(|ctx| {
        text(ctx, "Sign in").font_size(22.0).bold().color(theme.foreground);
        text_field(ctx, "email", &mut email);
        text_field(ctx, "password", &mut password);
        if button_primary(ctx, "Submit").clicked {
            login(&email, &password);
        }
    });
});
```

**Widgets:** button (flat & primary with optional icon), checkbox (with tree),
radio group, toggle, switch, slider, range-slider, stepper, progress bar, spinner,
badge, avatar, text field, search field, find bar, text input primitives, editable
text, code editor, diff viewer, rich text, markdown (headings, lists, code blocks,
tables), combobox, autocomplete, select, tabs, segmented control, menu button,
context menu, popover, tooltip, dropdown, dialog, alert dialog, toast notification,
sidebar, breadcrumbs, accordion, collapsible, tree/outline, list, grid view, table,
scroll view, split view, form field, rating, date/time input, decimal/number input,
stepper, token field, slider, range slider, color picker (with hue/SV/alpha bars
and presets), icon, image, meter, bar/line charts, calendar, transitions, animation,
drag & drop, reorderable lists, inspector, navigation router with back/forward
buttons, accessibility roles.

**Platform:** Linux (native, Xvfb-tested), macOS, Windows — same `NativeUi::new`
API everywhere, compiled once.

## IPC

Register a typed handler on the router:

```rust
router.register::<RequestType, ResponseType, ErrorType>(
    "channel_name",
    "Description of what this handler does.",
    |request| { /* ... */ },
)?;
```

The WebView receives the JavaScript bridge script automatically:

```js
// Request-response
const result = await window.__myrust_ipc__.invoke("channel", payload);

// Listen for Rust-pushed events
window.__myrust_ipc__.on("event_channel", (data) => { /* ... */ });
```

Run with `--generate-ts` to produce `frontend/myrust.d.ts`:

```typescript
export interface Stats {
  cpu: CpuStats;
  memory: MemoryStats;
  disks: DiskStats[];
  // ...
}
export const getStats: () => Promise<Stats>;
```

## Desktop APIs

```rust
use myrust_core::desktop::{
    Clipboard, DesktopNotification, Dialog, FileDialogOptions,
    GlobalShortcutManager, SingleInstance, SystemTray, SystemTrayConfig,
};
use myrust_core::desktop::menu::MenuBuilder;

// Native menu
let menu = MenuBuilder::new()
    .add_submenu("File", |menu| {
        menu.add_item("quit", "Quit").add_separator()
    })
    .build()?;
app.set_app_menu(menu);

// Clipboard
let mut clipboard = Clipboard::new()?;
clipboard.write_text("copied from Rust")?;

// System tray
let tray = SystemTray::new(SystemTrayConfig::default())?;
tray.on_event(|event| println!("{event:?}"));

// Notification
DesktopNotification::new("myrust", "Hello from Rust").send()?;

// File dialog
let path = Dialog::open_file(FileDialogOptions::default());

// Global shortcut
let shortcuts = GlobalShortcutManager::new()?;
shortcuts.register("control+shift+KeyD")?;

// Single instance
let _guard = SingleInstance::acquire("myrust-app")?;
```

## Project status

**Phase 1** — complete. The core crate, WebView wrapper, typed IPC, desktop APIs, CLI, and dashboard example build and run on Linux.

**Phase 2** — complete through Batch D. It added native menus, clipboard access, drag & drop, hot reload, window state, sessions, page APIs, appearance controls, recent documents, deep links, downloads, protocols, system/media APIs, updater foundations, package descriptions, signing interfaces, and single-instance forwarding.

**Phase 3** — complete through the final polish round described below. The widget layer uses mygo's immediate-mode `View(callback)` shape: `NativeUi::new(|ctx| ...)`, `Context` widget constructors, inline per-frame state queries, and display-list generation.

### Honest boundaries

- Download byte progress is not emitted by wry 0.57; only started/completed events are live.
- WebKit content-rule structures are serialized metadata only; filtering execution is not implemented.
- `untrusted` page mode is recorded configuration only; permission gating is not implemented.
- Jump-list publication, OS deep-link registration, native system-proxy readers, accent color/reduced-motion/high-contrast reads, power assertions, and portable number/date formatting are not implemented.
- Update signature verification, delta updates, and Sparkle-style update UI are not implemented.
- Package generation writes metadata skeletons only; it does not invoke `hdiutil`, `dpkg-deb`, `makepkg`, MSI tooling, or code-signing tools.
- Code signing and notarization always return explicit errors until Phase 4 platform tooling is wired.
- Windows named-pipe activation forwarding is not implemented; Windows single-instance support remains detection-only.
- The wgpu renderer requires a working GPU or graphics driver at runtime. This Linux container has no `/dev/dri`, so the GPU path is compile-validated but not device-validated here; no adapter returns an explicit error and `RendererKind::Auto` falls back to software.
- `NativeUiWindow` also falls back to `SoftwareRenderer` if an explicitly requested renderer cannot initialize, keeps the window/other windows alive, logs the failure, and exposes it through `AppContext::native_ui_last_error`.
- `SoftwareRenderer` is not a pixel rasterizer. It validates and analyzes display-list commands, transforms, scissors, and bounds only.
- Native UI has been run under an Xvfb virtual X11 display (`DISPLAY=:77`, 1280x900): the `myrust Native Dashboard` window opened at 1024x820 with the Software backend, rendered, and accepted real mouse clicks and keyboard input; click follow-up frames now settle synchronously. Real hardware desktop sessions and macOS/Windows behavior are still unverified. XTest buttons 4/5 are not interpreted as wheel events by GDK under Xvfb, so wheel behavior remains unit-tested rather than Xvfb-verified.
- The wgpu renderer targets a single-sample offscreen texture. It does not expose a native wgpu surface or swapchain, and its offscreen texture is not copied to the window. The `--native-ui` window presents the same display list through `CpuRasterizer` and `softbuffer`.
- `CpuRasterizer` is a basic CPU presenter. It prefers one discovered system TTF and falls back to fixed 8x8 bitmap glyphs if loading fails. It supports text alignment (start/center/end) with optional max-width wrapping, bold synthesis via coverage-bitmap offset, SVG path fill and stroke, and line/stroke overlay on filled outlines. It does not implement shaping, kerning, bidi layout, multi-font fallback, IME composition, image decoding, or optimized spatial hit testing.
- `Renderer` reports Text, SVG path, and image commands as unsupported through `RenderStats`; the separate CPU presenter (`CpuRasterizer`) draws TTF text, SVG paths, and supported geometry independently from those statistics.
- `Theme::system()` only detects light versus dark appearance. It does not read the system accent color or high-contrast preference.
- Animation is scalar/easing infrastructure plus button hover/pressed interpolation; content transitions, timeline orchestration, and spring physics are not implemented. Reduced motion checks `MYRUST_REDUCE_MOTION` and the existing neutral appearance layer, but native macOS and Windows reduced-motion APIs are not implemented. Animation redraw is best-effort through `request_redraw`; there is no timer-driven frame scheduler, and Linux wakeup behavior still depends on the host event loop.
- Accessibility output is semantic data only. No AT-SPI, UIA, or NSAccessibility bridge is connected, so platform assistive technologies cannot observe the emitted nodes.
- Hit testing remains a linear reverse-order scan of previous-frame widget regions. Tab order follows widget construction order. Duplicate explicit ids share interaction/focus/widget state and are therefore risky.
- Text fields have no selection, clipboard operations, undo/redo, IME composition, dead-key composition, or text shaping. Slider and scroll behavior have no inertia or animation. Accessibility is exposed as semantic data without a platform bridge, and optimized spatial hit testing is not implemented.

### Roadmap

| Phase | Focus |
|-------|-------|
| **✓ Phase 1** | Multi-window WebView, typed IPC, desktop APIs, CLI |
| **✓ Phase 2** | Menus, clipboard, drop/hot reload, window state, sessions, page APIs, deep links, downloads, protocols, system/media/login, updater, packaging descriptions, signing interfaces, single-instance forwarding |
| **✓ Phase 3** | Immediate-mode `Context` + `NativeUi`, Theme/spacing/font tokens, hover animations and reduced-motion handling, semantic accessibility nodes, hover/focus/Tab and keyboard activation, TextField/Checkbox/Slider/ScrollView, pure-CPU layout, display lists, TTF/bitmap text, SVG icons, renderer abstraction, optional wgpu offscreen backend, CPU fallback/presenter, `WindowContent::NativeUi`, event/redraw wiring, `--native-ui` demo; platform AT bridges, native reduced-motion APIs, full text/SVG/image rendering, wgpu window presentation, IME, and cross-platform desktop validation remain incomplete |
| **Phase 4** | Packaging (.app, .dmg, .msi, .deb, PKGBUILD, code signing, delta updates) |

## License

MIT

---

*Inspired by [mygo](https://github.com/egoist/mygo) — desktop apps in Go.*
