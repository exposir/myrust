//! Windowless view driver and assertions for native UI tests.
//!
//! The tester owns a [`NativeUi`], renders it at a fixed logical size, locates
//! elements by accessible text/label/key, and injects the same [`UiEvent`]
//! values a window host would. It deliberately has no window, browser, display
//! server or platform accessibility dependency.

use super::access::{AccessNode, Role};
use super::event::{InteractiveRegion, Key, PointerButton, UiEvent};
use super::layout::{Point, Size};
use super::native::{NativeUi, NativeUiError, ViewFrame};
use super::paint::{Color, DrawCommand, Rect};
use super::render::TransformMatrix;
use super::text::layout_text_shaped;

/// Error produced while locating or driving a native view.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TesterError {
    /// The immediate-mode callback failed.
    #[error("native UI render failed: {0}")]
    Render(String),
    /// No element matched the query.
    #[error("no element matches {query:?}")]
    NotFound {
        /// Label, key or value supplied by the test.
        query: String,
    },
    /// More than one element matched an ambiguous query.
    #[error("{query:?} is ambiguous: {count} elements match")]
    Ambiguous {
        /// Label, key or value supplied by the test.
        query: String,
        /// Number of matching elements.
        count: usize,
    },
    /// The matched element has no interaction region.
    #[error("element {query:?} is not interactive (role {role:?})")]
    NotInteractive {
        /// Matched label, key or value.
        query: String,
        /// Accessibility role.
        role: Role,
    },
    /// The element has no on-screen area or nothing paints at its center.
    #[error("element {query:?} is not visible in the {width}x{height} view")]
    NotVisible {
        /// Matched label, key or value.
        query: String,
        /// Logical view width.
        width: f32,
        /// Logical view height.
        height: f32,
    },
    /// Another interactive element is above the requested one.
    #[error("element {query:?} is covered by {covering:?} at ({x:.1}, {y:.1})")]
    Occluded {
        /// Matched label, key or value.
        query: String,
        /// Topmost interactive widget id.
        covering: String,
        /// Hit-test x coordinate.
        x: f32,
        /// Hit-test y coordinate.
        y: f32,
    },
    /// No interactive element exists at the requested point.
    #[error("no interactive element at ({x:.1}, {y:.1})")]
    NoTargetAt {
        /// Hit-test x coordinate.
        x: f32,
        /// Hit-test y coordinate.
        y: f32,
    },
    /// An assertion did not hold.
    #[error("{0}")]
    Assertion(String),
}

impl From<NativeUiError> for TesterError {
    fn from(error: NativeUiError) -> Self {
        Self::Render(error.to_string())
    }
}

/// Snapshot of one accessible element in the current frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TestElement {
    /// Accessible label.
    pub label: String,
    /// Accessibility role.
    pub role: Role,
    /// Optional textual value/state.
    pub value: Option<String>,
    /// Stable widget key, when present.
    pub key: Option<String>,
    /// Whether the element is currently focused.
    pub focused: bool,
    /// Logical bounds from the current frame.
    pub bounds: Rect,
}

impl TestElement {
    /// Returns the center point used by pointer actions.
    pub fn center(&self) -> Point {
        Point::new(
            self.bounds.x + self.bounds.width / 2.0,
            self.bounds.y + self.bounds.height / 2.0,
        )
    }

    fn from_access(node: &AccessNode) -> Self {
        Self {
            label: node.label.clone(),
            role: node.role,
            value: node.value.clone(),
            key: node.key.clone(),
            focused: node.focused,
            bounds: node.bounds,
        }
    }

    fn matches(&self, query: &str) -> bool {
        self.label == query
            || self.key.as_deref() == Some(query)
            || self.value.as_deref() == Some(query)
    }
}

/// Windowless driver for an immediate-mode [`NativeUi`].
#[derive(Debug)]
pub struct Tester {
    ui: NativeUi,
    size: Size,
    frame: ViewFrame,
    last_pointer: Option<Point>,
}

