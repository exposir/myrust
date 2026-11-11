`diff_viewer(c, "id", old, new, &mut state)` 计算 line LCS diff，并以 unified 或 paired side-by-side 绘制。`DiffViewerState` 只存 layout；两段文本由调用方持有且只读。

```rust
use myrust_core::ui::{diff_viewer, DiffViewerState};

let mut state = DiffViewerState::default();
let response = diff_viewer(c, "diff", before, after, &mut state);
app.diff_lines = response.lines;
```

## Layout

`DiffLayout::Unified` 用单列 `+`/`-` 前缀和 old/new 行号。`DiffLayout::SideBySide` 把变更块配对到两列；顶部按钮切换 layout。

side-by-side 的配对是变更块内按序，不是 intraline matcher。长行按文本宽度裁切；没有 wrap 或水平滚动。

## Editing and review

新文本不可编辑。没有 accept/reject、inline comment、hunk navigation、staging 或 merge 动作；viewer 只读。

## Styling

增/删背景由 `theme.surface` 与 green/red 混合。没有逐 diff palette、line-height、font 或 border 选项。

## Patches

没有 unified diff/patch export、import、apply 或 source hunk 元数据。`diff_lines` 返回 `DiffLine`（kind/text/old_line/new_line）；调用方可自行序列化 patch 格式。

## Accessibility

根发出 `Role::Group`，label 为控件 id。diff 行只是视觉：不发 line/cell AccessNode，也不宣告增删文本。

没有 diff/table/list role、行数、变更数或 AT 桥。`DiffViewerResponse` 向调用方暴露 added/removed/same 计数。
