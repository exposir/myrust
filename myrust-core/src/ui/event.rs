//! Platform-neutral native UI input events and button hit testing.
//!
//! Tao window events are translated into these structures by the runtime.
//! Hit testing is intentionally a linear reverse-order scan over registered
//! widget rectangles. Named navigation keys are translated separately from
//! text input. IME composition, dead-key composition, accessibility trees, and
//! optimized spatial indexes are not implemented.

use super::layout::LayoutResult;
use super::paint::Rect;

/// Pointer button supplied by the windowing system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// Primary/left button.
    Left,
    /// Secondary/right button.
    Right,
    /// Middle button.
    Middle,
    /// Platform-specific extra button.
    Other(u16),
}

/// Pointer or key transition state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputState {
    /// Button or key was pressed.
    Pressed,
    /// Button or key was released.
    Released,
}

/// Keyboard keys used by immediate-mode navigation and editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Move focus to the next focusable widget.
    Tab,
    /// Move focus to the previous focusable widget.
    ShiftTab,
    /// Activate the focused control or submit a single-line edit.
    Enter,
    /// Activate the focused control or insert a space into a focused text field.
    Space,
    /// Delete the character before the cursor.
    Backspace,
    /// Delete the character after the cursor.
    Delete,
    /// Move left.
    ArrowLeft,
    /// Move right.
    ArrowRight,
    /// Move to the beginning.
    Home,
    /// Move to the end.
    End,
}

/// Platform-neutral input emitted by the window integration layer.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeInput {
    /// Cursor moved to a physical window position.
    CursorMoved {
        /// Physical x coordinate.
        x: f64,
        /// Physical y coordinate.
        y: f64,
    },
    /// Mouse button state changed at the most recent cursor position.
    MouseInput {
        /// New button state.
        state: InputState,
        /// Changed button.
        button: PointerButton,
    },
    /// Scroll wheel or touchpad movement.
    Wheel {
        /// Horizontal delta in logical pixels.
        delta_x: f32,
        /// Vertical delta in logical pixels.
        delta_y: f32,
    },
    /// Text produced by a keyboard event.
    Text(String),
    /// Named keyboard key.
    Key {
        /// Translated key.
        key: Key,
    },
}

/// Native UI event after physical-to-logical coordinate conversion.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// Pointer moved to logical window coordinates.
    PointerMoved {
        /// Logical x coordinate.
        x: f32,
        /// Logical y coordinate.
        y: f32,
    },
    /// Pointer button pressed.
    PointerPressed {
        /// Logical x coordinate.
        x: f32,
        /// Logical y coordinate.
        y: f32,
        /// Pressed button.
        button: PointerButton,
    },
    /// Pointer button released.
    PointerReleased {
        /// Logical x coordinate.
        x: f32,
        /// Logical y coordinate.
        y: f32,
        /// Released button.
        button: PointerButton,
    },
    /// Scroll event in logical pixels.
    Wheel {
        /// Horizontal delta.
        delta_x: f32,
        /// Vertical delta.
        delta_y: f32,
    },
    /// Text character or composition result supplied by the platform.
    TextInput {
        /// Text produced by the keyboard event.
        text: String,
    },
    /// Named keyboard key with UI navigation semantics.
    Key {
        /// Pressed key.
        key: Key,
    },
}

/// Converts physical cursor events into logical UI events.
#[derive(Debug, Clone)]
pub struct InputTranslator {
    scale_factor: f64,
    cursor_x: f32,
    cursor_y: f32,
}

impl InputTranslator {
    /// Creates a translator for a display scale factor.
    pub fn new(scale_factor: f64) -> Self {
        Self {
            scale_factor: normalize_scale(scale_factor),
            cursor_x: 0.0,
            cursor_y: 0.0,
        }
    }

    /// Updates the display scale factor.
    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.scale_factor = normalize_scale(scale_factor);
    }

    /// Returns the latest logical cursor position.
    pub fn cursor_position(&self) -> (f32, f32) {
        (self.cursor_x, self.cursor_y)
    }

    /// Converts one platform-neutral native input event.
    pub fn translate(&mut self, input: &NativeInput) -> Option<UiEvent> {
        match input {
            NativeInput::CursorMoved { x, y } => {
                self.cursor_x = (*x / self.scale_factor) as f32;
                self.cursor_y = (*y / self.scale_factor) as f32;
                Some(UiEvent::PointerMoved {
                    x: self.cursor_x,
                    y: self.cursor_y,
                })
            }
            NativeInput::MouseInput { state, button } => {
                let (x, y) = self.cursor_position();
                Some(match state {
                    InputState::Pressed => UiEvent::PointerPressed {
                        x,
                        y,
                        button: *button,
                    },
                    InputState::Released => UiEvent::PointerReleased {
                        x,
                        y,
                        button: *button,
                    },
                })
            }
            NativeInput::Wheel { delta_x, delta_y } => Some(UiEvent::Wheel {
                delta_x: *delta_x,
                delta_y: *delta_y,
            }),
            NativeInput::Text(text) if !text.is_empty() => {
                Some(UiEvent::TextInput { text: text.clone() })
            }
            NativeInput::Text(_) => None,
            NativeInput::Key { key } => Some(UiEvent::Key { key: *key }),
        }
    }
}