impl Tester {
    /// Creates a tester and renders the first frame.
    pub fn new(ui: NativeUi, size: Size) -> Result<Self, TesterError> {
        let frame = ui.render(size)?;
        Ok(Self {
            ui,
            size,
            frame,
            last_pointer: None,
        })
    }

    /// Returns the current size in logical pixels.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Changes the view size and renders immediately.
    pub fn set_size(&mut self, size: Size) -> Result<&ViewFrame, TesterError> {
        self.size = size;
        self.render()
    }

    /// Returns the underlying immediate-mode view.
    pub fn ui(&self) -> &NativeUi {
        &self.ui
    }

    /// Returns the frame from the most recent render.
    pub fn frame(&self) -> &ViewFrame {
        &self.frame
    }

    /// Returns the last pointer point sent by the tester.
    pub fn pointer_position(&self) -> Option<Point> {
        self.last_pointer
    }

    /// Renders again with no new input.
    pub fn render(&mut self) -> Result<&ViewFrame, TesterError> {
        self.frame = self.ui.render(self.size)?;
        Ok(&self.frame)
    }

    /// Returns every accessible element in the current frame.
    pub fn elements(&self) -> Vec<TestElement> {
        self.frame
            .access_nodes
            .iter()
            .map(TestElement::from_access)
            .collect()
    }

