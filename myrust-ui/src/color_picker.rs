//! Color swatch with preset palette and optional hex input.
//!
//! [`color_picker`] is a 28px swatch that opens a palette popup.
//! [`color_panel`] is the always-open panel MyGo's `ColorPicker` draws: a
//! saturation/brightness square, hue and opacity sliders, the color in hex,
//! and swatches of common colors.

use super::context::Context;
use super::paint::Color;

/// Colors a panel offers, in MyGo's order (`ui/colorpicker.go` swatches).
pub const SWATCH_COLORS: [Color; 12] = [
    Color::rgb(0xef, 0x44, 0x44),
    Color::rgb(0xf9, 0x73, 0x16),
    Color::rgb(0xea, 0xb3, 0x08),
    Color::rgb(0x22, 0xc5, 0x5e),
    Color::rgb(0x14, 0xb8, 0xa6),
    Color::rgb(0x3b, 0x82, 0xf6),
    Color::rgb(0x63, 0x66, 0xf1),
    Color::rgb(0xa8, 0x55, 0xf7),
    Color::rgb(0xec, 0x48, 0x99),
    Color::rgb(0x92, 0x40, 0x0e),
    Color::rgb(0x71, 0x71, 0x7a),
    Color::rgb(0x00, 0x00, 0x00),
];

/// The names of [`SWATCH_COLORS`], as MyGo labels them.
pub const SWATCH_NAMES: [&str; 12] = [
    "red", "orange", "yellow", "green", "teal", "blue", "indigo", "purple", "pink", "brown",
    "gray", "black",
];

/// A color as hue, saturation, value and alpha, which a panel keeps: a
/// color of red, green and blue loses the hue of its grays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    /// Hue in degrees, 0..360.
    pub hue: f32,
    /// Saturation, 0..1.
    pub saturation: f32,
    /// Brightness, 0..1.
    pub value: f32,
    /// Alpha, 0..1.
    pub alpha: f32,
}

impl Hsv {
    /// Converts a color to its hue, saturation, value and alpha.
    pub fn from_color(color: Color) -> Self {
        let red = f32::from(color.r) / 255.0;
        let green = f32::from(color.g) / 255.0;
        let blue = f32::from(color.b) / 255.0;
        let high = red.max(green).max(blue);
        let low = red.min(green).min(blue);
        let delta = high - low;
        let hue = if delta <= f32::EPSILON {
            0.0
        } else if high == red {
            60.0 * ((green - blue) / delta).rem_euclid(6.0)
        } else if high == green {
            60.0 * ((blue - red) / delta + 2.0)
        } else {
            60.0 * ((red - green) / delta + 4.0)
        };
        Self {
            hue: if hue < 0.0 { hue + 360.0 } else { hue },
            saturation: if high > 0.0 { delta / high } else { 0.0 },
            value: high,
            alpha: f32::from(color.a) / 255.0,
        }
    }

    /// Converts back to a color.
    pub fn to_color(self) -> Color {
        let chroma = self.value * self.saturation;
        let sector = (self.hue.rem_euclid(360.0)) / 60.0;
        let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
        let (red, green, blue) = match sector as u32 {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let base = self.value - chroma;
        let channel = |part: f32| ((part + base) * 255.0).round().clamp(0.0, 255.0) as u8;
        Color::rgba(
            channel(red),
            channel(green),
            channel(blue),
            (self.alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        )
    }

    /// Clamps every channel into range, as a panel does before setting one.
    pub fn clamped(self) -> Self {
        Self {
            hue: if self.hue.is_finite() {
                self.hue.rem_euclid(360.0)
            } else {
                0.0
            },
            saturation: self.saturation.clamp(0.0, 1.0),
            value: self.value.clamp(0.0, 1.0),
            alpha: self.alpha.clamp(0.0, 1.0),
        }
    }
}

/// Caller-owned state of an always-open [`color_panel`].
#[derive(Debug, Clone, PartialEq)]
pub struct ColorPanelState {
    /// The color as the panel keeps it, so a gray keeps its hue.
    pub hsv: Hsv,
    /// The color the panel last set or saw from the caller.
    pub last: Color,
    /// The text of the hex field, as typed.
    pub hex: String,
    /// Which of the square, hue and opacity controls the pointer drags.
    pub dragging: Option<PanelDrag>,
}

/// The control of a [`color_panel`] the pointer is dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelDrag {
    /// The saturation/brightness square.
    Square,
    /// The hue slider.
    Hue,
    /// The opacity slider.
    Opacity,
}

impl ColorPanelState {
    /// Creates the state of a panel showing `color`.
    pub fn new(color: Color) -> Self {
        Self {
            hsv: Hsv::from_color(color),
            last: color,
            hex: hex_of(color),
            dragging: None,
        }
    }
}

impl Default for ColorPanelState {
    fn default() -> Self {
        Self::new(Color::BLACK)
    }
}

/// Result of drawing a [`color_panel`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorPanelResponse {
    /// The color after this frame.
    pub color: Color,
    /// Whether this frame changed the color.
    pub changed: bool,
}

/// Returns `c` as `#rrggbb`, or `#rrggbbaa` when it is not opaque.
pub fn hex_of(color: Color) -> String {
    if color.a == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", color.r, color.g, color.b, color.a)
    }
}

/// Draws MyGo's `ColorPicker`: a square choosing the saturation, across,
/// and the brightness, up, of the hue its slider chooses below it, a slider
/// of opacity, the color in hex, which the user edits, and swatches of
/// common colors (`ui/colorpicker.go` coreColorPicker).
pub fn color_panel(
    context: &mut Context,
    id: &str,
    color: &mut Color,
    state: &mut ColorPanelState,
) -> ColorPanelResponse {
    context.color_panel(id, color, state)
}

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
    /// Hue in degrees, 0..359. Kept when saturation or value is zero.
    pub hue: u16,
    /// Saturation percentage, 0..100.
    pub saturation: u8,
    /// Value percentage, 0..100.
    pub value: u8,
    /// Alpha channel, 0..255.
    pub alpha: u8,
}

impl Default for ColorPickerState {
    fn default() -> Self {
        Self {
            open: false,
            hex: "#000000FF".to_string(),
            hue: 0,
            saturation: 0,
            value: 0,
            alpha: 255,
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
    use crate::event::{Key, PointerButton, UiEvent};
    use crate::layout::Size;
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
        let size = Size::new(360.0, 480.0);
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
