`Direction` 为之后构造的控件选择 LTR 或 RTL。默认 `Direction::Ltr`；临时作用域结束后恢复原方向。

```rust
use myrust_core::ui::Direction;

c.with_direction(Direction::Rtl, |c| {
    row(c).gap(8.0).children(|c| {
        text(c, "One");
        text(c, "Two");
        text(c, "Three");
    });
});
```

## The direction

`Direction::Ltr` 与 `Direction::Rtl` 是公开值。`Context::direction()` 读取，`set_direction` 改变后续节点，`with_direction(direction, closure)` 限定作用域并在结束后恢复。

`Direction` 不是 locale，也不是 CSS 属性。它不改变文字 shaping、段落基础方向或数字格式；只驱动布局顺序与少量呈现默认值。

## What mirrors

RTL 下创建的 `row` 会反转 relative 子节点，因此第一个逻辑子项放在右侧。未显式指定对齐时文本默认 End，`icon(c, ...)` 把内置 SVG 图标在目标框内水平镜像。

overlay 和普通 column 保持物理位置；只有 row 子序与已记录默认值镜像。图标镜像只是 icon 命令中的 transform，不是单独 RTL 图标集。

## What does not

padding、margin、border 是物理边，不交换。滚动条位置、split-view 分隔条顺序、grid 列顺序、`left`/`top` 偏移与绝对定位都不翻转。

任意 `SvgPath`、图片、bidi 文本 run、标点、URL 与数字不镜像/重排。混合方向文本没有 shaping/bidi 算法；除非调用方拆分，renderer 按单一从左到右字符串布局。

## Trying it

本页 wasm demo 在 row 上切换 `Direction`。同一批控件在 `with_direction` 下重建；下一帧行序、默认文本对齐与图标镜像更新。

没有 RTL 专用主题或本地化。真实 RTL 界面应把 `Direction` 与翻译文本、显式对齐以及滚动条/装饰图的物理摆放结合。
