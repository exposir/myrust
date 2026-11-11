Text is measured and wrapped on the CPU without shaping, kerning or font fallback. `text(c, "...")` creates a `TextBuilder`; `max_width` turns wrapping on within that width.

```rust
use myrust_core::ui::text;

text(c, "A wrapped paragraph")
    .font_size(15.0)
    .weight(500)
    .color(myrust_core::ui::Color::rgb(226, 232, 240))
    .max_width(320.0);
```

## Style

`font_size`, `weight`, `bold`, `color`, `max_width` and `align` are the public text style controls. `TextAlignment` is `Start`, `Center` or `End`; the default follows the current `Direction` (Start in LTR, End in RTL).

The implementation has no italic flag on `TextBuilder`, no underline/decoration, no line-height, letter-spacing or text transform. There is one font size per text node; mixed styles require separate nodes or Markdown inline spans.

## Rich text

`markdown(c, "id", source)` renders headings 1–6, paragraphs, unordered/ordered lists and inline code/bold/italic/links. `parse_markdown` exposes parsed blocks for custom layout.

This is a small subset, not CommonMark: it does not render fenced code blocks, tables, task lists, footnotes, images or raw HTML. `RichText`/`editable_text` are separate controls; there is no attributed-text model shared with Markdown.

## Elements within a sentence

There is no public inline-row API that places arbitrary widgets inside a wrapped sentence. A `row` can place text and controls side by side, but the row does not wrap at word boundaries and each child keeps its own intrinsic width.

Markdown internal spans are laid out as adjacent text nodes. For custom inline behavior, draw the pieces yourself with `container().draw(...)` and measured positions; automatic wrapping around controls is not available.

## Fonts

`FontRegistry::load_bytes` loads a TTF/OpenType face; `default_font` probes common system paths and the CPU presenter can be given an explicit face with `CpuRasterizer::rasterize_with_font`. The wasm demo embeds DejaVu Sans rather than reading a system path.

The UI text API has no per-widget font-family selection, no font fallback and no shaping/kerning/bidi reordering. A face is selected at the presenter/test level, not by `TextBuilder`; a missing glyph is counted by the CPU presenter.
