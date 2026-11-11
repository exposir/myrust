//! Minimal Markdown parser and renderer.

use super::context::Context;

/// Inline formatting applied to one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineStyle {
    /// Unformatted text.
    Plain,
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Inline code.
    Code,
    /// Link label with a target.
    Link {
        /// Link target URL.
        target: String,
    },
}

/// One inline text span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineSpan {
    /// Span text.
    pub text: String,
    /// Span formatting.
    pub style: InlineStyle,
}

impl InlineSpan {
    /// Creates an inline span.
    pub fn new(text: impl Into<String>, style: InlineStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// One parsed Markdown block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    /// Heading with level 1..=6.
    Heading {
        /// Heading level.
        level: u8,
        /// Inline heading content.
        spans: Vec<InlineSpan>,
    },
    /// Paragraph.
    Paragraph(Vec<InlineSpan>),
    /// Unordered list items.
    UnorderedList(Vec<Vec<InlineSpan>>),
    /// Ordered list items.
    OrderedList(Vec<Vec<InlineSpan>>),
}

/// Result of rendering Markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MarkdownResponse {
    /// Number of rendered blocks.
    pub blocks: usize,
}

/// Parses and renders basic Markdown using text/layout nodes.
pub fn markdown(context: &mut Context, id: &str, source: &str) -> MarkdownResponse {
    context.markdown(id, source)
}

/// Parses basic Markdown blocks.
///
/// Supported blocks are headings 1..=6, paragraphs, unordered lists (`- `,
/// `* `), and ordered lists (`1. `). Paragraph continuation lines are joined
/// with spaces.
pub fn parse_markdown(source: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut unordered: Vec<Vec<InlineSpan>> = Vec::new();
    let mut ordered: Vec<Vec<InlineSpan>> = Vec::new();

    let flush_paragraph = |paragraph: &mut String, blocks: &mut Vec<MarkdownBlock>| {
        if !paragraph.trim().is_empty() {
            let text = std::mem::take(paragraph);
            blocks.push(MarkdownBlock::Paragraph(parse_inline(text.trim())));
        }
    };
    let flush_lists = |unordered: &mut Vec<Vec<InlineSpan>>,
                       ordered: &mut Vec<Vec<InlineSpan>>,
                       blocks: &mut Vec<MarkdownBlock>| {
        if !unordered.is_empty() {
            blocks.push(MarkdownBlock::UnorderedList(std::mem::take(unordered)));
        }
        if !ordered.is_empty() {
            blocks.push(MarkdownBlock::OrderedList(std::mem::take(ordered)));
        }
    };

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_lists(&mut unordered, &mut ordered, &mut blocks);
            continue;
        }
        if let Some((level, text)) = heading(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_lists(&mut unordered, &mut ordered, &mut blocks);
            blocks.push(MarkdownBlock::Heading {
                level,
                spans: parse_inline(text),
            });
            continue;
        }
        if let Some(text) = unordered_item(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            if !ordered.is_empty() {
                flush_lists(&mut unordered, &mut ordered, &mut blocks);
            }
            unordered.push(parse_inline(text));
            continue;
        }
        if let Some(text) = ordered_item(trimmed) {
            flush_paragraph(&mut paragraph, &mut blocks);
            if !unordered.is_empty() {
                flush_lists(&mut unordered, &mut ordered, &mut blocks);
            }
            ordered.push(parse_inline(text));
            continue;
        }
        flush_lists(&mut unordered, &mut ordered, &mut blocks);
        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(trimmed);
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    flush_lists(&mut unordered, &mut ordered, &mut blocks);
    blocks
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let level = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=6).contains(&level) {
        return None;
    }
    line[level..]
        .strip_prefix(' ')
        .map(|text| (level as u8, text.trim()))
}

fn unordered_item(line: &str) -> Option<&str> {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .map(str::trim)
}

fn ordered_item(line: &str) -> Option<&str> {
    let digits = line
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    line[digits..].strip_prefix(". ").map(str::trim)
}

