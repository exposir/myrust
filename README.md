# myrust

**Desktop apps in Rust, with a web frontend or native WebView.**

myrust is a Rust framework for building cross-platform desktop applications. Each window runs a **web page** in the system's native WebView — WKWebView on macOS, WebKitGTK on Linux, WebView2 on Windows. Build the frontend with any web framework; it calls your Rust code through typed IPC with auto-generated TypeScript bindings.

> Phase 1 delivers the web-viewer foundation: multi-window management, typed IPC, desktop APIs, and a CLI tool. A native GPU-rendered UI toolkit (wgpu) is planned for Phase 3.

## Features

- **Pure Rust, no unsafe code** — the core crate is `#![forbid(unsafe_code)]`.
- **System WebView** — uses the OS-native renderer; small binaries, low memory.
- **Typed IPC** — register Rust handlers, generate TypeScript clients automatically.
- **Desktop APIs** — system tray, notifications, file dialogs, global shortcuts, single-instance lock.
- **Dual mode** — same app runs as a native desktop window or a web server.
- **Hot reload** — watches frontend assets and reloads WebViews on change.
- **Workspace structure** — `myrust-core` library, `myrust-cli` tool, and standalone examples.

## Getting started

```bash
# Build the dashboard example
cargo build -p dashboard

# Run in browser mode
cargo run -p dashboard -- --web
# Open http://0.0.0.0:8090

# Run in desktop mode (native WebView window)
cargo run -p dashboard

# Generate TypeScript declarations
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
│       ├── app.rs          # App lifecycle, multi-window manager
│       ├── webview.rs      # wry WebView wrapper
│       ├── ipc/
│       │   ├── mod.rs      # IPC router + JS bridge
│       │   └── gen.rs      # TypeScript declaration generator
│       └── desktop/
│           ├── tray.rs     # System tray icon + menu
│           ├── notify.rs   # Desktop notifications
│           ├── dialog.rs   # File open/save + message dialogs
│           ├── shortcut.rs # Global keyboard shortcuts
│           └── instance.rs # Single-instance lock
├── myrust-cli/             # CLI (dev server, build)
├── examples/dashboard/     # Dashboard example
└── frontend/               # Web frontend assets
```

## IPC example

**Rust** — register a typed handler:

```rust
use myrust_core::ipc::{IpcRouter, IpcType};

// Types are automatically derived for serde + TS generation
#[derive(Serialize, Deserialize)]
struct Stats { cpu: f64, memory: f64 }

impl IpcType for Stats {
    const TS_NAME: &'static str = "Stats";
    const TS_DECLARATION: &'static str = r#"
export interface Stats { cpu: number; memory: number }
"#;
}

// Register on the router
router.register::<NoRequest, Stats, _>(
    "get_stats",
    "Returns system stats.",
    |_| Ok(Stats { cpu: 42.0, memory: 66.0 }),
)?;
```

**JavaScript** — call from the WebView:

```js
const stats = await window.__myrust_ipc__.invoke("get_stats");
console.log(stats.cpu); // 42.0

// Listen for Rust-pushed events
window.__myrust_ipc__.on("stats_updated", (data) => { /* ... */ });
```

The generated **TypeScript declaration** (`frontend/myrust.d.ts`):

```typescript
export const getStats: () => Promise<Stats>;
```

## Desktop APIs

```rust
// System tray
let tray = TrayBuilder::new("app-icon.png")
    .on_click(|event| println!("{event:?}"))
    .build()?;

// Notification
notify::send("Hello", "myrust is running", None)?;

// File dialog
let path = dialog::open_file(Some("Select a file"))?;

// Global shortcut
let shortcut = GlobalShortcut::register("Ctrl+Shift+D", || {
    println!("triggered");
})?;

// Single instance
let guard = SingleInstanceGuard::lock("myrust-app")?;
```

## Project status

**Phase 1** — foundation complete. The core crate, WebView wrapper, typed IPC, desktop APIs, CLI, and dashboard example compile and run on all three platforms.

### Roadmap

| Phase | Focus |
|-------|-------|
| **✓ Phase 1** | Multi-window WebView, typed IPC, desktop APIs, CLI |
| **Phase 2** | Menu system, drag & drop, deep links, auto-updater |
| **Phase 3** | wgpu native UI renderer (flexbox/grid layout, widgets, SVG) |
| **Phase 4** | Packaging (.app, .dmg, .msi, .deb, PKGBUILD) |

## License

MIT
