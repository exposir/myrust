`Direction` selects LTR or RTL for widgets constructed after it is set. The default is `Direction::Ltr`; a temporary scope restores the previous direction.

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

`Direction::Ltr` and `Direction::Rtl` are public values. `Context::direction()` reads it, `set_direction` changes it for subsequent nodes, and `with_direction(direction, closure)` scopes the change and restores the previous value.

`Direction` is not a locale and not a CSS property. It does not change text shaping, paragraph base direction or number formatting; it drives layout order and a few presentation defaults.

## What mirrors

A `row` created while RTL reverses its relative children, so the first logical child is placed at the right. Text alignment defaults to End when no explicit alignment is given, and `icon(c, ...)` mirrors built-in SVG icons horizontally inside their target box.

Overlays and ordinary columns keep their physical placement; only row child order and the documented defaults mirror. The icon mirror is a transform in the icon command, not a separate RTL icon set.

## What does not

Padding, margin and border sides are physical and do not swap. Scrollbar placement, split-view divider order, grid column order, `left`/`top` offsets and absolute positioning do not flip.

Arbitrary `SvgPath`, images, bidi text runs, punctuation, URLs and numbers are not mirrored or reordered. Mixed-direction text has no shaping/bidi algorithm; the renderer lays out a single left-to-right string unless the caller splits it.

## Trying it

The wasm demo for this page toggles `Direction` around a row. The same widgets are rebuilt under `with_direction`; the row order, default text alignment and icon mirroring update on the next frame.

No RTL-specific theme or localization is provided. For a real RTL interface, combine `Direction` with translated strings, explicit alignment and manual placement of scrollbars and decorative art.
