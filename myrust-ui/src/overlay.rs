//! Floating surfaces above the window content.
//!
//! [`OverlayState`] is caller-owned: an application opens and closes an
//! overlay by setting `open`. The framework owns the z-order, hit-testing,
//! focus handoff, and dismiss bookkeeping for that overlay id. Content is
//! rebuilt each open frame, so it uses the same immediate-mode state as the
//! rest of the view.
//!
//! Escape and a release outside the previous overlay bounds close it. Clicks
//! below an open overlay are suppressed, including the click that dismisses
//! it, so a popup cannot leak a click into the control that opened it.
//!
//! Dismissal reads [`super::event::Key::Escape`] and arrow keys from the frame
//! input. A window adapter must translate its platform events to those
//! [`super::event::Key`] values; [`super::event::InputTranslator`] forwards
//! them once they are translated.

use super::context::Context;
use super::paint::Rect;

/// Caller-owned open flag for one overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlayState {
    /// Whether the overlay is open.
    pub open: bool,
}

impl OverlayState {
    /// Creates an overlay state with an explicit open flag.
    pub fn new(open: bool) -> Self {
        Self { open }
    }
}

/// Result of constructing an overlay for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OverlayResponse {
    /// Whether the overlay is open after this frame's input.
    pub open: bool,
    /// Whether this frame closed an overlay that was open.
    pub closed: bool,
    /// Whether a pointer release outside the overlay closed it.
    pub outside_clicked: bool,
    /// Whether Escape closed it.
    pub escape_pressed: bool,
    /// Previous-frame bounds, when the overlay was open before.
    pub bounds: Option<Rect>,
}

/// Builds a popup anchored below an element's previous bounds.
///
/// `anchor` is usually the trigger's [`super::event::InteractiveRegion`]
/// bounds. The popup uses the anchor width, flips above the anchor when it
/// would overflow the window, and draws after all non-overlay roots. While it
/// is open, the layers below it do not receive pointer input.
pub fn overlay(
    context: &mut Context,
    id: &str,
    state: &mut OverlayState,
    anchor: Rect,
    content: impl FnOnce(&mut Context),
) -> OverlayResponse {
    context.overlay(id, state, anchor, content)
}

/// Alias for [`overlay`], named for the popover primitive it provides.
pub fn popup(
    context: &mut Context,
    id: &str,
    state: &mut OverlayState,
    anchor: Rect,
    content: impl FnOnce(&mut Context),
) -> OverlayResponse {
    context.overlay(id, state, anchor, content)
}
