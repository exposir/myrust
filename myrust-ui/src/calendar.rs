//! Month calendar.

use super::context::Context;

/// Caller-owned calendar state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarState {
    /// Displayed year.
    pub year: i32,
    /// Displayed month, 1..=12.
    pub month: u32,
    /// Selected day, if any.
    pub selected: Option<u32>,
}

impl Default for CalendarState {
    fn default() -> Self {
        Self {
            year: 2024,
            month: 1,
            selected: Some(1),
        }
    }
}

impl CalendarState {
    /// Creates a calendar state.
    pub fn new(year: i32, month: u32, selected: Option<u32>) -> Self {
        Self {
            year,
            month,
            selected,
        }
    }
}

/// Result of constructing a calendar for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CalendarResponse {
    /// Displayed year.
    pub year: i32,
    /// Displayed month.
    pub month: u32,
    /// Selected day.
    pub selected: Option<u32>,
    /// Whether selection or displayed month changed.
    pub changed: bool,
    /// Day clicked this frame.
    pub clicked: Option<u32>,
}

/// Creates a 6x7 month calendar.
///
/// Clicking a day selects it. While focused, arrows move by day/week, Home/End
/// move to the first/last day of the month, and PageUp/PageDown move to the
/// previous/next month while clamping the selected day.
pub fn calendar(context: &mut Context, id: &str, state: &mut CalendarState) -> CalendarResponse {
    context.calendar(id, state)
}

pub(crate) fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub(crate) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) as i64
}

pub(crate) fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        (year + i64::from(month <= 2)) as i32,
        month as u32,
        day as u32,
    )
}

pub(crate) fn weekday_monday(year: i32, month: u32, day: u32) -> u32 {
    (days_from_civil(year, month, day) + 3).rem_euclid(7) as u32
}

pub(crate) fn move_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let total = month as i32 - 1 + delta;
    (
        year + total.div_euclid(12),
        (total.rem_euclid(12) + 1) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
    use std::cell::RefCell;

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        state: &RefCell<CalendarState>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        calendar(context, "cal", &mut state.borrow_mut());
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
    fn calendar_click_arrows_and_page_month_changes() {
        let size = Size::new(420.0, 420.0);
        let mut context = Context::new(size);
        let state = RefCell::new(CalendarState::new(2024, 2, Some(10)));
        let initial = render(&mut context, size, None, &state);
        let point = center_of(&initial, "cal:day:15");
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
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
            &state,
        );
        assert_eq!(state.borrow().selected, Some(15));

        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowRight,
            }),
            &state,
        );
        assert_eq!(state.borrow().selected, Some(16));
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &state,
        );
        assert_eq!(state.borrow().selected, Some(23));
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::PageDown }),
            &state,
        );
        assert_eq!(state.borrow().month, 3);
        assert_eq!(state.borrow().selected, Some(23));
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::PageUp }),
            &state,
        );
        assert_eq!(state.borrow().month, 2);
        assert_eq!(state.borrow().selected, Some(23));
    }
}
