`outline(c, "id", &mut state, nodes, &mut expanded)` is the panel-style variant of `tree`: same `TreeState`/`TreeNode`/expanded data, different row chrome and selection emphasis.

```rust
use myrust_core::ui::{TreeNode, TreeState};

let response = outline(c, "outline", &mut app.outline, &nodes, &mut app.expanded);
```

## Outline state

`TreeState.selected_id` is the only selection; the expanded set is caller-owned. There is no separate outline state or selected-path model.

## Opening and closing

The chevron Button toggles a node in the expanded list; clicking a row selects it. Keyboard expand/collapse and row navigation are absent.

## Rows of your own

Rows are label-only. There is no custom row content, badges, icons or per-node actions; compose custom widgets outside the outline.

## In columns

An outline can be placed in a normal column/sidebar; it has no table-columns mode, resizable column or multi-column outline. The reference “outline in columns” is not implemented.

## Accessibility

Root `Role::Group`; rows `Role::Option` label/value selected; chevrons `Role::Button` label = node label, value expanded/collapsed. No Outline/Tree role, level or AT bridge.
