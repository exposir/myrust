`calendar(c, "id", &mut state)` 绘制 6×7 月网格与年月 header。点击日期选择；方向键、Home/End、PageUp/PageDown 移动选择。

```rust
use myrust_core::ui::CalendarState;

let mut state = CalendarState::new(2024, 2, Some(10));
let response = calendar(c, "cal", &mut state);
if response.changed {
    app.sync_date(state.year, state.month, state.selected);
}
```

## Browsing without choosing

没有独立 browse month 与 selected day：PageUp/PageDown 改显示月并 clamp 选中日；方向键跨月移动选中日期。

没有 year picker、min/max date、disabled day、“today” 跳转、多月视图或 locale week start。此实现周一开始。

## Accessibility

根发出 `Role::Group`，label 为 id，value 为 `"YYYY-MM-DD"`。日期格发出 `Role::Button`，label 为 `"YYYY-MM-DD"`，value 为 `"selected"`/`"unselected"`。header `<`/`>` 按钮是 `Role::Button`，label 为 `"<"`/`">"`。

没有 Grid/Calendar role、weekday header 语义、disabled date 或 AT 桥。
