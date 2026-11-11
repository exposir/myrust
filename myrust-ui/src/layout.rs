//! Deterministic CPU layout solver for the native UI engine.
//!
//! The solver covers nested Row/Column flow, flexible growth, intrinsic
//! content sizes, min/max constraints, padding, margins, gaps, absolute
//! positioning, and equal-width grid columns. It does not shape text, render
//! graphics, or require a GPU/display.

use serde::{Deserialize, Serialize};

/// Two-dimensional size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
}

impl Size {
    /// Creates a size.
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Two-dimensional point in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
}

impl Point {
    /// Creates a point.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Four logical edge values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Left edge.
    pub left: f32,
}

impl Edges {
    /// Creates edge values.
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Creates equal horizontal and vertical edges.
    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self::new(vertical, horizontal, vertical, horizontal)
    }

    /// Creates equal values on all sides.
    pub fn all(value: f32) -> Self {
        Self::new(value, value, value, value)
    }

    pub(crate) fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub(crate) fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

/// Main flex direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    /// Horizontal main axis.
    #[default]
    Row,
    /// Vertical main axis.
    Column,
}

/// Main-axis distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    /// Pack toward the start.
    #[default]
    FlexStart,
    /// Pack toward the end.
    FlexEnd,
    /// Center the group.
    Center,
    /// Put remaining space between children.
    SpaceBetween,
    /// Put half the remaining space around each child.
    SpaceAround,
}

/// Cross-axis alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    /// Stretch auto-sized children across the cross axis.
    #[default]
    Stretch,
    /// Align children to the cross-axis start.
    FlexStart,
    /// Center children on the cross axis.
    Center,
    /// Align children to the cross-axis end.
    FlexEnd,
}

/// Flow participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    /// Participates in normal flow.
    #[default]
    Relative,
    /// Excluded from normal flow and positioned by `left`/`top`.
    Absolute,
}

/// Visual and layout style for one node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Explicit border-box width; `None` uses intrinsic or available width.
    pub width: Option<f32>,
    /// Explicit border-box height; `None` uses intrinsic or available height.
    pub height: Option<f32>,
    /// Minimum width.
    pub min_width: f32,
    /// Minimum height.
    pub min_height: f32,
    /// Maximum width.
    pub max_width: f32,
    /// Maximum height.
    pub max_height: f32,
    /// Outer margin.
    pub margin: Edges,
    /// Inner padding.
    pub padding: Edges,
    /// Gap between relative children.
    pub gap: f32,
    /// Flex direction.
    pub flex_direction: FlexDirection,
    /// Main-axis alignment.
    pub justify_content: JustifyContent,
    /// Cross-axis alignment.
    pub align_items: AlignItems,
    /// Share of remaining main-axis space.
    pub flex_grow: f32,
    /// Relative or absolute flow participation.
    pub position: Position,
    /// Absolute x offset from the parent's content origin.
    pub left: Option<f32>,
    /// Absolute y offset from the parent's content origin.
    pub top: Option<f32>,
    /// Uses the simplified equal-width grid layout.
    pub display_grid: bool,
    /// Equal-width grid column count.
    pub grid_columns: usize,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
            margin: Edges::default(),
            padding: Edges::default(),
            gap: 0.0,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            flex_grow: 0.0,
            position: Position::Relative,
            left: None,
            top: None,
            display_grid: false,
            grid_columns: 1,
        }
    }
}

impl Style {
    /// Sets an explicit width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets an explicit height.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Sets width constraints.
    pub fn width_range(mut self, min: f32, max: f32) -> Self {
        self.min_width = min;
        self.max_width = max;
        self
    }

    /// Sets height constraints.
    pub fn height_range(mut self, min: f32, max: f32) -> Self {
        self.min_height = min;
        self.max_height = max;
        self
    }
}

/// Layout tree node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Stable caller-defined node id.
    pub id: u64,
    /// Node style.
    pub style: Style,
    /// Child nodes.
    pub children: Vec<Node>,
    /// Intrinsic content size for leaf nodes.
    pub content_size: Size,
}

