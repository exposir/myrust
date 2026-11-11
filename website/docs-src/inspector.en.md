The inspector is a per-frame snapshot, not a live devtools protocol. `inspect_frame(&ViewFrame)` builds an `InspectorTree`; `inspector(c, id, &tree)` renders it as an indented list of ids, roles, bounds and paint summaries.

```rust
use myrust_core::ui::{inspect_frame, inspector};

let tree = inspect_frame(&frame);
inspector(c, "debug-tree", &tree);
```

## Elements

Each `InspectorItem` has `id`, `role`, `label`, `bounds`, `paint` and nested `children`. Nesting is inferred by bounds containment, not by a renderer-owned parent pointer; overlapping or identical bounds can produce an approximate tree.

`PaintSummary` counts rects, rounded rects, circles, lines, text, SVG paths, gradients and images whose command bounds intersect the item. A parent therefore counts its children; counts are diagnostic, not allocation data.

## Changing styles

There is no live style editor or mutation API. The tree is immutable data derived from the previous frame; to change a look, change the caller's state or `Theme` and render again.

## Forcing states

Hover, focus, disabled and selected cannot be forced through the inspector. Tests can inject `UiEvent`s and call `Context::request_focus`; there is no `force_hover`/`force_active` flag.

## Showing layouts

The list shows absolute `bounds` from access nodes. It does not draw overlay boxes, padding, margin, flex lines or clipping regions; use the `ViewFrame.layout`/`display` data for deeper inspection.

## Opening the source

There are no source file/line links. `id` is the stable interaction/control id when one matches the bounds, otherwise a role/label-derived string; labels and paint summaries are the only provenance.

## Performance and issues

`inspect_frame` walks the frame's access nodes and commands; it allocates a tree and vector-clones labels, so it is a debug path, not a frame-time tool. `FrameStats` reports production time separately; the inspector does not profile widgets.

Overlapping bounds, duplicate labels and transformed/content-clipped geometry can make the tree misleading. No live updates, selection, search, expand/collapse or issue annotations exist.
