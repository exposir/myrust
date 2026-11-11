`search_field(c, "id", &mut query)` 是带尾部 clear 按钮的文本字段。Enter 返回 `submitted`；Escape 或 clear 按钮返回 `cleared`。

```rust
let state = search_field(c, "find", &mut app.query);
if state.submitted {
    app.search();
}
```

## Accessibility

根发出 `Role::Group`，label 为控件 id。input 发出 `Role::TextField`，label 为 `"text field"`，value 为 query，带 focused 布尔；clear 按钮发出 `Role::Button`，label 为 `"clear search"`，无 value。

没有 Search/SearchBox role、description、error、结果数或 AT 桥。clear 按钮不能单独聚焦。
