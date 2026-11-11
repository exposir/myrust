Styling is explicit data: colors, radii, padding and drawing commands are passed to widgets each frame. There is no stylesheet, selector matching or inherited visual rule beyond the `Theme` lookup.

```rust
use myrust_core::ui::{text, Theme};

let theme = Theme::dark();
text(c, "Save").color(theme.accent);
```

## State, not selectors

A widget's appearance is a function of the values passed to it and the caller's state. Hover, focus and chosen states are handled inside the control or by branching in the view; there are no selectors like `:hover`, no class cascade and no media queries.

To express a custom state, read the caller's state and choose a `Color`/`DrawCommand` explicitly. `mix_color` is available for simple blends, and `DrawCommand` exposes raw rect, rounded-rect, circle, line, gradient, path and text operations.

## Themes

`Theme` is a value with `background`, `surface`, `foreground`, `muted`, `accent`, `on_accent`, `border`, `spacing`, `font_family` and `font_sizes`. `Theme::dark()`, `Theme::light()` and `Theme::system()` construct built-in themes.

Pass it into `NativeUi::with_theme(theme)`. Widgets read the current theme from `Context` each frame. There is no public `Context::set_theme`; changing theme means rebuilding/replacing the `NativeUi` value, not mutating a live context.

## The focus ring

Controls that own keyboard focus draw a focus ring in `theme.accent`. The ring geometry and thickness are internal; there is no public radius/width/offset setting.

Focus color can only be changed through the theme's `accent`; an individual widget cannot override its focus ring. Focus is a boolean `Role` state in the emitted access node, not a separate style selector.

## A theme for part of the window

There is no per-subtree theme override. Because widgets read colors from one `Theme`, a different look for one section requires passing explicit `Color` values and custom `DrawCommand`s to that section.

You can build a local palette by cloning a `Theme` and using its fields in the section, but there is no context scope that swaps the active theme for children.

## The desktop's preferences

On native targets `Theme::system()` asks `dark-light` for light/dark preference; wasm builds always use `Theme::dark()` because they have no platform appearance probe. There is no public callback for live OS theme changes.

Reduce-motion is read from `MYRUST_REDUCE_MOTION` at context creation; high-contrast, transparency and accent-color preferences are not exposed. The desktop can choose a theme at startup, but runtime preference updates require constructing a new UI.
