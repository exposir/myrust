//! Form field layout helpers.
//!
//! A form field is layout plus semantics, not a state container. The caller
//! keeps its input values and passes the current validation error. The field
//! registers a group node whose accessible label is the visible label, so
//! assistive technology can associate the label with the controls inside.

use super::context::Context;

/// Result of laying out one form field for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormFieldState {
    /// Whether an error message was rendered.
    pub has_error: bool,
    /// Whether a description was rendered.
    pub has_description: bool,
}

/// Lays out a label, optional description, caller content, and error message.
///
/// The error text uses [`super::theme::Theme::warn`] and the group node's
/// value is `"error"` or `"valid"`. `content` usually builds a text field,
/// select, checkbox, or another input control.
pub fn form_field(
    context: &mut Context,
    id: &str,
    label: &str,
    description: Option<&str>,
    error: Option<&str>,
    content: impl FnOnce(&mut Context),
) -> FormFieldState {
    context.form_field(id, label, description, error, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::layout::Size;
    use crate::paint::DrawCommand;

    #[test]
    fn form_field_reports_error_state_and_associates_the_label() {
        let size = Size::new(420.0, 280.0);
        let mut context = Context::new(size);
        let mut email = String::new();
        context.begin_frame(size);
        let state = context.form_field(
            "email-form",
            "Email",
            Some("We only use this for notifications."),
            Some("Email is required"),
            |context| {
                context.text_field("email", &mut email);
            },
        );
        let frame = context.produce().unwrap();
        assert!(state.has_error);
        assert!(state.has_description);

        let group = frame
            .access_nodes
            .iter()
            .find(|node| node.role == Role::Group && node.label == "Email")
            .expect("form group semantics");
        assert_eq!(group.value.as_deref(), Some("error"));
        let input = frame
            .access_nodes
            .iter()
            .find(|node| node.role == Role::TextField)
            .expect("text field semantics");
        assert!(
            group.bounds.x <= input.bounds.x
                && group.bounds.y <= input.bounds.y
                && group.bounds.right() >= input.bounds.right()
                && group.bounds.bottom() >= input.bounds.bottom(),
            "the labeled group must contain its input"
        );
        let warn = context.theme().warn();
        assert!(frame.display.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Text { text, color, .. }
                    if text == "Email is required" && *color == warn
            )
        }));
        context.end_frame();

        context.begin_frame(size);
        let valid = context.form_field(
            "email-form",
            "Email",
            Some("We only use this for notifications."),
            None,
            |context| {
                context.text_field("email", &mut email);
            },
        );
        let valid_frame = context.produce().unwrap();
        assert!(!valid.has_error);
        let group = valid_frame
            .access_nodes
            .iter()
            .find(|node| node.role == Role::Group && node.label == "Email")
            .expect("form group semantics");
        assert_eq!(group.value.as_deref(), Some("valid"));
        assert!(!valid_frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "Email is required")));
        context.end_frame();
    }
}