impl Node {
    /// Creates an empty node.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            style: Style::default(),
            children: Vec::new(),
            content_size: Size::default(),
        }
    }

    /// Sets the node style.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Sets intrinsic content size.
    pub fn with_content_size(mut self, size: Size) -> Self {
        self.content_size = size;
        self
    }

    /// Appends a child.
    pub fn push(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// Appends children.
    pub fn extend(mut self, children: impl IntoIterator<Item = Node>) -> Self {
        self.children.extend(children);
        self
    }
}

/// Computed absolute layout rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutResult {
    /// Layout node id.
    pub id: u64,
    /// Absolute x coordinate.
    pub x: f32,
    /// Absolute y coordinate.
    pub y: f32,
    /// Border-box width.
    pub width: f32,
    /// Border-box height.
    pub height: f32,
}

/// Deterministic CPU layout solver.
#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutEngine;

impl LayoutEngine {
    /// Computes absolute rectangles for a node tree.
    pub fn compute(root: &Node, available_size: Size) -> Vec<LayoutResult> {
        let width = clamp_axis(
            root.style.width.unwrap_or(available_size.width),
            root.style.min_width,
            root.style.max_width,
        );
        let height = clamp_axis(
            root.style.height.unwrap_or(available_size.height),
            root.style.min_height,
            root.style.max_height,
        );
        let mut results = Vec::new();
        layout_node(root, Point::new(0.0, 0.0), width, height, &mut results);
        results
    }
}

fn layout_node(
    node: &Node,
    origin: Point,
    width: f32,
    height: f32,
    results: &mut Vec<LayoutResult>,
) {
    results.push(LayoutResult {
        id: node.id,
        x: origin.x,
        y: origin.y,
        width,
        height,
    });

    let content_origin = Point::new(
        origin.x + node.style.padding.left,
        origin.y + node.style.padding.top,
    );
    let content_width = (width - node.style.padding.horizontal()).max(0.0);
    let content_height = (height - node.style.padding.vertical()).max(0.0);

    let relative = node
        .children
        .iter()
        .filter(|child| child.style.position == Position::Relative)
        .collect::<Vec<_>>();

    if node.style.display_grid {
        layout_grid(
            node,
            &relative,
            content_origin,
            content_width,
            content_height,
            results,
        );
    } else {
        layout_flex(
            node,
            &relative,
            content_origin,
            content_width,
            content_height,
            results,
        );
    }

    layout_absolute(node, content_origin, results);
}

