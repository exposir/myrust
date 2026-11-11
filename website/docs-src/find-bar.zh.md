`find_bar(c, "id", &mut query, &mut current, matches)` 绘制搜索输入、匹配计数与上一个/下一个按钮。`current` 会 clamp；有匹配时导航循环。

```rust
let state = find_bar(c, "find", &mut app.find_query, &mut app.find_index, app.matches);
if state.changed {
    app.highlight_match(state.current);
}
```

## Keyboard

input 使用普通 text-field 编辑器。find bar 本身没有 Enter-to-next、Escape-to-close、方向键匹配导航或搜索快捷键；previous/next 是指针按钮，0 匹配时禁用。

Tab 可按构造顺序聚焦 input 与按钮。没有控件自有的 `Cmd+F`/focus shortcut；宿主/app 必须打开它。

## Accessibility

根发出 `Role::Group`，label 为控件 id。input 是 `Role::TextField`，label 为 `"text field"`，value 为 query，带 focused 布尔。导航按钮是 `Role::Button`，label 为 `"previous match"`/`"next match"`，value 为 `"enabled"`/`"disabled"`；计数是 `Role::Label` 文本节点。

没有 search-results/list role、current/total 语义元数据、error 状态或 AT 桥。
