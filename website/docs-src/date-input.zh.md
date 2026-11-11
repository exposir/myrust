`date_input(c, "id", &mut text, &mut state)` 把 ISO `YYYY-MM-DD` 文本字段与 overlay 日历结合。合法文本同步日历；点击日期写回并关闭 popup。

```rust
use myrust_core::ui::DateInputState;

let mut state = DateInputState::default();
let response = date_input(c, "date", &mut app.date_text, &mut state);
if response.submitted {
    app.commit_date();
}
```

## Accessibility

根发出 `Role::Group`，label 为 id。text field 发出 `Role::TextField`，label 为 `"text field"`，value 为 ISO 文本，带 focused 布尔。calendar 按钮是 `Role::Button`，label 为 `"calendar"`。

打开的日历是 `Role::Popup`（overlay id），日期是 `Role::Button`，header 为 `<`/`>`。没有 Date/Calendar role、expanded 状态、invalid/error 元数据或 AT 桥。
