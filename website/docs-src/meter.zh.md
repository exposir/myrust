`meter(c, "id", value, min, max, low, high)` 绘制三档 quantity bar；tone 由阈值派生。

```rust
meter(c, "disk", used, 0.0, 100.0, 60.0, 85.0);
```

## Colors

`MeterTone` 通过主题 accent/warn 类色带选择填充色。没有自定义颜色或阈值样式 API；需要不同颜色时调用方可自绘。

## Accessibility

meter 发出 `Role::ProgressBar`，label 为 `"meter"`，value 为 `"value/max"`（两位小数），无 focused flag。没有 label/min/max/threshold 元数据或 AT 桥。
