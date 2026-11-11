`avatar(c, "id", label, initials, size, shape)` 绘制 initials avatar。`avatar_image(c, "id", label, data, size, shape)` 绘制调用方解码 RGBA 像素，可选圆形遮罩。

```rust
avatar(c, "user", "Ada Lovelace", "AL", 40.0, AvatarShape::Circle);
```

## Accessibility

两个变体都发出 `Role::Label`：label 为 accessible name（`label` 参数），value 在有像素时为 `"image"`，否则为可见 initials。没有 Avatar role、online/status indicator、`label` 之外的 image alt text 或 AT 桥。

avatar 不可聚焦，也没有 fallback/error node。缺图像数据时回退 initials 或占位图。
