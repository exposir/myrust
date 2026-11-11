`spinner(c, "id")` 绘制加载指示器；reduce-motion 关闭时请求下一帧。

```rust
spinner(c, "loading");
```

## Accessibility

spinner 发出 `Role::Spinner`，label 为 `"loading"`，value 为 `"busy"`/`"static"`，无 focused flag。没有 accessible name 参数、进度值或 live announcement；无 AT 桥。
