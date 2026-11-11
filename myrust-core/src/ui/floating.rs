//! Anchored interaction surfaces layered on the overlay primitive.
//!
//! Batch 2a groups four controls that share one idea: a widget owns an anchor
//! rectangle in the base view, and a floating surface is positioned from that
//! anchor. Menus and popovers reuse [`OverlayState`] bookkeeping for z-order,
//! focus trapping, Escape, and outside-click dismissal. Tooltips are
//! deliberately different: they are non-interactive, never take focus, and
//! never block pointer input below them.

use std::time::Duration;

use super::context::Context;
use super::layout::Point;
use super::overlay::{OverlayResponse, OverlayState};
use super::paint::Rect;

/// Default time the pointer or focus must remain on a tooltip anchor.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);

/// Caller-owned persistent state for a menu surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuState {
    /// Open flag and dismissal bookkeeping shared with the overlay primitive.
    pub open: OverlayState,
    /// Keyboard-highlighted item.
    pub highlight: usize,
}

impl MenuState {
    /// Creates a menu state with an explicit open flag.
    pub fn new(open: bool) -> Self {
        Self {
            open: OverlayState::new(open),
            highlight: 0,
        }
    }
}

/// Result of constructing a menu button for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuButtonState {
    /// Whether the menu is open after this frame's input.
    pub open: bool,
    /// Whether this frame's click or Enter activated the trigger.
    pub trigger_clicked: bool,
    /// Item selected this frame, if any.
    pub selected: Option<usize>,
    /// Whether this frame closed an open menu.
    pub closed: bool,
    /// Whether the trigger owns keyboard focus.
    pub focused: bool,
    /// Whether the pointer is over the trigger.
    pub hovered: bool,
}

/// Caller-owned persistent state for a context menu.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContextMenuState {
    /// Open flag and dismissal bookkeeping shared with the overlay primitive.
    pub open: OverlayState,
    /// Keyboard-highlighted item.
    pub highlight: usize,
    /// Window-space point where the menu was opened.
    pub anchor: Option<Point>,
}

/// Result of constructing a context menu for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextMenuResponse {
    /// Whether the menu is open after this frame's input.
    pub open: bool,
    /// Whether a secondary-button press opened the menu this frame.
    pub opened: bool,
    /// Item selected this frame, if any.
    pub selected: Option<usize>,
    /// Whether this frame closed an open menu.
    pub closed: bool,
}

/// Result of constructing a tooltip for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TooltipState {
    /// Whether the tooltip is painted this frame.
    pub visible: bool,
    /// Whether the pointer is over the anchor.
    pub hovering: bool,
    /// Whether the optional focus target owns focus.
    pub focused: bool,
    /// Time the pointer or focus has stayed on the anchor.
    pub elapsed: Duration,
}

/// Creates a button whose menu is anchored below its previous bounds.
///
/// The menu opens by click or focused Enter/Space, moves its highlight with
/// ArrowUp/ArrowDown, commits with Enter/Space, and closes on Escape or a
/// primary-button release outside its previous bounds.
pub fn menu_button(
    context: &mut Context,
    id: &str,
    label: &str,
    state: &mut MenuState,
    items: &[&str],
) -> MenuButtonState {
    context.menu_button(id, label, state, items)
}

/// Creates a system-style context menu over `area`.
///
/// A non-primary pointer press inside `area` opens the menu at the press
/// point. `UiEvent::PointerPressed` carries the originating button, so an
/// adapter must forward right-button events unchanged; the native tao path
/// maps `MouseButton::Right` to [`super::event::PointerButton::Right`] before
/// this point. Platforms that synthesize context menus from Ctrl+click or
/// trackpad gestures must translate those gestures to a secondary press.
pub fn context_menu(
    context: &mut Context,
    id: &str,
    state: &mut ContextMenuState,
    area: Rect,
    items: &[&str],
) -> ContextMenuResponse {
    context.context_menu(id, state, area, items)
}

/// Creates a hover-only tooltip with [`TOOLTIP_DELAY`].
pub fn tooltip(context: &mut Context, id: &str, anchor: Rect, text: &str) -> TooltipState {
    context.tooltip_with_delay(id, anchor, None, text, TOOLTIP_DELAY)
}

/// Creates a tooltip shown by hover or by focus on `target`.
pub fn tooltip_for(
    context: &mut Context,
    id: &str,
    anchor: Rect,
    target: &str,
    text: &str,
) -> TooltipState {
    context.tooltip_with_delay(id, anchor, Some(target), text, TOOLTIP_DELAY)
}

/// Creates a tooltip with explicit focus target and delay.
///
/// The timer is owned by the context and reset whenever neither the pointer
/// nor the focus target is on the anchor. While the delay is pending the
/// context requests another frame, so the tooltip can appear without extra
/// platform input.
pub fn tooltip_with_delay(
    context: &mut Context,
    id: &str,
    anchor: Rect,
    target: Option<&str>,
    text: &str,
    delay: Duration,
) -> TooltipState {
    context.tooltip_with_delay(id, anchor, target, text, delay)
}

