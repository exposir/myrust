Browsers are an architectural boundary, not a supported MyRust target. The documentation site has a wasm demo for UI previews, but MyRust windows, WebView content, app lifecycle and platform integrations are native-only.

`myrust-ui` itself is platform-neutral and can be compiled to `wasm32-unknown-unknown`; that is what powers `/docs-demo`. It is not a browser application runtime.

## What works

The UI crate's layout, widgets, `DisplayList` and CPU rasterizer compile to wasm. The docs host supplies `env.myrust_now_ms`, calls `demo_render`, reads the RGBA buffer and forwards pointer/key/wheel/text events. No wasm-bindgen is used.

This proves the immediate-mode UI core can run in a browser via a custom host. It does not provide a MyRust window, router/native app lifecycle, WebView IPC, filesystem, menu/tray, accessibility bridge or OS integrations.

## What does not

There is no browser/WebGL/WebGPU renderer, canvas integration in `myrust-core`, browser event loop, JS interop package or WebAssembly app target. The optional wgpu backend is not wired to a browser surface.

No MyRust app can be shipped as a web page today. The docs demo is a purpose-built raw ABI plus a CPU raster upload, not the native runtime compiled for the web.

## Size

The current docs demo wasm is about 1.31–1.37 MB depending on the last build, including embedded DejaVu Sans and the UI core. This is a docs artifact, not a minimal application bundle; no tree-shaking, compression policy or startup benchmark is published.

## Under the hood

The wasm module exports `demo_render`, `demo_buffer_ptr`, `demo_pointer`, `demo_wheel`, `demo_key`, `demo_text` and debug probes. It imports only the host clock, renders with `CpuRasterizer`, and the page copies RGBA into a canvas with `putImageData`.

For browser content, use a WebView window backed by the system engine. For a real browser-first app, use a JS/WebGL stack; MyRust native UI is not that stack.
