//! Non-modal, stacking toast notifications.
//!
//! The caller owns a [`ToastQueue`]. [`toasts`] paints the queue on a floating
//! layer without registering an overlay: it never steals focus, never blocks
//! pointer input below it, and does not participate in overlay dismissal.
//! Action and close buttons are ordinary controls, so a click that hits a
//! toast can also reach a control underneath it. Auto-dismissal uses the
//! frame clock and requests frames while a toast is still alive.

use super::time::{Duration, Instant};

use super::context::Context;

/// Default time a toast remains visible.
pub const TOAST_DURATION: Duration = Duration::from_secs(4);

/// One queued toast message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub(crate) id: u64,
    pub(crate) message: String,
    pub(crate) action: Option<String>,
    pub(crate) duration: Duration,
    pub(crate) started_at: Option<Instant>,
}

impl Toast {
    /// Creates a toast with the default duration and no action.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            id: 0,
            message: message.into(),
            action: None,
            duration: TOAST_DURATION,
            started_at: None,
        }
    }

    /// Adds an action button and returns the updated toast.
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// Overrides how long the toast remains visible.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Returns the message text.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the optional action label.
    pub fn action_label(&self) -> Option<&str> {
        self.action.as_deref()
    }

    /// Returns the configured lifetime.
    pub fn duration_value(&self) -> Duration {
        self.duration
    }
}

/// Caller-owned FIFO of toasts.
#[derive(Debug, Clone, Default)]
pub struct ToastQueue {
    pub(crate) entries: Vec<Toast>,
    next_id: u64,
}

impl ToastQueue {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a toast and starts its lifetime at the current frame time.
    pub fn push(&mut self, context: &Context, mut toast: Toast) {
        self.next_id = self.next_id.saturating_add(1);
        toast.id = self.next_id;
        if toast.started_at.is_none() {
            toast.started_at = Some(context.frame_time());
        }
        self.entries.push(toast);
    }

    /// Pushes a plain text toast.
    pub fn push_message(&mut self, context: &Context, message: impl Into<String>) {
        self.push(context, Toast::new(message));
    }

    /// Returns the queued toasts in display order.
    pub fn entries(&self) -> &[Toast] {
        &self.entries
    }

    /// Returns the number of queued toasts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes all queued toasts.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Why a toast left the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastDismissal {
    /// The configured duration elapsed.
    Timeout,
    /// The close button was clicked.
    Close,
    /// The action button was clicked.
    Action,
}

/// One toast dismissal observed during a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastEvent {
    /// Index in the queue at the start of the frame.
    pub index: usize,
    /// Dismissal reason.
    pub kind: ToastDismissal,
    /// Action label for [`ToastDismissal::Action`].
    pub action: Option<String>,
}

/// Result of painting a toast queue for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToastResponse {
    /// Dismissals observed this frame, in queue order.
    pub events: Vec<ToastEvent>,
    /// Number of toasts still queued after dismissals.
    pub visible: usize,
}

