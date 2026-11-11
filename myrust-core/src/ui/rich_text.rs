//! Read-only formatted text.

use super::context::Context;

/// Result of rendering rich text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RichTextResponse {
    /// Number of inline spans rendered.
    pub spans: usize,
}

/// Renders read-only formatted text using the Markdown inline parser.
///
/// Supported styles are bold, italic, inline code, and link labels. Editing is
/// intentionally out of scope; use `EditableText` or `TextField` for input.
pub fn rich_text(context: &mut Context, id: &str, source: &str) -> RichTextResponse {
    context.rich_text(id, source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::access::Role;
    use crate::ui::layout::Size;
    use crate::ui::paint::{Color, DrawCommand, Transform};

    #[test]
    fn rich_text_renders_bold_italic_code_and_link_spans() {
        let size = Size::new(460.0, 160.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let response = rich_text(
            &mut context,
            "rich",
            "Plain **bold** *italic* `code` [link](https://example.com)",
        );
        let frame = context.produce().unwrap();
        assert_eq!(response.spans, 8);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "rich"));
        let mut bold_weight = None;
        let mut rotated = false;
        let mut code_background = false;
        let mut link_color = None;
        let accent = Color::rgb(34, 211, 238);
        for command in &frame.display.commands {
            match command {
                DrawCommand::Text {
                    text,
                    weight,
                    color,
                    ..
                } if text == "bold" => bold_weight = Some(*weight),
                DrawCommand::Text { text, color, .. } if text == "link" => {
                    link_color = Some(*color)
                }
                DrawCommand::TransformPush { transform } => {
                    rotated |= *transform
                        == Transform {
                            rotation: 0.08,
                            ..Transform::default()
                        };
                }
                DrawCommand::RoundedRect { .. } => code_background = true,
                _ => {}
            }
        }
        assert_eq!(bold_weight, Some(700));
        assert!(rotated, "italic span must carry a slant transform");
        assert!(code_background, "inline code must paint a background");
        assert_eq!(link_color, Some(accent));
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Label
                && node.label == "italic"
                && node.value.as_deref() == Some("italic")
        }));
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Label
                && node.label == "link"
                && node.value.as_deref() == Some("link")
        }));
    }
}
