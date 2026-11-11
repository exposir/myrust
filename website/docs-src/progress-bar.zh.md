`progress(c, "id", value, max)` 绘制细确定进度条；`progress_indeterminate(c, "id")` 绘制未知时长的动画条。

```rust
progress(c, "upload", 0.65, 1.0);
```

## Accessibility

bar 发出 `Role::ProgressBar`，label 为 `"progress"`，value 为 `"65%"`（或 `"indeterminate"`），无 focused flag。没有 label 参数、min/max 元数据或 AT 桥。需要命名时在旁边单独绘制 label。
