MyRust has a read-only `rich_text`, not an editor. It renders paragraphs of inline Markdown with bold, italic, code and link labels from a caller-owned string.

```rust
use myrust_core::ui::rich_text;

rich_text(c, "note", "A **bold** word and `code` with a [link](https://example.com).");
```

## Text

The source is a plain `String`; there is no attributed-text model, span tree API, style runs, font/color overrides or per-character metadata. Inline emphasis is parsed each frame.

## Documents

There is no document model, rich clipboard format, serialization, undo history or multi-section editor. A document is just the string passed to `rich_text`/`markdown`.

## The editor

There is no editor, caret, selection, IME or command stack. `rich_text` is read-only; `editable_text`/`text_field` edit plain text, not formatted documents.

## Theme

Colors and font sizes come from the current `Theme`; inline code/link/emphasis use built-in colors. There is no rich-text theme override or CSS-like style cascade.

## Paragraphs on screen

Paragraphs are split on blank lines and laid out as inline rows. Long paragraphs do not wrap around controls or respect a width automatically; they can clip.

## The caret on screen

There is no caret, selection highlight, drag handle or scroll-to-caret. `TextInputState` exists only for text fields, not rich text.

## Typing at the edges of formats

No editing at format boundaries exists. Text cannot be typed into bold/italic/link runs; the document is rebuilt from the source string.

## Attachments

No images, files, embeds, mentions or attachment model exist. Use separate `image`/`text` nodes for media; they are not part of the rich-text run.

## Copy and paste

There is no rich text copy/paste, selection serialization or clipboard format negotiation. Callers can copy the source string themselves; no structured clipboard exists.

## Input methods

No IME composition, preedit, candidate window or bidi input. `rich_text` does not accept input; plain text fields also lack IME.

## Accessibility

The root emits `Role::Group`, label = control id, value = `"N spans"`. Each inline span emits `Role::Label`, label = span text, value = `plain`/`bold`/`italic`/`code`/`link`.

There is no editor/document role, selection/caret metadata, link target, error state or AT bridge.

## Large documents

Every frame re-parses and re-lays out the source; there is no virtualization, incremental parse or retained paragraph cache. Large documents cost proportionally and are not intended for editing.

## Markdown

`rich_text` uses the Markdown inline parser only: bold/italic/code/links inside paragraphs. `markdown` renders headings, paragraphs and lists as blocks; neither is CommonMark and neither supports code fences/tables/HTML.