/// Parses inline formatting: bold, italic, inline code, and links.
pub fn parse_inline(text: &str) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        if let Some(after) = rest.strip_prefix("**").or_else(|| rest.strip_prefix("__")) {
            if let Some(end) = after.find("**").or_else(|| after.find("__")) {
                push_plain(&mut spans, &mut plain);
                spans.push(InlineSpan::new(&after[..end], InlineStyle::Bold));
                index += 2 + end + 2;
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                push_plain(&mut spans, &mut plain);
                spans.push(InlineSpan::new(&after[..end], InlineStyle::Code));
                index += 1 + end + 1;
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('[') {
            if let Some(label_end) = after.find("](") {
                let target_start = label_end + 2;
                if let Some(target_end) = after[target_start..].find(')') {
                    let label = &after[..label_end];
                    let target = &after[target_start..target_start + target_end];
                    push_plain(&mut spans, &mut plain);
                    spans.push(InlineSpan::new(
                        label,
                        InlineStyle::Link {
                            target: target.to_string(),
                        },
                    ));
                    index += 1 + label_end + 2 + target_end + 1;
                    continue;
                }
            }
        }
        if rest.starts_with('*') || rest.starts_with('_') {
            let marker = &rest[..1];
            if let Some(end) = rest[1..].find(marker) {
                push_plain(&mut spans, &mut plain);
                spans.push(InlineSpan::new(&rest[1..1 + end], InlineStyle::Italic));
                index += 1 + end + 1;
                continue;
            }
        }
        let character = rest.chars().next().expect("non-empty rest");
        plain.push(character);
        index += character.len_utf8();
    }
    push_plain(&mut spans, &mut plain);
    spans
}

fn push_plain(spans: &mut Vec<InlineSpan>, plain: &mut String) {
    if !plain.is_empty() {
        spans.push(InlineSpan::new(std::mem::take(plain), InlineStyle::Plain));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::layout::Size;
    use crate::paint::DrawCommand;

    #[test]
    fn parses_headings_lists_and_inline_styles() {
        let blocks = parse_markdown(
            "# Title\n\nText with `code` and [docs](https://example.com).\n\n- one\n- two\n\n1. first\n2. second",
        );
        assert_eq!(blocks.len(), 4);
        let MarkdownBlock::Heading { level, spans } = &blocks[0] else {
            panic!("heading");
        };
        assert_eq!(*level, 1);
        assert_eq!(spans[0].text, "Title");
        let MarkdownBlock::Paragraph(spans) = &blocks[1] else {
            panic!("paragraph");
        };
        assert!(spans
            .iter()
            .any(|span| matches!(span.style, InlineStyle::Code) && span.text == "code"));
        assert!(spans.iter().any(|span| {
            matches!(&span.style, InlineStyle::Link { target } if target == "https://example.com")
        }));
        assert!(matches!(&blocks[2], MarkdownBlock::UnorderedList(items) if items.len() == 2));
        assert!(matches!(&blocks[3], MarkdownBlock::OrderedList(items) if items.len() == 2));

        let inline = parse_inline("**bold** *italic* `code` [link](url)");
        assert!(inline
            .iter()
            .any(|span| matches!(span.style, InlineStyle::Bold)));
        assert!(inline
            .iter()
            .any(|span| matches!(span.style, InlineStyle::Italic)));
        assert!(inline
            .iter()
            .any(|span| matches!(span.style, InlineStyle::Code)));
        assert!(inline.iter().any(|span| {
            matches!(&span.style, InlineStyle::Link { target } if target == "url")
        }));
    }

    #[test]
    fn renders_heading_paragraph_and_list_nodes() {
        let size = Size::new(420.0, 260.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let response = markdown(
            &mut context,
            "md",
            "# Title\n\nParagraph with `code`.\n\n- one\n- two\n\n1. first",
        );
        let frame = context.produce().unwrap();
        assert_eq!(response.blocks, 4);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "md"));
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, font_size, .. } if text == "Title" && *font_size == 26.0)));
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "one")));
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "•")));
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "1.")));
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Label
                && node.label == "code"
                && node.value.as_deref() == Some("code")
        }));
    }
}
