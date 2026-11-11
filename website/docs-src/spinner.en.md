`spinner(c, "id")` draws an animated loading indicator and requests another frame while reduce-motion is off.

```rust
spinner(c, "loading");
```

## Accessibility

The spinner emits `Role::Spinner`, label `"loading"`, value `"busy"`/`"static"`, no focused flag. There is no accessible name argument, progress value or live announcement; no AT bridge.
