`icon(c, "id", name, size, color)` draws one built-in SVG path icon as a fixed-size widget. `IconState` reports name/size/found/commands.

```rust
icon(c, "arrow", "arrow_right", 20.0, theme.foreground);
```

## In buttons

Icons are not button content by themselves; place them beside text in a `row` and let the button label carry the action. `icon` is not focusable and has no click state.

There is no external SVG file loader, icon set registry, multi-color icon or rotation API. `Icon::to_commands` exposes the generated commands for custom composition; `to_commands_with_direction` mirrors in RTL.

## Accessibility

The icon emits `Role::Label`, label = icon name, value `"icon"` or `"missing"`, no focus. There is no Image/Icon role, decorative flag or alt-text parameter; no AT bridge.

When inside a button row, the button emits its own `Role::Button` node; the icon remains a separate label node.
