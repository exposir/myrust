Browsers 是架构边界，不是受支持的 MyRust 目标。文档站有用于 UI 预览的 wasm demo，但 MyRust 窗口、WebView 内容、app 生命周期与平台集成都是 native-only。

`myrust-ui` 本身平台无关，可编译到 `wasm32-unknown-unknown`；这正是 `/docs-demo` 的基础。它不是浏览器应用 runtime。

## What works

UI crate 的布局、控件与 `DisplayList` 可编译到 wasm。文档宿主提供 `env.myrust_now_ms`，调用 `demo_render`，读取序列化后的 display list，并转发 pointer/key/wheel/text 事件。页面用 WebGL 2 重放这份列表：每条命令一个 instanced quad，由 signed-distance shader 画圆角覆盖；text、SVG 与 shadow 的覆盖先用 2D canvas 光栅化，再作为纹理上传。没有 WebGL 2 的浏览器退回 Canvas 2D renderer。没有 wasm-bindgen。

这证明 immediate-mode UI core 可经自定义宿主在浏览器运行；不提供 MyRust 窗口、router/native app 生命周期、WebView IPC、文件系统、菜单/tray、无障碍桥或 OS 集成。

wasm 宿主通过 hidden textarea 提供 IME composition；native 窗口没有：tao 0.37 在 Linux、macOS、Windows 都不提供 preedit 流。可移植的宿主边界仍是 `UiEvent::CompositionStart/Update/Commit`。

## What does not

没有 `myrust-core` 内置的浏览器 renderer、canvas 集成、浏览器 event loop、JS interop 包或 WebAssembly app target；文档 demo 页自己的 WebGL 2 presenter 不属于 `myrust-core`。可选 wgpu backend 没有接到浏览器 surface。

今天没有 MyRust app 能作为网页发布。文档 demo 是专用 raw ABI + CPU raster 上传，不是把 native runtime 编译到 Web。

## Size

当前 docs demo wasm 约 1.31–1.37 MB（随最后构建变化），包含内嵌 DejaVu Sans 与 UI core。这是文档产物，不是最小应用 bundle；没有发布 tree-shaking、压缩策略或启动基准。

## Under the hood

wasm 模块导出 `demo_render`、`demo_command_ptr`、`demo_command_len`、`demo_pointer`、`demo_wheel`、`demo_key`、`demo_text` 与 debug probe。只导入宿主时钟与文本测量，页面用 WebGL 2 重放返回的 display list（`?renderer=canvas` 强制退回 Canvas 2D）。

浏览器内容请使用系统引擎驱动的 WebView 窗口。真正 browser-first 应用应使用 JS/WebGL 技术栈；MyRust native UI 不是该技术栈。
