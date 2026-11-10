# Examples

Small runnable apps, one idea each. They follow the
[mygo examples](https://github.com/egoist/mygo/tree/main/examples) where the
current MyRust API can express them.

Run any of them from the workspace root:

```bash
cargo run -p example-hello
```

## WebView windows

| Example | Crate | Shows |
|---|---|---|
| Hello | `example-hello` | A page calling one typed Rust method |
| Todo | `example-todo` | Typed IPC in both directions, JSON persistence under the data directory |
| Drag and drop | `example-drag-drop` | Native file-drop events forwarded into page JavaScript |

## Native UI windows

MyRust draws these itself with the CPU presenter; there is no web engine in the
process.

| Example | Crate | Shows |
|---|---|---|
| Counter | `example-counter-native` | A view rebuilt every frame from Rust state, with buttons |
| Clipboard | `example-clipboard` | Text fields plus system clipboard read/write |
| Text input | `example-text-input` | Text field, checkbox, slider, and button state |
| Loading | `example-loading` | A frame-driven animation without a web page |
| Streaming chart | `example-chart-streaming` | `DisplayList` lines redrawn from animated state |
| Gallery | `example-gallery` | Built-in widgets and raw drawing commands |

## Coverage against mygo

Nine of mygo's 38 examples have a direct equivalent. The rest are not stubs:
they need capabilities the current API does not provide.

| Not ported | Reason |
|---|---|
| `blackhole`, `wind`, `chat-glow`, `liquid-glass`, `effort-slider` | Shader and effect rendering; the native presenter draws geometry and text only. |
| `farm`, `mayoi` | Game loops, sprites, and collision input beyond the current widget set. |
| `piano`, `video`, `terminal` | No audio, video, or PTY backends. |
| `notes`, `documents`, `code-editor`, `text-buffer`, `markdown-chat` | No rich-text document model; the native text field is single-line. |
| `launcher`, `frameless`, `flyout`, `menubar-flyout`, `menubar-native`, `permissionflow` | No frameless drag regions, popover/panel windows, menu-bar-only mode, or permission APIs. Tray icons and global shortcuts exist, but the window behaviors around them do not. |
| `embed-webview` | A window hosts either a WebView or a native UI, not both. |
| `native-module` | No dynamic native-module loading. |
| `rust-native` | The host language is already Rust; every example calls into the core crate directly. |
| `vibrancy`, `sf-symbols` | macOS-only appearance APIs that are not exposed. |
| `confetti`, `timeline` | Animation is immediate-mode and frame-driven; there are no pose or timeline composition primitives. |
| `native` | Menus, tray, dialogs, notifications, and shortcuts exist individually, but there is no combined desktop-integration tour yet. |
