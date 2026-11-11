//! Pressed-state toggle button with optional grouping.

use super::context::Context;

/// Caller-owned single-active toggle group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToggleGroup {
    /// Active toggle id, if any.
    pub active: Option<String>,
}

impl ToggleGroup {
    /// Creates an empty group.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing a toggle for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToggleState {
    /// Whether the toggle is active after this frame.
    pub active: bool,
    /// Whether this frame changed the active state.
    pub changed: bool,
    /// Whether the toggle was clicked or activated by Space.
    pub clicked: bool,
    /// Whether the toggle owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the toggle.
    pub hovered: bool,
    /// Active id of the shared group, if a group was supplied.
    pub group_active: Option<String>,
}

/// Creates a pressed-state toggle.
///
/// Without a group, `active` is caller-owned and toggles independently. With
/// a group, the group's single active id is the source of truth; activating
/// one toggle clears the others and activating the current one clears the
/// group.
pub fn toggle(
    context: &mut Context,
    id: &str,
    label: &str,
    active: &mut bool,
    group: Option<&mut ToggleGroup>,
) -> ToggleState {
    context.toggle(id, label, active, group)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn standalone_toggle_clicks_and_space_toggle() {
        let size = Size::new(240.0, 100.0);
        let mut context = Context::new(size);
        let active = Rc::new(RefCell::new(false));
        let last = Rc::new(RefCell::new(ToggleState::default()));
        let view_active = Rc::clone(&active);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            *view_last.borrow_mut() =
                toggle(context, "bold", "Bold", &mut view_active.borrow_mut(), None);
        };
        click(&mut context, size, "bold", &mut build);
        assert!(*active.borrow());
        assert!(last.borrow().clicked);
        click(&mut context, size, "bold", &mut build);
        assert!(!*active.borrow());
        context.request_focus("bold");
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Space }),
            &mut build,
        );
        assert!(*active.borrow());
    }

    #[test]
    fn grouped_toggles_keep_one_active_id() {
        let size = Size::new(300.0, 120.0);
        let mut context = Context::new(size);
        let group = Rc::new(RefCell::new(ToggleGroup::new()));
        let left = Rc::new(RefCell::new(false));
        let right = Rc::new(RefCell::new(false));
        let last = Rc::new(RefCell::new(ToggleState::default()));
        let view_group = Rc::clone(&group);
        let view_left = Rc::clone(&left);
        let view_right = Rc::clone(&right);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut group = view_group.borrow_mut();
            let _ = toggle(
                context,
                "left",
                "Left",
                &mut view_left.borrow_mut(),
                Some(&mut group),
            );
            *view_last.borrow_mut() = toggle(
                context,
                "right",
                "Right",
                &mut view_right.borrow_mut(),
                Some(&mut group),
            );
        };
        click(&mut context, size, "left", &mut build);
        assert!(*left.borrow());
        assert!(!*right.borrow());
        assert_eq!(group.borrow().active.as_deref(), Some("left"));
        click(&mut context, size, "right", &mut build);
        assert!(*right.borrow());
        assert_eq!(group.borrow().active.as_deref(), Some("right"));
        render(&mut context, size, None, &mut build);
        assert!(!*left.borrow());
        click(&mut context, size, "right", &mut build);
        assert!(!*right.borrow());
        assert_eq!(group.borrow().active, None);
        render(&mut context, size, None, &mut build);
        assert!(!*left.borrow());
    }
}
