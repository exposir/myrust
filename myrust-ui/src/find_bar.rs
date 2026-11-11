//! Find bar with match count and previous/next navigation.

use super::context::Context;

/// Result of constructing a find bar for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FindBarState {
    /// Whether the query or current match changed.
    pub changed: bool,
    /// Current match index after this frame's input.
    pub current: usize,
    /// Total match count supplied by the caller.
    pub matches: usize,
    /// Current query text.
    pub query: String,
    /// Whether previous was clicked.
    pub previous_clicked: bool,
    /// Whether next was clicked.
    pub next_clicked: bool,
    /// Whether the text input owns focus.
    pub focused: bool,
    /// Whether the pointer is over the input.
    pub hovered: bool,
}

/// Creates a find bar.
///
/// `current` is clamped to `0..matches`; previous/next wrap around. When
/// `matches == 0`, both navigation buttons are disabled.
pub fn find_bar(
    context: &mut Context,
    id: &str,
    query: &mut String,
    current: &mut usize,
    matches: usize,
) -> FindBarState {
    context.find_bar(id, query, current, matches)
}

/// Counts case-insensitive non-overlapping occurrences of `query` in
/// `haystack`. An empty query returns zero.
pub fn count_matches(haystack: &str, query: &str) -> usize {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }
    haystack.to_lowercase().match_indices(&query).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{PointerButton, UiEvent};
    use crate::layout::Size;
    use crate::paint::DrawCommand;
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
    fn find_bar_wraps_previous_and_next_counts() {
        let size = Size::new(360.0, 90.0);
        let mut context = Context::new(size);
        let query = Rc::new(RefCell::new("needle".to_string()));
        let current = Rc::new(RefCell::new(0_usize));
        let matches = Rc::new(RefCell::new(3_usize));
        let last = Rc::new(RefCell::new(FindBarState::default()));
        let view_query = Rc::clone(&query);
        let view_current = Rc::clone(&current);
        let view_matches = Rc::clone(&matches);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut query = view_query.borrow_mut();
            let mut current = view_current.borrow_mut();
            *view_last.borrow_mut() = find_bar(
                context,
                "find",
                &mut query,
                &mut current,
                *view_matches.borrow(),
            );
        };

        click(&mut context, size, "find:next", &mut build);
        assert_eq!(*current.borrow(), 1);
        assert!(last.borrow().next_clicked);
        click(&mut context, size, "find:next", &mut build);
        assert_eq!(*current.borrow(), 2);
        click(&mut context, size, "find:next", &mut build);
        assert_eq!(*current.borrow(), 0, "next wraps to the first match");
        click(&mut context, size, "find:prev", &mut build);
        assert_eq!(*current.borrow(), 2, "previous wraps to the last match");
        assert!(last.borrow().previous_clicked);
        let frame = render(&mut context, size, None, &mut build);
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "3/3")));

        *current.borrow_mut() = 1;
        *matches.borrow_mut() = 0;
        let zero = render(&mut context, size, None, &mut build);
        assert!(!zero
            .interactions
            .iter()
            .any(|region| region.button_id == "find:next"));
        assert!(zero
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "0/0")));
        assert_eq!(count_matches("Foo foo foo", "foo"), 3);
        assert_eq!(count_matches("anything", ""), 0);
    }
}
