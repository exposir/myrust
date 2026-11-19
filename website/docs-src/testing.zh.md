测试用同一个 immediate-mode 视图，不需要窗口。`NativeUi::render(size)` 运行一帧并返回 `ViewFrame`，包含 `display`、`layout`、`interactions`、`access_nodes`、`stats`、`frame_stats` 与帧标志。

```rust
use myrust_core::ui::{column, text, NativeUi, Size};

let ui = NativeUi::new(|c| {
    column(c).children(|c| text(c, "ready"));
});
let frame = ui.render(Size::new(320.0, 180.0))?;
assert!(!frame.display.commands.is_empty());
assert_eq!(frame.access_nodes[0].role, myrust_core::ui::Role::Group);
```

## 无窗口 Tester

`Tester` 持有一个 `NativeUi`，按固定逻辑尺寸渲染，并注入桌面宿主机相同的 `UiEvent`。元素可按无障碍 label、稳定 key 或 value 查找。指针动作会先检查元素在屏幕内确实有绘制内容，且命中的位置没有被更上层的交互区域遮挡。

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

驱动接口包括 `click`、`click_at`、`press`、`press_at`、`release`、`release_at`、`move_to`、`move_to_point`、`scroll`、`type_text`、`type_into`、`key` 与 `focus`。错误会区分找不到、查找歧义、不可交互、屏幕外或未绘制、以及被其他交互区域遮挡。`elements()`、`frame()` 与 `texts()` 提供快照，便于自定义断言。

Tester 不依赖窗口、浏览器、显示服务器或平台无障碍桥。输入走正常的 `NativeUi::render` 路径，因此 click 仍由同一个上一帧交互区域上的 press 帧与 release 帧组成。

## Golden 帧

`golden` 用 `CpuRasterizer` 捕获确定性 CPU 像素，写成普通 RGBA PNG，并用内置 PNG 解码器读取；比较按通道逐像素进行，并支持 tolerance：

```rust
use myrust_core::ui::{
    capture_cpu, Color, GoldenHarness, GoldenTolerance, RenderSize,
};

let image = capture_cpu(&frame.display, RenderSize::new(320, 180), Color::WHITE)?;
let harness = GoldenHarness::new("tests/golden");
harness.compare_or_update("ready-page", &image, GoldenTolerance::EXACT)?;
```

仓库内 golden 覆盖完整控件页、文本、阴影与 partial damage 重绘。普通测试直接比较；设置 `UPDATE_GOLDEN=1` 才会显式重写期望 PNG。失败时会在同目录写出以 `.actual.png` 结尾的实际图。`GoldenTolerance` 控制单通道最大差值和允许超出该差值的像素比例；CPU 精确 golden 使用零容差。

编译 `gpu` feature 且有适配器时，`capture_gpu` 通过离屏 wgpu 渲染同一 display list 并读回像素；无适配器时 GPU 测试跳过。CPU 与 GPU 对文字和路径边缘的抗锯齿不同，GPU 对比使用 `GoldenTolerance::ANTIALIASED`。

## 基准 fixtures

`fixtures` 提供 1000 labels、百万行虚拟列表和大 Markdown 文档。压力测试标记为 `#[ignore]`，复用现有测试运行器，不引入 benchmark 框架：

```bash
cargo test -p myrust-ui --release fixtures::tests -- --ignored --nocapture
```

`FrameStats` 报告 build、layout、paint，以及在呈现后补录的 present 耗时。百万行 fixture 提供完整的百万行语料；列表实现每帧只构造可见行窗口。

## 辅助技术看到什么

`ViewFrame::access_nodes` 就是平台 AT 桥消费的逐帧列表：`Role`、`label`、可选字符串 `value`、`disabled`、`focused`、`key`、`bounds`。Tester 直接读取它；Linux runtime 把同一份列表发布到 AT-SPI。

测试可断言控件发出的 role/label/value。AT-SPI、UIA、NSAccessibility 不在 harness 范围内；具体 role 与 Linux bridge 见 Accessibility。

## 测试构建后的应用

`cargo test --workspace` 覆盖 UI crate 与 native runtime。`examples/gallery` 无窗口渲染所有控件并断言标签。真实像素使用上面的 golden 与 fixtures。原生窗口呈现本身仍需宿主显示环境；没有显示环境的 CI 会跳过 GPU 读回。

没有 accessibility audit、fuzzer 或 UI recorder。行为测试用 `Tester`，确定性像素用 `CpuRasterizer` + `GoldenHarness`，其他检查直接读公开帧数据。
