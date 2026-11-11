MyRust 每帧在 CPU 上布局。容器是 flex 树节点，控件是带固有尺寸的叶子。没有保留式布局树，也没有 CSS。

```rust
use myrust_core::ui::{column, container, row, text};

column(c).padding(16.0).gap(8.0).children(|c| {
    row(c).gap(8.0).children(|c| {
        text(c, "Left");
        text(c, "Right");
    });
});
```

## Containers

`column`、`row`、`container` 和 `grid` 创建 flex 或等宽 grid 作用域。子节点在 `.children(|c| ...)` 内构建。`container` 是用于自绘的中性作用域；`grid().columns(n)` 启用简单的等宽 grid。

除这些 builder 外没有通用 group 渲染对象。overlay 由锚点内部定位，不是公开的绝对定位 builder。

## Sizes

builder 的 `width`、`height`、`size`、`fill` 和 `flex_grow` 设置显式尺寸、填满可用尺寸和主轴增长。未显式设置尺寸的叶子使用固有内容尺寸。

公开 builder 不暴露 `min_width`、`max_width`、`min_height`、`max_height`；`Style` 有这些字段，自定义代码需直接构造 `Style`，标准 builder 方法不提供。

## Spacing

`.padding(n)`、`.margin(n)` 和 `.gap(n)` 取一个值并应用到四边。这是已实现的间距模型。

builder 不暴露逐边 padding/margin 或逻辑 start/end 间距，尽管内部 `Edges` 支持分别设置。RTL 会反转行顺序，但不会交换物理 padding。

## Alignment

`.align(AlignItems)` 设置交叉轴对齐，`.justify(JustifyContent)` 分配主轴空间，`.center()` 双轴居中。`AlignItems` 与 `JustifyContent` 是公开枚举。

没有基线对齐、逐项 space-between 或 grid 对齐控制。grid 列等宽。

## Positioning

所有 builder 子节点参与正常流（`Position::Relative`）。`Position::Absolute`、`left`、`top` 存在于 `Style`，但没有公开绝对定位 builder；overlay、popover 与 dialog 使用内部绝对路径。

`column`/`row`/`container` 没有公开 x/y 偏移、滚动锚定或 sticky 定位。

## Paint order

绘制命令按构造/根顺序发出，overlay 最后在自己的层绘制。后构造的兄弟覆盖先构造的。没有公开 `z-index`；分层由 overlay layer 编号和根构造顺序控制。

命中测试使用上一帧交互区域并遵守 overlay 层。绘制顺序与命中顺序由同一层规则关联，不是独立的 CSS stacking context。

## Clipping and visibility

`scroll_view` 裁剪 viewport，并通过滚轮、拖条和键盘滚动偏移内容。`DrawCommand::ScissorPush`/`ScissorPop` 可通过自定义 container 的 `.draw(...)` 发出，CPU presenter 会应用，但没有更高级的 clip builder。

可见性通过不构造控件表达。没有 `visible(false)` 标志、opacity 属性或 CSS 式 hidden 状态；隐藏内容应省略构建，或用背景覆盖。
