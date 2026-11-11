`list(c, "id", &mut state, rows, row_height, viewport_height)` 是 `ListRow { id, label, height }` 的虚拟化纵向列表，只构建与 viewport 相交的行。

```rust
use myrust_core::ui::{ListRow, ListState, list};

let rows = [ListRow::new("a", "Alpha"), ListRow::new("b", "Beta")];
let state = list(c, "rows", &mut app.list, &rows, 28.0, 160.0);
```

## Layout

行宽满宽，高度由调用方/默认值决定；列表纵向、不换行。没有水平列表、多列列表或 sticky row。

## Rows wider than the list

内容被 viewport 裁切；没有水平滚动、ellipsis 或逐行文本换行。更宽内容由调用方负责。

## List state

`ListState` 只有 `selected_id`。scroll offset 存在 list id 下的 context；`ListResponse` 报告 `visible_start/end`、`scroll`、`max_scroll`。

## Choosing rows

行点击设置 `selected_id`；`changed`/`clicked_id` 报告。没有多选、checkbox 模式、range select 或方向键行导航。

## Scrolling

滚轮、滚动条拖动与键盘滚动复用 `scroll_view` 状态。列表不拥有 scroll controller；返回 `scroll`/`max_scroll`。

## Reordering

列表自身不重排行。`reorder_list` 是带 drag state 的独立 helper；`reorder` 对调用方数据应用索引。

## Dragging rows out

没有 OS drag、跨窗口行拖动或 drop representation。窗口内 reorder 是唯一行拖动路径。

## The focus

行不能单独聚焦；列表不是 focus stop。没有 arrow-key 行光标或 selection 宣告。

## Accessibility

每个构建出来的行发出 `Role::Option`，label 为行文本，value 为 `"selected"`/`"unselected"`。没有 list/table role、行数、position-in-set、多选或 AT 桥。
