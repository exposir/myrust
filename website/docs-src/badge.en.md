`badge(c, label)` draws a small pill with text, such as a count or status.

```rust
badge(c, &format!("{}", app.count));
```

## Accessibility

The badge emits `Role::Badge`, label = its text, no value, no focus. There is no count/tone/icon parameter, dismissal or AT bridge; it is a plain label-like pill.
