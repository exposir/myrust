MyRust lays out every frame on the CPU. Containers are nodes in a flex tree; widgets are leaves with an intrinsic size. There is no retained layout tree and no CSS.

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

`column`, `row`, `container` and `grid` create flex or equal-width grid scopes. Children are built inside `.children(|c| ...)`. A `container` is a neutral scope for custom drawing; `grid().columns(n)` enables the simple equal-width grid layout.

There is no general “group” render object beyond these builders. Overlays are positioned internally by their anchor, not by a public absolute-position builder.

## Sizes

Builder methods `width`, `height`, `size`, `fill` and `flex_grow` set explicit size, available-size filling and main-axis growth. A leaf without an explicit size uses its intrinsic content size.

The public builders do not expose `min_width`, `max_width`, `min_height` or `max_height`; `Style` has those fields, but custom code would have to construct a node with `Style` rather than use the standard builder methods.

## Spacing

`.padding(n)`, `.margin(n)` and `.gap(n)` take one value and apply it on all edges. This is the implemented spacing model.

Per-edge padding/margin and logical start/end spacing are not exposed by the builders, even though `Edges` supports separate values internally. RTL reverses row order but does not swap physical padding.

## Alignment

`.align(AlignItems)` sets cross-axis alignment, `.justify(JustifyContent)` distributes main-axis space, and `.center()` centers on both axes. `AlignItems` and `JustifyContent` are public enums.

Baseline alignment, space-between with per-item gaps, and grid alignment controls are not implemented. Grid columns are equal width.

## Positioning

All builder children participate in normal flow (`Position::Relative`). `Position::Absolute`, `left` and `top` exist in `Style`, but there is no public absolute-position builder; overlays, popovers and dialogs use the internal absolute path.

There are no public x/y offsets on `column`/`row`/`container`, no scroll anchoring and no sticky positioning.

## Paint order

Display commands are emitted in construction/root order, then overlays last on their own layer. A later sibling paints over an earlier one. There is no public `z-index`; layering is controlled by overlay layer numbers and by the order in which roots are constructed.

Hit-testing uses the previous frame's interaction regions and respects overlay layers. Painting order and hit order are related by the same layer rule, not by separate CSS stacking contexts.

## Clipping and visibility

`scroll_view` clips its viewport and offsets its content with wheel, drag-bar and keyboard scrolling. `DrawCommand::ScissorPush`/`ScissorPop` can be emitted through a custom container's `.draw(...)` and the CPU presenter applies them, but there is no higher-level clip builder.

Visibility is expressed by not constructing a widget. There is no `visible(false)` flag, no opacity property and no CSS-like hidden state; to hide content, omit it or draw a background over it.
