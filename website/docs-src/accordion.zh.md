`accordion(c, "id", &mut state, sections)` 绘制纵向可展开 section。`AccordionState` 保存 open ids 与 `AccordionMode`（Single/Multiple）；`AccordionSection` content 是文本字符串。

```rust
use myrust_core::ui::{accordion, AccordionMode, AccordionSection, AccordionState};

let sections = [AccordionSection::new("one", "One", "First body")];
accordion(c, "acc", &mut app.accordion, &sections);
```

## Without a look

没有自定义 section content、icon slot 或底层 accordion builder。section body 只有文本；任意控件放 accordion 外。

没有 height 动画、disabled section、lazy content 或嵌套 accordion 协调。

## Accessibility

每个 header 发出 `Role::Disclosure`，label 为 section title，value 为 `"expanded"`/`"collapsed"`，带 focused 布尔。根发出 `Role::Group`，label 为 id。

没有 Accordion role、section position/level、controls 关系或 AT 桥。