/// Paints and expires the caller-owned toast queue.
///
/// Toasts stack from the top-right of the window. The layer is above normal
/// content but is deliberately not registered as an overlay, so lower widgets
/// stay interactive and focus is unchanged.
pub fn toasts(context: &mut Context, id: &str, queue: &mut ToastQueue) -> ToastResponse {
    context.toasts(id, queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::event::{PointerButton, UiEvent};
    use crate::layout::Size;
    use crate::paint::DrawCommand;

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        queue: &mut ToastQueue,
        response: &mut Option<ToastResponse>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        *response = Some(toasts(context, "toasts", queue));
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
        queue: &mut ToastQueue,
        response: &mut Option<ToastResponse>,
    ) -> super::super::native::ViewFrame {
        let initial = render(context, size, None, queue, response);
        let point = center_of(&initial, id);
        render(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            queue,
            response,
        );
        render(
            context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            queue,
            response,
        )
    }

    #[test]
    fn toasts_stack_and_expire_with_the_frame_clock() {
        let size = Size::new(420.0, 320.0);
        let mut context = Context::new(size);
        let mut queue = ToastQueue::new();
        context.begin_frame(size);
        queue.push(
            &context,
            Toast::new("first").duration(Duration::from_millis(100)),
        );
        queue.push(
            &context,
            Toast::new("second").duration(Duration::from_secs(5)),
        );
        toasts(&mut context, "toasts", &mut queue);
        let frame = context.produce().unwrap();
        context.end_frame();
        assert_eq!(queue.len(), 2);
        assert_eq!(
            frame
                .access_nodes
                .iter()
                .filter(|node| node.role == Role::Group && node.label == "toast")
                .count(),
            2
        );
        let text_y = |text: &str| {
            frame
                .display
                .commands
                .iter()
                .find_map(|command| match command {
                    DrawCommand::Text { text: value, y, .. } if value == text => Some(*y),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing toast text {text}"))
        };
        assert!(text_y("second") > text_y("first"), "toasts must stack");

        context.begin_frame(size);
        context.advance_frame_time_for_test(Duration::from_millis(150));
        let expired = toasts(&mut context, "toasts", &mut queue);
        let expired_frame = context.produce().unwrap();
        context.end_frame();
        assert_eq!(queue.len(), 1);
        assert!(expired
            .events
            .iter()
            .any(|event| { event.index == 0 && event.kind == ToastDismissal::Timeout }));
        assert!(expired_frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "second")));
        assert!(!expired_frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "first")));

        context.begin_frame(size);
        context.advance_frame_time_for_test(Duration::from_secs(5));
        let drained = toasts(&mut context, "toasts", &mut queue);
        assert!(queue.is_empty());
        assert_eq!(drained.visible, 0);
        assert!(drained
            .events
            .iter()
            .any(|event| event.kind == ToastDismissal::Timeout));
    }

    #[test]
    fn toast_action_and_close_buttons_dismiss_the_entry() {
        let size = Size::new(420.0, 320.0);
        let mut context = Context::new(size);
        let mut queue = ToastQueue::new();
        queue.push(
            &context,
            Toast::new("Saved")
                .action("Undo")
                .duration(Duration::from_secs(10)),
        );
        let action_id = format!("toasts:toast:{}:action", queue.entries[0].id);
        let mut response = None;
        click(&mut context, size, &action_id, &mut queue, &mut response);
        let action_event = response
            .as_ref()
            .unwrap()
            .events
            .iter()
            .find(|event| event.kind == ToastDismissal::Action)
            .expect("action event");
        assert_eq!(action_event.index, 0);
        assert_eq!(action_event.action.as_deref(), Some("Undo"));
        assert!(queue.is_empty());

        queue.push(
            &context,
            Toast::new("Closable").duration(Duration::from_secs(10)),
        );
        let close_id = format!("toasts:toast:{}:close", queue.entries[0].id);
        click(&mut context, size, &close_id, &mut queue, &mut response);
        assert!(response
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.kind == ToastDismissal::Close));
        assert!(queue.is_empty());
    }
}

#[cfg(test)]
mod non_modal_tests {
    use super::*;
    use crate::access::Role;
    use crate::event::{PointerButton, UiEvent};
    use crate::layout::Size;
    use std::cell::RefCell;

    fn render(
        context: &mut Context,
        size: Size,
        input: Option<UiEvent>,
        queue: &mut ToastQueue,
        clicks: &RefCell<u32>,
    ) -> super::super::native::ViewFrame {
        context.begin_frame(size);
        if let Some(event) = input {
            context.set_input(event);
        }
        let label = "Base layer button with a wide label";
        if context.id("base").button(label).clicked {
            *clicks.borrow_mut() += 1;
        }
        toasts(context, "toasts", queue);
        let frame = context.produce().unwrap();
        context.end_frame();
        frame
    }

    #[test]
    fn toast_keeps_focus_and_pointer_on_the_layer_below() {
        let size = Size::new(420.0, 320.0);
        let mut context = Context::new(size);
        let mut queue = ToastQueue::new();
        let clicks = RefCell::new(0);
        context.request_focus("base");
        let initial = render(&mut context, size, None, &mut queue, &clicks);
        assert!(initial.access_nodes.iter().any(|node| {
            node.role == Role::Button
                && node.label == "Base layer button with a wide label"
                && node.focused
        }));
        let base = initial
            .interactions
            .iter()
            .find(|region| region.button_id == "base")
            .expect("base button region");
        assert!(
            base.contains(150.0, 20.0),
            "test point must hit the base button"
        );
        queue.push(
            &context,
            Toast::new("Non-modal notice").duration(Duration::from_secs(10)),
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: 150.0,
                y: 20.0,
                button: PointerButton::Left,
            }),
            &mut queue,
            &clicks,
        );
        render(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: 150.0,
                y: 20.0,
                button: PointerButton::Left,
            }),
            &mut queue,
            &clicks,
        );
        assert_eq!(*clicks.borrow(), 1, "the toast must not block the button");
        assert_eq!(queue.len(), 1, "the toast is still visible");
    }
}
