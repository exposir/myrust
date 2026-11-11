`link(c, "id", "label")` 是可点击文本控件，不是 URL 打开器。返回 `LinkState { clicked, focused, hovered }`；动作由调用方决定。

```rust
if link(c, "docs", "Read the docs").clicked {
    app.open_link("https://example.com/docs");
}
```

## Paths in a router

没有内置 link-to-route 集成。link 只返回 `clicked`；路由场景在 click handler 调用 `RouterState::push` 并渲染对应页面。

URL 解析、目标页面、外部浏览器启动与 visited 状态由调用方负责。控件不保存 href。

## Within a sentence

link 是独立 `Button` 节点，不是段落中的 inline span。`row` 可并排文字与 link，但文字不会围绕 link 换行；Markdown inline link 由 `markdown` 渲染，不由 `link` 渲染。

## Other actions

聚焦 link 用 Enter 激活；hover/focus 绘制下划线与 accent 色。没有 Space 激活、右键菜单、“copy link”、visited 样式或 disabled 状态。

## Accessibility

link 发出 `Role::Button`，label 为可见文本，value 为 `"link"`，带 focused 布尔。没有 `Role::Link`；node 不含 URL、visited 或 target。