/// Interactive button rectangle in logical window coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractiveRegion {
    /// Layout node id that owns the interaction.
    pub node_id: u64,
    /// Logical bounds.
    pub bounds: Rect,
    /// Stable application-defined button id.
    pub button_id: String,
}

impl InteractiveRegion {
    /// Creates an interactive button region.
    pub fn new(node_id: u64, bounds: Rect, button_id: impl Into<String>) -> Self {
        Self {
            node_id,
            bounds,
            button_id: button_id.into(),
        }
    }

    /// Returns whether a logical point lies inside the region.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.bounds.x
            && y >= self.bounds.y
            && x <= self.bounds.right()
            && y <= self.bounds.bottom()
    }
}

/// Resolves button registrations against computed layout rectangles.
pub fn resolve_button_regions(
    layout: &[LayoutResult],
    registrations: &[(u64, String)],
) -> Vec<InteractiveRegion> {
    registrations
        .iter()
        .filter_map(|(node_id, button_id)| {
            let result = layout.iter().find(|result| result.id == *node_id)?;
            Some(InteractiveRegion::new(
                *node_id,
                Rect::new(result.x, result.y, result.width, result.height),
                button_id.clone(),
            ))
        })
        .collect()
}

/// UI action delivered to the native view callback.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    /// Pointer moved and optionally entered a button.
    PointerMoved {
        /// Logical x coordinate.
        x: f32,
        /// Logical y coordinate.
        y: f32,
        /// Button under the pointer, if any.
        target: Option<String>,
    },
    /// Pointer button pressed.
    PointerPressed {
        /// Logical x coordinate.
        x: f32,
        /// Logical y coordinate.
        y: f32,
        /// Pressed button.
        button: PointerButton,
        /// Button under the pointer, if any.
        target: Option<String>,
    },
    /// Pointer button released.
    PointerReleased {
        /// Logical x coordinate.
        x: f32,
        /// Logical y coordinate.
        y: f32,
        /// Released button.
        button: PointerButton,
        /// Button activated by a matching press and release, if any.
        clicked_button: Option<String>,
    },
    /// Scroll wheel movement.
    Wheel {
        /// Horizontal delta in logical pixels.
        delta_x: f32,
        /// Vertical delta in logical pixels.
        delta_y: f32,
    },
    /// Text input produced by the platform.
    TextInput {
        /// Input text.
        text: String,
    },
    /// Named keyboard key.
    Key {
        /// Pressed key.
        key: Key,
    },
}

/// Stateful router that turns pointer events into button actions.
#[derive(Debug, Clone, Default)]
pub struct UiEventRouter {
    active_button: Option<String>,
}

impl UiEventRouter {
    /// Creates an empty router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Routes one UI event against the latest button regions.
    ///
    /// Only the primary/left button activates buttons. Other pointer buttons
    /// are surfaced to callbacks but do not capture or click a button.
    pub fn route(&mut self, event: &UiEvent, regions: &[InteractiveRegion]) -> Option<UiAction> {
        match event {
            UiEvent::PointerMoved { x, y } => Some(UiAction::PointerMoved {
                x: *x,
                y: *y,
                target: hit_button(regions, *x, *y),
            }),
            UiEvent::PointerPressed { x, y, button } => {
                let target = if *button == PointerButton::Left {
                    hit_button(regions, *x, *y)
                } else {
                    None
                };
                self.active_button.clone_from(&target);
                Some(UiAction::PointerPressed {
                    x: *x,
                    y: *y,
                    button: *button,
                    target,
                })
            }
            UiEvent::PointerReleased { x, y, button } => {
                let released = if *button == PointerButton::Left {
                    hit_button(regions, *x, *y)
                } else {
                    None
                };
                let clicked_button = self
                    .active_button
                    .take()
                    .filter(|active| Some(active.clone()) == released);
                Some(UiAction::PointerReleased {
                    x: *x,
                    y: *y,
                    button: *button,
                    clicked_button,
                })
            }
            UiEvent::Wheel { delta_x, delta_y } => Some(UiAction::Wheel {
                delta_x: *delta_x,
                delta_y: *delta_y,
            }),
            UiEvent::TextInput { text } if !text.is_empty() => {
                Some(UiAction::TextInput { text: text.clone() })
            }
            UiEvent::TextInput { .. } => None,
            UiEvent::Key { key } => Some(UiAction::Key { key: *key }),
        }
    }

