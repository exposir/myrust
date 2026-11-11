`meter(c, "id", value, min, max, low, high)` draws a three-band quantity bar. The tone is derived from thresholds.

```rust
meter(c, "disk", used, 0.0, 100.0, 60.0, 85.0);
```

## Colors

`MeterTone` selects the fill color through theme accent/warn-like bands. There is no custom color or threshold style API; callers can draw their own bar if they need distinct colors.

## Accessibility

The meter emits `Role::ProgressBar`, label `"meter"`, value `"value/max"` (two decimals), no focused flag. There is no label/min/max/threshold metadata or AT bridge.
