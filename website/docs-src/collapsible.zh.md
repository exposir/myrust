`collapsible(c, "id", &mut open, "Details", content)` 绘制 disclosure header，并隐藏/显示 content 闭包。点击或聚焦时 Enter/Space 切换调用方 boolean。

```rust
let state = collapsible(c, "details", &mut app.open, "Details", |c| {
    text(c, "Hidden until open");
});
if state.changed {
    app.persist_open();
}
```

## Without a look

没有底层 disclosure header builder。自定义 header 需自己的可聚焦 container 与 interaction；`collapsible` 是唯一公开 disclosure primitive。

没有动画/height transition、disabled、icon slot 或嵌套 disclosure 管理。

## Accessibility

header 发出 `Role::Disclosure`，label 为 title，value 为 `"expanded"`/`"collapsed"`，带 focused 布尔。根发出 `Role::Group`，label 为 id。

没有 `aria-controls`/region 关系、level 元数据、announcement 或 AT 桥。
