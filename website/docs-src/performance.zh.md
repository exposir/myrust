每个产出的帧保存 wall-clock 测量的 `FrameStats { build_ms, layout_ms, paint_ms, present_ms, total_ms }`。`ViewFrame::frame_stats` 描述刚返回的帧；视图内 `Context::frame_stats()` 返回上一帧样本，`frame_stats_panel` 渲染它。

```rust
let stats = c.frame_stats();
text(c, &format!("{:.2} ms build, {:.2} ms layout", stats.build_ms, stats.layout_ms));
frame_stats_panel(c, "stats", stats);
```

## 一帧的成本

`build_ms` 覆盖视图 callback 与可选的 preview 包装；`layout_ms` 覆盖 `compute_layout`；`paint_ms` 覆盖 display-list 构建、interaction/access 收集与命令统计。

`NativeUi::render` 返回时 `present_ms = 0`，`total_ms` 覆盖 build、layout 与 paint。桌面 runtime 在报告前把 CPU 光栅化、GPU surface 工作与 softbuffer present 加进 `present_ms` 和 `total_ms`；present 之后的 compositor 工作仍不在范围内。

字段是 `f32` 毫秒，不做平均或平滑。消费输入时一次 `NativeUi::render` 内部可能运行两次 build/layout/paint pass；返回帧描述最终稳定 pass，而不是两 pass 之和。

## 运行时统计

设置 `MYRUST_FRAME_STATS` 可把桌面帧耗时输出到 stderr：

```bash
MYRUST_FRAME_STATS=all cargo run -p examples-gallery
MYRUST_FRAME_STATS=8.0 cargo run -p examples-gallery
MYRUST_FRAME_STATS=4ms cargo run -p examples-gallery
```

`all` 每帧输出。纯数字或以 `ms` 结尾的值作为阈值，只输出总耗时不低于该值的帧。`off`、空值、负数或非法值关闭输出。输出是一行：

```text
myrust: Frame 8.42 ms  build 2.10  layout 0.30  paint 1.80  present 4.22
```

环境变量只控制观测，不改变 layout、paint、batching、vsync 或 renderer 选择。

## 保留内存

`myrust-ui` 没有 memory API、retained-memory counter 或 heap profiler。`Context` 保留 widget state、animations、overlays、当前 access-node 元数据与上一帧交互区域；display list 与 layout result 每 pass 重建。

字体 glyph cache、image buffer 与 GPU 资源不在 `FrameStats` 内。用平台 allocator 或 GPU 工具测量；没有公开预算或 eviction policy。

## 测量范围

测量是 wall-clock CPU 时间加 runtime 的呈现区间。没有 per-widget attribution、分配次数、逐命令耗时、GPU timestamp query、percentile 历史或 frame-deadline API。调用方自行保留历史时可用 `FrameStats::accumulate` 求和。隐藏或完全被遮挡的窗口可能跳过 GPU 呈现工作，因此只比较相同可见状态与 backend 的样本。

## 基准 fixtures

默认测试只检查行为。三个 ignored 压力 fixture 复用现有测试运行器，不引入 benchmark 依赖：

```bash
cargo test -p myrust-ui --release fixtures::tests -- --ignored --nocapture
```

fixture 覆盖 1000 个同时可见 labels、每帧只构造可见窗口的百万行虚拟列表，以及包含标题、段落、列表、代码与表格的大 Markdown 文档。每个测试渲染多帧并用 `FrameStats::accumulate` 累积，打印聚合样本。

`bench_cpu_damage_and_cache_work` 是只用于测量的 ignored fixture，针对 CPU 回退。运行 `cargo test -p myrust-ui fixtures::tests::bench_cpu_damage_and_cache_work -- --ignored --nocapture`，会打印 full 与 damaged 的命令数、光栅像素数，以及进程级 glyph/text-layout cache 命中与未命中。它没有 pass/fail 阈值；应在相同宿主、尺寸与 release profile 下比较。

这些是本地压力测量，不是 CI 性能门禁。结果依赖宿主、加载的系统字体和 release profile；比较时固定窗口尺寸并使用相同条件。

CPU 回退会复用上一帧 `CpuFrame`，只光栅化 `render::damage::diff` 给出的矩形。`RenderStats::rasterized_pixels` 与 `rasterized_commands` 记录实际工作量；无变化帧为 0。resize、背景变化或 full damage 会整帧重绘，softbuffer 上屏仍是整缓冲拷贝。

Shaped 文本布局按文本、字号、换行宽度、对齐、等宽字体与字体栈 generation 缓存。缓存有容量上限，达到后清空；`text_layout_cache_stats()` 暴露命中、未命中与条目数。新字体栈使用新 generation，不会复用旧布局。

CPU 字形位图使用有界缓存，key 为 font-stack generation、face id、glyph id、字号与合成加粗/斜体标志。`glyph_cache_stats()` 暴露命中、未命中与条目数；测试钩子关闭缓存后像素结果一致。`MYRUST_FRAME_STATS` 会附带 glyph 与 text-layout 的命中/未命中总数。
