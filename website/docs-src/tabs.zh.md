`tabs(c, "settings", &mut app.tab, &["General", "Appearance", "Advanced"])` 绘制一行标签页。选中索引由调用方持有，返回的 `TabsState` 报告输入是否改变了它。

```rust
let state = tabs(c, "settings", &mut app.tab, &["General", "Appearance", "Advanced"]);
if state.changed {
    // 加载新选中的页面
}

match app.tab {
    0 => general(c),
    1 => appearance(c),
    _ => advanced(c),
}
```

标签行是一个焦点停靠点。点击标签会选中它；聚焦后 Left/Right 移动选择。当前 crate 没有禁用标签变体，也没有底层 tab-list 构建器。

## 无障碍

控件发出一个 `Role::TabList` 节点，label 是稳定 id，并带焦点状态。每个标签发出 `Role::Tab`，label 是可见文本，value 为 `"selected"` 或 `"unselected"`；标签本身不能单独聚焦。目前还没有平台无障碍桥消费这些节点。
