//! Modal dialogs and alert dialogs.
//!
//! [`dialog`] centers a caller-built panel in the window. It reuses the
//! overlay layer, focus trap, lower-layer pointer blocking, and focus
//! restoration, but deliberately ignores outside clicks: a modal dialog is
//! dismissed by Escape or by the content returning a [`DialogAction`].

use super::context::Context;

/// What a dialog's content decided during the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogAction {
    /// Keep the dialog open.
    #[default]
    None,
    /// Confirm and close the dialog.
    Confirm,
    /// Cancel and close the dialog.
    Cancel,
}

/// Result of constructing a dialog for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DialogResponse {
    /// Whether the dialog remains open after this frame.
    pub open: bool,
    /// Whether this frame closed the dialog.
    pub closed: bool,
    /// Whether the content returned [`DialogAction::Confirm`].
    pub confirmed: bool,
    /// Whether the content returned [`DialogAction::Cancel`].
    pub cancelled: bool,
    /// Whether Escape closed the dialog.
    pub escape_pressed: bool,
}

/// Creates a modal centered dialog.
///
/// `open` is caller-owned. The content closure builds the panel body and
/// returns a [`DialogAction`]; returning `Confirm` or `Cancel` closes the
/// dialog and is reported through [`DialogResponse`]. Escape also closes it.
/// The panel width is clamped to the window and the panel height follows the
/// content, up to the window height.
pub fn dialog(
    context: &mut Context,
    id: &str,
    open: &mut bool,
    width: f32,
    content: impl FnOnce(&mut Context) -> DialogAction,
) -> DialogResponse {
    context.dialog(id, open, width, content)
}

/// Creates a modal alert with a title, body, and confirm/cancel buttons.
///
/// The returned response reports which button was chosen. Escape counts as a
/// cancellation and sets both `cancelled` and `escape_pressed`.
pub fn alert_dialog(
    context: &mut Context,
    id: &str,
    open: &mut bool,
    title: &str,
    message: &str,
    confirm_label: &str,
    cancel_label: &str,
) -> DialogResponse {
    context.alert_dialog(id, open, title, message, confirm_label, cancel_label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::access::Role;
    use crate::ui::event::{Key, PointerButton, UiEvent};
    use crate::ui::layout::Size;
    use std::{cell::RefCell, rc::Rc};

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        build: &mut impl FnMut(&mut Context),
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        build(context);
        let frame = context.produce().unwrap();
        context.end_frame();
        frame
    }

    fn center_of(frame: &super::super::native::ViewFrame, id: &str) -> crate::ui::layout::Point {
        let region = frame
            .interactions
            .iter()
            .find(|region| region.button_id == id)
            .unwrap_or_else(|| panic!("missing interaction region {id}"));
        crate::ui::layout::Point::new(
            region.bounds.x + region.bounds.width / 2.0,
            region.bounds.y + region.bounds.height / 2.0,
        )
    }

    fn click(
        context: &mut Context,
        size: Size,
        id: &str,
        build: &mut impl FnMut(&mut Context),
    ) -> super::super::native::ViewFrame {
        let initial = render(context, size, None, build);
        let point = center_of(&initial, id);
        render(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        );
        render(
            context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        )
    }

    #[test]
    fn dialog_traps_focus_and_escape_restores_the_trigger() {
        let size = Size::new(420.0, 320.0);
        let mut context = Context::new(size);
        let open = Rc::new(RefCell::new(true));
        let last = Rc::new(RefCell::new(DialogResponse::default()));
        let first = Rc::new(RefCell::new(true));
        let view_open = Rc::clone(&open);
        let view_last = Rc::clone(&last);
        let view_first = Rc::clone(&first);
        let mut build = move |context: &mut Context| {
            if *view_first.borrow() {
                context.request_focus("base");
                *view_first.borrow_mut() = false;
            }
            context.id("base").button("Base");
            let mut open = view_open.borrow_mut();
            let action = DialogAction::None;
            let response = dialog(context, "dialog", &mut open, 360.0, |context| {
                context.id("dialog-first").button("First");
                context.id("dialog-second").button("Second");
                action
            });
            *view_last.borrow_mut() = response;
        };

        render(&mut context, size, None, &mut build);
        let focused = render(&mut context, size, None, &mut build);
        assert!(focused
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Button && node.label == "First" && node.focused }));
        assert!(!focused
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Button && node.label == "Base" && node.focused }));

        let second = render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Tab }),
            &mut build,
        );
        assert!(second
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Button && node.label == "Second" && node.focused }));
        let trapped = render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Tab }),
            &mut build,
        );
        assert!(trapped
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Button && node.label == "First" && node.focused }));
        assert!(!trapped
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Button && node.label == "Base" && node.focused }));

        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!*open.borrow());
        assert!(last.borrow().closed);
        assert!(last.borrow().escape_pressed);
        let restored = render(&mut context, size, None, &mut build);
        assert!(restored
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Button && node.label == "Base" && node.focused }));
    }

    #[test]
    fn dialog_confirm_button_closes_and_returns_confirmed() {
        let size = Size::new(420.0, 320.0);
        let mut context = Context::new(size);
        let open = Rc::new(RefCell::new(true));
        let last = Rc::new(RefCell::new(DialogResponse::default()));
        let view_open = Rc::clone(&open);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut open = view_open.borrow_mut();
            let mut action = DialogAction::None;
            let response = dialog(context, "dialog", &mut open, 360.0, |context| {
                if context.id("dialog-confirm").button("Confirm").clicked {
                    action = DialogAction::Confirm;
                }
                action
            });
            *view_last.borrow_mut() = response;
        };

        render(&mut context, size, None, &mut build);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert!(!*open.borrow());
        assert!(last.borrow().confirmed);
        assert!(last.borrow().closed);
        assert!(!last.borrow().open);
    }

    #[test]
    fn alert_dialog_buttons_return_the_choice() {
        let size = Size::new(440.0, 340.0);
        let mut context = Context::new(size);
        let open = Rc::new(RefCell::new(true));
        let last = Rc::new(RefCell::new(DialogResponse::default()));
        let view_open = Rc::clone(&open);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut open = view_open.borrow_mut();
            let response = alert_dialog(
                context,
                "alert",
                &mut open,
                "Delete project?",
                "This cannot be undone.",
                "Delete",
                "Cancel",
            );
            *view_last.borrow_mut() = response;
        };

        click(&mut context, size, "alert:confirm", &mut build);
        assert!(!*open.borrow());
        assert!(last.borrow().confirmed);
        assert!(!last.borrow().cancelled);

        *open.borrow_mut() = true;
        click(&mut context, size, "alert:cancel", &mut build);
        assert!(!*open.borrow());
        assert!(last.borrow().cancelled);
        assert!(!last.borrow().confirmed);

        *open.borrow_mut() = true;
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!*open.borrow());
        assert!(last.borrow().cancelled);
        assert!(last.borrow().escape_pressed);
    }
}
