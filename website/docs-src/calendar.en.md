`calendar(c, "id", &mut state)` draws a 6×7 month grid with a year-month header. Clicking a day selects it; arrows, Home/End and PageUp/PageDown move the selection.

```rust
use myrust_core::ui::CalendarState;

let mut state = CalendarState::new(2024, 2, Some(10));
let response = calendar(c, "cal", &mut state);
if response.changed {
    app.sync_date(state.year, state.month, state.selected);
}
```

## Browsing without choosing

There is no separate browse month vs selected day: PageUp/PageDown change the displayed month and clamp the selected day; arrows move the selected date across months.

No year picker, min/max date, disabled day, “today” jump, multi-month view or locale week start exists. Week starts Monday in this implementation.

## Accessibility

The root emits `Role::Group` with the id as label and value `"YYYY-MM-DD"`. Day cells emit `Role::Button`, label `"YYYY-MM-DD"`, value `"selected"`/`"unselected"`. Header `<`/`>` buttons are `Role::Button` with labels `"<"`/`">"`.

There is no Grid/Calendar role, weekday header semantics, disabled date or AT bridge.
