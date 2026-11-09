<p align="center">
  <img src="docs/logo.svg" alt="MyRust" width="320">
</p>

# MyRust

**Desktop apps in Rust, with a web frontend or a native WebView.**

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

# Generate TypeScript IPC declarations
cargo run -p dashboard -- --generate-ts
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
├── myrust-cli/             # CLI (dev server, build)
├── examples/dashboard/     # Dashboard example
└── frontend/               # Web frontend assets
```

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

**Phase 2** — in progress. Batch A added native menus, clipboard access, WebView file drag & drop, and event-driven `notify`-based hot reload. Batch B added window-state persistence, logical WebView sessions, page-level APIs, window appearance options, fit-content sizing, and input regions. Batch C added recent documents, a public file watcher, in-process deep-link dispatch, downloads/custom protocols, locale and appearance reads, media controls, and login items. Batch D adds the three-stage updater, package-description generation, signing/notarization interfaces, and Unix single-instance activation forwarding.

### Honest boundaries

- Download byte progress is not emitted by wry 0.57; only started/completed events are live.
- WebKit content-rule structures are serialized metadata only; filtering execution is not implemented.
- `untrusted` page mode is recorded configuration only; permission gating is not implemented.
- Jump-list publication, OS deep-link registration, native system-proxy readers, accent color/reduced-motion/high-contrast reads, power assertions, and portable number/date formatting are not implemented.
- Update signature verification, delta updates, and Sparkle-style update UI are not implemented.
- Package generation writes metadata skeletons only; it does not invoke `hdiutil`, `dpkg-deb`, `makepkg`, MSI tooling, or code-signing tools.
- Code signing and notarization always return explicit errors until Phase 4 platform tooling is wired.
- Windows named-pipe activation forwarding is not implemented; Windows single-instance support remains detection-only.

### Roadmap

| Phase | Focus |
|-------|-------|
| **✓ Phase 1** | Multi-window WebView, typed IPC, desktop APIs, CLI |
| **Phase 2 (in progress)** | Batch A: menus/clipboard/drop/hot reload. Batch B: window state/sessions/page APIs/appearance/input regions. Batch C: recent docs/watcher/deep links/downloads/protocols/system/media/login. Batch D: updater/packaging descriptions/signing interfaces/single-instance forwarding |
| **Phase 3** | wgpu-based native UI renderer (flexbox/grid layout, widgets, SVG) — not started |
| **Phase 4** | Packaging (.app, .dmg, .msi, .deb, PKGBUILD, code signing, delta updates) |

## License

MIT

---

*Inspired by [mygo](https://github.com/egoist/mygo) — desktop apps in Go.*
