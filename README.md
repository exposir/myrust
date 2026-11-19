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
- **Embedded WebView regions** — place WKWebView/WebView2/WebKitGTK children in NativeUi flex layout, with clipping, visibility, physical-pixel bounds and click focus.
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

Native UI windows present through wgpu when the `gpu` feature is enabled
(default in `myrust-core`) and a compatible adapter exists: Vulkan on Linux,
Metal on macOS, DX12 on Windows. The presenter draws shapes, gradients, text,
SVG paths and shadows on the GPU, and redraws only the regions that changed
since the previous display list.

When no adapter or surface is available, or the surface is lost at runtime,
the window falls back to `CpuRasterizer` plus `softbuffer`; set `NO_GPU=1` to
force that path. `SoftwareRenderer` remains a non-rasterizing stats backend.

The offscreen `WgpuRenderer` is also available for tests and tools:

```bash
cargo build -p myrust-ui --features gpu
```

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

The API shape and the demo catalogue follow [mygo](https://github.com/egoist/mygo),
egoist's immediate-mode desktop UI for Go. `NativeUi::new(|ctx| ...)` mirrors mygo's
`View(callback)`; the `column`/`row`/`container`/widget constructors mirror its `Context`
methods; and the docs demos are ports of its examples where this API can express them
(see [`examples/README.md`](examples/README.md) for which ones, and why the rest are not
stubs). mygo builds on Go's own rendering stack — this is an independent Rust
implementation with its own layout solver, TTF rasterizer, and display list.

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

**Phase 4** — complete for the core release loop. `Packager::package` stages an executable and invokes native Linux, macOS, and Windows tools; signing uses `codesign`, `signtool`, or `rcodesign`; macOS notarization uses `xcrun notarytool`; updates support Ed25519/minisign verification, signed delta patches, path-safe unpacking, and rollback.

### Honest boundaries

- Download byte progress is not emitted by wry 0.57; only started/completed events are live.
- WebKit content-rule structures are serialized metadata only; filtering execution is not implemented.
- `untrusted` page mode is recorded configuration only; permission gating is not implemented.
- Windows JumpList publication, Windows named-pipe activation forwarding, power assertions and native appearance reads are implemented at the API level; real taskbar/COM/pipe/ExecutionState behavior still needs platform validation. OS deep-link registration, native system-proxy readers and portable number/date formatting remain incomplete.
- Update manifests without a configured verifier still fail closed through `UnimplementedSignatureVerifier`. Sparkle-style update UI, background scheduling, and platform credential storage are not implemented.
- Packaging produces Linux tar/deb/PKGBUILD, a macOS app/DMG, and a Windows NSIS installer. MSI, universal macOS binaries, and notarization against Apple's live service are not covered by CI.
- Packaging and signing tests validate command construction, tool-missing errors, test Ed25519 keys, delta application, and archive path safety. Real `hdiutil`, `codesign`, `signtool`, `makensis`, and Apple notarization require their platform hosts and credentials.
- The wgpu path is device-validated against Mesa lavapipe (Vulkan on CPU) under Xvfb: the GPU presenter opened a real window, presented it, redrew only chart-sized damage rectangles on later frames, and fell back to `CpuRasterizer`/`softbuffer` with `NO_GPU=1`. Real desktop GPUs and macOS/Windows surface behavior remain unverified here; no adapter produces an explicit fallback reason instead of a failure.
- `NativeUiWindow` also falls back to `SoftwareRenderer` if an explicitly requested renderer cannot initialize, keeps the window/other windows alive, logs the failure, and exposes it through `AppContext::native_ui_last_error`.
- `SoftwareRenderer` is not a pixel rasterizer. It validates and analyzes display-list commands, transforms, scissors, and bounds only.
- Native UI has been run under an Xvfb virtual X11 display (`DISPLAY=:77`, 1280x900): the `myrust Native Dashboard` window opened at 1024x820 with the Software backend, rendered, and accepted real mouse clicks and keyboard input; click follow-up frames now settle synchronously. Real hardware desktop sessions and macOS/Windows behavior are still unverified. XTest buttons 4/5 are not interpreted as wheel events by GDK under Xvfb, so wheel behavior remains unit-tested rather than Xvfb-verified.
- Window presentation keeps a persistent front texture, diffs adjacent display lists into at most eight damage rectangles, and blits the texture to a wgpu surface; the separate offscreen `WgpuRenderer` still targets a single-sample texture for tests and tools.
- `CpuRasterizer` is a CPU presenter. Text uses a shared `TextLayout` for OpenType kerning/ligatures, Unicode bidi ordering, UAX #14 wrapping and an ordered sans/mono/CJK/emoji fallback stack; the same positioned glyph runs drive CPU rasterization, the GPU atlas and damage bounds. The 8x8 bitmap path remains only when no font loads. It supports alignment (start/center/end), bold synthesis via coverage-bitmap offset, SVG path fill and stroke, line/stroke overlay on filled outlines, and decoded RGBA images with bilinear sampling, source alpha, rounded corners, transforms and scissors. It does not implement native inline IME preedit or optimized spatial hit testing. PNG/JPEG/WebP decoding is opt-in through `image-decode`.
- `Renderer` reports Text, SVG path, and image commands as unsupported through `RenderStats` when they cannot be resolved; resolved images contribute transformed, clipped bounds. The separate CPU presenter (`CpuRasterizer`) draws TTF text, SVG paths, geometry and images independently from those statistics.
- `VideoPlayer` and the `video` widget are host-driven: callers decode and timestamp RGBA frames, then advance a deterministic playhead. Frame changes reuse `DrawCommand::Image` and damage; MyRust links no video decoder and provides no audio or A/V synchronization.
- `Theme::system()` only detects light versus dark appearance. `myrust_core::system_theme()` and `AppearancePreferences::detect()` expose native macOS/Windows accent color, reduced motion and high contrast; Linux leaves the native-only fields as `None`.
- Animation includes scalar tweens, keyed springs (`animate_spring`), keyframes, timelines, transitions and a deadline-based `FrameScheduler`; tao `ControlFlow::WaitUntil` drives the next native frame instead of idle polling. `Context::reduce_motion()` checks `MYRUST_REDUCE_MOTION`, and reduced motion settles springs directly. Native preference callbacks and real-platform frame pacing still need device validation.
- Accessibility output is semantic data from the immediate-mode runtime. With the `a11y` feature (on by default), AccessKit publishes the same tree through AT-SPI on Linux, NSAccessibility on macOS and UI Automation on Windows, with `Focus` and `Click` actions for widgets that carry a key.
- Hit testing remains a linear reverse-order scan of previous-frame widget regions. Tab order follows widget construction order. Duplicate explicit ids share interaction/focus/widget state and are therefore risky.
- Text fields support cluster-aware mouse/keyboard selection, horizontal scrolling, cut/copy/paste with a replaceable clipboard provider, word/line navigation, grouped undo/redo, and composition rendering with commit/cancel. The browser demo maps hidden-textarea composition events; native preedit is unavailable through tao 0.37 on Linux/macOS/Windows, while hosts may feed composition events directly. Dead keys, slider inertia and optimized spatial hit testing remain unimplemented.

### Roadmap

| Phase | Focus |
|-------|-------|
| **✓ Phase 1** | Multi-window WebView, typed IPC, desktop APIs, CLI |
| **✓ Phase 2** | Menus, clipboard, drop/hot reload, window state, sessions, page APIs, deep links, downloads, protocols, system/media/login, updater, packaging descriptions, signing interfaces, single-instance forwarding |
| **✓ Phase 3** | Immediate-mode `Context` + `NativeUi`, Theme/spacing/font tokens, hover animations and reduced-motion handling, semantic accessibility nodes, hover/focus/Tab and keyboard activation, TextField/Checkbox/Slider/ScrollView, shared shaped/bidi/fallback text layout, display lists, SVG icons, renderer abstraction, optional wgpu offscreen backend, CPU fallback/presenter, `WindowContent::NativeUi`, event/redraw wiring, `--native-ui` demo, wgpu window presentation with damage-only redraw, CPU/GPU image textures with optional PNG/JPEG/WebP decoding, Linux/macOS/Windows AccessKit bridges, and native appearance reads; inline native IME preedit and macOS/Windows assistive-technology validation remain incomplete |
| **✓ Phase 4** | Packaging (`.app`, `.dmg`, NSIS, `.deb`, tar, PKGBUILD), signing/notarization, signed Ed25519/minisign updates, delta patches and rollback |

## License

MIT

---

*Inspired by [mygo](https://github.com/egoist/mygo) — desktop apps in Go.*
