`badge(c, label)` 绘制短文本胶囊，如计数或状态。

```rust
badge(c, &format!("{}", app.count));
```

## Accessibility

badge 发出 `Role::Badge`，label 为文本，无 value、无 focus。没有 count/tone/icon 参数、dismiss 或 AT 桥；它是普通 label 式胶囊。
