自定义控件由公开 builder 组合：container、row/column、`.draw(...)` 命令、标准控件与可聚焦区域。没有 `Widget` trait、基类、生命周期钩子或 retained component protocol。

```rust
use myrust_core::ui::{container, DrawCommand, Radius, Color};

container(c)
    .size(120.0, 44.0)
    .focusable()
    .draw([
        DrawCommand::RoundedRect {
            x: 0.0, y: 0.0, width: 120.0, height: 44.0,
            radius: Radius::all(8.0), color: Color::rgb(17, 24, 39),
        },
        DrawCommand::Text {
            x: 12.0, y: 12.0, text: "Custom".to_string(),
            font_size: 14.0, weight: 500,
            color: Color::rgb(226, 232, 240),
            max_width: Some(96.0), alignment: myrust_core::ui::TextAlignment::Start,
        },
    ]);
```

## Widgets of your own

builder 创建节点；`.draw` 给 container 附加原始命令，`.focusable()` 把 container 注册为一个焦点停靠点并使用默认 id。`Context::hover(id)` 可查询指针 hover，但没有公开的“该自定义节点是否被点击”查询。

点击/键盘行为可在自绘内组合标准 `button`/`checkbox`/控件，或在外部处理原始 `UiEvent`。没有 disabled 标志、builder 状态结构、测量协议或自定义 layout engine。

### Stopping what a widget started

没有取消句柄或 teardown 钩子。自定义控件通过下一帧不再构造而停止；其交互区域在上一帧后消失。它拥有的动画/状态必须由调用方丢弃或忽略。

标准控件的瞬态状态按字符串 id 存在 `Context`；`migrate_state` 可移动它，但除释放指针外没有公开 API 取消已开始的拖动/手势。

自定义 container 发出 `Role::Group`，label 为 `"container"`；原始 draw 命令不发出 AccessNode。自定义无障碍 role/name 不支持；需要语义时在里面放标准控件或 text 节点。
