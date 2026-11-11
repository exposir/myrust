`date_input(c, "id", &mut text, &mut state)` combines an ISO `YYYY-MM-DD` text field with an overlay calendar. Valid text syncs the calendar; a calendar day writes back and closes the popup.

```rust
use myrust_core::ui::DateInputState;

let mut state = DateInputState::default();
let response = date_input(c, "date", &mut app.date_text, &mut state);
if response.submitted {
    app.commit_date();
}
```

## Accessibility

The root emits `Role::Group` with the id. The text field emits `Role::TextField`, label `"text field"`, value = ISO text, focused boolean. The calendar button is `Role::Button`, label `"calendar"`.

The open calendar is `Role::Popup` (overlay id) with day `Role::Button` nodes and header `<`/`>`. There is no Date/Calendar role, expanded state, invalid/error metadata or AT bridge.
