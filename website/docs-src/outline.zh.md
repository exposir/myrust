`outline(c, "id", &mut state, nodes, &mut expanded)` 是 `tree` 的 panel 风格变体：同一 `TreeState`/`TreeNode`/expanded 数据，不同 row chrome 与选中强调。

```rust
use myrust_core::ui::{TreeNode, TreeState};

let response = outline(c, "outline", &mut app.outline, &nodes, &mut app.expanded);
```

## Outline state

`TreeState.selected_id` 是唯一选择；expanded 集合由调用方持有。没有独立 outline state 或 selected-path 模型。

## Opening and closing

chevron Button 在 expanded list 中切换节点；点击行选中。没有键盘展开/收起或行导航。

## Rows of your own

行只有 label。没有自定义 row content、badge、icon 或逐节点 action；自定义控件在 outline 外组合。

## In columns

outline 可放在普通 column/sidebar；没有 table-columns 模式、可 resize 列或多列 outline。参考页的 “outline in columns” 未实现。

## Accessibility

根 `Role::Group`；行 `Role::Option` label/value selected；chevron `Role::Button` label 为 node label，value expanded/collapsed。没有 Outline/Tree role、level 或 AT 桥。
