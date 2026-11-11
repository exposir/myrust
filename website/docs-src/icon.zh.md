`icon(c, "id", name, size, color)` 绘制一个内置 SVG path icon，固定尺寸。`IconState` 报告 name/size/found/commands。

```rust
icon(c, "arrow", "arrow_right", 20.0, theme.foreground);
```

## In buttons

icon 本身不是 button content；把它与文字放入 `row`，让 button label 承载动作。`icon` 不可聚焦、无 click state。

没有外部 SVG 文件 loader、icon set registry、多色 icon 或 rotation API。`Icon::to_commands` 暴露生成命令供自定义组合；`to_commands_with_direction` 在 RTL 镜像。

## Accessibility

icon 发出 `Role::Label`，label 为 icon name，value 为 `"icon"`/`"missing"`，无 focus。没有 Image/Icon role、decorative flag 或 alt-text 参数；无 AT 桥。

在 button row 内时，button 发出自己的 `Role::Button` node；icon 仍是独立 label node。
