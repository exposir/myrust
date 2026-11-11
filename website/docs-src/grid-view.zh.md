`grid_view(c, "id", &mut state, count, item_size, columns, gap, height, render)` 是固定尺寸 item 的虚拟化滚动网格。`GridViewState { selected: Option<usize> }` 由调用方持有。

```rust
use myrust_core::ui::GridViewState;

let state = grid_view(
    c, "photos", &mut app.grid, 40,
    Size::new(64.0, 48.0), 4, 8.0, 220.0,
    |c, i| { text(c, &format!("Photo {i}")); },
);
```

## Grid state

`selected` 保存一个 item 索引；`GridViewResponse` 报告 `changed`、`clicked`、`visible_start/end`、`scroll`、`max_scroll`。没有多选、键盘 cursor 或 item reorder。

## Accessibility

每个构建 item 发出 `Role::Button`，label 为 `"grid item N"`，value `"selected"`/`"unselected"`。根是 `Group`，label 为控件 id。

没有 Grid/GridCell role、行列索引、item count、position-in-set 或 AT 桥。离屏 item 不构建，因此不出现在 nodes。
