//! Layout direction.

/// Horizontal layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Left-to-right, the default.
    #[default]
    Ltr,
    /// Right-to-left.
    Rtl,
}

impl Direction {
    /// Whether this direction is RTL.
    pub fn is_rtl(self) -> bool {
        self == Self::Rtl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::context::{row, text, Context};
    use crate::ui::icon::icon;
    use crate::ui::layout::Size;
    use crate::ui::paint::{Color, DrawCommand, TextAlignment};

    fn labels(frame: &crate::ui::native::ViewFrame) -> Vec<(String, f32)> {
        frame
            .access_nodes
            .iter()
            .filter(|node| matches!(node.label.as_str(), "A" | "B" | "C"))
            .map(|node| (node.label.clone(), node.bounds.x))
            .collect()
    }

    #[test]
    fn rtl_reverses_row_order_but_ltr_stays_default() {
        let size = Size::new(300.0, 100.0);
        let mut ltr = Context::new(size);
        ltr.begin_frame(size);
        row(&mut ltr).gap(4.0).children(|context| {
            text(context, "A").max_width(30.0);
            text(context, "B").max_width(30.0);
            text(context, "C").max_width(30.0);
        });
        let ltr_frame = ltr.produce().unwrap();
        let ltr_labels = labels(&ltr_frame);
        assert!(
            ltr_labels.iter().find(|(label, _)| label == "A").unwrap().1
                < ltr_labels.iter().find(|(label, _)| label == "B").unwrap().1
        );
        assert!(
            ltr_labels.iter().find(|(label, _)| label == "B").unwrap().1
                < ltr_labels.iter().find(|(label, _)| label == "C").unwrap().1
        );

        let mut rtl = Context::new(size);
        rtl.begin_frame(size);
        rtl.set_direction(Direction::Rtl);
        row(&mut rtl).gap(4.0).children(|context| {
            text(context, "A").max_width(30.0);
            text(context, "B").max_width(30.0);
            text(context, "C").max_width(30.0);
        });
        let rtl_frame = rtl.produce().unwrap();
        let rtl_labels = labels(&rtl_frame);
        assert!(
            rtl_labels.iter().find(|(label, _)| label == "C").unwrap().1
                < rtl_labels.iter().find(|(label, _)| label == "B").unwrap().1
        );
        assert!(
            rtl_labels.iter().find(|(label, _)| label == "B").unwrap().1
                < rtl_labels.iter().find(|(label, _)| label == "A").unwrap().1
        );
        assert!(rtl_frame.display.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Text { text, alignment, .. }
                    if text == "A" && *alignment == TextAlignment::End
            )
        }));
    }

    #[test]
    fn rtl_mirrors_icons() {
        let size = Size::new(120.0, 80.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        context.set_direction(Direction::Rtl);
        icon(&mut context, "icon", "arrow_right", 24.0, Color::WHITE);
        let frame = context.produce().unwrap();
        assert!(frame.display.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::TransformPush { transform } if transform.scale.x < 0.0
            )
        }));
    }
}
