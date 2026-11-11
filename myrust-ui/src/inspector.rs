//! Debug inspector for a previous frame.

use super::access::Role;
use super::context::Context;
use super::native::ViewFrame;
use super::paint::{DisplayList, DrawCommand, Rect};

/// Paint-command counts for one inspector element or frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PaintSummary {
    /// Rect count.
    pub rects: usize,
    /// Rounded rect count.
    pub rounded_rects: usize,
    /// Circle count.
    pub circles: usize,
    /// Line count.
    pub lines: usize,
    /// Text count.
    pub text: usize,
    /// SVG path count.
    pub svg_paths: usize,
    /// Gradient count.
    pub gradients: usize,
    /// Image count.
    pub images: usize,
}

impl PaintSummary {
    /// Total commands counted.
    pub fn total(self) -> usize {
        self.rects
            + self.rounded_rects
            + self.circles
            + self.lines
            + self.text
            + self.svg_paths
            + self.gradients
            + self.images
    }

    /// Compact debug string.
    pub fn summary(self) -> String {
        format!(
            "rect {} rr {} text {} line {} path {} img {}",
            self.rects, self.rounded_rects, self.text, self.lines, self.svg_paths, self.images
        )
    }

    fn record(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Rect { .. } => self.rects += 1,
            DrawCommand::RoundedRect { .. } => self.rounded_rects += 1,
            DrawCommand::Circle { .. } => self.circles += 1,
            DrawCommand::Line { .. } => self.lines += 1,
            DrawCommand::Text { .. } => self.text += 1,
            DrawCommand::SvgPath { .. } => self.svg_paths += 1,
            DrawCommand::LinearGradient { .. } => self.gradients += 1,
            DrawCommand::Image { .. } => self.images += 1,
            _ => {}
        }
    }
}

/// One inspector tree element.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectorItem {
    /// Best-effort stable id: interaction id when bounds match, otherwise a
    /// role/label-derived id.
    pub id: String,
    /// Accessibility role.
    pub role: Role,
    /// Accessible label.
    pub label: String,
    /// Absolute bounds from the previous frame.
    pub bounds: Rect,
    /// Commands intersecting this element's bounds.
    pub paint: PaintSummary,
    /// Nested elements by bounds containment.
    pub children: Vec<InspectorItem>,
}

/// Inspector tree for one previous frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InspectorTree {
    /// Root elements.
    pub roots: Vec<InspectorItem>,
    /// All paint commands in the frame.
    pub total: PaintSummary,
}

impl InspectorTree {
    /// Total element count, recursively.
    pub fn len(&self) -> usize {
        fn count(items: &[InspectorItem]) -> usize {
            items.iter().map(|item| 1 + count(&item.children)).sum()
        }
        count(&self.roots)
    }

    /// Whether there are no elements.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

/// Result of rendering an inspector tree as a list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InspectorResponse {
    /// Rendered element count.
    pub items: usize,
    /// Total paint commands in the frame.
    pub total_commands: usize,
}

/// Summarizes a display list without building an element tree.
pub fn inspect_display(display: &DisplayList) -> PaintSummary {
    let mut summary = PaintSummary::default();
    for command in &display.commands {
        summary.record(command);
    }
    summary
}

/// Builds an inspector tree from the access nodes and display list of a frame.
///
/// Nesting is derived from bounds containment; overlapping sibling bounds may
/// produce an approximate tree.
pub fn inspect_frame(frame: &ViewFrame) -> InspectorTree {
    let total = inspect_display(&frame.display);
    let mut flat: Vec<(InspectorItem, Rect)> = frame
        .access_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let id = frame
                .interactions
                .iter()
                .find(|region| region.bounds == node.bounds)
                .map(|region| region.button_id.clone())
                .unwrap_or_else(|| {
                    if node.label.is_empty() {
                        format!("{:?}#{index}", node.role)
                    } else {
                        node.label.clone()
                    }
                });
            let paint = commands_for_bounds(&frame.display, node.bounds);
            (
                InspectorItem {
                    id,
                    role: node.role,
                    label: node.label.clone(),
                    bounds: node.bounds,
                    paint,
                    children: Vec::new(),
                },
                node.bounds,
            )
        })
        .collect();
    if flat.is_empty() {
        return InspectorTree {
            roots: Vec::new(),
            total,
        };
    }
    flat.sort_by(|left, right| {
        let left_area = left.1.width * left.1.height;
        let right_area = right.1.width * right.1.height;
        right_area.total_cmp(&left_area)
    });
    let mut parent: Vec<Option<usize>> = vec![None; flat.len()];
    for index in 0..flat.len() {
        let bounds = flat[index].1;
        let mut best: Option<(usize, f32)> = None;
        for (candidate, (_, candidate_bounds)) in flat.iter().enumerate().take(index) {
            if contains(*candidate_bounds, bounds) {
                let area = candidate_bounds.width * candidate_bounds.height;
                if best.is_none_or(|(_, best_area)| area < best_area) {
                    best = Some((candidate, area));
                }
            }
        }
        parent[index] = best.map(|(candidate, _)| candidate);
    }
    let mut nodes = flat.into_iter().map(|(item, _)| item).collect::<Vec<_>>();
    for index in (0..nodes.len()).rev() {
        if let Some(parent_index) = parent[index] {
            let child = nodes.remove(index);
            nodes[parent_index].children.push(child);
        }
    }
    InspectorTree {
        roots: nodes,
        total,
    }
}

