//! Numeric text-field wrappers.

use super::context::Context;

/// Options for [`number_input`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberInputOptions {
    /// Whether a leading minus sign is accepted.
    pub allow_negative: bool,
    /// Maximum fraction digits. Zero means integer-only.
    pub decimals: usize,
    /// Amount added or subtracted by ArrowUp/ArrowDown.
    pub step: f64,
}

impl Default for NumberInputOptions {
    fn default() -> Self {
        Self {
            allow_negative: false,
            decimals: 0,
            step: 1.0,
        }
    }
}

impl NumberInputOptions {
    /// Sets whether negative values are accepted.
    pub const fn allow_negative(mut self, allow: bool) -> Self {
        self.allow_negative = allow;
        self
    }

    /// Sets the maximum fraction digits.
    pub const fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// Sets the ArrowUp/ArrowDown step.
    pub const fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }
}

/// Result of constructing a number input for one frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumberInputState {
    /// Whether the text or parsed value changed.
    pub changed: bool,
    /// Parsed numeric value, or `None` for an incomplete/invalid buffer.
    pub value: Option<f64>,
    /// Whether the text field owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the text field.
    pub hovered: bool,
}

/// Options for [`decimal_input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalInputOptions {
    /// Whether a leading minus sign is accepted.
    pub allow_negative: bool,
    /// Exact number of fraction digits used for the scaled integer.
    pub decimals: usize,
}

impl Default for DecimalInputOptions {
    fn default() -> Self {
        Self {
            allow_negative: true,
            decimals: 2,
        }
    }
}

/// Result of constructing a decimal input for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecimalInputState {
    /// Whether the filtered text changed.
    pub changed: bool,
    /// Exact scaled integer, e.g. `1234` for `"12.34"` with two decimals.
    pub units: Option<i64>,
    /// Whether the current buffer parses as a fixed-point decimal.
    pub valid: bool,
    /// Whether the text field owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the text field.
    pub hovered: bool,
}

/// Creates a text field that accepts only numeric characters.
///
/// Illegal characters are dropped, extra fraction digits are discarded, and
/// ArrowUp/ArrowDown step the parsed value using [`NumberInputOptions::step`].
pub fn number_input(
    context: &mut Context,
    id: &str,
    text: &mut String,
    options: NumberInputOptions,
) -> NumberInputState {
    context.number_input(id, text, options)
}

/// Creates a fixed-point decimal text field.
///
/// Extra fraction digits are truncated, never rounded. The exact scaled
/// integer is reported through [`DecimalInputState::units`]. Use
/// [`format_decimal_units`] to render a canonical fixed-decimal string.
pub fn decimal_input(
    context: &mut Context,
    id: &str,
    text: &mut String,
    options: DecimalInputOptions,
) -> DecimalInputState {
    context.decimal_input(id, text, options)
}

/// Parses `text` into an exact scaled integer at `decimals` fraction digits.
///
/// Returns `None` for empty, incomplete, illegal, or overflowing input.
pub fn parse_decimal_units(text: &str, decimals: usize) -> Option<i64> {
    let decimals = decimals.min(18);
    let text = text.trim();
    if text.is_empty() || text == "-" || text == "." || text == "-." {
        return None;
    }
    let negative = text.starts_with('-');
    let body = if negative { &text[1..] } else { text };
    if body.is_empty() {
        return None;
    }
    let mut parts = body.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some() || integer.is_empty() || !integer.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let scale = 10_i64.checked_pow(decimals as u32)?;
    let integer = integer.parse::<i64>().ok()?;
    let mut fraction_digits = String::with_capacity(decimals);
    for index in 0..decimals {
        fraction_digits.push(fraction.chars().nth(index).unwrap_or('0'));
    }
    let fraction = if decimals == 0 {
        0
    } else {
        fraction_digits.parse::<i64>().ok()?
    };
    let units = integer.checked_mul(scale)?.checked_add(fraction)?;
    Some(if negative {
        units.checked_neg()?
    } else {
        units
    })
}

