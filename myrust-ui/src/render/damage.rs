//! Damage diffing between successive display lists.
//!
//! A window redraws only the rectangles where two frames differ: the bounds
//! of the operations a change touches, in the frame it left and in the frame
//! it entered. Transform and scissor state are resolved before comparing, so
//! an operation that starts drawing under another clip counts as changed even
//! when its own fields did not.

use super::{intersect_rects, RenderSize, TransformMatrix};
use crate::font::default_font;
use crate::layout::Point;
use crate::paint::{DisplayList, DrawCommand, Rect, TextAlignment};

/// Rectangles of a frame that changed since the previous one.
///
/// `full` means the frame is better drawn whole: its size changed, it is the
/// first frame, or the changed rectangles cover more than half the surface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Damage {
    rects: Vec<Rect>,
    full: bool,
}

impl Damage {
    /// Creates damage that asks for a whole-frame redraw.
    pub fn full() -> Self {
        Self {
            rects: Vec::new(),
            full: true,
        }
    }

    /// Returns whether the whole frame must be redrawn.
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// Returns whether the frame did not change.
    pub fn is_empty(&self) -> bool {
        !self.full && self.rects.is_empty()
    }

    /// Changed rectangles in physical pixels, apart from each other.
    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// Builds partial damage from explicit rectangles, for tests.
    #[cfg(test)]
    pub(crate) fn partial(rects: Vec<Rect>) -> Self {
        Self { rects, full: false }
    }
}

/// How many rectangles a partial redraw keeps at most.
const MAX_RECTS: usize = 8;
/// Nearer than this, two changed rectangles are merged.
const MERGE_GAP: f32 = 8.0;

/// Finds where `next` differs from `prev` on a `size` surface.
pub fn diff(prev: &DisplayList, next: &DisplayList, size: RenderSize) -> Damage {
    if size.is_empty() {
        return Damage::full();
    }
    let frame = Rect::new(0.0, 0.0, size.width as f32, size.height as f32);
    let previous = collect_ops(prev);
    let current = collect_ops(next);
    let n = previous.len();
    let m = current.len();

    let mut prefix = 0;
    while prefix < n && prefix < m && previous[prefix].same(&current[prefix]) {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < n - prefix
        && suffix < m - prefix
        && previous[n - 1 - suffix].same(&current[m - 1 - suffix])
    {
        suffix += 1;
    }

    let mut changed = Vec::new();
    if n == m {
        for index in prefix..n - suffix {
            if !previous[index].same(&current[index]) {
                add_bounds(&mut changed, &previous[index], &current[index], frame);
            }
        }
    } else {
        for op in &previous[prefix..n - suffix] {
            add_bounds(&mut changed, op, op, frame);
        }
        for op in &current[prefix..m - suffix] {
            add_bounds(&mut changed, op, op, frame);
        }
    }
    if changed.is_empty() {
        return Damage::default();
    }
    let area: f32 = changed
        .iter()
        .map(|rect| rect.width * rect.height)
        .sum();
    if area * 2.0 >= frame.width * frame.height {
        return Damage::full();
    }
    Damage {
        rects: changed,
        full: false,
    }
}

/// One drawing operation with the state it drew under.
struct Op<'a> {
    command: &'a DrawCommand,
    transform: [f32; 6],
    clip: Option<Rect>,
    bounds: Rect,
}

impl Op<'_> {
    fn same(&self, other: &Self) -> bool {
        self.command == other.command
            && self.transform == other.transform
            && self.clip == other.clip
    }
}

fn collect_ops(display: &DisplayList) -> Vec<Op<'_>> {
    let mut ops = Vec::new();
    let mut transform = TransformMatrix::IDENTITY;
    let mut transforms = Vec::new();
    let mut clip: Option<Rect> = None;
    let mut clips = Vec::new();

    for command in &display.commands {
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
                clips.push(clip);
                let transformed = transform.transform_rect(*rect);
                clip = Some(match clip {
                    Some(current) => intersect_rects(current, transformed),
                    None => transformed,
                });
            }
            DrawCommand::ScissorPop => {
                if let Some(previous) = clips.pop() {
                    clip = previous;
                }
            }
            // Images carry no pixels in this batch and are not drawn.
            DrawCommand::Image { .. } => {}
            _ => {
                let Some(bounds) = device_bounds(command, transform) else {
                    continue;
                };
                let Some(visible) = intersect(bounds, clip) else {
                    continue;
                };
                ops.push(Op {
                    command,
                    transform: transform.to_array(),
                    clip,
                    bounds: visible,
                });
            }
        }
    }
    ops
}

