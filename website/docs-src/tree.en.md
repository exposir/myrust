`tree(c, "id", &mut state, nodes, &mut expanded)` renders indented `TreeNode { id, label, depth, has_children }` rows. `TreeState.selected_id` and the caller-owned expanded id list drive state.

```rust
use myrust_core::ui::{TreeNode, TreeState};

let nodes = [
    TreeNode::new("src", "src", 0, true),
    TreeNode::new("main", "main.rs", 1, false),
];
let response = tree(c, "files", &mut app.tree, &nodes, &mut app.expanded);
```

## Keyboard

Rows are not individually focusable; there is no tree focus cursor, Left/Right expand/collapse or Up/Down row navigation. Expansion is pointer-driven; Tab does not walk tree rows.

## Without a look

There is no low-level tree-row builder or custom row content. `tree`/`outline` are the public primitives; arbitrary row widgets require a custom column of controls.

## Accessibility

The root emits `Role::Group` with the tree id. Each row emits `Role::Option`, label = node label, value `"selected"`/`"unselected"`. Nodes with children also emit a `Role::Button` chevron, label = node label, value `"expanded"`/`"collapsed"`, focused boolean.

There is no Tree/TreeItem role, level metadata, expanded state on the option, position-in-set or AT bridge.