    /// Returns the button captured by the most recent press.
    pub fn active_button(&self) -> Option<&str> {
        self.active_button.as_deref()
    }
}

fn hit_button(regions: &[InteractiveRegion], x: f32, y: f32) -> Option<String> {
    regions
        .iter()
        .rev()
        .find(|region| region.contains(x, y))
        .map(|region| region.button_id.clone())
}

fn normalize_scale(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions() -> Vec<InteractiveRegion> {
        vec![
            InteractiveRegion::new(1, Rect::new(10.0, 10.0, 40.0, 20.0), "primary"),
            InteractiveRegion::new(2, Rect::new(100.0, 100.0, 30.0, 10.0), "secondary"),
        ]
    }

    #[test]
    fn input_translator_applies_scale_and_preserves_cursor_for_mouse_buttons() {
        let mut translator = InputTranslator::new(2.0);
        assert_eq!(
            translator.translate(&NativeInput::CursorMoved { x: 40.0, y: 60.0 }),
            Some(UiEvent::PointerMoved { x: 20.0, y: 30.0 })
        );
        assert_eq!(
            translator.translate(&NativeInput::MouseInput {
                state: InputState::Pressed,
                button: PointerButton::Left,
            }),
            Some(UiEvent::PointerPressed {
                x: 20.0,
                y: 30.0,
                button: PointerButton::Left,
            })
        );
    }

    #[test]
    fn router_emits_click_when_press_and_release_hit_same_button() {
        let mut router = UiEventRouter::new();
        let regions = regions();
        let pressed = router.route(
            &UiEvent::PointerPressed {
                x: 20.0,
                y: 15.0,
                button: PointerButton::Left,
            },
            &regions,
        );
        assert!(matches!(
            pressed,
            Some(UiAction::PointerPressed {
                target: Some(ref id),
                ..
            }) if id == "primary"
        ));

        let released = router.route(
            &UiEvent::PointerReleased {
                x: 21.0,
                y: 16.0,
                button: PointerButton::Left,
            },
            &regions,
        );
        assert!(matches!(
            released,
            Some(UiAction::PointerReleased {
                clicked_button: Some(ref id),
                ..
            }) if id == "primary"
        ));
        assert_eq!(router.active_button(), None);
    }

    #[test]
    fn router_does_not_click_when_release_moves_outside() {
        let mut router = UiEventRouter::new();
        let regions = regions();
        router.route(
            &UiEvent::PointerPressed {
                x: 20.0,
                y: 15.0,
                button: PointerButton::Left,
            },
            &regions,
        );
        let released = router.route(
            &UiEvent::PointerReleased {
                x: 200.0,
                y: 200.0,
                button: PointerButton::Left,
            },
            &regions,
        );
        assert!(matches!(
            released,
            Some(UiAction::PointerReleased {
                clicked_button: None,
                ..
            })
        ));
    }

    #[test]
    fn wheel_and_text_events_are_preserved() {
        let mut router = UiEventRouter::new();
        assert_eq!(
            router.route(
                &UiEvent::Wheel {
                    delta_x: 2.0,
                    delta_y: -18.0,
                },
                &[],
            ),
            Some(UiAction::Wheel {
                delta_x: 2.0,
                delta_y: -18.0,
            })
        );
        assert_eq!(
            router.route(
                &UiEvent::TextInput {
                    text: "hello".to_string(),
                },
                &[],
            ),
            Some(UiAction::TextInput {
                text: "hello".to_string(),
            })
        );
    }

    #[test]
    fn animated_scale_changes_are_applied_to_later_cursor_events() {
        let mut translator = InputTranslator::new(1.0);
        translator.set_scale_factor(1.5);
        assert_eq!(
            translator.translate(&NativeInput::CursorMoved { x: 30.0, y: 45.0 }),
            Some(UiEvent::PointerMoved { x: 20.0, y: 30.0 })
        );
    }

    #[test]
    fn named_keys_are_translated_without_touching_text_input() {
        let mut translator = InputTranslator::new(1.0);
        assert_eq!(
            translator.translate(&NativeInput::Key { key: Key::ShiftTab }),
            Some(UiEvent::Key { key: Key::ShiftTab })
        );
        assert_eq!(
            translator.translate(&NativeInput::Text("a".to_string())),
            Some(UiEvent::TextInput {
                text: "a".to_string()
            })
        );
    }
}
