Testing uses the same immediate-mode view without a window. `NativeUi::render(size)` runs one frame and returns a `ViewFrame` containing `display`, `layout`, `interactions`, `access_nodes`, `stats`, `frame_stats` and frame flags.

```rust
use myrust_core::ui::{column, text, NativeUi, Size};

let ui = NativeUi::new(|c| {
    column(c).children(|c| text(c, "ready"));
});
let frame = ui.render(Size::new(320.0, 180.0))?;
assert!(!frame.display.commands.is_empty());
assert_eq!(frame.access_nodes[0].role, myrust_core::ui::Role::Group);
```

## Windowless Tester

`Tester` owns a `NativeUi`, renders at a fixed logical size, and drives the same `UiEvent` values a desktop host sends. It finds elements by accessible label, stable key or value. Pointer actions first verify that the element has a non-empty on-screen paint and that no later interaction region covers the hit point.

```rust
use myrust_core::ui::{NativeUi, Size, Tester};

let mut name = String::new();
let ui = NativeUi::new(move |c| {
    c.text_field("name", &mut name);
    c.button("Save");
});
let mut tester = Tester::new(ui, Size::new(320.0, 180.0))?;
tester.type_into("name", "Linus")?;
tester.assert_focused("name")?;
tester.click("Save")?;
tester.assert_has_text("Linus")?;
tester.assert_texts(&["Save"])?;
```

The driver exposes `click`, `click_at`, `press`, `press_at`, `release`, `release_at`, `move_to`, `move_to_point`, `scroll`, `type_text`, `type_into`, `key` and `focus`. Errors distinguish missing and ambiguous lookups, non-interactive elements, off-screen or unpainted bounds, and occlusion by another interactive region. `elements()`, `frame()` and `texts()` expose snapshots for custom assertions.

The tester has no window, browser, display server or platform accessibility dependency. Input settles through the normal `NativeUi::render` path, so a click still spans a press frame and a release frame against the same previous interaction region.

## Golden frames

`golden` captures deterministic CPU pixels with `CpuRasterizer`, encodes ordinary RGBA PNG files, reads them with a built-in PNG decoder, and compares per-channel pixels with a tolerance:

```rust
use myrust_core::ui::{
    capture_cpu, Color, GoldenHarness, GoldenTolerance, RenderSize,
};

let image = capture_cpu(&frame.display, RenderSize::new(320, 180), Color::WHITE)?;
let harness = GoldenHarness::new("tests/golden");
harness.compare_or_update("ready-page", &image, GoldenTolerance::EXACT)?;
```

The checked-in corpus covers a complete control page, text, a shadow and a partial-damage replay. Run normal tests to compare. Set `UPDATE_GOLDEN=1` to rewrite the expected PNGs explicitly. A mismatch writes a sibling ending in `.actual.png`. `GoldenTolerance` controls the largest channel delta and the fraction of pixels allowed outside it; exact CPU goldens use zero tolerance.

When the `gpu` feature is compiled and an adapter exists, `capture_gpu` renders the same display list through offscreen wgpu and reads it back. Without an adapter the GPU tests skip. GPU parity uses `GoldenTolerance::ANTIALIASED` because CPU and GPU antialias glyph and path edges differently.

## Benchmark fixtures

`fixtures` provides one thousand labels, one million virtual-list rows and a large Markdown document. The stress tests are marked `#[ignore]` and use the existing test runner, not a new benchmark framework:

```bash
cargo test -p myrust-ui --release fixtures::tests -- --ignored --nocapture
```

`FrameStats` reports build, layout, paint and, after presentation, present time. The million-row fixture still provides a million-row corpus; the list implementation constructs only the visible row window each frame.

## What assistive technology sees

`ViewFrame::access_nodes` is the exact per-frame list a platform AT bridge consumes: `Role`, `label`, optional string `value`, `disabled`, `focused`, `key` and `bounds`. The tester reads it directly; the Linux runtime publishes the same list to AT-SPI.

Tests can assert roles, labels and values emitted by controls. AT-SPI, UIA and NSAccessibility are not part of the harness; see the Accessibility guide for the exact roles and the Linux bridge.

## Testing the built app

`cargo test --workspace` covers the UI crate and native runtime. `examples/gallery` renders all controls headlessly and asserts their labels. For real pixels, use the golden corpus and the fixtures above. Native window presentation itself still needs a host display; CI without one skips GPU readback.

There is no accessibility audit, fuzzer or UI recorder. Use `Tester` for behavior, `CpuRasterizer` plus `GoldenHarness` for deterministic pixels, and the public frame data for custom checks.
