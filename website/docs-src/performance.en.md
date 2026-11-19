Each produced frame stores wall-clock `FrameStats { build_ms, layout_ms, paint_ms, present_ms, total_ms }`. `ViewFrame::frame_stats` describes the frame just returned; `Context::frame_stats()` inside a view returns the previous sample, and `frame_stats_panel` renders it.

```rust
let stats = c.frame_stats();
text(c, &format!("{:.2} ms build, {:.2} ms layout", stats.build_ms, stats.layout_ms));
frame_stats_panel(c, "stats", stats);
```

## What a frame costs

`build_ms` covers the view callback and the optional preview wrapper. `layout_ms` covers `compute_layout`. `paint_ms` covers display-list construction, interaction/access collection and command statistics.

`NativeUi::render` returns `present_ms = 0` and a `total_ms` covering build, layout and paint. The desktop runtime adds CPU rasterization, GPU surface work and softbuffer presentation to `present_ms` and `total_ms` before reporting. Compositor work after presentation is still outside the measurement.

The fields are `f32` milliseconds and are not averaged or smoothed. A consumed-input frame can run two build/layout/paint passes inside one `NativeUi::render` call; the returned frame describes the settled pass, not the sum.

## Runtime frame statistics

Set `MYRUST_FRAME_STATS` to report the measured desktop frame to stderr:

```bash
MYRUST_FRAME_STATS=all cargo run -p examples-gallery
MYRUST_FRAME_STATS=8.0 cargo run -p examples-gallery
MYRUST_FRAME_STATS=4ms cargo run -p examples-gallery
```

`all` emits every presented frame. A bare number or a value ending in `ms` emits frames whose measured total is at least that threshold. `off`, an empty value, a negative value or an invalid value disables reporting. The output is one line:

```text
myrust: Frame 8.42 ms  build 2.10  layout 0.30  paint 1.80  present 4.22
```

The environment variable only controls the observer. It does not alter layout, paint, batching, vsync or renderer selection.

## Retained memory

There is no memory API, retained-memory counter or heap profiler in `myrust-ui`. `Context` retains widget state, animations, overlays, access-node metadata and previous interaction regions; display lists and layout results are rebuilt each pass.

Font glyph caches, image buffers and GPU resources are outside `FrameStats`. Measure them with platform allocators or GPU tools; there is no public budget or eviction policy.

## Measurement scope

The measurements are wall-clock CPU time plus the runtime's presentation interval. There is no per-widget attribution, allocation count, per-command cost, GPU timestamp query, percentile history or frame-deadline API. `FrameStats::accumulate` sums samples when a caller keeps history. A hidden or occluded window may skip GPU presentation work, so compare only samples with the same visibility and backend.

## Benchmark fixtures

The default test run checks behavior. Three ignored stress fixtures use the existing test runner and no benchmark dependency:

```bash
cargo test -p myrust-ui --release fixtures::tests -- --ignored --nocapture
```

The fixtures cover one thousand simultaneously visible labels, a one-million-row virtual list that constructs only the visible window, and a large Markdown document with headings, paragraphs, lists, code and tables. Each test renders several frames with `FrameStats::accumulate` and prints the aggregate sample.

`bench_cpu_damage_and_cache_work` is a measurement-only ignored fixture for the CPU fallback. Run it with `cargo test -p myrust-ui fixtures::tests::bench_cpu_damage_and_cache_work -- --ignored --nocapture`. It prints full versus damaged command counts and rasterized pixels, plus process-wide glyph and text-layout cache hits/misses. It has no pass/fail threshold; compare runs on the same host, size and release profile.

These are local stress measurements, not a CI performance gate. Results depend on the host, the loaded system font and the release profile. Use a fixed window size and compare like-for-like runs.

The CPU fallback reuses the previous `CpuFrame` and rasterizes only `render::damage::diff` rectangles. `RenderStats::rasterized_pixels` and `rasterized_commands` report actual work; unchanged frames report zero. Resize, background changes, or full damage repaint the whole buffer, while the softbuffer copy remains whole-buffer.

Shaped text layouts are cached by text, size, wrap width, alignment, mono face and font-stack generation. The cache is bounded and clears at capacity; `text_layout_cache_stats()` exposes hit, miss and entry counts. A new font stack has a new generation, so stale layouts are not reused.

CPU glyph bitmaps use a bounded cache keyed by font-stack generation, face id, glyph id, size and synthetic bold/italic flags. `glyph_cache_stats()` exposes hit, miss and entry counts; disabling it with the test hook produces the same pixels. `MYRUST_FRAME_STATS` includes glyph and text-layout hit/miss totals.
