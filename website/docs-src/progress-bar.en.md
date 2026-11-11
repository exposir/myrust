`progress(c, "id", value, max)` draws a thin determinate bar; `progress_indeterminate(c, "id")` draws an animated unknown-duration bar.

```rust
progress(c, "upload", 0.65, 1.0);
```

## Accessibility

The bar emits `Role::ProgressBar`, label `"progress"`, value `"65%"` (or `"indeterminate"`), no focused flag. There is no label argument, min/max metadata or AT bridge. For a named progress bar, draw a separate label beside it.
