//! Read-only formatted text.

use super::context::Context;
use super::paint::Color;

/// One run of rich text with its own background or color, as MyGo's
/// `ui.Span` in `RichText`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RichTextSpan {
    /// The run's text.
    pub text: String,
    /// Optional background behind the run, a search highlight.
    pub background: Option<Color>,
    /// Optional text color.
    pub color: Option<Color>,
    /// Numeric font weight, conventionally `100..=900`.
    pub weight: u16,
    /// Whether the run is drawn in the italic face.
    pub italic: bool,
    /// Whether the run is drawn in the monospaced face.
    pub mono: bool,
    /// Whether a line is drawn through the run's middle.
    pub strikethrough: bool,
    /// Font size in logical pixels; zero uses the theme's body size.
    pub font_size: f32,
    /// Corner radius of the run's background.
    pub background_radius: f32,
}

impl RichTextSpan {
    /// Creates a plain span.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            background: None,
            color: None,
            weight: 400,
            italic: false,
            mono: false,
            strikethrough: false,
            font_size: 0.0,
            background_radius: 0.0,
        }
    }

    /// Sets the span's background.
    pub const fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Sets the span's text color.
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the span's font weight.
    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    /// Draws the span in the italic face.
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Draws the span in the monospaced face.
    pub const fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// Draws a line through the span's middle.
    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Sets the span's font size, which also scales its code pills.
    pub const fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Rounds the span's background to `radius`.
    pub const fn background_radius(mut self, radius: f32) -> Self {
        self.background_radius = radius;
        self
    }
}

/// Renders rich text spans that wrap across lines, as MyGo's `RichText`.
pub fn rich_text_spans(
    context: &mut Context,
    id: &str,
    spans: &[RichTextSpan],
) -> RichTextResponse {
    context.rich_text_spans(id, spans)
}

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
    use crate::access::Role;
    use crate::layout::Size;
    use crate::paint::{Color, DrawCommand, Transform};

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
