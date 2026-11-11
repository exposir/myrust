Migration is explicit and id-based. `Context::migrate_state(old, new)` moves context-owned transient widget state; `migrate_value(map, old, new)` moves a caller map entry. There is no source rewrite or runtime compatibility layer.

```rust
if renamed {
    c.migrate_state("editor-old", "editor-new");
}
```

## Apply the source migration

No automated source migration, codemod, version-upgrade script or deprecated API shim exists. Renaming a control or field is a manual Rust change; keep ids stable where possible.

## Element values and keys

`migrate_value` moves one `HashMap<String, T>` entry when the new key is absent. `Context::migrate_state` moves `widget_states` (text cursor/selection, select open/highlight, scroll offset, slider drag), id-prefixed animation keys, overlay runtimes and focus.

Neither overwrites an existing new key; equal ids and missing old keys return false. Serialized application values are not migrated automatically.

## Input and actions

Input events are not migrated. `UiEvent`s target the current frame's rectangles; after an id/structure change the caller must keep the same interaction ids or accept that pending input is dropped.

There is no command map, keybinding migration or action-id registry. Actions are ordinary Rust calls in the view.

## Persistent identity

Persistent identity is the stable string id for context state and focus, plus caller-owned state structs. There is no element handle generation or identity object to migrate.

If a control moves into a new parent, keep its id and call `migrate_state` when the id itself changes. Interaction regions rebuild every frame; only state/focus need explicit movement.

## Focus bound to app data

Focus is a string id, not bound to data. `migrate_state` moves `focused_id` when the old id was focused; a data-collection reorder needs stable row ids or explicit `request_focus`.

There is no `Focus<T>` migration or data-key mapping helper.

## Lists and keyboard commands

List state contains selection; `ListReorderState` has drag/from/to, and list rows use Option nodes. There is no list identity migration beyond stable row ids, and no keyboard-command table to migrate.

`Context::migrate_state` can move scroll/selection-like widget state by list id, but row ids and data order remain the caller's responsibility.

## Persistent services and verification

There is no service registry, callback handle or background-task migration. Callbacks live in the caller and must be recreated; `migrate_state` only handles UI context state.

Verify with `cargo test` and explicit assertions on `ViewFrame`/`AccessNode`. `migrate_value`/`migrate_state` return false when old is missing or new exists, and do not merge partial state.
