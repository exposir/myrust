//! Read-only code viewer with line numbers and lightweight Rust highlighting.

use super::context::Context;

/// Color class for one code token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeTokenKind {
    /// Plain text, whitespace, or punctuation.
    Text,
    /// Rust keyword.
    Keyword,
    /// String or character literal.
    String,
    /// Line comment.
    Comment,
    /// Numeric literal.
    Number,
}

/// One highlighted source token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeToken {
    /// Token text, including whitespace.
    pub text: String,
    /// Highlight class.
    pub kind: CodeTokenKind,
}

/// Tokenizes one line for the built-in Rust highlighter.
pub fn tokenize_rust_line(line: &str) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let mut index = 0;
    let bytes = line.as_bytes();
    while index < line.len() {
        let start = index;
        if line[index..].starts_with("//") {
            tokens.push(CodeToken {
                text: line[index..].to_string(),
                kind: CodeTokenKind::Comment,
            });
            break;
        }
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            while index < line.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            tokens.push(CodeToken {
                text: line[start..index].to_string(),
                kind: CodeTokenKind::Text,
            });
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            index += 1;
            while index < line.len() {
                let current = bytes[index];
                index += 1;
                if current == b'\\' {
                    index = (index + 1).min(line.len());
                } else if current == byte {
                    break;
                }
            }
            tokens.push(CodeToken {
                text: line[start..index].to_string(),
                kind: CodeTokenKind::String,
            });
            continue;
        }
        if byte.is_ascii_digit() {
            while index < line.len()
                && (bytes[index].is_ascii_digit() || bytes[index] == b'.' || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: line[start..index].to_string(),
                kind: CodeTokenKind::Number,
            });
            continue;
        }
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            while index < line.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let text = &line[start..index];
            tokens.push(CodeToken {
                text: text.to_string(),
                kind: if is_rust_keyword(text) {
                    CodeTokenKind::Keyword
                } else {
                    CodeTokenKind::Text
                },
            });
            continue;
        }
        index += 1;
        tokens.push(CodeToken {
            text: line[start..index].to_string(),
            kind: CodeTokenKind::Text,
        });
    }
    tokens
}

fn is_rust_keyword(word: &str) -> bool {
    matches!(
        word,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

/// Caller-owned code viewer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeEditorState {
    /// Selected zero-based line.
    pub selected_line: Option<usize>,
}

/// Result of constructing a code viewer for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeEditorResponse {
    /// Selected zero-based line.
    pub selected_line: Option<usize>,
    /// Line clicked this frame.
    pub clicked_line: Option<usize>,
    /// Number of visible lines.
    pub line_count: usize,
    /// Whether the selected line changed.
    pub changed: bool,
}

/// Creates a read-only code viewer.
///
/// Clicking a line selects it. Keyword, string, comment, and number spans are
/// painted with distinct colors; editing, completion, and folding are out of
/// scope.
pub fn code_editor(
    context: &mut Context,
    id: &str,
    code: &str,
    state: &mut CodeEditorState,
) -> CodeEditorResponse {
    context.code_editor(id, code, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{PointerButton, UiEvent};
    use crate::layout::Size;
    use std::cell::RefCell;

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        code: &str,
        state: &RefCell<CodeEditorState>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        code_editor(context, "code", code, &mut state.borrow_mut());
        let frame = context.produce().unwrap();
        context.end_frame();
        frame
    }

    fn center_of(frame: &super::super::native::ViewFrame, id: &str) -> crate::layout::Point {
        let region = frame
            .interactions
            .iter()
            .find(|region| region.button_id == id)
            .unwrap_or_else(|| panic!("missing interaction region {id}"));
        crate::layout::Point::new(
            region.bounds.x + region.bounds.width / 2.0,
            region.bounds.y + region.bounds.height / 2.0,
        )
    }

    #[test]
    fn tokenizes_rust_and_clicks_lines() {
        let tokens = tokenize_rust_line("let answer = 42; // note");
        assert!(tokens
            .iter()
            .any(|token| { token.text == "let" && token.kind == CodeTokenKind::Keyword }));
        assert!(tokens
            .iter()
            .any(|token| { token.text == "42" && token.kind == CodeTokenKind::Number }));
        assert!(tokens
            .iter()
            .any(|token| { token.text == "// note" && token.kind == CodeTokenKind::Comment }));

        let size = Size::new(360.0, 160.0);
        let mut context = Context::new(size);
        let state = RefCell::new(CodeEditorState::default());
        let code = "fn main() {\n    let x = 1;\n}";
        let initial = render(&mut context, size, None, code, &state);
        assert_eq!(state.borrow().selected_line, None);
        let point = center_of(&initial, "code:line:1");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            code,
            &state,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            code,
            &state,
        );
        assert_eq!(state.borrow().selected_line, Some(1));
    }
}
