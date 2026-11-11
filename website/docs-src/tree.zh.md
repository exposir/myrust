`tree(c, "id", &mut state, nodes, &mut expanded)` 渲染缩进 `TreeNode { id, label, depth, has_children }` 行。`TreeState.selected_id` 与调用方 expanded id 列表驱动状态。

```rust
use myrust_core::ui::{TreeNode, TreeState};

let nodes = [
    TreeNode::new("src", "src", 0, true),
    TreeNode::new("main", "main.rs", 1, false),
];
let response = tree(c, "files", &mut app.tree, &nodes, &mut app.expanded);
```

## Keyboard

行不能单独聚焦；没有 tree focus cursor、左右展开/收起或上下行导航。展开由指针驱动；Tab 不遍历 tree 行。

## Without a look

没有底层 tree-row builder 或自定义 row content。`tree`/`outline` 是公开 primitive；任意 row 控件需自定义 control 列。

## Accessibility

根发出 `Role::Group`，label 为 tree id。每行发出 `Role::Option`，label 为 node label，value `"selected"`/`"unselected"`。有 children 的节点还发出 `Role::Button` chevron，label 为 node label，value `"expanded"`/`"collapsed"`，带 focused 布尔。

没有 Tree/TreeItem role、level 元数据、option 上的 expanded 状态、position-in-set 或 AT 桥。
