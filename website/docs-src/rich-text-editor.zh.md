MyRust 只有只读 `rich_text`，没有编辑器。它把调用方字符串中的段落以内联 Markdown 渲染为 bold、italic、code、link label。

```rust
use myrust_core::ui::rich_text;

rich_text(c, "note", "A **bold** word and `code` with a [link](https://example.com).");
```

## Text

source 是普通 `String`；没有 attributed-text 模型、span tree API、style run、字体/颜色覆盖或 per-character 元数据。内联强调每帧解析。

## Documents

没有 document model、rich clipboard 格式、序列化、undo history 或多 section editor。document 只是传给 `rich_text`/`markdown` 的字符串。

## The editor

没有 editor、caret、selection、IME 或 command stack。`rich_text` 只读；`editable_text`/`text_field` 编辑纯文本，不是格式化文档。

## Theme

颜色与字号来自当前 `Theme`；inline code/link/emphasis 用内置色。没有 rich-text theme override 或 CSS 式 style cascade。

## Paragraphs on screen

段落按空行拆分并以 inline row 布局。长段落不会围绕控件自动换行，也不会自动遵守宽度；可能被裁切。

## The caret on screen

没有 caret、selection highlight、drag handle 或 scroll-to-caret。`TextInputState` 只属于 text field，不属于 rich text。

## Typing at the edges of formats

格式边界没有编辑。不能向 bold/italic/link run 内输入；document 从 source 字符串重建。

## Attachments

没有 image、file、embed、mention 或 attachment 模型。用独立 `image`/`text` 节点放媒体；它们不属于 rich-text run。

## Copy and paste

没有 rich text copy/paste、selection 序列化或 clipboard 格式协商。调用方可自行复制 source 字符串；没有 structured clipboard。

## Input methods

没有 IME composition、preedit、候选窗或 bidi 输入。`rich_text` 不接受输入；普通 text field 也无 IME。

## Accessibility

根发出 `Role::Group`，label 为控件 id，value 为 `"N spans"`。每个 inline span 发出 `Role::Label`，label 为 span 文本，value 为 `plain`/`bold`/`italic`/`code`/`link`。

没有 editor/document role、selection/caret 元数据、link target、error 状态或 AT 桥。

## Large documents

每帧重新解析与布局 source；没有 virtualization、增量解析或 retained paragraph cache。大文档成本线性增长，不适合编辑。

## Markdown

`rich_text` 只用 Markdown 内联 parser：段落中的 bold/italic/code/link。`markdown` 按 block 渲染标题、段落、列表；两者都不是 CommonMark，都不支持 code fence/table/HTML。
