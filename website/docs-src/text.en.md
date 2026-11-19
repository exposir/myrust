Text is laid out on the CPU with OpenType shaping, UAX #14 line breaking, Unicode bidi ordering and ordered font fallback. `text(c, "...")` creates a `TextBuilder`; `max_width` turns wrapping on within that width.

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

`FontRegistry::load_bytes` loads a TTF/OpenType face. `default_font_stack` probes a primary sans face, a monospace face and bounded CJK/emoji/symbol fallback groups; `install_default_font_stack` supplies an explicit stack for hosts without system-font access. `default_font` still returns the primary face for metrics and compatibility. The wasm demo embeds its Geist face rather than reading a system path.

`layout_text_shaped` returns one shared `TextLayout` with visual lines, bidi/font runs and positioned glyphs. CPU rasterization, the GPU glyph atlas and damage bounds consume that same layout, so kerning and fallback cannot change the painted width behind the damage tracker. Explicit `family` selection is still not implemented; the host falls back through its ordered stack, and unsupported code points are counted instead of being hidden.

This batch has no vertical text, justified text, arbitrary OpenType feature controls or full fontconfig/CTFont discovery. The fallback list is fixed and platform-specific, and color-emoji bitmap fonts depend on what the selected outline face can rasterize.
