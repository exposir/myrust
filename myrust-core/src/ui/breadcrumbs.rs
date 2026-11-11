//! Clickable path breadcrumbs.

use super::context::Context;

/// One breadcrumb path item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreadcrumbItem<'a> {
    /// Stable id returned when the item is clicked.
    pub id: &'a str,
    /// Visible label.
    pub label: &'a str,
}

impl<'a> BreadcrumbItem<'a> {
    /// Creates a breadcrumb item.
    pub const fn new(id: &'a str, label: &'a str) -> Self {
        Self { id, label }
    }
}

/// Result of constructing breadcrumbs for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BreadcrumbsResponse {
    /// Id of the item clicked this frame, if any.
    pub clicked: Option<String>,
}

/// Creates a row of breadcrumb links.
///
/// Every item except the last is a button-like interaction and can be
/// activated by click or by focused Enter/Space. The final item is the current
/// page and is rendered as a label with no interaction region.
pub fn breadcrumbs(
    context: &mut Context,
    id: &str,
    items: &[BreadcrumbItem<'_>],
) -> BreadcrumbsResponse {
    context.breadcrumbs(id, items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::access::Role;
    use crate::ui::event::{PointerButton, UiEvent};
    use crate::ui::layout::Size;
    use std::cell::RefCell;

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
    fn breadcrumbs_return_clicked_ids_and_leave_the_last_item_inert() {
        let size = Size::new(420.0, 140.0);
        let mut context = Context::new(size);
        let items = [
            BreadcrumbItem::new("home", "Home"),
            BreadcrumbItem::new("projects", "Projects"),
            BreadcrumbItem::new("current", "Current"),
        ];
        let clicked = RefCell::new(None);
        let mut build = |context: &mut Context| {
            *clicked.borrow_mut() = breadcrumbs(context, "crumbs", &items).clicked;
        };

        let initial = render(&mut context, size, None, &mut build);
        assert!(initial
            .interactions
            .iter()
            .any(|region| region.button_id == "crumbs:crumb:0"));
        assert!(initial
            .interactions
            .iter()
            .any(|region| region.button_id == "crumbs:crumb:1"));
        assert!(!initial
            .interactions
            .iter()
            .any(|region| region.button_id == "crumbs:crumb:2"));
        assert!(initial
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Button && node.label == "Home"));
        assert!(initial
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Label && node.label == "Current"));

        click(&mut context, size, "crumbs:crumb:0", &mut build);
        assert_eq!(clicked.borrow().as_deref(), Some("home"));
        click(&mut context, size, "crumbs:crumb:1", &mut build);
        assert_eq!(clicked.borrow().as_deref(), Some("projects"));

        let current = initial
            .access_nodes
            .iter()
            .find(|node| node.role == Role::Label && node.label == "Current")
            .expect("current breadcrumb label");
        let point = crate::ui::layout::Point::new(
            current.bounds.x + current.bounds.width / 2.0,
            current.bounds.y + current.bounds.height / 2.0,
        );
        *clicked.borrow_mut() = Some("stale".to_string());
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert_eq!(
            clicked.borrow().as_deref(),
            None,
            "the last breadcrumb must not be clickable"
        );
    }
}