/// Local bounds an operation may paint, with antialiasing and shadow reach.
pub(crate) fn op_bounds(command: &DrawCommand) -> Option<Rect> {
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
        } => Some(Rect::new(*x, *y, *width, *height)),
        DrawCommand::Circle { x, y, radius, .. } => Some(Rect::new(
            x - radius,
            y - radius,
            radius * 2.0,
            radius * 2.0,
        )),
        DrawCommand::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            ..
        } => {
            let half = width.max(0.0) / 2.0;
            Some(Rect::new(
                x1.min(*x2) - half,
                y1.min(*y2) - half,
                (x1 - x2).abs() + half * 2.0,
                (y1 - y2).abs() + half * 2.0,
            ))
        }
        DrawCommand::Shadow {
            x,
            y,
            width,
            height,
            offset_x,
            offset_y,
            blur,
            ..
        } => {
            // Both presenters blur out to 2.5× the blur radius plus a pixel
            // of antialiasing; a tighter bound clips the shadow's own mask.
            let blur = blur.max(0.0);
            let reach = blur * 2.5 + 2.0;
            Some(Rect::new(
                x + offset_x - reach,
                y + offset_y - reach,
                width + reach * 2.0,
                height + reach * 2.0,
            ))
        }
        DrawCommand::Text {
            x,
            y,
            text,
            font_size,
            italic,
            max_width,
            alignment,
            ..
        } => Some(text_bounds(
            *x,
            *y,
            text,
            *font_size,
            *italic,
            *max_width,
            *alignment,
        )),
        DrawCommand::SvgPath {
            x,
            y,
            width,
            height,
            stroke_width,
            ..
        } => {
            let reach = stroke_width.max(0.0) / 2.0 + 2.0;
            Some(Rect::new(
                x - reach,
                y - reach,
                width + reach * 2.0,
                height + reach * 2.0,
            ))
        }
        DrawCommand::Image { .. }
        | DrawCommand::ScissorPush { .. }
        | DrawCommand::ScissorPop
        | DrawCommand::TransformPush { .. }
        | DrawCommand::TransformPop => None,
    }
}

/// Device-space bounds of an operation under `transform`.
///
/// Lines are special: both presenters draw them as a device-space capsule
/// whose thickness uses the transform's average scale, not a transformed
/// local rectangle, so non-uniform scales would otherwise be clipped.
pub(crate) fn device_bounds(command: &DrawCommand, transform: TransformMatrix) -> Option<Rect> {
    match command {
        DrawCommand::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            ..
        } => {
            let a = transform.apply(Point::new(*x1, *y1));
            let b = transform.apply(Point::new(*x2, *y2));
            let half = width.max(0.0) / 2.0 * transform.scale_factor();
            Some(Rect::new(
                a.x.min(b.x) - half,
                a.y.min(b.y) - half,
                (a.x - b.x).abs() + half * 2.0,
                (a.y - b.y).abs() + half * 2.0,
            ))
        }
        _ => op_bounds(command).map(|bounds| transform.transform_rect(bounds)),
    }
}

fn text_bounds(
    x: f32,
    y: f32,
    text: &str,
    font_size: f32,
    italic: bool,
    max_width: Option<f32>,
    alignment: TextAlignment,
) -> Rect {
    let font_size = if font_size.is_finite() {
        font_size.max(1.0)
    } else {
        1.0
    };
    let maximum = max_width
        .filter(|width| width.is_finite() && *width > 0.0)
        .map(|width| width.max(0.0));
    let lines: Vec<&str> = text.split('\n').collect();
    let (widest, height, start_x) = match default_font() {
        Some(font) => {
            let widest = lines
                .iter()
                .map(|line| {
                    line.chars()
                        .map(|character| {
                            if font.has_glyph(character) {
                                font.glyph_advance(character, font_size)
                            } else {
                                // The presenters keep a half-em advance for
                                // characters the face has no glyph for.
                                font_size * 0.5
                            }
                        })
                        .sum::<f32>()
                })
                .fold(0.0_f32, f32::max);
            let height = lines.len() as f32 * font.line_height(font_size);
            let mut start_x = x;
            if let Some(maximum) = maximum {
                if widest <= maximum {
                    start_x += match alignment {
                        TextAlignment::Center => (maximum - widest) / 2.0,
                        TextAlignment::End => maximum - widest,
                        TextAlignment::Start => 0.0,
                    };
                }
            }
            (widest, height, start_x)
        }
        None => {
            // The CPU presenter draws scaled 8x8 bitmap glyphs, one em wide
            // per character, and ignores alignment and max_width.
            let widest = lines
                .iter()
                .map(|line| line.chars().count() as f32 * font_size)
                .fold(0.0_f32, f32::max);
            (widest, lines.len() as f32 * font_size * 1.25, x)
        }
    };
    // Real advances bound the glyph origins; antialiasing and the italic
    // shear can paint a little past them.
    let horizontal = 2.0 + if italic { font_size * 0.35 } else { 0.0 };
    let vertical = 2.0;
    Rect::new(
        start_x - horizontal,
        y - vertical,
        widest + horizontal * 2.0,
        height + vertical * 2.0,
    )
}

