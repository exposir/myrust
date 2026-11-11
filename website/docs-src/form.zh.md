`form_field(c, "id", "Label", Some("Description"), None, content)` 布局 label、可选 description、调用方 content 与可选 error。它是视觉分组，不是完整 form 模型。

```rust
let state = form_field(c, "name-field", "Name", Some("Display name"), None, |c| {
    text_field(c, "name-input", &mut app.name);
});
```

## Forms

没有 `Form` container、submit event、validation model 或 field registry。每个 `form_field` 独立；提交/校验由调用方接线。

## Fieldsets

没有 fieldset/legend 或嵌套 form grouping。用 column + label/header 与多个 `form_field`。

## Labels of your own

`label` 参数渲染为 `Role::Label` 文本节点；AccessNode 没有显式 `for`/label 关联。自定义 label 可在外部绘制，但不发语义链接。

## Hints and errors of your own

`description`/`error` 字符串渲染为文本，只在 field root 的 value（`"valid"`/`"error"`）反映。没有 per-control invalid flag、`aria-describedby`、live error 宣告或 error icon。

## Accessibility

field root 发出 `Role::Group`，label 为可见 label，value 为 `"error"`/`"valid"`。description/error/label 字符串是 `Role::Label` 文本节点。被包裹控件发出自己的 node。

没有 Form/Fieldset role、控件上的 required/invalid flag、label 关联或 AT 桥。
