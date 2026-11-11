`diff_viewer(c, "id", old, new, &mut state)` computes a line LCS diff and draws it unified or paired side-by-side. `DiffViewerState` stores only the layout; both texts are caller-owned and read-only.

```rust
use myrust_core::ui::{diff_viewer, DiffViewerState};

let mut state = DiffViewerState::default();
let response = diff_viewer(c, "diff", before, after, &mut state);
app.diff_lines = response.lines;
```

## Layout

`DiffLayout::Unified` uses one column with `+`/`-` prefixes and old/new line numbers. `DiffLayout::SideBySide` pairs changed blocks into two columns; a top button toggles the layout.

Side-by-side pairing is sequential within a changed block, not an intraline matcher. Long lines are clipped by text width; there is no wrap or horizontal scroll.

## Editing and review

The new text is not editable. There are no accept/reject controls, inline comments, hunk navigation, staging or merge actions; the viewer is read-only.

## Styling

Added/removed backgrounds derive from `theme.surface` mixed with green/red. There is no per-diff palette, line-height, font or border option.

## Patches

No unified diff/patch export, import, apply or source hunk metadata exists. `diff_lines` returns `DiffLine` structs with kind/text/old_line/new_line; callers can serialize their own patch format.

## Accessibility

The root emits `Role::Group` with the control id. Diff rows are visual only: they do not emit line/cell AccessNodes or announce added/removed text.

There is no diff/table/list role, row count, change count or AT bridge. `DiffViewerResponse` exposes added/removed/same counts to the caller.
