Browsers 是架构边界，不是受支持的 MyRust 目标。文档站有用于 UI 预览的 wasm demo，但 MyRust 窗口、WebView 内容、app 生命周期与平台集成都是 native-only。

`myrust-ui` 本身平台无关，可编译到 `wasm32-unknown-unknown`；这正是 `/docs-demo` 的基础。它不是浏览器应用 runtime。

## What works

UI crate 的布局、控件、`DisplayList` 与 CPU rasterizer 可编译到 wasm。文档宿主提供 `env.myrust_now_ms`，调用 `demo_render`，读取 RGBA 缓冲并转发 pointer/key/wheel/text 事件。没有 wasm-bindgen。

这证明 immediate-mode UI core 可经自定义宿主在浏览器运行；不提供 MyRust 窗口、router/native app 生命周期、WebView IPC、文件系统、菜单/tray、无障碍桥或 OS 集成。

## What does not

没有 browser/WebGL/WebGPU renderer、`myrust-core` 内的 canvas 集成、浏览器 event loop、JS interop 包或 WebAssembly app target。可选 wgpu backend 没有接到浏览器 surface。

今天没有 MyRust app 能作为网页发布。文档 demo 是专用 raw ABI + CPU raster 上传，不是把 native runtime 编译到 Web。

## Size

当前 docs demo wasm 约 1.31–1.37 MB（随最后构建变化），包含内嵌 DejaVu Sans 与 UI core。这是文档产物，不是最小应用 bundle；没有发布 tree-shaking、压缩策略或启动基准。

## Under the hood

wasm 模块导出 `demo_render`、`demo_buffer_ptr`、`demo_pointer`、`demo_wheel`、`demo_key`、`demo_text` 与 debug probe。只导入宿主时钟，用 `CpuRasterizer` 渲染，页面把 RGBA 复制进 canvas 的 `putImageData`。

浏览器内容请使用系统引擎驱动的 WebView 窗口。真正 browser-first 应用应使用 JS/WebGL 技术栈；MyRust native UI 不是该技术栈。
