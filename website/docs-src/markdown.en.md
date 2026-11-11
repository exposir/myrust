`markdown(c, "readme", &app.source)` parses basic Markdown each frame and lays it out with text and row nodes. The renderer is deliberately small: headings 1–6, paragraphs, unordered and ordered lists, and inline code, bold, italic and links.

```rust
use myrust_core::ui::{markdown, scroll_view};

scroll_view(c, "readme-scroll", 420.0, |c| {
    markdown(c, "readme", &app.source);
});
```

`parse_markdown` exposes the block list separately. Paragraph continuation lines are joined with spaces. There are no fenced code blocks, tables, task lists, footnotes or raw HTML; those remain outside the implemented subset.

## Accessibility

The Markdown root is a `Role::Group` labelled by the stable id, with a value such as `"12 blocks"`. Every inline span is a separate `Role::Label` whose value is `"plain"`, `"bold"`, `"italic"`, `"code"` or `"link"`. This is metadata only; the platform AT bridge is still out of scope.