    /// Finds exactly one element by label, key or value.
    pub fn find(&self, query: &str) -> Result<TestElement, TesterError> {
        let matches = self
            .frame
            .access_nodes
            .iter()
            .map(TestElement::from_access)
            .filter(|element| element.matches(query))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Err(TesterError::NotFound {
                query: query.to_owned(),
            }),
            [element] => Ok(element.clone()),
            elements => Err(TesterError::Ambiguous {
                query: query.to_owned(),
                count: elements.len(),
            }),
        }
    }

    /// Finds exactly one element by accessible label.
    pub fn find_by_label(&self, label: &str) -> Result<TestElement, TesterError> {
        self.find(label)
    }

    /// Finds exactly one element whose label or value contains `text`.
    pub fn find_by_text(&self, text: &str) -> Result<TestElement, TesterError> {
        let matches = self
            .frame
            .access_nodes
            .iter()
            .map(TestElement::from_access)
            .filter(|element| {
                element.label.contains(text)
                    || element
                        .value
                        .as_deref()
                        .is_some_and(|value| value.contains(text))
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Err(TesterError::NotFound {
                query: text.to_owned(),
            }),
            [element] => Ok(element.clone()),
            elements => Err(TesterError::Ambiguous {
                query: text.to_owned(),
                count: elements.len(),
            }),
        }
    }

    /// Returns all text strings painted in command order.
    pub fn texts(&self) -> Vec<String> {
        self.frame
            .display
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Returns whether any painted text contains `text`.
    pub fn has_text(&self, text: &str) -> bool {
        self.texts().iter().any(|line| line.contains(text))
    }

    /// Clicks an interactive element by label, key or value.
    pub fn click(&mut self, query: &str) -> Result<&ViewFrame, TesterError> {
        let point = self.action_point(query)?;
        self.press_point(point)?;
        self.release_point(point)
    }

    /// Clicks the topmost interactive element at a logical point.
    pub fn click_at(&mut self, x: f32, y: f32) -> Result<&ViewFrame, TesterError> {
        let target = self
            .topmost_region(x, y)
            .cloned()
            .ok_or(TesterError::NoTargetAt { x, y })?;
        self.validate_region_visibility(&target.button_id, &target, x, y)?;
        self.press_point(Point::new(x, y))?;
        self.release_point(Point::new(x, y))
    }

    /// Presses an interactive element without releasing it.
    pub fn press(&mut self, query: &str) -> Result<&ViewFrame, TesterError> {
        let point = self.action_point(query)?;
        self.press_point(point)
    }

    /// Presses at a logical point.
    pub fn press_at(&mut self, x: f32, y: f32) -> Result<&ViewFrame, TesterError> {
        let target = self
            .topmost_region(x, y)
            .cloned()
            .ok_or(TesterError::NoTargetAt { x, y })?;
        self.validate_region_visibility(&target.button_id, &target, x, y)?;
        self.press_point(Point::new(x, y))
    }

    /// Releases an interactive element after [`Self::press`].
    pub fn release(&mut self, query: &str) -> Result<&ViewFrame, TesterError> {
        let point = self.action_point(query)?;
        self.release_point(point)
    }

    /// Releases at a logical point, or at the last press if one is pending.
    pub fn release_at(&mut self, x: f32, y: f32) -> Result<&ViewFrame, TesterError> {
        self.release_point(Point::new(x, y))
    }

    /// Moves the pointer to an element center.
    pub fn move_to(&mut self, query: &str) -> Result<&ViewFrame, TesterError> {
        let element = self.find(query)?;
        let point = element.center();
        self.validate_screen_point(point)?;
        self.last_pointer = Some(point);
        self.dispatch(UiEvent::PointerMoved {
            x: point.x,
            y: point.y,
        })
    }

    /// Moves the pointer to a logical point.
    pub fn move_to_point(&mut self, x: f32, y: f32) -> Result<&ViewFrame, TesterError> {
        let point = Point::new(x, y);
        self.validate_screen_point(point)?;
        self.last_pointer = Some(point);
        self.dispatch(UiEvent::PointerMoved { x, y })
    }

    /// Sends a wheel event at the current pointer position.
    pub fn scroll(&mut self, delta_x: f32, delta_y: f32) -> Result<&ViewFrame, TesterError> {
        self.dispatch(UiEvent::Wheel { delta_x, delta_y })
    }

    /// Sends text to the currently focused field.
    pub fn type_text(&mut self, text: &str) -> Result<&ViewFrame, TesterError> {
        self.dispatch(UiEvent::TextInput {
            text: text.to_owned(),
        })
    }

    /// Clicks a field and then sends text to it.
    pub fn type_into(
        &mut self,
        query: &str,
        text: &str,
    ) -> Result<&ViewFrame, TesterError> {
        self.click(query)?;
        self.type_text(text)
    }

    /// Sends one named key.
    pub fn key(&mut self, key: Key) -> Result<&ViewFrame, TesterError> {
        self.dispatch(UiEvent::Key { key })
    }

    /// Focuses an element by stable key or label.
    pub fn focus(&mut self, query: &str) -> Result<&ViewFrame, TesterError> {
        let element = self.find(query)?;
        let id = self
            .region_for_element(&element)
            .map(|region| region.button_id)
            .or(element.key)
            .unwrap_or(element.label);
        self.dispatch(UiEvent::Focus { id })
    }

    /// Asserts that an element is focused.
    pub fn assert_focused(&self, query: &str) -> Result<(), TesterError> {
        let focused = self.frame.access_nodes.iter().any(|node| {
            node.focused
                && (node.label == query
                    || node.key.as_deref() == Some(query)
                    || node.value.as_deref() == Some(query))
        });
        if focused {
            Ok(())
        } else {
            Err(TesterError::Assertion(format!(
                "expected {query:?} to be focused"
            )))
        }
    }

    /// Asserts that any painted text contains `text`.
    pub fn assert_has_text(&self, text: &str) -> Result<(), TesterError> {
        if self.has_text(text) {
            Ok(())
        } else {
            Err(TesterError::Assertion(format!(
                "expected view to contain text {text:?}"
            )))
        }
    }

    /// Asserts that every expected string is painted exactly once or more.
    pub fn assert_texts(&self, expected: &[&str]) -> Result<(), TesterError> {
        let actual = self.texts();
        let missing = expected
            .iter()
            .filter(|text| !actual.iter().any(|actual| actual == **text))
            .copied()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(TesterError::Assertion(format!(
                "missing painted texts: {missing:?}"
            )))
        }
    }

    /// Asserts that the painted text list is exactly the supplied list.
    pub fn assert_texts_exact(&self, expected: &[&str]) -> Result<(), TesterError> {
        let actual = self.texts();
        let actual = actual.iter().map(String::as_str).collect::<Vec<_>>();
        if actual == expected {
            Ok(())
        } else {
            Err(TesterError::Assertion(format!(
                "expected texts {expected:?}, got {actual:?}"
            )))
        }
    }

    fn dispatch(&mut self, event: UiEvent) -> Result<&ViewFrame, TesterError> {
        self.ui.input(event)?;
        self.render()
    }

    fn press_point(&mut self, point: Point) -> Result<&ViewFrame, TesterError> {
        self.validate_screen_point(point)?;
        self.last_pointer = Some(point);
        self.dispatch(UiEvent::PointerPressed {
            x: point.x,
            y: point.y,
            button: PointerButton::Left,
        })
    }

    fn release_point(&mut self, point: Point) -> Result<&ViewFrame, TesterError> {
        self.validate_screen_point(point)?;
        self.last_pointer = Some(point);
        self.dispatch(UiEvent::PointerReleased {
            x: point.x,
            y: point.y,
            button: PointerButton::Left,
        })
    }

    fn action_point(&self, query: &str) -> Result<Point, TesterError> {
        let matches = self
            .frame
            .access_nodes
            .iter()
            .map(TestElement::from_access)
            .filter(|element| element.matches(query))
            .filter(|element| self.region_for_element(element).is_some())
            .collect::<Vec<_>>();
        let element = match matches.as_slice() {
            [] => {
                let element = self.find(query)?;
                return Err(TesterError::NotInteractive {
                    query: query.to_owned(),
                    role: element.role,
                });
            }
            [element] => element,
            elements => {
                return Err(TesterError::Ambiguous {
                    query: query.to_owned(),
                    count: elements.len(),
                });
            }
        };
        let region = self
            .region_for_element(element)
            .expect("interactive candidate has a region");
        self.visible_hit_point(query, &region)
    }

    fn region_for_element(&self, element: &TestElement) -> Option<InteractiveRegion> {
        if let Some(key) = &element.key {
            if let Some(region) = self
                .frame
                .interactions
                .iter()
                .find(|region| &region.button_id == key)
            {
                return Some(region.clone());
            }
        }
        self.frame
            .interactions
            .iter()
            .find(|region| {
                rect_contains(region.bounds, element.center())
                    || rect_contains(element.bounds, region_center(region))
            })
            .cloned()
    }

    fn topmost_region(&self, x: f32, y: f32) -> Option<&InteractiveRegion> {
        self.frame
            .interactions
            .iter()
            .rev()
            .find(|region| region.contains(x, y))
    }

    fn visible_hit_point(
        &self,
        query: &str,
        region: &InteractiveRegion,
    ) -> Result<Point, TesterError> {
        let mut covered_by = None;
        for point in sample_region_points(region) {
            if !self.frame_contains(point) || !self.point_is_painted(point.x, point.y) {
                continue;
            }
            match self.topmost_region(point.x, point.y) {
                Some(topmost) if topmost.button_id == region.button_id => return Ok(point),
                Some(topmost) => {
                    covered_by.get_or_insert_with(|| topmost.button_id.clone());
                }
                None => {}
            }
        }
        if let Some(covering) = covered_by {
            let center = region_center(region);
            return Err(TesterError::Occluded {
                query: query.to_owned(),
                covering,
                x: center.x,
                y: center.y,
            });
        }
        Err(self.not_visible(query))
    }

    fn validate_region_visibility(
        &self,
        query: &str,
        region: &InteractiveRegion,
        x: f32,
        y: f32,
    ) -> Result<(), TesterError> {
        if region.bounds.width <= 0.0
            || region.bounds.height <= 0.0
            || !self.frame_contains(Point::new(x, y))
            || !self.point_is_painted(x, y)
        {
            return Err(self.not_visible(query));
        }
        if let Some(topmost) = self.topmost_region(x, y) {
            if topmost.button_id != region.button_id {
                return Err(TesterError::Occluded {
                    query: query.to_owned(),
                    covering: topmost.button_id.clone(),
                    x,
                    y,
                });
            }
        }
        Ok(())
    }

    fn validate_screen_point(&self, point: Point) -> Result<(), TesterError> {
        if self.frame_contains(point) {
            Ok(())
        } else {
            Err(TesterError::NotVisible {
                query: format!("point ({:.1}, {:.1})", point.x, point.y),
                width: self.size.width,
                height: self.size.height,
            })
        }
    }

    fn not_visible(&self, query: &str) -> TesterError {
        TesterError::NotVisible {
            query: query.to_owned(),
            width: self.size.width,
            height: self.size.height,
        }
    }

    fn frame_contains(&self, point: Point) -> bool {
        point.x >= 0.0
            && point.y >= 0.0
            && point.x <= self.size.width
            && point.y <= self.size.height
    }

    fn point_is_painted(&self, x: f32, y: f32) -> bool {
        let mut transform = TransformMatrix::IDENTITY;
        let mut transforms = Vec::new();
        let mut scissor = None;
        let mut scissors = Vec::new();

        for command in &self.frame.display.commands {
            match command {
                DrawCommand::TransformPush {
                    transform: local, ..
                } => {
                    transforms.push(transform);
                    transform = transform.then(TransformMatrix::from_transform(*local));
                }
                DrawCommand::TransformPop => {
                    if let Some(previous) = transforms.pop() {
                        transform = previous;
                    }
                }
                DrawCommand::ScissorPush { rect } => {
                    scissors.push(scissor);
                    let transformed = transform.transform_rect(*rect);
                    scissor = Some(match scissor {
                        Some(current) => super::render::intersect_rects(current, transformed),
                        None => transformed,
                    });
                }
                DrawCommand::ScissorPop => {
                    if let Some(previous) = scissors.pop() {
                        scissor = previous;
                    }
                }
                _ => {
                    if scissor.is_some_and(|clip| !rect_contains(clip, Point::new(x, y))) {
                        continue;
                    }
                    if command_paints_point(command, transform, x, y) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn command_paints_point(command: &DrawCommand, transform: TransformMatrix, x: f32, y: f32) -> bool {
    let point = Point::new(x, y);
    let local_rect = match command {
        DrawCommand::Rect {
            x,
            y,
            width,
            height,
            color,
        } => {
            if *color == Color::TRANSPARENT {
                return false;
            }
            Rect::new(*x, *y, *width, *height)
        }
        DrawCommand::RoundedRect {
            x,
            y,
            width,
            height,
            color,
            ..
        } => {
            if *color == Color::TRANSPARENT {
                return false;
            }
            Rect::new(*x, *y, *width, *height)
        }
        DrawCommand::Image {
            x,
            y,
            width,
            height,
            ..
        } => Rect::new(*x, *y, *width, *height),
        DrawCommand::LinearGradient {
            x,
            y,
            width,
            height,
            ..
        } => Rect::new(*x, *y, *width, *height),
        DrawCommand::SvgPath {
            x,
            y,
            width,
            height,
            ..
        } => Rect::new(*x, *y, *width, *height),
        DrawCommand::Circle {
            x,
            y,
            radius,
            color,
        } => {
            if *color == Color::TRANSPARENT || *radius <= 0.0 {
                return false;
            }
            Rect::new(*x - *radius, *y - *radius, *radius * 2.0, *radius * 2.0)
        }
        DrawCommand::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            color,
        } => {
            if *color == Color::TRANSPARENT {
                return false;
            }
            let half = width.max(0.0) / 2.0;
            Rect::new(
                x1.min(*x2) - half,
                y1.min(*y2) - half,
                (x1 - x2).abs() + half * 2.0,
                (y1 - y2).abs() + half * 2.0,
            )
        }
        DrawCommand::Text {
            x,
            y,
            text,
            font_size,
            mono,
            color,
            max_width,
            alignment,
            ..
        } => {
            if *color == Color::TRANSPARENT {
                return false;
            }
            let Some(layout) =
                layout_text_shaped(text, *font_size, *max_width, *alignment, *mono)
            else {
                return false;
            };
            Rect::new(
                *x + layout.content_x,
                *y,
                layout.content_width,
                layout.height,
            )
        }
        DrawCommand::Shadow {
            x,
            y,
            width,
            height,
            offset_x,
            offset_y,
            blur,
            color,
            ..
        } => {
            if *color == Color::TRANSPARENT {
                return false;
            }
            let reach = blur.max(0.0) * 2.5 + 2.0;
            Rect::new(
                *x + *offset_x - reach,
                *y + *offset_y - reach,
                *width + reach * 2.0,
                *height + reach * 2.0,
            )
        }
        DrawCommand::ScissorPush { .. }
        | DrawCommand::ScissorPop
        | DrawCommand::TransformPush { .. }
        | DrawCommand::TransformPop => return false,
    };
    let transformed = transform.transform_rect(local_rect);
    rect_contains(transformed, point)
}

fn rect_contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x && point.y >= rect.y && point.x <= rect.right() && point.y <= rect.bottom()
}

fn region_center(region: &InteractiveRegion) -> Point {
    Point::new(
        region.bounds.x + region.bounds.width / 2.0,
        region.bounds.y + region.bounds.height / 2.0,
    )
}

fn sample_region_points(region: &InteractiveRegion) -> Vec<Point> {
    let xs = [0.5_f32, 0.25, 0.75, 0.1, 0.9];
    let ys = [0.5_f32, 0.25, 0.75];
    let mut points = Vec::with_capacity(xs.len() * ys.len());
    for y in ys {
        for x in xs {
            points.push(Point::new(
                region.bounds.x + region.bounds.width * x,
                region.bounds.y + region.bounds.height * y,
            ));
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{button, container, text, text_field};
    use crate::theme::Theme;
    use std::{cell::RefCell, rc::Rc};

    fn test_ui() -> NativeUi {
        let value = Rc::new(RefCell::new(String::new()));
        NativeUi::new(move |context| {
            container(context).fill().padding(12.0).column().gap(8.0).children(|context| {
                text(context, "Tester page");
                let mut value = value.borrow_mut();
                text_field(context, "name", &mut value);
                button(context, "Save");
            });
        })
        .with_theme(Theme::light())
    }

    #[test]
    fn finds_by_label_text_and_key_and_asserts_text() {
        let tester = Tester::new(test_ui(), Size::new(260.0, 160.0)).unwrap();
        let save = tester.find("Save").unwrap();
        assert_eq!(save.role, Role::Button);
        assert!(save.center().x > 0.0);
        assert_eq!(tester.find("name").unwrap().key.as_deref(), Some("name"));
        assert!(tester.has_text("Tester page"));
        tester.assert_texts(&["Tester page", "Save"]).unwrap();
        assert!(tester.texts().iter().any(|text| text == "Save"));
    }

    #[test]
    fn clicks_and_types_without_a_window() {
        let mut tester = Tester::new(test_ui(), Size::new(260.0, 160.0)).unwrap();
        tester.type_into("name", "Linus").unwrap();
        tester.assert_focused("name").unwrap();
        tester.click("Save").unwrap();
        tester.assert_focused("Save").unwrap();
        assert!(tester.find("Linus").is_ok());
    }

    #[test]
    fn reports_occluded_and_offscreen_elements() {
        let ui = NativeUi::new(|context| {
            container(context).fill().children(|context| {
                container(context).absolute(20.0, 20.0).children(|context| {
                    button(context, "Back");
                });
                container(context).absolute(20.0, 20.0).children(|context| {
                    button(context, "Front");
                });
                container(context).absolute(240.0, 20.0).children(|context| {
                    button(context, "Far");
                });
            });
        });
        let mut tester = Tester::new(ui, Size::new(200.0, 100.0)).unwrap();
        assert!(matches!(
            tester.click("Back").unwrap_err(),
            TesterError::Occluded { .. }
        ));
        assert!(matches!(
            tester.click("Far").unwrap_err(),
            TesterError::NotVisible { .. }
        ));
    }

    #[test]
    fn scroll_and_key_helpers_dispatch_real_events() {
        let mut tester = Tester::new(
            NativeUi::new(|context| {
                button(context, "Activate");
            }),
            Size::new(180.0, 90.0),
        )
        .unwrap();
        tester.focus("Activate").unwrap();
        tester.assert_focused("Activate").unwrap();
        tester.key(Key::Enter).unwrap();
        tester.move_to("Activate").unwrap();
        tester.scroll(0.0, 10.0).unwrap();
    }
}
