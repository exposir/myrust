Testing uses the same immediate-mode view without a window. `NativeUi::render(size)` runs one frame and returns a `ViewFrame` containing `display`, `layout`, `interactions`, `access_nodes`, `stats` and frame flags.

```rust
use myrust_core::ui::{column, text, NativeUi, Size};

let ui = NativeUi::new(|c| {
    column(c).children(|c| text(c, "ready"));
});
let frame = ui.render(Size::new(320.0, 180.0))?;
assert!(frame.display.commands.len() > 0);
assert_eq!(frame.access_nodes[0].role, myrust_core::ui::Role::Group);
```

## What a tester does

There is no `Tester`, element handle or scripted UI-runner API. A test constructs the view, injects `UiEvent` values through `NativeUi::input`, calls `render`, then asserts on the returned `ViewFrame`: interaction bounds, display commands, access nodes, focus and frame stats.

Input is coalesced per frame; a click requires a press frame and a release frame over the same previous interaction region. Tests can use `advance_frame_time_for_test` internally, but public tests should drive real `UiEvent`s and inspect outputs.

## What assistive technology sees

`ViewFrame::access_nodes` is the exact per-frame list a platform AT bridge consumes: `Role`, `label`, optional string `value`, `disabled`, `focused`, `key` and `bounds`. The test harness reads it directly; the Linux runtime publishes the same list to AT-SPI.

Tests can assert roles/labels/values emitted by controls. AT-SPI/UIA/NSAccessibility are not part of the harness; see the Accessibility guide for the exact roles and the Linux bridge.

## Testing the built app

`cargo test --workspace` covers the UI crate and native runtime. `examples/gallery` renders all controls headlessly and asserts their labels. For real pixels, the docs use a wasm build plus Chromium/CDP; native windows still require a host display and are not unit-testable in CI without one.

There is no built-in screenshot diff, accessibility audit, fuzzer or UI recorder. Use `CpuRasterizer` for deterministic frame pixels and the public `ViewFrame` for behavior assertions.
