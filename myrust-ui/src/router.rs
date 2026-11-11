//! Small UI-level page router and history buttons.

use super::context::Context;

/// One route page rendered by a caller closure.
#[derive(Clone, Copy)]
pub struct Page<'a> {
    /// Stable page id.
    pub id: &'a str,
    /// Page content closure.
    pub render: &'a dyn Fn(&mut Context),
}

impl<'a> Page<'a> {
    /// Creates a page.
    pub const fn new(id: &'a str, render: &'a dyn Fn(&mut Context)) -> Self {
        Self { id, render }
    }
}

/// Caller-owned page history.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RouterState {
    history: Vec<String>,
    index: usize,
}

impl RouterState {
    /// Creates a router at one page.
    pub fn new(page: &str) -> Self {
        Self {
            history: vec![page.to_string()],
            index: 0,
        }
    }

    /// Current page id.
    pub fn current(&self) -> &str {
        self.history
            .get(self.index)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Whether a previous page exists.
    pub fn can_back(&self) -> bool {
        self.index > 0
    }

    /// Whether a next page exists.
    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.history.len()
    }

    /// Full history.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Pushes a page, discarding any forward history.
    pub fn push(&mut self, page: &str) {
        self.history.truncate(self.index + 1);
        self.history.push(page.to_string());
        self.index = self.history.len() - 1;
    }

    /// Removes the current page and returns to the previous one.
    ///
    /// The root page is never removed; at the root this returns `None`.
    pub fn pop(&mut self) -> Option<String> {
        if self.index == 0 || self.history.is_empty() {
            return None;
        }
        let removed = self.history.remove(self.index);
        self.index -= 1;
        Some(removed)
    }

    /// Moves back one page; returns whether the index moved.
    pub fn back(&mut self) -> bool {
        if self.can_back() {
            self.index -= 1;
            true
        } else {
            false
        }
    }

    /// Moves forward one page; returns whether the index moved.
    pub fn forward(&mut self) -> bool {
        if self.can_forward() {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

/// Result of rendering the router.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RouterResponse {
    /// Current page id.
    pub page: String,
    /// Whether a matching page closure was rendered.
    pub rendered: bool,
    /// Whether history has a previous page.
    pub can_back: bool,
    /// Whether history has a next page.
    pub can_forward: bool,
}

/// Renders the current page and updates the router group semantics.
///
/// Unknown page ids render a short "Page not found" label and report
/// `rendered = false`.
pub fn router(
    context: &mut Context,
    id: &str,
    state: &mut RouterState,
    pages: &[Page<'_>],
) -> RouterResponse {
    context.router(id, state, pages)
}

/// Result of constructing back/forward buttons.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackForwardState {
    /// Whether back was clicked.
    pub back_clicked: bool,
    /// Whether forward was clicked.
    pub forward_clicked: bool,
    /// Enabled state used to paint back.
    pub back_enabled: bool,
    /// Enabled state used to paint forward.
    pub forward_enabled: bool,
    /// Current page after navigation.
    pub page: String,
}

/// Creates back/forward buttons driven by [`RouterState`].
pub fn back_forward_buttons(
    context: &mut Context,
    id: &str,
    state: &mut RouterState,
) -> BackForwardState {
    context.back_forward_buttons(id, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::context::text;
    use crate::event::{PointerButton, UiEvent};
    use crate::layout::Size;
    use crate::paint::DrawCommand;

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
    fn router_state_push_pop_back_and_forward() {
        let mut state = RouterState::new("home");
        assert_eq!(state.current(), "home");
        state.push("detail");
        state.push("settings");
        assert_eq!(state.current(), "settings");
        assert!(state.can_back());
        assert!(!state.can_forward());
        assert!(state.back());
        assert_eq!(state.current(), "detail");
        assert!(state.forward());
        assert_eq!(state.current(), "settings");
        assert!(state.back());
        state.push("about");
        assert_eq!(state.history(), ["home", "detail", "about"]);
        assert!(!state.can_forward());
        assert_eq!(state.pop().as_deref(), Some("about"));
        assert_eq!(state.current(), "detail");
        assert!(state.back());
        assert_eq!(state.current(), "home");
        assert!(!state.back());
    }

    #[test]
    fn router_renders_the_current_page_closure() {
        let size = Size::new(320.0, 180.0);
        let mut context = Context::new(size);
        let mut state = RouterState::new("home");
        state.push("detail");
        let home = |context: &mut Context| {
            text(context, "Home page");
        };
        let detail = |context: &mut Context| {
            text(context, "Detail page");
        };
        let pages = [Page::new("home", &home), Page::new("detail", &detail)];
        let mut build = move |context: &mut Context| {
            router(context, "router", &mut state, &pages);
        };
        let frame = render(&mut context, size, None, &mut build);
        assert!(frame.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Detail page")
        ));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "detail"));
    }

    #[test]
    fn history_buttons_enable_from_history_and_navigate() {
        let size = Size::new(260.0, 100.0);
        let mut context = Context::new(size);
        let state = std::rc::Rc::new(std::cell::RefCell::new(RouterState::new("home")));
        let last = std::rc::Rc::new(std::cell::RefCell::new(BackForwardState::default()));
        let view_state = std::rc::Rc::clone(&state);
        let view_last = std::rc::Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() = back_forward_buttons(context, "hist", &mut state);
        };
        let initial = render(&mut context, size, None, &mut build);
        assert!(!last.borrow().back_enabled);
        assert!(!last.borrow().forward_enabled);
        assert!(!initial
            .interactions
            .iter()
            .any(|region| region.button_id == "hist:back"));

        state.borrow_mut().push("detail");
        render(&mut context, size, None, &mut build);
        assert!(last.borrow().back_enabled);
        assert!(!last.borrow().forward_enabled);
        click(&mut context, size, "hist:back", &mut build);
        assert!(last.borrow().back_clicked);
        assert_eq!(state.borrow().current(), "home");
        assert!(last.borrow().forward_enabled);
        click(&mut context, size, "hist:forward", &mut build);
        assert!(last.borrow().forward_clicked);
        assert_eq!(state.borrow().current(), "detail");
    }
}