fn layout_absolute(node: &Node, content_origin: Point, results: &mut Vec<LayoutResult>) {
    for child in node
        .children
        .iter()
        .filter(|child| child.style.position == Position::Absolute)
    {
        let size = base_size(child);
        let x = content_origin.x + child.style.left.unwrap_or(0.0);
        let y = content_origin.y + child.style.top.unwrap_or(0.0);
        layout_node(child, Point::new(x, y), size.width, size.height, results);
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_flex(
    node: &Node,
    children: &[&Node],
    content_origin: Point,
    content_width: f32,
    content_height: f32,
    results: &mut Vec<LayoutResult>,
) {
    if children.is_empty() {
        return;
    }

    let direction = node.style.flex_direction;
    let content_main = match direction {
        FlexDirection::Row => content_width,
        FlexDirection::Column => content_height,
    };
    let content_cross = match direction {
        FlexDirection::Row => content_height,
        FlexDirection::Column => content_width,
    };
    let gap_total = node.style.gap * children.len().saturating_sub(1) as f32;

    let mut main_sizes = Vec::with_capacity(children.len());
    let mut cross_sizes = Vec::with_capacity(children.len());
    for child in children {
        let base = base_size(child);
        let (main, cross) = match direction {
            FlexDirection::Row => (base.width, base.height),
            FlexDirection::Column => (base.height, base.width),
        };
        main_sizes.push(main);
        cross_sizes.push(cross);
    }

    let margin_main = |child: &Node| match direction {
        FlexDirection::Row => child.style.margin.left + child.style.margin.right,
        FlexDirection::Column => child.style.margin.top + child.style.margin.bottom,
    };
    let margin_cross = |child: &Node| match direction {
        FlexDirection::Row => child.style.margin.top + child.style.margin.bottom,
        FlexDirection::Column => child.style.margin.left + child.style.margin.right,
    };

    let base_total = main_sizes
        .iter()
        .zip(children.iter())
        .map(|(size, child)| *size + margin_main(child))
        .sum::<f32>()
        + gap_total;
    let remaining = (content_main - base_total).max(0.0);
    let grow_total = children
        .iter()
        .map(|child| child.style.flex_grow.max(0.0))
        .sum::<f32>();

    if grow_total > 0.0 {
        for (index, child) in children.iter().enumerate() {
            if child.style.flex_grow <= 0.0 {
                continue;
            }
            let extra = remaining * child.style.flex_grow / grow_total;
            let axis_main = main_sizes[index] + extra;
            main_sizes[index] = clamp_axis_for_child(axis_main, child, direction, true);
        }
    }

    let used_main = main_sizes
        .iter()
        .zip(children.iter())
        .map(|(size, child)| *size + margin_main(child))
        .sum::<f32>()
        + gap_total;
    let leftover = (content_main - used_main).max(0.0);
    let (lead, distributed_gap) = justify_offsets(
        node.style.justify_content,
        leftover,
        node.style.gap,
        children.len(),
    );
    let mut cursor = lead;

    for (index, child) in children.iter().enumerate() {
        let main_size = main_sizes[index];
        let cross_margin = margin_cross(child);
        let cross_auto = match direction {
            FlexDirection::Row => child.style.height.is_none(),
            FlexDirection::Column => child.style.width.is_none(),
        };
        let cross_size = if node.style.align_items == AlignItems::Stretch && cross_auto {
            clamp_axis_for_child(
                (content_cross - cross_margin).max(0.0),
                child,
                direction,
                false,
            )
        } else {
            cross_sizes[index]
        };
        let outer_cross = cross_size + cross_margin;
        let cross_offset = match node.style.align_items {
            AlignItems::Stretch | AlignItems::FlexStart => 0.0,
            AlignItems::Center => ((content_cross - outer_cross) / 2.0).max(0.0),
            AlignItems::FlexEnd => (content_cross - outer_cross).max(0.0),
        };
        let (margin_main_start, margin_cross_start) = match direction {
            FlexDirection::Row => (child.style.margin.left, child.style.margin.top),
            FlexDirection::Column => (child.style.margin.top, child.style.margin.left),
        };
        let (child_x, child_y, child_width, child_height) = match direction {
            FlexDirection::Row => (
                content_origin.x + cursor + margin_main_start,
                content_origin.y + cross_offset + margin_cross_start,
                main_size,
                cross_size,
            ),
            FlexDirection::Column => (
                content_origin.x + cross_offset + margin_cross_start,
                content_origin.y + cursor + margin_main_start,
                cross_size,
                main_size,
            ),
        };

        layout_node(
            child,
            Point::new(child_x, child_y),
            child_width,
            child_height,
            results,
        );
        cursor += main_size + margin_main(child) + distributed_gap;
    }
}

fn layout_grid(
    node: &Node,
    children: &[&Node],
    content_origin: Point,
    content_width: f32,
    content_height: f32,
    results: &mut Vec<LayoutResult>,
) {
    if children.is_empty() {
        return;
    }

    let columns = node.style.grid_columns.max(1);
    let total_column_gap = node.style.gap * columns.saturating_sub(1) as f32;
    let column_width = ((content_width - total_column_gap) / columns as f32).max(0.0);
    let row_height = children
        .iter()
        .map(|child| {
            let size = base_size(child);
            size.height + child.style.margin.top + child.style.margin.bottom
        })
        .fold(0.0_f32, f32::max);
    let mut y_cursor = 0.0;

    for row in children.chunks(columns) {
        for (column, child) in row.iter().enumerate() {
            let margins = child.style.margin;
            let width = clamp_axis_for_child(
                (column_width - margins.left - margins.right).max(0.0),
                child,
                FlexDirection::Row,
                true,
            );
            let height = base_size(child).height;
            let x =
                content_origin.x + column as f32 * (column_width + node.style.gap) + margins.left;
            let y = content_origin.y + y_cursor + margins.top;
            layout_node(child, Point::new(x, y), width, height, results);
        }

        y_cursor += row_height + node.style.gap;
    }

    let _ = content_height;
}

fn base_size(node: &Node) -> Size {
    let intrinsic = intrinsic_size(node);
    Size::new(
        clamp_axis(
            node.style.width.unwrap_or(intrinsic.width),
            node.style.min_width,
            node.style.max_width,
        ),
        clamp_axis(
            node.style.height.unwrap_or(intrinsic.height),
            node.style.min_height,
            node.style.max_height,
        ),
    )
}

fn intrinsic_size(node: &Node) -> Size {
    let relative = node
        .children
        .iter()
        .filter(|child| child.style.position == Position::Relative)
        .collect::<Vec<_>>();
    let content = if node.style.display_grid {
        intrinsic_grid(node, &relative)
    } else {
        intrinsic_flex(node, &relative)
    };
    Size::new(
        clamp_axis(
            node.style
                .width
                .unwrap_or(content.width + node.style.padding.horizontal()),
            node.style.min_width,
            node.style.max_width,
        ),
        clamp_axis(
            node.style
                .height
                .unwrap_or(content.height + node.style.padding.vertical()),
            node.style.min_height,
            node.style.max_height,
        ),
    )
}

fn intrinsic_flex(node: &Node, children: &[&Node]) -> Size {
    if children.is_empty() {
        return node.content_size;
    }
    let mut main = node.style.gap * children.len().saturating_sub(1) as f32;
    let mut cross = 0.0_f32;
    for child in children {
        let size = base_size(child);
        let main_size = match node.style.flex_direction {
            FlexDirection::Row => size.width + child.style.margin.left + child.style.margin.right,
            FlexDirection::Column => {
                size.height + child.style.margin.top + child.style.margin.bottom
            }
        };
        let cross_size = match node.style.flex_direction {
            FlexDirection::Row => size.height + child.style.margin.top + child.style.margin.bottom,
            FlexDirection::Column => {
                size.width + child.style.margin.left + child.style.margin.right
            }
        };
        main += main_size;
        cross = cross.max(cross_size);
    }
    match node.style.flex_direction {
        FlexDirection::Row => Size::new(main, cross),
        FlexDirection::Column => Size::new(cross, main),
    }
}

fn intrinsic_grid(node: &Node, children: &[&Node]) -> Size {
    if children.is_empty() {
        return node.content_size;
    }
    let columns = node.style.grid_columns.max(1);
    let column_width = children
        .iter()
        .map(|child| {
            let size = base_size(child);
            size.width + child.style.margin.left + child.style.margin.right
        })
        .fold(0.0_f32, f32::max);
    let row_height = children
        .iter()
        .map(|child| {
            let size = base_size(child);
            size.height + child.style.margin.top + child.style.margin.bottom
        })
        .fold(0.0_f32, f32::max);
    let rows = children.len().div_ceil(columns);
    Size::new(
        column_width * columns as f32 + node.style.gap * columns.saturating_sub(1) as f32,
        row_height * rows as f32 + node.style.gap * rows.saturating_sub(1) as f32,
    )
}

fn clamp_axis_for_child(value: f32, child: &Node, direction: FlexDirection, main: bool) -> f32 {
    let (min, max) = match (direction, main) {
        (FlexDirection::Row, true) => (child.style.min_width, child.style.max_width),
        (FlexDirection::Row, false) => (child.style.min_height, child.style.max_height),
        (FlexDirection::Column, true) => (child.style.min_height, child.style.max_height),
        (FlexDirection::Column, false) => (child.style.min_width, child.style.max_width),
    };
    clamp_axis(value, min, max)
}

fn clamp_axis(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max).max(0.0)
}

fn justify_offsets(justify: JustifyContent, leftover: f32, gap: f32, count: usize) -> (f32, f32) {
    match justify {
        JustifyContent::FlexStart => (0.0, gap),
        JustifyContent::FlexEnd => (leftover, gap),
        JustifyContent::Center => (leftover / 2.0, gap),
        JustifyContent::SpaceBetween if count > 1 => (0.0, gap + leftover / (count - 1) as f32),
        JustifyContent::SpaceBetween => (leftover / 2.0, gap),
        JustifyContent::SpaceAround if count > 0 => (
            leftover / (count as f32 * 2.0),
            gap + leftover / count as f32,
        ),
        JustifyContent::SpaceAround => (0.0, gap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(results: &[LayoutResult], id: u64) -> LayoutResult {
        *results.iter().find(|result| result.id == id).expect("node")
    }

    fn leaf(id: u64, width: f32, height: f32) -> Node {
        Node::new(id).with_content_size(Size::new(width, height))
    }

    #[test]
    fn nested_row_and_column_layout() {
        let root = Node::new(1)
            .with_style(Style {
                width: Some(300.0),
                height: Some(200.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                ..Style::default()
            })
            .push(
                Node::new(2)
                    .with_style(
                        Style::default()
                            .height(40.0)
                            .width_range(0.0, f32::INFINITY),
                    )
                    .push(leaf(3, 60.0, 40.0))
                    .push(leaf(4, 60.0, 40.0)),
            )
            .push(leaf(5, 50.0, 20.0));
        let result = LayoutEngine::compute(&root, Size::new(300.0, 200.0));
        assert_eq!(find(&result, 1).width, 300.0);
        assert_eq!(find(&result, 2).x, 0.0);
        assert_eq!(find(&result, 2).y, 0.0);
        assert_eq!(find(&result, 2).width, 120.0);
        assert_eq!(find(&result, 3).x, 0.0);
        assert_eq!(find(&result, 4).x, 60.0);
    }

    #[test]
    fn flex_grow_distributes_remaining_space() {
        let root = Node::new(1)
            .with_style(Style {
                width: Some(300.0),
                height: Some(100.0),
                padding: Edges::all(10.0),
                gap: 10.0,
                flex_direction: FlexDirection::Row,
                ..Style::default()
            })
            .push(leaf(2, 50.0, 20.0))
            .push(
                Node::new(3)
                    .with_style(Style {
                        flex_grow: 1.0,
                        ..Style::default()
                    })
                    .with_content_size(Size::new(20.0, 20.0)),
            );
        let result = LayoutEngine::compute(&root, Size::new(300.0, 100.0));
        assert_eq!(find(&result, 2).x, 10.0);
        assert_eq!(find(&result, 2).width, 50.0);
        assert_eq!(find(&result, 3).x, 70.0);
        assert_eq!(find(&result, 3).width, 220.0);
    }

    #[test]
    fn min_and_max_constraints_are_applied() {
        let root = Node::new(1)
            .with_style(Style::default().width(300.0).height(100.0))
            .push(Node::new(2).with_style(Style {
                width: Some(500.0),
                max_width: 120.0,
                height: Some(100.0),
                max_height: 50.0,
                ..Style::default()
            }))
            .push(Node::new(3).with_style(Style {
                width: Some(40.0),
                min_width: 80.0,
                ..Style::default()
            }));
        let result = LayoutEngine::compute(&root, Size::new(300.0, 100.0));
        assert_eq!(find(&result, 2).width, 120.0);
        assert_eq!(find(&result, 2).height, 50.0);
        assert_eq!(find(&result, 3).width, 80.0);
    }

    #[test]
    fn padding_margin_and_gap_affect_offsets() {
        let root = Node::new(1)
            .with_style(Style {
                width: Some(300.0),
                height: Some(200.0),
                padding: Edges::all(10.0),
                gap: 8.0,
                flex_direction: FlexDirection::Column,
                ..Style::default()
            })
            .push(leaf(2, 50.0, 20.0).with_style(Style {
                margin: Edges::all(5.0),
                ..Style::default()
            }))
            .push(leaf(3, 50.0, 20.0).with_style(Style {
                margin: Edges::all(5.0),
                ..Style::default()
            }));
        let result = LayoutEngine::compute(&root, Size::new(300.0, 200.0));
        assert_eq!(find(&result, 2).x, 15.0);
        assert_eq!(find(&result, 2).y, 15.0);
        assert_eq!(find(&result, 3).y, 53.0);
    }

    #[test]
    fn absolute_positioning_is_out_of_flow() {
        let root = Node::new(1).with_style(Style {
            width: Some(200.0),
            height: Some(150.0),
            padding: Edges::all(10.0),
            ..Style::default()
        });
        let root = root
            .push(leaf(2, 100.0, 50.0))
            .push(Node::new(3).with_style(Style {
                width: Some(50.0),
                height: Some(30.0),
                position: Position::Absolute,
                left: Some(20.0),
                top: Some(30.0),
                ..Style::default()
            }));
        let result = LayoutEngine::compute(&root, Size::new(200.0, 150.0));
        assert_eq!(find(&result, 2).x, 10.0);
        assert_eq!(find(&result, 2).y, 10.0);
        assert_eq!(find(&result, 3).x, 30.0);
        assert_eq!(find(&result, 3).y, 40.0);
    }

    #[test]
    fn grid_columns_are_equal_width() {
        let root = Node::new(1).with_style(Style {
            width: Some(320.0),
            height: Some(100.0),
            padding: Edges::all(10.0),
            gap: 10.0,
            display_grid: true,
            grid_columns: 3,
            ..Style::default()
        });
        let root = root
            .push(leaf(2, 30.0, 20.0))
            .push(leaf(3, 30.0, 20.0))
            .push(leaf(4, 30.0, 20.0));
        let result = LayoutEngine::compute(&root, Size::new(320.0, 100.0));
        let width = (300.0 - 20.0) / 3.0;
        assert!((find(&result, 2).width - width).abs() < 0.001);
        assert!((find(&result, 3).x - (10.0 + width + 10.0)).abs() < 0.001);
        assert!((find(&result, 4).x - (10.0 + 2.0 * (width + 10.0))).abs() < 0.001);
    }

    #[test]
    fn justify_content_center() {
        let root = Node::new(1).with_style(Style {
            width: Some(300.0),
            height: Some(100.0),
            padding: Edges::all(10.0),
            justify_content: JustifyContent::Center,
            ..Style::default()
        });
        let root = root.push(leaf(2, 100.0, 40.0));
        let result = LayoutEngine::compute(&root, Size::new(300.0, 100.0));
        assert!((find(&result, 2).x - 100.0).abs() < 0.001);
    }

    #[test]
    fn align_items_center() {
        let root = Node::new(1).with_style(Style {
            width: Some(300.0),
            height: Some(100.0),
            padding: Edges::all(10.0),
            align_items: AlignItems::Center,
            ..Style::default()
        });
        let root = root.push(leaf(2, 50.0, 20.0));
        let result = LayoutEngine::compute(&root, Size::new(300.0, 100.0));
        assert!((find(&result, 2).y - 40.0).abs() < 0.001);
    }

    #[test]
    fn column_with_explicit_sizes_and_gap() {
        let root = Node::new(1).with_style(Style {
            width: Some(200.0),
            height: Some(200.0),
            padding: Edges::all(10.0),
            gap: 8.0,
            flex_direction: FlexDirection::Column,
            ..Style::default()
        });
        let root = root.push(leaf(2, 50.0, 50.0)).push(leaf(3, 50.0, 50.0));
        let result = LayoutEngine::compute(&root, Size::new(200.0, 200.0));
        assert_eq!(find(&result, 2).x, 10.0);
        assert_eq!(find(&result, 2).y, 10.0);
        assert_eq!(find(&result, 3).y, 68.0);
    }

    #[test]
    fn deterministic_results() {
        let root = Node::new(1)
            .with_style(Style::default())
            .push(leaf(2, 10.0, 10.0));
        let first = LayoutEngine::compute(&root, Size::new(100.0, 100.0));
        let second = LayoutEngine::compute(&root, Size::new(100.0, 100.0));
        assert_eq!(first, second);
    }
}
