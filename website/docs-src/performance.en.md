Performance data is deliberately narrow: each produced frame stores `FrameStats { layout_ms, paint_ms, total_ms }` measured with wall-clock time. `Context::frame_stats()` returns the last sample; `frame_stats_panel` renders it.

```rust
let stats = c.frame_stats();
text(c, &format!("{:.2} ms layout", stats.layout_ms));
frame_stats_panel(c, "stats", stats);
```

## What a frame costs

`layout_ms` covers `compute_layout`; `paint_ms` covers display-list construction, interaction/access collection and stats; `total_ms` covers both plus their overhead. The fields are `f32` milliseconds and are not averaged or smoothed.

The timings do not include host presentation, softbuffer/GPU upload, compositor work, font discovery at startup or input translation. They are useful for comparing view/layout work, not for end-to-end latency.

## Retained memory

There is no memory API, retained-memory counter or heap profiler in `myrust-ui`. `Context` retains widget state, animations, overlays, access nodes for the current frame and previous interaction regions; display lists/layout are rebuilt.

Font glyph caches, image buffers and GPU resources are outside `FrameStats`. Measure them with platform allocators/GPU tools; there is no public budget or eviction policy.

## Measurement scope

`FrameStats` is per `produce` call, so `NativeUi::render` may record up to two passes when input is consumed. Reading `Context::frame_stats` inside a view returns the previous frame's sample.

Only wall-clock CPU time is measured. There is no per-widget attribution, allocation count, command-count timing, GPU timestamp query or percentile history. `FrameStats::accumulate` sums samples if a caller keeps its own history.

## Run the benchmark fixtures

No benchmark crate, Criterion target, fixture corpus or CI performance gate exists. `cargo test --workspace` covers behavior, and the gallery renders all controls; `/docs-demo` proves wasm CPU rendering but is not a benchmark.

For a repeatable local check, render a fixed `NativeUi` in a loop, accumulate `FrameStats` with `FrameStats::accumulate`, and report min/mean/max yourself. Use the release profile and a fixed window size; results remain host- and font-dependent.