/// Creates an anchored panel with the full overlay focus and dismiss behavior.
pub fn popover(
    context: &mut Context,
    id: &str,
    state: &mut OverlayState,
    anchor: Rect,
    content: impl FnOnce(&mut Context),
) -> OverlayResponse {
    context.overlay(id, state, anchor, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::access::Role;
    use crate::ui::event::{Key, PointerButton, UiEvent};
    use crate::ui::layout::Size;
    use crate::ui::paint::DrawCommand;
    use std::{cell::RefCell, rc::Rc, time::Duration};

    fn render_frame(
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
        build: &mut impl FnMut(&mut Context),
    ) -> super::super::native::ViewFrame {
        let initial = render_frame(context, size, None, build);
        let point = center_of(&initial, id);
        render_frame(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        );
        render_frame(
            context,
            size,
            Some(UiEvent::PointerReleased {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        )
    }

    #[test]
    fn menu_button_opens_navigates_commits_and_dismisses() {
        let size = Size::new(320.0, 260.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(MenuState::default()));
        let last = Rc::new(RefCell::new(MenuButtonState::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let response = menu_button(
                context,
                "actions",
                "Actions",
                &mut state,
                &["Copy", "Paste", "Delete"],
            );
            *view_last.borrow_mut() = response;
        };

        let first = render_frame(&mut context, size, None, &mut build);
        let trigger = center_of(&first, "actions");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: trigger.x,
                y: trigger.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        let opened = render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: trigger.x,
                y: trigger.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(last.borrow().open);
        assert!(state.borrow().open.open);
        assert!(opened
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Popup));
        assert!(opened
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Option && node.label == "Copy" && node.focused }));

        let moved = render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        assert!(moved
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Option && node.label == "Paste" && node.focused }));

        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(last.borrow().selected, Some(1));
        assert!(!last.borrow().open);
        assert!(!state.borrow().open.open);

        render_frame(&mut context, size, None, &mut build);
        assert!(
            last.borrow().focused,
            "menu selection restores focus to the trigger"
        );

        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert!(last.borrow().open, "Enter opens the focused trigger");
        assert_eq!(
            last.borrow().selected,
            None,
            "opening with Enter must not commit the first item"
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!last.borrow().open);
        assert!(last.borrow().closed);

        click(&mut context, size, "actions", &mut build);
        assert!(last.borrow().open);
        let outside = crate::ui::layout::Point::new(size.width - 5.0, size.height - 5.0);
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: outside.x,
                y: outside.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: outside.x,
                y: outside.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(!last.borrow().open, "outside release closes the menu");
        assert!(!state.borrow().open.open);
    }

    #[test]
    fn context_menu_opens_at_the_secondary_press_and_commits_with_enter() {
        let size = Size::new(320.0, 260.0);
        let area = Rect::new(20.0, 20.0, 280.0, 120.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(ContextMenuState::default()));
        let last = Rc::new(RefCell::new(ContextMenuResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let response = context_menu(
                context,
                "canvas",
                &mut state,
                area,
                &["Cut", "Copy", "Paste"],
            );
            *view_last.borrow_mut() = response;
        };

        render_frame(&mut context, size, None, &mut build);
        let opened = render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: 60.0,
                y: 60.0,
                button: PointerButton::Right,
            }),
            &mut build,
        );
        assert!(last.borrow().opened);
        assert!(last.borrow().open);
        assert_eq!(last.borrow().selected, None);
        assert!(opened
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Popup));
        assert!(opened
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Option && node.label == "Cut" && node.focused }));

        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: 60.0,
                y: 60.0,
                button: PointerButton::Right,
            }),
            &mut build,
        );
        assert!(last.borrow().open, "secondary release keeps the menu open");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key {
                key: Key::ArrowDown,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Enter }),
            &mut build,
        );
        assert_eq!(last.borrow().selected, Some(1));
        assert!(!last.borrow().open);

        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: 310.0,
                y: 250.0,
                button: PointerButton::Right,
            }),
            &mut build,
        );
        assert!(!last.borrow().opened, "a press outside the area is inert");
        assert!(!last.borrow().open);

        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: 80.0,
                y: 70.0,
                button: PointerButton::Right,
            }),
            &mut build,
        );
        assert!(last.borrow().open);
        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!last.borrow().open);
        assert!(last.borrow().closed);

        let reopened = render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: 80.0,
                y: 70.0,
                button: PointerButton::Right,
            }),
            &mut build,
        );
        assert!(last.borrow().open);
        let item = center_of(&reopened, "canvas:menu:item:2");
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: item.x,
                y: item.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: item.x,
                y: item.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert_eq!(
            last.borrow().selected,
            Some(2),
            "clicking a menu item selects it"
        );
        assert!(!last.borrow().open);
    }

    #[test]
    fn tooltip_waits_for_the_delay_and_hides_on_leave_or_focus_loss() {
        let size = Size::new(320.0, 200.0);
        let anchor = Rect::new(20.0, 20.0, 100.0, 30.0);
        let mut context = Context::new(size);
        let last = Rc::new(RefCell::new(TooltipState::default()));
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            *view_last.borrow_mut() = tooltip(context, "hint", anchor, "Helpful hint");
        };

        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerMoved { x: 5.0, y: 5.0 }),
            &mut build,
        );
        assert!(!last.borrow().visible);
        assert!(!last.borrow().hovering);

        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerMoved { x: 50.0, y: 30.0 }),
            &mut build,
        );
        assert!(last.borrow().hovering);
        assert!(!last.borrow().visible);
        assert_eq!(last.borrow().elapsed, Duration::ZERO);

        context.begin_frame(size);
        context.set_input(UiEvent::PointerMoved { x: 50.0, y: 30.0 });
        context.advance_frame_time_for_test(Duration::from_millis(300));
        let pending = tooltip(&mut context, "hint", anchor, "Helpful hint");
        let pending_frame = context.produce().unwrap();
        context.end_frame();
        assert!(!pending.visible);
        assert!(pending_frame.needs_animation_frame);

        context.begin_frame(size);
        context.set_input(UiEvent::PointerMoved { x: 50.0, y: 30.0 });
        context.advance_frame_time_for_test(Duration::from_millis(300));
        let shown = tooltip(&mut context, "hint", anchor, "Helpful hint");
        let shown_frame = context.produce().unwrap();
        context.end_frame();
        assert!(shown.visible);
        assert!(shown_frame.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Helpful hint")
        ));
        assert!(!shown_frame.needs_animation_frame);

        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerMoved { x: 5.0, y: 5.0 }),
            &mut build,
        );
        assert!(!last.borrow().visible);
        assert_eq!(last.borrow().elapsed, Duration::ZERO);

        let focus_anchor = Rect::new(180.0, 140.0, 80.0, 20.0);
        let focus_last = Rc::new(RefCell::new(TooltipState::default()));
        let view_focus_last = Rc::clone(&focus_last);
        let mut build_focus = move |context: &mut Context| {
            *view_focus_last.borrow_mut() =
                tooltip_for(context, "focus-hint", focus_anchor, "target", "Focus hint");
        };
        context.request_focus("target");
        render_frame(&mut context, size, None, &mut build_focus);
        assert!(focus_last.borrow().focused);
        assert!(!focus_last.borrow().visible);
        context.begin_frame(size);
        context.advance_frame_time_for_test(Duration::from_millis(600));
        let focused = tooltip_for(
            &mut context,
            "focus-hint",
            focus_anchor,
            "target",
            "Focus hint",
        );
        let focused_frame = context.produce().unwrap();
        context.end_frame();
        assert!(focused.visible);
        assert!(focused_frame.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Focus hint")
        ));
    }

    #[test]
    fn popover_traps_focus_and_closes_on_escape_and_outside_click() {
        let size = Size::new(360.0, 280.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(OverlayState::default()));
        let view_state = Rc::clone(&state);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let trigger = context.id("open-panel").button("Open panel");
            if trigger.clicked {
                state.open = true;
            }
            if state.open {
                let anchor = trigger
                    .bounds
                    .unwrap_or_else(|| Rect::new(20.0, 20.0, 120.0, 30.0));
                popover(context, "panel", &mut state, anchor, |context| {
                    context.id("panel-action").button("Panel action");
                });
            }
        };

        click(&mut context, size, "open-panel", &mut build);
        assert!(state.borrow().open);
        let focused_frame = render_frame(&mut context, size, None, &mut build);
        assert!(focused_frame.access_nodes.iter().any(|node| {
            node.role == Role::Button && node.label == "Panel action" && node.focused
        }));

        render_frame(
            &mut context,
            size,
            Some(UiEvent::Key { key: Key::Escape }),
            &mut build,
        );
        assert!(!state.borrow().open, "Escape closes the popover");
        let restored = render_frame(&mut context, size, None, &mut build);
        assert!(restored
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Button && node.label == "Open panel" && node.focused));

        click(&mut context, size, "open-panel", &mut build);
        assert!(state.borrow().open);
        let outside = crate::ui::layout::Point::new(size.width - 5.0, size.height - 5.0);
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerPressed {
                x: outside.x,
                y: outside.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        render_frame(
            &mut context,
            size,
            Some(UiEvent::PointerReleased {
                x: outside.x,
                y: outside.y,
                button: PointerButton::Left,
            }),
            &mut build,
        );
        assert!(!state.borrow().open, "outside release closes the popover");
    }
}
