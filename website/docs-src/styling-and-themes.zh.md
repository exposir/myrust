样式是显式数据：颜色、圆角、内边距与绘制命令每帧传给控件。没有 stylesheet、选择器匹配或除 `Theme` 查找外的继承视觉规则。

```rust
use myrust_core::ui::{text, Theme};

let theme = Theme::dark();
text(c, "Save").color(theme.accent);
```

## State, not selectors

控件外观是传入值和调用方状态的函数。hover、focus 与选中态由控件内部处理，或在视图中分支；没有 `:hover` 之类的选择器、class 级联或 media query。

自定义状态需读取调用方状态并显式选择 `Color`/`DrawCommand`。可用 `mix_color` 做简单混合，`DrawCommand` 暴露 rect、rounded-rect、circle、line、gradient、path 与 text 原始操作。

## Themes

`Theme` 包含 `background`、`surface`、`foreground`、`muted`、`accent`、`on_accent`、`border`、`spacing`、`font_family` 与 `font_sizes`。`Theme::dark()`、`Theme::light()`、`Theme::system()` 构造内置主题。

通过 `NativeUi::with_theme(theme)` 传入。控件每帧从 `Context` 读取当前主题。没有公开 `Context::set_theme`；换主题需重建/替换 `NativeUi`，不能修改活动 context。

## The focus ring

拥有键盘焦点的控件用 `theme.accent` 绘制焦点环。环的几何与厚度是内部实现，没有公开 radius/width/offset 设置。

焦点颜色只能通过主题的 `accent` 改变；单个控件不能覆盖自己的焦点环。焦点在发出的 access node 中是布尔 `Role` 状态，不是独立样式选择器。

## A theme for part of the window

没有逐子树主题覆盖。控件从同一个 `Theme` 读取颜色，因此某个区域的不同外观需要显式传入 `Color` 值和自定义 `DrawCommand`。

可以克隆 `Theme` 并在该区域使用其字段构建局部调色板，但没有为 children 切换活动主题的 context 作用域。

## The desktop's preferences

native 目标上 `Theme::system()` 通过 `dark-light` 查询明暗偏好；wasm 构建因为没有平台外观探测，始终使用 `Theme::dark()`。没有公开的 OS 主题变化回调。

Reduce-motion 在创建 Context 时从 `MYRUST_REDUCE_MOTION` 读取；高对比度、透明度与强调色偏好未暴露。桌面可在启动时选择主题，但运行时偏好更新需要重建 UI。
