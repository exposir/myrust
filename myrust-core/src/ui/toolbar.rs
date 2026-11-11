//! Horizontal toolbar with tail overflow menu.

use super::context::Context;
use super::floating::MenuState;

/// One toolbar item and its caller-declared width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolbarItem<'a> {
    /// Stable item id.
    pub id: &'a str,
    /// Menu label used when the item overflows.
    pub label: &'a str,
    /// Width used for overflow calculation.
    pub width: f32,
}

impl<'a> ToolbarItem<'a> {
    /// Creates a toolbar item.
    pub const fn new(id: &'a str, label: &'a str, width: f32) -> Self {
        Self { id, label, width }
    }
}

/// Caller-owned toolbar overflow state.
#[derive(Debug, Clone, Default)]
pub struct ToolbarState {
    /// Menu state for the overflow button.
    pub overflow: MenuState,
}

impl ToolbarState {
    /// Creates an empty toolbar state.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing a toolbar for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolbarResponse {
    /// Number of rendered leading items.
    pub visible: usize,
    /// Number of items moved into overflow.
    pub overflow: usize,
    /// Original item index selected from the overflow menu, if any.
    pub selected: Option<usize>,
    /// Whether the overflow menu is open.
    pub overflow_open: bool,
}

/// Creates a toolbar.
///
/// `available_width` and the item widths determine which leading items remain
/// visible; the tail goes into a menu button labelled `…`. `render` builds the
/// visible items in order inside fixed-width item containers.
pub fn toolbar(
    context: &mut Context,
    id: &str,
    state: &mut ToolbarState,
    items: &[ToolbarItem<'_>],
    available_width: f32,
    render: impl FnMut(&mut Context, usize),
) -> ToolbarResponse {
    context.toolbar(id, state, items, available_width, render)
}

/// Returns how many leading items fit before reserving the overflow button.
pub fn overflow_split(items: &[ToolbarItem<'_>], available_width: f32, gap: f32) -> usize {
    if items.is_empty() {
        return 0;
    }
    let width = |item: &ToolbarItem<'_>| {
        if item.width.is_finite() && item.width > 0.0 {
            item.width
        } else {
            48.0
        }
    };
    let total = items.iter().map(width).sum::<f32>() + gap * items.len().saturating_sub(1) as f32;
    if total <= available_width {
        return items.len();
    }
    let reserve = 44.0;
    let mut visible = 0;
    let mut used = 0.0;
    for (index, item) in items.iter().enumerate() {
        let next = if index == 0 {
            width(item)
        } else {
            used + gap + width(item)
        };
        let needed = next + gap + reserve;
        if needed > available_width {
            break;
        }
        used = next;
        visible += 1;
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::context::text;
    use crate::ui::event::{PointerButton, UiEvent};
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
        let initial = render(context, size, None, build);
        let point = center_of(&initial, id);
        render(
            context,
            size,
            Some(UiEvent::PointerPressed {
                x: point.x,
                y: point.y,
                button: PointerButton::Left,
            }),
            build,
        );
        render(
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
    fn toolbar_overflows_tail_and_menu_selection_maps_back() {
        let size = Size::new(420.0, 160.0);
        let mut context = Context::new(size);
        let items = [
            ToolbarItem::new("a", "Alpha", 100.0),
            ToolbarItem::new("b", "Beta", 100.0),
            ToolbarItem::new("c", "Gamma", 100.0),
        ];
        assert_eq!(overflow_split(&items, 300.0, 4.0), 2);
        let state = Rc::new(RefCell::new(ToolbarState::new()));
        let last = Rc::new(RefCell::new(ToolbarResponse::default()));
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let view_rendered = Rc::clone(&rendered);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let mut rendered = view_rendered.borrow_mut();
            rendered.clear();
            *view_last.borrow_mut() = toolbar(
                context,
                "bar",
                &mut state,
                &items,
                300.0,
                |context, index| {
                    rendered.push(index);
                    text(context, items[index].label);
                },
            );
        };
        let initial = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().visible, 2);
        assert_eq!(last.borrow().overflow, 1);
        assert_eq!(*rendered.borrow(), vec![0, 1]);
        assert!(initial
            .interactions
            .iter()
            .any(|region| region.button_id == "bar:overflow"));

        click(&mut context, size, "bar:overflow", &mut build);
        assert!(last.borrow().overflow_open);
        click(&mut context, size, "bar:overflow:menu:item:0", &mut build);
        assert_eq!(last.borrow().selected, Some(2));
        assert!(!last.borrow().overflow_open);
    }

    #[test]
    fn toolbar_keeps_all_items_when_they_fit() {
        let items = [
            ToolbarItem::new("a", "Alpha", 80.0),
            ToolbarItem::new("b", "Beta", 80.0),
        ];
        assert_eq!(overflow_split(&items, 200.0, 4.0), 2);
        let size = Size::new(300.0, 100.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(ToolbarState::new()));
        let last = Rc::new(RefCell::new(ToolbarResponse::default()));
        let view_state = Rc::clone(&state);
        let view_last = Rc::clone(&last);
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            *view_last.borrow_mut() = toolbar(
                context,
                "fit",
                &mut state,
                &items,
                200.0,
                |context, index| {
                    text(context, items[index].label);
                },
            );
        };
        let frame = render(&mut context, size, None, &mut build);
        assert_eq!(last.borrow().visible, 2);
        assert_eq!(last.borrow().overflow, 0);
        assert!(!frame
            .interactions
            .iter()
            .any(|region| region.button_id == "fit:overflow"));
    }
}
