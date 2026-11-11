`time_input(c, "id", &mut text)` shows an `HH:MM` text field with four step buttons. Text is filtered to digits and one colon; Enter normalizes a valid value.

```rust
let state = time_input(c, "time", &mut app.time_text);
if state.valid {
    app.hour = state.hour;
    app.minute = state.minute;
}
```

## Accessibility

The root emits `Role::Group` with the id. The input emits `Role::TextField`, label `"text field"`, value = `HH:MM`, focused boolean.

The four step controls emit `Role::Button`; their labels are the internal keys `"<id>:hour_dec"`, `"<id>:minute_inc"` etc., not friendly words. There is no Time/SpinButton role, min/max, invalid/error state or AT bridge.
