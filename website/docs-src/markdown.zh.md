`markdown(c, "readme", &app.source)` 每帧解析基础 Markdown，并用文本与行节点布局。渲染器刻意保持很小：1–6 级标题、段落、无序/有序列表，以及行内 code、bold、italic、link。

```rust
use myrust_core::ui::{markdown, scroll_view};

scroll_view(c, "readme-scroll", 420.0, |c| {
    markdown(c, "readme", &app.source);
});
```

`parse_markdown` 可单独取得 block 列表。段落续行会以空格拼接。不支持 fenced code block、表格、任务列表、脚注或 raw HTML；这些不在已实现子集内。

## 无障碍

Markdown 根节点是 `Role::Group`，label 为稳定 id，value 类似 `"12 blocks"`。每个行内 span 是独立的 `Role::Label`，value 为 `"plain"`、`"bold"`、`"italic"`、`"code"` 或 `"link"`。这些仍只是元数据；平台 AT 桥不在范围内。
