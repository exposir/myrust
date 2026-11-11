文本在 CPU 上测量与换行，不做字形整形、字距调整或字体回退。`text(c, "...")` 创建 `TextBuilder`；`max_width` 在该宽度内启用换行。

```rust
use myrust_core::ui::text;

text(c, "A wrapped paragraph")
    .font_size(15.0)
    .weight(500)
    .color(myrust_core::ui::Color::rgb(226, 232, 240))
    .max_width(320.0);
```

## Style

`font_size`、`weight`、`bold`、`color`、`max_width` 与 `align` 是公开文本样式控制。`TextAlignment` 为 `Start`、`Center`、`End`；默认跟随当前 `Direction`（LTR 为 Start，RTL 为 End）。

`TextBuilder` 没有 italic 标志、下划线/装饰、line-height、letter-spacing 或 text transform。一个文本节点只有一个字号；混合样式需分开节点或使用 Markdown 行内 span。

## Rich text

`markdown(c, "id", source)` 渲染 1–6 级标题、段落、无序/有序列表和行内 code/bold/italic/link。`parse_markdown` 暴露解析后的 block 供自定义布局。

这是小子集，不是 CommonMark：不渲染 fenced code block、表格、任务列表、脚注、图片或 raw HTML。`RichText`/`editable_text` 是独立控件，没有与 Markdown 共享的 attributed-text 模型。

## Elements within a sentence

没有公开 inline-row API 能把任意控件放进可换行句子。`row` 可并排文字与控件，但不会按词边界换行，每个子节点保留自身固有宽度。

Markdown 内部 span 作为相邻文本节点布局。自定义行内行为需用 `container().draw(...)` 和测量位置自行绘制；围绕控件的自动换行不可用。

## Fonts

`FontRegistry::load_bytes` 加载 TTF/OpenType 字体；`default_font` 探测常见系统路径，CPU presenter 可通过 `CpuRasterizer::rasterize_with_font` 接收显式字体。wasm demo 内嵌 DejaVu Sans，不读系统路径。

UI 文本 API 没有逐控件 font-family 选择、字体回退或 shaping/kerning/bidi 重排。字体在 presenter/测试层选择，不由 `TextBuilder` 选择；缺字由 CPU presenter 计数。
