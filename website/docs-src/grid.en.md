`grid(c).columns(n)` lays children into equal-width columns and uniform rows. It is a simple grid mode of the flex builder, not a full CSS Grid.

```rust
grid(c).columns(2).gap(8.0).children(|c| {
    for i in 0..6 {
        container(c).size(40.0, 30.0);
    }
});
```

## Minimums

There are no per-column minimum/maximum widths or content-based auto sizing. Width is the parent's available width divided by `columns`.

## Placing children

Children are placed row-major in construction order; there is no explicit row/column placement, span, area name or order override.

## Sizing children

Each child keeps its own width/height; the grid cell is equal-width. There is no grid fraction (`fr`), auto track or aspect-ratio sizing.

## Aligning

Grid uses the parent's `align_items`/`justify_content` where applicable, but there is no per-cell alignment, row gap separate from column gap, or dense packing. `.gap` applies uniformly.
