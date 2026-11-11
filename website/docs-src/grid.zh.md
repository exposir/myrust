`grid(c).columns(n)` 把 children 放进等宽列与统一行。它是 flex builder 的简单 grid 模式，不是完整 CSS Grid。

```rust
grid(c).columns(2).gap(8.0).children(|c| {
    for i in 0..6 {
        container(c).size(40.0, 30.0);
    }
});
```

## Minimums

没有逐列 min/max 宽度或 content-based auto sizing。宽度是父可用宽度除以 `columns`。

## Placing children

children 按构造顺序 row-major 放置；没有显式 row/column placement、span、area name 或 order override。

## Sizing children

每个 child 保留自己的 width/height；grid cell 等宽。没有 grid fraction（`fr`）、auto track 或 aspect-ratio sizing。

## Aligning

grid 在适用处使用父级 `align_items`/`justify_content`，但没有逐 cell 对齐、独立 row/column gap 或 dense packing。`.gap` 均匀应用。