/// Formats an exact scaled integer as a fixed-decimal string.
pub fn format_decimal_units(units: i64, decimals: usize) -> String {
    let decimals = decimals.min(18);
    let scale = 10_i128.pow(decimals as u32);
    let units = units as i128;
    let negative = units < 0;
    let absolute = units.abs();
    let integer = absolute / scale;
    let fraction = absolute % scale;
    let sign = if negative { "-" } else { "" };
    if decimals == 0 {
        format!("{sign}{integer}")
    } else {
        format!("{sign}{integer}.{fraction:0width$}", width = decimals)
    }
}

pub(crate) fn filter_numeric_text(raw: &str, allow_negative: bool, decimals: usize) -> String {
    let decimals = decimals.min(18);
    let mut output = String::new();
    let mut seen_dot = false;
    let mut fraction_digits = 0;
    for (index, character) in raw.chars().enumerate() {
        if character == '-' && allow_negative && index == 0 && !output.starts_with('-') {
            output.push('-');
        } else if character.is_ascii_digit() {
            if seen_dot && fraction_digits >= decimals {
                continue;
            }
            output.push(character);
            if seen_dot {
                fraction_digits += 1;
            }
        } else if character == '.' && decimals > 0 && !seen_dot {
            output.push('.');
            seen_dot = true;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::event::{Key, UiEvent};
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

    #[test]
    fn number_input_filters_characters_and_steps_with_arrows() {
        let size = Size::new(260.0, 120.0);
        let mut context = Context::new(size);
        let text = Rc::new(RefCell::new(String::new()));
        let last = Rc::new(RefCell::new(NumberInputState::default()));
        let view_text = Rc::clone(&text);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut text = view_text.borrow_mut();
            *view_last.borrow_mut() = number_input(
                context,
                "number",
                &mut text,
                NumberInputOptions::default()
                    .allow_negative(false)
                    .decimals(1)
                    .step(1.0),
            );
        };
        context.request_focus("number");
        render(&mut context, size, None, &mut build);

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "12a.5".to_string(),
            }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "12.5");
        assert!(last.borrow().changed);
        assert_eq!(last.borrow().value, Some(12.5));

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "9".to_string(),
            }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "12.5", "extra decimals are discarded");

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "-".to_string(),
            }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "12.5", "minus is rejected when disabled");

        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::ArrowUp }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "13.5");
        assert_eq!(last.borrow().value, Some(13.5));
        render(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "12.5");
        assert_eq!(last.borrow().value, Some(12.5));
    }

    #[test]
    fn decimal_input_truncates_fraction_digits_without_rounding() {
        let size = Size::new(280.0, 120.0);
        let mut context = Context::new(size);
        let text = Rc::new(RefCell::new(String::new()));
        let last = Rc::new(RefCell::new(DecimalInputState::default()));
        let view_text = Rc::clone(&text);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut text = view_text.borrow_mut();
            *view_last.borrow_mut() = decimal_input(
                context,
                "money",
                &mut text,
                DecimalInputOptions {
                    allow_negative: true,
                    decimals: 2,
                },
            );
        };
        context.request_focus("money");
        render(&mut context, size, None, &mut build);

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "12.345".to_string(),
            }),
            &mut build,
        );
        assert_eq!(*text.borrow(), "12.34", "digits are truncated, not rounded");
        assert_eq!(last.borrow().units, Some(1234));
        assert!(last.borrow().valid);
        assert!(last.borrow().changed);

        render(
            &mut context,
            size,
            Some(UiEvent::TextInput {
                text: "x9".to_string(),
            }),
            &mut build,
        );
        assert_eq!(
            *text.borrow(),
            "12.34",
            "illegal characters and extra digits are rejected"
        );
        assert_eq!(last.borrow().units, Some(1234));

        *text.borrow_mut() = "-12.345".to_string();
        render(&mut context, size, None, &mut build);
        assert_eq!(*text.borrow(), "-12.34");
        assert_eq!(last.borrow().units, Some(-1234));
        assert_eq!(format_decimal_units(-1234, 2), "-12.34");
    }
}