fn add_bounds(changed: &mut Vec<Rect>, previous: &Op<'_>, current: &Op<'_>, frame: Rect) {
    // Both frames' bounds are damaged: the pixels the operation leaves must
    // be repainted even when nothing draws there any more.
    for bounds in [previous.bounds, current.bounds] {
        if let Some(clipped) = intersect(bounds, Some(frame)) {
            add_rect(changed, clipped);
        }
    }
}

fn intersect(bounds: Rect, clip: Option<Rect>) -> Option<Rect> {
    let clipped = match clip {
        Some(clip) => bounds.intersection(clip)?,
        None => bounds,
    };
    (clipped.is_valid() && clipped.width > 0.0 && clipped.height > 0.0).then_some(clipped)
}

/// Adds `rect` to `rects`, merging any it overlaps or nearly meets, and
/// keeping at most [`MAX_RECTS`] by unioning the cheapest pair.
fn add_rect(rects: &mut Vec<Rect>, rect: Rect) {
    if !rect.is_valid() || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let mut rect = rect;
    loop {
        let mut merged = false;
        let mut index = 0;
        while index < rects.len() {
            if near(rects[index], rect) {
                rect = rects[index].union(rect);
                rects.swap_remove(index);
                merged = true;
            } else {
                index += 1;
            }
        }
        if !merged {
            break;
        }
    }
    rects.push(rect);
    while rects.len() > MAX_RECTS {
        merge_cheapest(rects);
    }
}

fn near(left: Rect, right: Rect) -> bool {
    left.x - MERGE_GAP < right.right()
        && right.x < left.right() + MERGE_GAP
        && left.y - MERGE_GAP < right.bottom()
        && right.y < left.bottom() + MERGE_GAP
}

