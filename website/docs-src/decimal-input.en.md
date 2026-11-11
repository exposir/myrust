`decimal_input(c, "id", &mut text, options)` keeps an exact fixed-point value as a scaled `i64`; the visible string is filtered and never rounded to a float.

```rust
use myrust_core::ui::DecimalInputOptions;

let state = decimal_input(
    c,
    "amount",
    &mut app.amount_text,
    DecimalInputOptions { allow_negative: true, decimals: 2 },
);
if let Some(units) = state.units {
    app.amount_units = units;
}
```

## Nothing is rounded

Extra fraction digits are discarded by the input filter, not rounded. `DecimalInputState::units` is the exact scaled integer (e.g. `1234` for `"12.34"` with two decimals); `value` is not a float.

There is no currency symbol/locale formatting, grouping, min/max, decimal step or pasted normalization beyond the character filter.

## Accessibility

The field emits `Role::TextField`, label `"text field"`, value = the visible decimal string, focused boolean. No `SpinButton`, numeric range, currency or invalid/error metadata is emitted; no AT bridge exists.
