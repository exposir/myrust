测试用同一个 immediate-mode 视图，不需要窗口。`NativeUi::render(size)` 运行一帧并返回 `ViewFrame`，包含 `display`、`layout`、`interactions`、`access_nodes`、`stats` 与帧标志。

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

没有 `Tester`、element handle 或脚本化 UI runner。测试构造视图，通过 `NativeUi::input` 注入 `UiEvent`，调用 `render`，再断言返回的 `ViewFrame`：交互 bounds、绘制命令、access nodes、焦点与帧统计。

输入按帧合并；click 需要 press 帧与 release 帧落在同一上一帧交互区域。内部测试可用 `advance_frame_time_for_test`，公开测试应驱动真实 `UiEvent` 并检查输出。

## What assistive technology sees

`ViewFrame::access_nodes` 就是平台 AT 桥会消费的逐帧列表：`Role`、`label`、可选字符串 `value`、`disabled`、`focused`、`bounds`。测试 harness 没有语义树、action API 或平台桥。

测试可断言控件发出的 role/label/value。不能测试 AT-SPI/UIA/NSAccessibility，因为没有连接桥；具体 role 与缺失状态见 Accessibility。

## Testing the built app

`cargo test --workspace` 覆盖 UI crate 与 native runtime。`examples/gallery` 无窗口渲染所有控件并断言标签。真实像素由文档站 wasm + Chromium/CDP 验证；native 窗口仍需显示环境，CI 无头不能直接单测。

没有内置 screenshot diff、accessibility audit、fuzzer 或 UI recorder。可用 `CpuRasterizer` 做确定性帧像素，用公开 `ViewFrame` 做行为断言。
