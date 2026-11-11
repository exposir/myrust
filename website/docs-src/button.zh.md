`button(c, "Save")` 绘制主题按钮并返回当前帧 `ButtonState`。label 是普通字符串；点击由调用方内联消费。

```rust
if button(c, "Save").clicked {
    app.save();
}
```

## Content

按钮内容是一个文本 label。没有 child/content 闭包、前置 icon、快捷键标签或多行按钮；可用 `row`/`icon` 组合自定义控件，或另放 label。

## States

`ButtonState` 暴露 `clicked`、`pressed`、`focused`、`hovered` 与 `id`/`bounds`。hover/press 用主题色动画；click 在上一帧区域释放后报告，聚焦时 Enter/Space 激活。

没有 disabled、loading、toggle、long-press 或 double-click 状态。看起来禁用的按钮应通过不构建、或自绘/自定义行为模拟。

## Style

按钮填充、边框、圆角、文字与焦点环来自当前 `Theme`；没有逐按钮样式 builder 或颜色覆盖。用 `NativeUi::with_theme` 或自绘 container `.draw(...)` 改变外观。

## Accessibility

按钮发出 `Role::Button`，label 为可见文本，无 `value`，带当前 `focused` 布尔。不发出 disabled 或 shortcut；没有平台 AT 桥消费。

## Click actions

`ButtonState::clicked` 是动作钩子。没有延迟 `OnClick`、command object、action id 或 click callback；在构造按钮后立即调用动作。需要稳定 id 时用 `c.id("save").button("Save")`。
