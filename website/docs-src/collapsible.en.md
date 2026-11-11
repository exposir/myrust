`collapsible(c, "id", &mut open, "Details", content)` draws a disclosure header and hides/shows the content closure. Click or focused Enter/Space toggles the caller boolean.

```rust
let state = collapsible(c, "details", &mut app.open, "Details", |c| {
    text(c, "Hidden until open");
});
if state.changed {
    app.persist_open();
}
```

## Without a look

There is no low-level disclosure header builder. A custom header needs its own focusable container and interaction; `collapsible` is the only public disclosure primitive.

No animation/height transition, disabled state, icon slot or nested disclosure management exists.

## Accessibility

The header emits `Role::Disclosure`, label = title, value `"expanded"`/`"collapsed"`, focused boolean. The root emits `Role::Group` with the id.

There is no `aria-controls`/region relationship, level metadata, announcement or AT bridge.
