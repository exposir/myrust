`segmented(c, "id", &mut selected, &["Small", "Medium", "Large"])` draws an equal-width selector inside the current width. Clicking a segment selects it; arrows move while focused.

```rust
let state = segmented(c, "size", &mut app.size, &["Small", "Medium", "Large"]);
if state.changed {
    app.apply_size(state.selected);
}
```

## Full width

The control consumes the available width and divides it equally among labels, with one gap. It does not support an intrinsic/two-segment width or per-segment widths.

## Icons and other content

Segments are text labels only. There are no icons, badges, arbitrary child widgets or per-segment tooltips. Custom segment content needs a custom container; the public control exposes only labels.

## Accessibility

The root emits `Role::TabList` labelled by the control id; each segment emits `Role::Tab` with its label and value `"selected"`/`"unselected"`. Focus is on the root TabList; segments are not separately focusable.

No disabled segment, roving tab index or AT bridge exists. Arrow/Home/End keys change selection while the root is focused; selection clamps rather than wraps.
