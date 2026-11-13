//! Grouped, single-selection source list.

use super::context::Context;

/// One selectable sidebar item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarItem<'a> {
    /// Stable item id returned on selection.
    pub id: &'a str,
    /// Visible label.
    pub label: &'a str,
    /// Optional built-in icon name.
    pub icon: Option<&'a str>,
    /// Optional trailing badge text (e.g. an unread count).
    pub badge: Option<&'a str>,
}

impl<'a> SidebarItem<'a> {
    /// Creates a sidebar item without an icon or badge.
    pub const fn new(id: &'a str, label: &'a str) -> Self {
        Self { id, label, icon: None, badge: None }
    }

    /// Adds an optional built-in icon.
    pub const fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Adds an optional trailing badge.
    pub const fn badge(mut self, badge: &'a str) -> Self {
        self.badge = Some(badge);
        self
    }
}

/// One titled group of sidebar items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarSection<'a> {
    /// Section heading.
    pub title: &'a str,
    /// Items in this section.
    pub items: &'a [SidebarItem<'a>],
}

impl<'a> SidebarSection<'a> {
    /// Creates a sidebar section.
    pub const fn new(title: &'a str, items: &'a [SidebarItem<'a>]) -> Self {
        Self { title, items }
    }
}

/// Result of constructing a sidebar for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SidebarResponse {
    /// Selected item id after this frame's input.
    pub selected_id: String,
    /// Whether the selection changed this frame.
    pub changed: bool,
    /// Item clicked this frame, if any.
    pub clicked_id: Option<String>,
    /// Whether the sidebar owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the sidebar.
    pub hovered: bool,
}

/// Creates a grouped single-selection sidebar.
///
/// The caller owns `selected`. ArrowUp/ArrowLeft, ArrowDown/ArrowRight, Home,
/// and End move the selection while the sidebar is focused. Clicking an item
/// selects it, focuses the list, and returns the clicked id.
pub fn sidebar(
    context: &mut Context,
    id: &str,
    selected: &mut String,
    sections: &[SidebarSection<'_>],
) -> SidebarResponse {
    context.sidebar(id, selected, sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::container;
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
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
    fn sidebar_clicks_and_arrow_keys_change_the_selected_id() {
        let size = Size::new(300.0, 280.0);
        let mut context = Context::new(size);
        let workspace = [
            SidebarItem::new("inbox", "Inbox"),
            SidebarItem::new("sent", "Sent"),
        ];
        let settings = [SidebarItem::new("profile", "Profile")];
        let selected = Rc::new(RefCell::new("inbox".to_string()));
        let last = Rc::new(RefCell::new(SidebarResponse::default()));
        let view_selected = Rc::clone(&selected);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let sections = [
                SidebarSection::new("Workspace", &workspace),
                SidebarSection::new("Settings", &settings),
            ];
            let mut selected = view_selected.borrow_mut();
            container(context).size(220.0, 240.0).children(|context| {
                *view_last.borrow_mut() = sidebar(context, "nav", &mut selected, &sections);
            });
        };

        let initial = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().selected_id, "inbox");
        assert!(!last.borrow().changed);
        assert!(initial.access_nodes.iter().any(|node| {
            node.role == Role::Option
                && node.label == "Inbox"
                && node.value.as_deref() == Some("selected")
        }));

        click(&mut context, size, "nav:item:1", &mut build);
        assert_eq!(last.borrow().selected_id, "sent");
        assert_eq!(last.borrow().clicked_id.as_deref(), Some("sent"));
        assert!(last.borrow().changed);
        assert_eq!(*selected.borrow(), "sent");

        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(last.borrow().selected_id, "profile");
        assert!(last.borrow().changed);
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(
            last.borrow().selected_id,
            "profile",
            "selection clamps at the end"
        );
        assert!(!last.borrow().changed);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::ArrowUp }),
            &mut build,
        );
        assert_eq!(last.borrow().selected_id, "sent");
    }
}
