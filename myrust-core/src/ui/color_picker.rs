//! Color swatch with preset palette and optional hex input.

use super::context::Context;
use super::paint::Color;

/// Preset palette colors.
pub const PRESET_COLORS: [Color; 12] = [
    Color::rgb(0, 0, 0),
    Color::rgb(255, 255, 255),
    Color::rgb(239, 68, 68),
    Color::rgb(249, 115, 22),
    Color::rgb(245, 158, 11),
    Color::rgb(34, 197, 94),
    Color::rgb(20, 184, 166),
    Color::rgb(6, 182, 212),
    Color::rgb(59, 130, 246),
    Color::rgb(99, 102, 241),
    Color::rgb(168, 85, 247),
    Color::rgb(236, 72, 153),
];

/// Caller-owned color picker state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPickerState {
    /// Whether the palette popup is open.
    pub open: bool,
    /// Hex input buffer.
    pub hex: String,
}

impl Default for ColorPickerState {
    fn default() -> Self {
        Self {
            open: false,
            hex: "#000000FF".to_string(),
        }
    }
}

/// Result of constructing a color picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPickerResponse {
    /// Current color.
    pub color: Color,
    /// Whether the color changed.
    pub changed: bool,
    /// Whether the popup remains open.
    pub open: bool,
    /// Preset index clicked this frame.
    pub preset: Option<usize>,
    /// Whether the last hex submission parsed.
    pub valid_hex: bool,
}

/// Creates a color swatch that opens a preset palette and hex input.
pub fn color_picker(
    context: &mut Context,
    id: &str,
    color: &mut Color,
    state: &mut ColorPickerState,
) -> ColorPickerResponse {
    context.color_picker(id, color, state)
}

impl Default for ColorPickerResponse {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            changed: false,
            open: false,
            preset: None,
            valid_hex: true,
        }
    }
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
        color: &Rc<RefCell<Color>>,
        state: &Rc<RefCell<ColorPickerState>>,
        last: &Rc<RefCell<ColorPickerResponse>>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        *last.borrow_mut() = color_picker(
            context,
            "picker",
            &mut color.borrow_mut(),
            &mut state.borrow_mut(),
        );
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
        color: &Rc<RefCell<Color>>,
        state: &Rc<RefCell<ColorPickerState>>,
        last: &Rc<RefCell<ColorPickerResponse>>,
    ) -> super::super::native::ViewFrame {
        let initial = render(context, size, None, color, state, last);
        let point = center_of(&initial, id);
        render(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            color,
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
            color,
            state,
            last,
        )
    }

    #[test]
    fn color_picker_selects_presets_and_validates_hex() {
        let size = Size::new(300.0, 280.0);
        let mut context = Context::new(size);
        let color = Rc::new(RefCell::new(Color::rgb(255, 0, 0)));
        let state = Rc::new(RefCell::new(ColorPickerState::default()));
        let last = Rc::new(RefCell::new(ColorPickerResponse::default()));
        click(&mut context, size, "picker", &color, &state, &last);
        assert!(state.borrow().open);
        click(&mut context, size, "picker:preset:8", &color, &state, &last);
        assert_eq!(*color.borrow(), PRESET_COLORS[8]);
        assert_eq!(last.borrow().preset, Some(8));
        assert!(!state.borrow().open);

        click(&mut context, size, "picker", &color, &state, &last);
        render(&mut context, size, None, &color, &state, &last);
        state.borrow_mut().hex = "#00FF00FF".to_string();
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &color,
            &state,
            &last,
        );
        assert_eq!(*color.borrow(), Color::rgb(0, 255, 0));
        assert!(last.borrow().valid_hex);

        state.borrow_mut().hex = "not-a-color".to_string();
        render(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &color,
            &state,
            &last,
        );
        assert_eq!(*color.borrow(), Color::rgb(0, 255, 0));
        assert!(!last.borrow().valid_hex);
        assert_eq!(state.borrow().hex, color.borrow().to_hex());
    }
}
