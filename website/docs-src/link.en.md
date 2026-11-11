`link(c, "id", "label")` is a clickable text control, not a URL opener. It returns `LinkState { clicked, focused, hovered }`; the caller decides what to do.

```rust
if link(c, "docs", "Read the docs").clicked {
    app.open_link("https://example.com/docs");
}
```

## Paths in a router

There is no built-in link-to-route integration. A link returns only `clicked`; for a router, call `RouterState::push` from the click handler and render the appropriate page.

URL parsing, target pages, external browser launching and visited state are caller responsibilities. The control does not store an href.

## Within a sentence

Links are standalone `Button` nodes, not inline spans in a paragraph. A `row` can place text and a link side by side, but text does not wrap around the link; Markdown inline links are rendered by `markdown`, not by `link`.

## Other actions

Enter activates a focused link; hover/focus paints an underline and accent color. There is no Space activation, right-click menu, “copy link”, visited styling or disabled state.

## Accessibility

The link emits `Role::Button`, label = visible text, value = `"link"`, and focused boolean. There is no `Role::Link`; no URL, visited state or target is in the node.
