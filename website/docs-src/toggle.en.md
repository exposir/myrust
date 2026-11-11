`toggle(c, "id", "Bold", &mut active, group)` draws a pressed-state button. Without a group, the caller's `bool` toggles independently; with a `ToggleGroup`, one active id is the source of truth.

```rust
use myrust_core::ui::{toggle, ToggleGroup};

let left = toggle(c, "left", "Left", &mut app.left, Some(&mut app.toggle_group));
let right = toggle(c, "right", "Right", &mut app.right, Some(&mut app.toggle_group));
```

## Toggle groups

`ToggleGroup { active: Option<String> }` keeps at most one id. Activating a new toggle clears the previous; toggling the active one clears the group. Each toggle syncs its `bool` to the group during that frame.

Groups do not deduplicate ids, auto-remove missing members or support disabled/tri-state members. The caller must render all group members each frame to keep their bools in sync.

## Without a look

There is no low-level toggle builder or custom track/thumb. A custom pressed control needs `container`/`draw`, its own click handling and an AccessNode path; no public widget-base protocol exists.

## Accessibility

Each toggle emits `Role::Button`, label = its text, value = `"active"`/`"inactive"`, and focused boolean. The group itself has no separate node; no pressed/checked role or AT bridge exists.
