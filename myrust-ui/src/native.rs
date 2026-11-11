//! Immediate-mode native UI runtime used as application window content.
//!
//! [`NativeUi`] owns one [`Context`] and one user render callback. Every frame
//! starts with accumulated input, invokes the callback to construct widgets,
//! resolves layout, and returns a positioned display list.

use std::{cell::RefCell, fmt, rc::Rc};

use super::access::AccessNode;
use super::context::Context;
use super::event::{InteractiveRegion, UiEvent};
use super::layout::{LayoutResult, Size};
use super::paint::DisplayList;
use super::render::{RenderStats, RendererKind};
use super::theme::Theme;

/// Complete output of one immediate-mode frame.
#[derive(Debug, Clone, Default)]
pub struct ViewFrame {
    /// Absolute layout rectangles.
    pub layout: Vec<LayoutResult>,
    /// Positioned drawing instructions.
    pub display: DisplayList,
    /// Button interaction regions from this frame.
    pub interactions: Vec<InteractiveRegion>,
    /// CPU-side command statistics.
    pub stats: RenderStats,
    /// Accessibility metadata for this frame.
    pub access_nodes: Vec<AccessNode>,
    /// Whether another frame is needed to finish an active animation.
    ///
    /// The application requests a best-effort redraw while this is true. There
    /// is no private timer-driven frame scheduler; on Linux this relies on the
    /// host event loop's `request_redraw` wakeup behavior.
    pub needs_animation_frame: bool,
    /// Whether the final frame consumed pending input from the host window.
    ///
    /// [`NativeUi::render`] settles a consumed-input frame with one immediate
    /// no-input follow-up frame, so a normal returned frame has this value set
    /// to `false`.
    pub input_consumed: bool,
}

/// Error raised while driving an immediate-mode native view.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeUiError {
    /// The runtime is already borrowed, usually due to re-entrant callbacks.
    #[error("native UI runtime is already in use")]
    Busy,
    /// Frame production failed after the callback returned.
    #[error("native UI render failed: {0}")]
    Render(String),
}

/// Shareable immediate-mode native UI content for a window.
#[derive(Clone)]
pub struct NativeUi {
    runtime: Rc<RefCell<NativeRuntime>>,
    input_queue: Rc<RefCell<Vec<UiEvent>>>,
    renderer_kind: RendererKind,
    theme: Theme,
}

impl fmt::Debug for NativeUi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeUi")
            .field("renderer_kind", &self.renderer_kind)
            .finish_non_exhaustive()
    }
}

impl NativeUi {
    /// Creates immediate-mode UI from a per-frame render callback.
    pub fn new(render: impl FnMut(&mut Context) + 'static) -> Self {
        Self {
            runtime: Rc::new(RefCell::new(NativeRuntime {
                render: Box::new(render),
                context: Context::new(Size::new(1.0, 1.0)),
                last_size: Size::new(1.0, 1.0),
            })),
            input_queue: Rc::new(RefCell::new(Vec::new())),
            renderer_kind: RendererKind::Software,
            theme: Theme::dark(),
        }
    }

    /// Selects the renderer preference used by the application window.
    ///
    /// The default is software so creating a window never attempts GPU
    /// initialization implicitly.
    pub fn with_renderer_kind(mut self, kind: RendererKind) -> Self {
        self.renderer_kind = kind;
        self
    }

    /// Returns the selected renderer preference.
    pub fn renderer_kind(&self) -> RendererKind {
        self.renderer_kind
    }

    /// Sets the theme used by the immediate-mode context.
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Returns the theme applied to the next immediate-mode frame.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Stores one platform-neutral input event for the next frame.
    pub fn input(&self, event: UiEvent) -> Result<(), NativeUiError> {
        self.input_queue
            .try_borrow_mut()
            .map_err(|_| NativeUiError::Busy)?
            .push(event);
        Ok(())
    }

    /// Returns whether input is waiting for the next render.
    pub fn has_pending_input(&self) -> Result<bool, NativeUiError> {
        let queue = self
            .input_queue
            .try_borrow()
            .map_err(|_| NativeUiError::Busy)?;
        Ok(!queue.is_empty())
    }

