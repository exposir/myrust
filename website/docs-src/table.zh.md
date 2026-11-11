`table(c, "id", &mut state, columns, rows)` 绘制固定宽度列与可选行。排序是返回给调用方的请求；表格不自行重排数据。

```rust
use myrust_core::ui::{table, TableColumn, TableRow, TableState};

let columns = [TableColumn::new("Name", 120.0)];
let rows = [TableRow::new("a", &["Alpha"]), TableRow::new("b", &["Beta"])];
let response = table(c, "t", &mut app.table, &columns, &rows);
```

## Choosing rows

行点击更新 `TableState.selected_id`；`TableResponse` 报告 `selected_id`、`changed`、`clicked_id`。没有多行选择。

## Choosing cells

单元格不能单独选择或聚焦；没有 cell cursor、range selection 或 copy grid。

## Arranging columns

列宽是显式 `TableColumn::width`；行填充单元格。没有拖动 resize、列重排、隐藏/显示或比例宽度。

## Sorting

header 点击返回 `TableSort { column, direction }` 并更新 `state.sort_column/sort_direction`。调用方必须在下一帧前排序 rows；没有存储 comparator。

## Header menu

没有逐列 header menu（hide/pin/filter）。header 按钮只请求排序。

## Header cells

header 是 `Role::Button`，label 为 title，value 为排序态的 `ascending`/`descending`/`none`。它们不能按列单独聚焦。

## Cell menu

没有 context menu、edit、copy 或 row action menu。可用 table row bounds 另加 `context_menu`。

## Renaming in place

没有 inline cell editor。用 `editable_text` 或 text field 组合到表格外的自定义 row/cell。

## Accessibility

行发出 `Role::Option`，label 为第一格或 row id，value `"selected"`/`"unselected"`。header 发出 `Role::Button`；根发出 `Role::Group`，label 为 table id。

没有 Table/Row/Cell role、列索引、header label/value 之外的排序语义或 AT 桥。
