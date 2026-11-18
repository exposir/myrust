Browsers are an architectural boundary, not a supported MyRust target. The documentation site has a wasm demo for UI previews, but MyRust windows, WebView content, app lifecycle and platform integrations are native-only.

`myrust-ui` itself is platform-neutral and can be compiled to `wasm32-unknown-unknown`; that is what powers `/docs-demo`. It is not a browser application runtime.

## What works

The UI crate's layout, widgets and `DisplayList` compile to wasm. The docs host supplies `env.myrust_now_ms`, calls `demo_render`, reads the serialized display list, and forwards pointer/key/wheel/text events. The page paints that list with WebGL 2: one instanced quad per command through a signed-distance shader, while text, SVG and shadow coverage is rasterized on a 2D canvas and uploaded as textures. A browser without WebGL 2 falls back to the Canvas 2D renderer. No wasm-bindgen is used.

This proves the immediate-mode UI core can run in a browser via a custom host. It does not provide a MyRust window, router/native app lifecycle, WebView IPC, filesystem, menu/tray, accessibility bridge or OS integrations.

## What does not

There is no browser renderer inside `myrust-core`, no canvas integration, browser event loop, JS interop package or WebAssembly app target; the docs demo page's WebGL 2 presenter is not part of `myrust-core`. The optional wgpu backend is not wired to a browser surface.

No MyRust app can be shipped as a web page today. The docs demo is a purpose-built raw ABI plus a display-list replay, not the native runtime compiled for the web.

## Size

The current docs demo wasm is about 1.31–1.37 MB depending on the last build, including embedded DejaVu Sans and the UI core. This is a docs artifact, not a minimal application bundle; no tree-shaking, compression policy or startup benchmark is published.

## Under the hood

The wasm module exports `demo_render`, `demo_command_ptr`, `demo_command_len`, `demo_pointer`, `demo_wheel`, `demo_key`, `demo_text` and debug probes. It imports only the host clock and text measurement, and the page paints the returned display list with WebGL 2 (`?renderer=canvas` forces the Canvas 2D fallback).

For browser content, use a WebView window backed by the system engine. For a real browser-first app, use a JS/WebGL stack; MyRust native UI is not that stack.