    /// Runs one immediate-mode frame and settles consumed input synchronously.
    ///
    /// If the first pass consumes pending input, one no-input follow-up pass is
    /// run immediately so callback state changes are reflected without relying
    /// on a host event-loop wakeup. At most two passes are executed.
    pub fn render(&self, size: Size) -> Result<ViewFrame, NativeUiError> {
        let first = self.render_once(size)?;
        if first.input_consumed {
            self.render_once(size)
        } else {
            Ok(first)
        }
    }

    fn render_once(&self, size: Size) -> Result<ViewFrame, NativeUiError> {
        let events = std::mem::take(
            &mut *self
                .input_queue
                .try_borrow_mut()
                .map_err(|_| NativeUiError::Busy)?,
        );
        let input_consumed = !events.is_empty();
        let mut runtime = self
            .runtime
            .try_borrow_mut()
            .map_err(|_| NativeUiError::Busy)?;
        let NativeRuntime {
            render,
            context,
            last_size,
        } = &mut *runtime;
        *last_size = size;
        context.begin_frame(size);
        context.set_theme(self.theme.clone());
        for event in events {
            context.set_input(event);
        }
        (render)(context);
        let mut frame = context.produce().map_err(NativeUiError::Render)?;
        frame.input_consumed = input_consumed;
        context.end_frame();
        Ok(frame)
    }

    /// Re-renders using the most recent logical size.
    pub fn redraw(&self) -> Result<ViewFrame, NativeUiError> {
        let size = {
            let runtime = self.runtime.try_borrow().map_err(|_| NativeUiError::Busy)?;
            runtime.last_size
        };
        self.render(size)
    }
}

struct NativeRuntime {
    render: Box<dyn FnMut(&mut Context)>,
    context: Context,
    last_size: Size,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{button, column, text};
    use crate::event::PointerButton;
    use crate::paint::DrawCommand;

    #[derive(Default)]
    struct State {
        clicks: u32,
    }

