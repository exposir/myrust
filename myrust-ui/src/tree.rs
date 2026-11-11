//! Expandable tree rows with caller-owned expansion and selection state.

use super::context::Context;

/// One flat tree node. The caller supplies visible nodes in display order and
/// uses `depth` for indentation; collapsed descendants can simply be omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeNode<'a> {
    /// Stable node id. Also used as the key in the expanded set.
    pub id: &'a str,
    /// Visible label.
    pub label: &'a str,
    /// Indentation depth, starting at zero.
    pub depth: usize,
    /// Whether this node shows an expand/collapse affordance.
    pub has_children: bool,
}

impl<'a> TreeNode<'a> {
    /// Creates a tree node.
    pub const fn new(id: &'a str, label: &'a str, depth: usize, has_children: bool) -> Self {
        Self {
            id,
            label,
            depth,
            has_children,
        }
    }
}

/// Caller-owned tree selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreeState {
    /// Selected node id.
    pub selected_id: String,
}

impl TreeState {
    /// Creates an empty selection.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of constructing a tree or outline for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreeResponse {
    /// Selected node id after this frame's click input.
    pub selected_id: String,
    /// Whether the selected node changed this frame.
    pub changed: bool,
    /// Row id clicked this frame, if any.
    pub clicked_id: Option<String>,
    /// Node id whose expansion toggled this frame, if any.
    pub toggled_id: Option<String>,
}

/// Creates indented tree rows. `expanded` is caller-owned and contains node ids
/// whose disclosure arrow is open. Clicking a disclosure arrow toggles that id;
/// clicking the row selects it.
pub fn tree(
    context: &mut Context,
    id: &str,
    state: &mut TreeState,
    nodes: &[TreeNode<'_>],
    expanded: &mut Vec<String>,
) -> TreeResponse {
    context.tree(id, state, nodes, expanded)
}

/// Creates a panel-style outline over the same tree interaction semantics.
pub fn outline(
    context: &mut Context,
    id: &str,
    state: &mut TreeState,
    nodes: &[TreeNode<'_>],
    expanded: &mut Vec<String>,
) -> TreeResponse {
    context.outline(id, state, nodes, expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Role;
    use crate::event::{PointerButton, UiEvent};
    use crate::layout::Size;
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

    fn nodes() -> [TreeNode<'static>; 3] {
        [
            TreeNode::new("root", "Root", 0, true),
            TreeNode::new("child", "Child", 1, false),
            TreeNode::new("sibling", "Sibling", 0, false),
        ]
    }

    #[test]
    fn tree_toggles_expansion_and_selects_rows() {
        let size = Size::new(360.0, 220.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(TreeState::new()));
        let expanded = Rc::new(RefCell::new(Vec::new()));
        let last = Rc::new(RefCell::new(TreeResponse::default()));
        let view_state = Rc::clone(&state);
        let view_expanded = Rc::clone(&expanded);
        let view_last = Rc::clone(&last);
        let nodes = nodes();
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let mut expanded = view_expanded.borrow_mut();
            *view_last.borrow_mut() = tree(context, "tree", &mut state, &nodes, &mut expanded);
        };

        click(&mut context, size, "tree:toggle:0", &mut build);
        assert_eq!(last.borrow().toggled_id.as_deref(), Some("root"));
        assert_eq!(expanded.borrow().as_slice(), ["root".to_string()]);
        click(&mut context, size, "tree:toggle:0", &mut build);
        assert!(expanded.borrow().is_empty());

        click(&mut context, size, "tree:row:1", &mut build);
        assert_eq!(last.borrow().selected_id, "child");
        assert_eq!(last.borrow().clicked_id.as_deref(), Some("child"));
        assert!(last.borrow().changed);
        assert_eq!(state.borrow().selected_id.as_str(), "child");
        click(&mut context, size, "tree:row:0", &mut build);
        assert_eq!(last.borrow().selected_id, "root");
        assert_eq!(state.borrow().selected_id.as_str(), "root");
    }

    #[test]
    fn outline_reuses_tree_toggle_and_selection_semantics() {
        let size = Size::new(360.0, 220.0);
        let mut context = Context::new(size);
        let state = Rc::new(RefCell::new(TreeState::new()));
        let expanded = Rc::new(RefCell::new(Vec::new()));
        let last = Rc::new(RefCell::new(TreeResponse::default()));
        let view_state = Rc::clone(&state);
        let view_expanded = Rc::clone(&expanded);
        let view_last = Rc::clone(&last);
        let nodes = nodes();
        let mut build = move |context: &mut Context| {
            let mut state = view_state.borrow_mut();
            let mut expanded = view_expanded.borrow_mut();
            *view_last.borrow_mut() =
                outline(context, "outline", &mut state, &nodes, &mut expanded);
        };

        click(&mut context, size, "outline:toggle:0", &mut build);
        assert_eq!(last.borrow().toggled_id.as_deref(), Some("root"));
        assert_eq!(expanded.borrow().as_slice(), ["root".to_string()]);
        click(&mut context, size, "outline:row:2", &mut build);
        assert_eq!(last.borrow().selected_id, "sibling");
        assert!(last.borrow().changed);
        assert_eq!(state.borrow().selected_id.as_str(), "sibling");
        let frame = render(&mut context, size, None, &mut build);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "outline"));
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Option
                && node.label == "Sibling"
                && node.value.as_deref() == Some("selected")
        }));
    }
}
