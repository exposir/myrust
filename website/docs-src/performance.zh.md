性能数据刻意很窄：每个产出的帧保存 wall-clock 测量的 `FrameStats { layout_ms, paint_ms, total_ms }`。`Context::frame_stats()` 返回上一帧样本；`frame_stats_panel` 渲染它。

```rust
let stats = c.frame_stats();
text(c, &format!("{:.2} ms layout", stats.layout_ms));
frame_stats_panel(c, "stats", stats);
```

## What a frame costs

`layout_ms` 覆盖 `compute_layout`；`paint_ms` 覆盖 display-list 构建、interaction/access 收集与统计；`total_ms` 覆盖两者及其开销。字段是 `f32` 毫秒，不做平均或平滑。

不包含宿主呈现、softbuffer/GPU upload、compositor、启动时字体发现或输入翻译。适合比较视图/布局工作，不是端到端延迟。

## Retained memory

`myrust-ui` 没有 memory API、retained-memory counter 或 heap profiler。`Context` 保留 widget state、animations、overlays、当前帧 access nodes 与上一帧交互区域；display list/layout 每帧重建。

字体 glyph cache、image buffer 与 GPU 资源不在 `FrameStats` 内。用平台 allocator/GPU 工具测量；没有公开预算或 eviction policy。

## Measurement scope

`FrameStats` 按每次 `produce` 调用记录，因此 `NativeUi::render` 消费输入时可能记录最多两次 pass。视图内读取 `Context::frame_stats` 得到上一帧样本。

只测 wall-clock CPU 时间。没有 per-widget attribution、分配次数、命令计数耗时、GPU timestamp query 或 percentile 历史。调用方自行保留历史时可用 `FrameStats::accumulate` 求和。

## Run the benchmark fixtures

没有 benchmark crate、Criterion target、fixture corpus 或 CI performance gate。`cargo test --workspace` 覆盖行为，gallery 渲染所有控件；`/docs-demo` 证明 wasm CPU 渲染，但不是 benchmark。

可重复本地检查：循环渲染固定 `NativeUi`，用 `FrameStats::accumulate` 累积，自己报告 min/mean/max。使用 release profile 与固定窗口尺寸；结果仍依赖宿主与字体。
