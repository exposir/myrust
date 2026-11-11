`code_editor(c, "code", source, &mut state)` 是只读行号查看器，带轻量 Rust 高亮。state 保存选中行。

```rust
use myrust_core::ui::CodeEditorState;

let mut state = CodeEditorState::default();
let response = code_editor(c, "code", source, &mut state);
if let Some(line) = response.clicked_line {
    app.selected_line = line;
}
```

## Buffers

没有 buffer 模型、文件 loader、undo stack 或多 buffer 视图。每次调用渲染一个 `&str`；source 与持久化由调用方持有。

## Editing

未实现编辑：没有 caret、selection、插入、删除、undo、save，或行选择之外的按键。可编辑纯文本用 `text_field`。

## Options

没有 font family、line height、gutter width、wrapping、folding、tab 或 editable 选项。查看器固定行高与 gutter。

## Languages and highlighting

只内置 Rust-ish keyword/string/comment/number tokenizer。`tokenize_rust_line` 公开，返回 `CodeToken`/`CodeTokenKind`；它是行扫描器，不是 parser。

### Highlighters

没有 highlighter registry、grammar 文件、embedded language 或自定义 token class。其他语言按纯文本用同一 tokenizer 渲染。

## Completions, hovers and diagnostics

都不存在。没有 language server、completion popup、hover tooltip、diagnostic underline 或 problem list。

## Code snippets

没有 snippet 语法、tab stop、placeholder 编辑或 snippet registry。snippet 只是 source 文本。

## Styling

keyword/string/comment/number 颜色固定或由 theme 派生；没有逐 editor palette。gutter 与选中行颜色来自当前 `Theme`。

### Themes

改变 `NativeUi` 的 `Theme` 会改 accent/muted/background，但 string/number 颜色硬编码。没有 code-editor theme token set。

### Bars and popups

没有 minimap、scrollbar、status bar、find widget 或 hover popup。查看器是普通布局中的 column；长源码用 `scroll_view`。

## Accessibility

根发出 `Role::Group`，label 为控件 id。每行发出 `Role::Button`，label 为 `"line N"`，value 为 `"selected"`/`"unselected"`，不能单独聚焦。

没有 code-editor/textbox role、语法语义、行列选择元数据或 AT 桥。
