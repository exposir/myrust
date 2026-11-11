`code_editor(c, "code", source, &mut state)` is a read-only line-numbered viewer with lightweight Rust highlighting. State holds the selected line.

```rust
use myrust_core::ui::CodeEditorState;

let mut state = CodeEditorState::default();
let response = code_editor(c, "code", source, &mut state);
if let Some(line) = response.clicked_line {
    app.selected_line = line;
}
```

## Buffers

There is no buffer model, file loader, undo stack or multi-buffer view. One `&str` is rendered per call; the caller owns the source and persistence.

## Editing

Editing is not implemented: no caret, selection, insertion, deletion, undo, save or key handling beyond line selection. Use `text_field` for editable plain text.

## Options

There are no options for font family, line height, gutter width, wrapping, folding, tabs or read-only/editable flags. The viewer has fixed line height and gutter.

## Languages and highlighting

Only a Rust-ish keyword/string/comment/number tokenizer is built in. `tokenize_rust_line` is public and returns `CodeToken`/`CodeTokenKind`; it is a line scanner, not a parser.

### Highlighters

No highlighter registry, grammar files, embedded languages or custom token classes exist. Other languages render as plain text with the same tokenizer.

## Completions, hovers and diagnostics

None of these exist. There is no language server, completion popup, hover tooltip, diagnostic underline or problem list.

## Code snippets

No snippet syntax, tab stops, placeholder editing or snippet registry exists. Snippets are just source text.

## Styling

Keyword/string/comment/number colors are fixed or theme-derived; there is no per-editor palette. Gutter and selected-line colors come from the current `Theme`.

### Themes

Changing `NativeUi`'s `Theme` changes accent/muted/background, but string/number colors are hardcoded. There is no code-editor theme token set.

### Bars and popups

No minimap, scrollbar, status bar, find widget or hover popup exists. The viewer is a plain column in normal layout; use `scroll_view` for long source.

## Accessibility

The root emits `Role::Group` with the control id as label. Each line emits `Role::Button`, label `"line N"`, value `"selected"`/`"unselected"`, not focusable individually.

There is no code-editor/textbox role, syntax semantics, line/column selection metadata or AT bridge.