    #[test]
    fn clicked_uses_previous_frame_region_and_current_release() {
        let state = Rc::new(RefCell::new(State::default()));
        let render_state = state.clone();
        let ui = NativeUi::new(move |context| {
            column(context).padding(10.0).children(|context| {
                let clicked = button(context, "Increment").clicked;
                if clicked {
                    render_state.borrow_mut().clicks += 1;
                }
                text(
                    context,
                    &format!("clicks: {}", render_state.borrow().clicks),
                );
            });
        });

        let first = ui.render(Size::new(200.0, 120.0)).unwrap();
        let region = &first.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;

        ui.input(UiEvent::PointerPressed {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        ui.render(Size::new(200.0, 120.0)).unwrap();

        ui.input(UiEvent::PointerReleased {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        let released = ui.render(Size::new(200.0, 120.0)).unwrap();
        assert_eq!(state.borrow().clicks, 1);
        assert!(released.display.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Text { text, .. } if text == "clicks: 1"
            )
        }));
    }

    #[test]
    fn release_outside_previous_region_does_not_click() {
        let clicks = Rc::new(RefCell::new(0_u32));
        let render_clicks = clicks.clone();
        let ui = NativeUi::new(move |context| {
            if button(context, "Only").clicked {
                *render_clicks.borrow_mut() += 1;
            }
        });
        let frame = ui.render(Size::new(120.0, 80.0)).unwrap();
        let region = &frame.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;
        ui.input(UiEvent::PointerPressed {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        ui.render(Size::new(120.0, 80.0)).unwrap();
        ui.input(UiEvent::PointerReleased {
            x: x + 1000.0,
            y: y + 1000.0,
            button: PointerButton::Left,
        })
        .unwrap();
        ui.render(Size::new(120.0, 80.0)).unwrap();
        assert_eq!(*clicks.borrow(), 0);
    }

    #[test]
    fn pending_input_is_visible_to_the_callback() {
        let observed = Rc::new(RefCell::new((0.0, 0.0)));
        let render_observed = observed.clone();
        let ui = NativeUi::new(move |context| {
            if let Some(point) = context.pointer_position() {
                *render_observed.borrow_mut() = (point.x, point.y);
            }
        });
        ui.input(UiEvent::PointerMoved { x: 12.0, y: 34.0 })
            .unwrap();
        ui.render(Size::new(100.0, 100.0)).unwrap();
        assert_eq!(*observed.borrow(), (12.0, 34.0));
    }

    #[test]
    fn converged_render_returns_updated_text_without_consuming_again() {
        let clicks = Rc::new(RefCell::new(0_u32));
        let render_clicks = clicks.clone();
        let ui = NativeUi::new(move |context| {
            text(context, &format!("clicks: {}", render_clicks.borrow()));
            if button(context, "Increment").clicked {
                *render_clicks.borrow_mut() += 1;
            }
        });
        let initial = ui.render(Size::new(200.0, 120.0)).unwrap();
        assert!(!initial.input_consumed);
        let region = &initial.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;

        ui.input(UiEvent::PointerPressed {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        ui.input(UiEvent::PointerReleased {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        let click_frame = ui.render(Size::new(200.0, 120.0)).unwrap();
        assert!(!click_frame.input_consumed);
        assert_eq!(*clicks.borrow(), 1);
        assert!(click_frame.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "clicks: 1")
        ));

        let next_frame = ui.render(Size::new(200.0, 120.0)).unwrap();
        assert!(!next_frame.input_consumed);
        assert!(next_frame.display.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Text { text, .. } if text == "clicks: 1"
            )
        }));
    }

    #[test]
    fn raw_frame_marks_consumed_input_before_settling() {
        let clicks = Rc::new(RefCell::new(0_u32));
        let render_clicks = clicks.clone();
        let ui = NativeUi::new(move |context| {
            text(context, &format!("clicks: {}", render_clicks.borrow()));
            if button(context, "Increment").clicked {
                *render_clicks.borrow_mut() += 1;
            }
        });
        let initial = ui.render_once(Size::new(200.0, 120.0)).unwrap();
        let region = &initial.interactions[0];
        let x = region.bounds.x + region.bounds.width / 2.0;
        let y = region.bounds.y + region.bounds.height / 2.0;
        ui.input(UiEvent::PointerPressed {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        ui.input(UiEvent::PointerReleased {
            x,
            y,
            button: PointerButton::Left,
        })
        .unwrap();
        let click_frame = ui.render_once(Size::new(200.0, 120.0)).unwrap();
        assert!(click_frame.input_consumed);
        assert_eq!(*clicks.borrow(), 1);
        assert!(click_frame.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Text { text, .. } if text == "clicks: 0")
        }));
        let follow_up = ui.render_once(Size::new(200.0, 120.0)).unwrap();
        assert!(!follow_up.input_consumed);
        assert!(follow_up.display.commands.iter().any(|command| {
            matches!(command, DrawCommand::Text { text, .. } if text == "clicks: 1")
        }));
    }

    #[test]
    fn settle_loop_stops_after_two_frames_when_callback_keeps_injecting_input() {
        let handle = Rc::new(RefCell::new(None::<NativeUi>));
        let callback_handle = handle.clone();
        let calls = Rc::new(RefCell::new(0_u32));
        let callback_calls = calls.clone();
        let ui = NativeUi::new(move |_context| {
            *callback_calls.borrow_mut() += 1;
            let ui = callback_handle
                .borrow()
                .as_ref()
                .expect("native UI handle installed")
                .clone();
            ui.input(UiEvent::PointerMoved { x: 1.0, y: 1.0 }).unwrap();
        });
        *handle.borrow_mut() = Some(ui.clone());
        ui.input(UiEvent::PointerMoved { x: 0.0, y: 0.0 }).unwrap();
        let frame = ui.render(Size::new(100.0, 80.0)).unwrap();
        assert!(frame.input_consumed);
        assert_eq!(*calls.borrow(), 2);
        assert!(ui.has_pending_input().unwrap());
    }
}