/// Renders an inspector tree as an indented text list.
pub fn inspector(context: &mut Context, id: &str, tree: &InspectorTree) -> InspectorResponse {
    context.inspector(id, tree)
}

fn commands_for_bounds(display: &DisplayList, bounds: Rect) -> PaintSummary {
    let mut summary = PaintSummary::default();
    for command in &display.commands {
        if let Some(command_bounds) = command_bounds(command) {
            if intersects(command_bounds, bounds) {
                summary.record(command);
            }
        }
    }
    summary
}

fn command_bounds(command: &DrawCommand) -> Option<Rect> {
    match command {
        DrawCommand::Rect {
            x,
            y,
            width,
            height,
            ..
        }
        | DrawCommand::RoundedRect {
            x,
            y,
            width,
            height,
            ..
        }
        | DrawCommand::LinearGradient {
            x,
            y,
            width,
            height,
            ..
        }
        | DrawCommand::SvgPath {
            x,
            y,
            width,
            height,
            ..
        }
        | DrawCommand::Image {
            x,
            y,
            width,
            height,
            ..
        } => Some(Rect::new(*x, *y, *width, *height)),
        DrawCommand::Circle { x, y, radius, .. } => Some(Rect::new(
            *x - *radius,
            *y - *radius,
            *radius * 2.0,
            *radius * 2.0,
        )),
        DrawCommand::Line { x1, y1, x2, y2, .. } => Some(Rect::new(
            x1.min(*x2),
            y1.min(*y2),
            (x2 - x1).abs(),
            (y2 - y1).abs(),
        )),
        DrawCommand::Text {
            x,
            y,
            text,
            font_size,
            max_width,
            ..
        } => Some(Rect::new(
            *x,
            *y,
            max_width.unwrap_or_else(|| text.len() as f32 * font_size * 0.6),
            *font_size * 1.2,
        )),
        _ => None,
    }
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn intersects(left: Rect, right: Rect) -> bool {
    left.x < right.right()
        && left.right() > right.x
        && left.y < right.bottom()
        && left.bottom() > right.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{button, column, text};
    use crate::layout::Size;
    use crate::paint::DrawCommand;

    fn frame() -> ViewFrame {
        let size = Size::new(240.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        column(&mut context).padding(8.0).children(|context| {
            if button(context, "Inspect").clicked {
                // no-op
            }
            text(context, "Inspector body");
        });
        context.produce().unwrap()
    }

    #[test]
    fn builds_element_tree_and_paint_summaries() {
        let frame = frame();
        let tree = inspect_frame(&frame);
        assert!(!tree.is_empty());
        assert!(tree.len() >= 2);
        assert!(tree.total.text >= 1);
        let root = &tree.roots[0];
        assert!(root.role == Role::Group);
        assert!(root
            .children
            .iter()
            .any(|child| child.label == "Inspect" && child.role == Role::Button));
        assert!(root
            .children
            .iter()
            .any(|child| child.label == "Inspector body"));
        assert!(tree.total.rounded_rects > 0 || tree.total.text > 0);
    }

    #[test]
    fn empty_frame_builds_an_empty_tree() {
        let tree = inspect_frame(&ViewFrame::default());
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert_eq!(tree.total.total(), 0);

        let size = Size::new(200.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let empty_response = inspector(&mut context, "inspector", &tree);
        assert_eq!(empty_response.items, 0);
    }

    #[test]
    fn inspector_renders_an_indented_list() {
        let frame = frame();
        let tree = inspect_frame(&frame);
        let size = Size::new(360.0, 240.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let response = inspector(&mut context, "tree", &tree);
        let rendered = context.produce().unwrap();
        assert!(response.items >= 2);
        assert!(rendered.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text.contains("Inspect"))
        ));
        assert!(rendered
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "tree"));
    }
}
