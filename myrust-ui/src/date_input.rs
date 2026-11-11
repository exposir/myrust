//! ISO date input with calendar popup.

use super::calendar::CalendarState;
use super::context::Context;
use super::languages::{format_date, DateStyle, Locale};

/// Caller-owned date input state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateInputState {
    /// Whether the calendar popup is open.
    pub open: bool,
    /// Calendar selection and displayed month.
    pub calendar: CalendarState,
}

impl DateInputState {
    /// Creates a closed state with an explicit calendar.
    pub fn new(calendar: CalendarState) -> Self {
        Self {
            open: false,
            calendar,
        }
    }
}

/// Result of constructing a date input for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateInputResponse {
    /// Whether the popup remains open.
    pub open: bool,
    /// Whether the text parses as an ISO date.
    pub valid: bool,
    /// Whether the text or calendar selection changed.
    pub changed: bool,
    /// Whether Enter submitted a valid date.
    pub submitted: bool,
    /// Whether the text input owns focus.
    pub focused: bool,
}

/// Creates an ISO date input with an overlay calendar.
///
/// Typing a valid `YYYY-MM-DD` updates the calendar. The trailing button opens
/// the calendar; clicking a day updates the text and closes the popup. Escape
/// and outside clicks close the overlay through the shared overlay primitive.
pub fn date_input(
    context: &mut Context,
    id: &str,
    text: &mut String,
    state: &mut DateInputState,
) -> DateInputResponse {
    context.date_input(id, text, state)
}

pub(crate) fn parse_iso_date(text: &str) -> Option<(i32, u32, u32)> {
    if format_date(text, Locale::EnUs, DateStyle::Iso).is_err() {
        return None;
    }
    let mut parts = text.split('-');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
    use std::{cell::RefCell, rc::Rc};

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        text: &Rc<RefCell<String>>,
        state: &Rc<RefCell<DateInputState>>,
        last: &Rc<RefCell<DateInputResponse>>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        *last.borrow_mut() = date_input(
            context,
            "date",
            &mut text.borrow_mut(),
            &mut state.borrow_mut(),
        );
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
        text: &Rc<RefCell<String>>,
        state: &Rc<RefCell<DateInputState>>,
        last: &Rc<RefCell<DateInputResponse>>,
    ) -> super::super::native::ViewFrame {
        let initial = render(context, size, None, text, state, last);
        let point = center_of(&initial, id);
        render(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            text,
            state,
            last,
        );
        render(
            context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            text,
            state,
            last,
        )
    }

    #[test]
    fn date_input_parses_iso_and_selects_from_calendar() {
        let size = Size::new(320.0, 360.0);
        let mut context = Context::new(size);
        let text = Rc::new(RefCell::new("2024-02-29".to_string()));
        let state = Rc::new(RefCell::new(DateInputState::default()));
        let last = Rc::new(RefCell::new(DateInputResponse::default()));
        click(&mut context, size, "date:input", &text, &state, &last);
        render(&mut context, size, None, &text, &state, &last);
        assert!(last.borrow().valid);
        assert_eq!(state.borrow().calendar.year, 2024);
        assert_eq!(state.borrow().calendar.month, 2);
        assert_eq!(state.borrow().calendar.selected, Some(29));

        click(&mut context, size, "date:toggle", &text, &state, &last);
        assert!(state.borrow().open);
        click(
            &mut context,
            size,
            "date:calendar:day:15",
            &text,
            &state,
            &last,
        );
        assert_eq!(*text.borrow(), "2024-02-15");
        assert!(!state.borrow().open);

        click(&mut context, size, "date:toggle", &text, &state, &last);
        assert!(state.borrow().open);
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &text,
            &state,
            &last,
        );
        assert!(!state.borrow().open);
        *text.borrow_mut() = "2024-13-01".to_string();
        render(&mut context, size, None, &text, &state, &last);
        assert!(!last.borrow().valid);
        assert_eq!(state.borrow().calendar.month, 2);
    }
}
