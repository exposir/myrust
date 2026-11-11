//! HH:MM time input with step buttons.

use super::context::Context;

/// Result of constructing a time input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimeInputState {
    /// Whether the text changed.
    pub changed: bool,
    /// Whether the text parses to a valid 24-hour time.
    pub valid: bool,
    /// Parsed hour.
    pub hour: u32,
    /// Parsed minute.
    pub minute: u32,
    /// Whether Enter normalized the text.
    pub submitted: bool,
    /// Whether the input owns focus.
    pub focused: bool,
    /// Whether the pointer is over the input.
    pub hovered: bool,
}

/// Creates a `HH:MM` time input with hour/minute step buttons.
///
/// The text buffer accepts digits and one colon. Step buttons clamp at
/// `00:00..=23:59`; Enter normalizes a valid value to two-digit fields.
pub fn time_input(context: &mut Context, id: &str, text: &mut String) -> TimeInputState {
    context.time_input(id, text)
}

pub(crate) fn parse_time(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (hour, minute) = if let Some((hour, minute)) = text.split_once(':') {
        if minute.contains(':') {
            return None;
        }
        (hour, minute)
    } else if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
        (&text[..2], &text[2..])
    } else {
        return None;
    };
    if hour.is_empty()
        || minute.is_empty()
        || hour.len() > 2
        || minute.len() > 2
        || !hour.chars().all(|c| c.is_ascii_digit())
        || !minute.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

pub(crate) fn filter_time_text(raw: &str) -> String {
    let mut output = String::new();
    let mut colon = false;
    for character in raw.chars() {
        if output.len() >= 5 {
            break;
        }
        if character.is_ascii_digit() {
            output.push(character);
        } else if character == ':' && !colon {
            output.push(':');
            colon = true;
        }
    }
    output
}

pub(crate) fn format_time(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
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
        last: &Rc<RefCell<TimeInputState>>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        *last.borrow_mut() = time_input(context, "time", &mut text.borrow_mut());
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
        last: &Rc<RefCell<TimeInputState>>,
    ) -> super::super::native::ViewFrame {
        let initial = render(context, size, None, text, last);
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
            last,
        )
    }

    #[test]
    fn time_input_parses_steps_and_clamps() {
        let size = Size::new(320.0, 100.0);
        let mut context = Context::new(size);
        let text = Rc::new(RefCell::new(String::new()));
        let last = Rc::new(RefCell::new(TimeInputState::default()));
        click(&mut context, size, "time:input", &text, &last);
        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "23:59".to_string(),
            }),
            &text,
            &last,
        );
        assert_eq!(*text.borrow(), "23:59");
        assert!(last.borrow().valid);
        assert_eq!(last.borrow().hour, 23);
        assert_eq!(last.borrow().minute, 59);

        click(&mut context, size, "time:hour_inc", &text, &last);
        assert_eq!(*text.borrow(), "23:59");
        assert_eq!(last.borrow().hour, 23);
        click(&mut context, size, "time:minute_inc", &text, &last);
        assert_eq!(*text.borrow(), "23:59");
        assert_eq!(last.borrow().minute, 59);
        click(&mut context, size, "time:minute_dec", &text, &last);
        assert_eq!(*text.borrow(), "23:58");

        *text.borrow_mut() = "24:00".to_string();
        render(&mut context, size, None, &text, &last);
        assert!(!last.borrow().valid);

        *text.borrow_mut() = "9:5".to_string();
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &text,
            &last,
        );
        assert_eq!(*text.borrow(), "09:05");
        assert!(last.borrow().submitted);
        assert!(last.borrow().valid);
    }
}