fn merge_cheapest(rects: &mut Vec<Rect>) {
    let mut best = (0, 1, f32::INFINITY);
    for i in 0..rects.len() {
        for j in i + 1..rects.len() {
            let union = rects[i].union(rects[j]);
            let growth = union.width * union.height
                - rects[i].width * rects[i].height
                - rects[j].width * rects[j].height;
            if growth < best.2 {
                best = (i, j, growth);
            }
        }
    }
    let (i, j, _) = best;
    let union = rects[i].union(rects[j]);
    rects.swap_remove(j);
    rects.swap_remove(i);
    add_rect(rects, union);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Color;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> DrawCommand {
        DrawCommand::Rect {
            x,
            y,
            width,
            height,
            color: Color::WHITE,
        }
    }

    fn list(commands: Vec<DrawCommand>) -> DisplayList {
        DisplayList::from_commands(commands)
    }

    #[test]
    fn identical_lists_do_not_damage() {
        let display = list(vec![rect(1.0, 2.0, 10.0, 10.0)]);
        let damage = diff(&display, &display, RenderSize::new(100, 100));
        assert!(damage.is_empty());
        assert!(!damage.is_full());
    }

    #[test]
    fn a_moved_rectangle_damages_both_places() {
        let before = list(vec![rect(1.0, 2.0, 10.0, 10.0)]);
        let after = list(vec![rect(20.0, 2.0, 10.0, 10.0)]);
        let damage = diff(&before, &after, RenderSize::new(100, 100));
        assert!(!damage.is_full());
        let covered = |x: f32| {
            damage
                .rects()
                .iter()
                .any(|rect| rect.x <= x && rect.right() >= x + 10.0)
        };
        assert!(covered(1.0) && covered(20.0));
    }

    #[test]
    fn an_inserted_rectangle_damages_only_itself() {
        let before = list(vec![rect(0.0, 0.0, 10.0, 10.0), rect(80.0, 80.0, 10.0, 10.0)]);
        let after = list(vec![
            rect(0.0, 0.0, 10.0, 10.0),
            rect(40.0, 40.0, 10.0, 10.0),
            rect(80.0, 80.0, 10.0, 10.0),
        ]);
        let damage = diff(&before, &after, RenderSize::new(100, 100));
        assert!(!damage.is_full());
        assert_eq!(damage.rects().len(), 1);
        assert_eq!(damage.rects()[0], Rect::new(40.0, 40.0, 10.0, 10.0));
    }

    #[test]
    fn changed_text_damages_its_box() {
        let text = |value: &str| DrawCommand::Text {
            x: 10.0,
            y: 10.0,
            text: value.to_string(),
            font_size: 14.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::WHITE,
            max_width: None,
            alignment: TextAlignment::Start,
        };
        let damage = diff(
            &list(vec![text("hello")]),
            &list(vec![text("hellp")]),
            RenderSize::new(200, 100),
        );
        assert!(!damage.is_full());
        assert_eq!(damage.rects().len(), 1);
        let box_ = damage.rects()[0];
        assert!(box_.x <= 10.0 && box_.right() > 40.0);
    }

    #[test]
    fn changes_covering_half_the_frame_redraw_whole() {
        let before = list(vec![rect(0.0, 0.0, 200.0, 100.0)]);
        let after = list(vec![rect(5.0, 0.0, 200.0, 100.0)]);
        let damage = diff(&before, &after, RenderSize::new(200, 100));
        assert!(damage.is_full());
    }

    #[test]
    fn a_clip_change_damages_what_it_reveals_or_hides() {
        let before = list(vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
            },
            rect(0.0, 0.0, 10.0, 10.0),
            DrawCommand::ScissorPop,
        ]);
        let after = list(vec![
            DrawCommand::ScissorPush {
                rect: Rect::new(50.0, 50.0, 20.0, 20.0),
            },
            rect(0.0, 0.0, 10.0, 10.0),
            DrawCommand::ScissorPop,
        ]);
        let damage = diff(&before, &after, RenderSize::new(200, 200));
        assert!(!damage.is_full());
        assert_eq!(damage.rects().len(), 1);
        assert_eq!(damage.rects()[0], Rect::new(0.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn a_state_change_that_paints_nothing_does_not_damage() {
        let draw = |clip_x: f32| {
            list(vec![
                DrawCommand::ScissorPush {
                    rect: Rect::new(clip_x, 0.0, 20.0, 20.0),
                },
                rect(80.0, 80.0, 10.0, 10.0),
                DrawCommand::ScissorPop,
            ])
        };
        let damage = diff(&draw(0.0), &draw(40.0), RenderSize::new(200, 200));
        assert!(damage.is_empty());
    }

    #[test]
    fn zero_sized_surfaces_redraw_whole() {
        let display = list(vec![rect(0.0, 0.0, 1.0, 1.0)]);
        assert!(diff(&display, &display, RenderSize::new(0, 10)).is_full());
    }

    #[test]
    fn text_bounds_cover_wide_glyphs_and_ignore_a_tight_max_width() {
        let Some(font) = crate::font::default_font() else {
            return;
        };
        let text = "WWWWWWWWWW";
        let font_size = 14.0;
        let ink: f32 = text
            .chars()
            .map(|character| font.glyph_advance(character, font_size))
            .sum();
        // The presenters draw the whole line even when it exceeds max_width;
        // bounds must reach the same ink so a damage replay does not clip it.
        let bounds =
            text_bounds(10.0, 10.0, text, font_size, false, Some(40.0), TextAlignment::Start);
        assert!(
            bounds.right() >= 10.0 + ink,
            "bounds {bounds:?} must reach the drawn width {}",
            10.0 + ink
        );
    }

    #[test]
    fn shadow_bounds_cover_the_full_blur_reach() {
        let blur = 20.0;
        let bounds = op_bounds(&DrawCommand::Shadow {
            x: 30.0,
            y: 30.0,
            width: 20.0,
            height: 20.0,
            radius: 4.0,
            offset_x: 0.0,
            offset_y: 0.0,
            blur,
            color: Color::WHITE,
        })
        .unwrap();
        // Both presenters blur out to 2.5 × blur plus one pixel of AA.
        assert!(bounds.x <= 30.0 - 2.5 * blur);
        assert!(bounds.right() >= 30.0 + 20.0 + 2.5 * blur);
        assert!(bounds.bottom() >= 30.0 + 20.0 + 2.5 * blur);
    }

    #[test]
    fn line_bounds_use_the_device_space_thickness() {
        use crate::paint::Transform;

        let line = DrawCommand::Line {
            x1: 10.0,
            y1: 10.0,
            x2: 20.0,
            y2: 10.0,
            width: 4.0,
            color: Color::WHITE,
        };
        let transform = TransformMatrix::from_transform(Transform::scale(Point::new(4.0, 1.0)));
        let half = 2.0 * transform.scale_factor();
        let bounds = device_bounds(&line, transform).unwrap();
        assert!((bounds.y - (10.0 - half)).abs() < 0.001);
        assert!((bounds.bottom() - (10.0 + half)).abs() < 0.001);
    }
}
