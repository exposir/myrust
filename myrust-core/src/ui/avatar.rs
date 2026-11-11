//! Initials or RGBA image avatar.

use super::context::Context;
use super::image::ImageData;

/// Avatar silhouette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AvatarShape {
    /// Circular avatar.
    #[default]
    Circle,
    /// Rounded-square avatar.
    Rounded,
}

/// Result of constructing an avatar.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AvatarState {
    /// Logical size.
    pub size: f32,
    /// Silhouette.
    pub shape: AvatarShape,
    /// Whether pixels were supplied.
    pub has_image: bool,
    /// Whether image pixels were drawn.
    pub drawn: bool,
}

/// Draws an initials avatar.
///
/// `label` is the accessible name; `initials` is the visible short text.
pub fn avatar(
    context: &mut Context,
    id: &str,
    label: &str,
    initials: &str,
    size: f32,
    shape: AvatarShape,
) -> AvatarState {
    context.avatar(id, label, Some(initials), None, size, shape)
}

/// Draws an RGBA image avatar.
///
/// The caller supplies decoded pixels; no image decoding happens here. Circle
/// images are masked to the circular area.
pub fn avatar_image(
    context: &mut Context,
    id: &str,
    label: &str,
    data: ImageData<'_>,
    size: f32,
    shape: AvatarShape,
) -> AvatarState {
    context.avatar(id, label, None, Some(data), size, shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::access::Role;
    use crate::ui::layout::Size;
    use crate::ui::paint::DrawCommand;

    #[test]
    fn avatar_initials_use_label_and_configurable_size() {
        let size = Size::new(200.0, 180.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let small = avatar(
            &mut context,
            "avatar-small",
            "User",
            "AB",
            48.0,
            AvatarShape::Circle,
        );
        let large = avatar(
            &mut context,
            "avatar-large",
            "User",
            "AB",
            96.0,
            AvatarShape::Rounded,
        );
        let zero = avatar(
            &mut context,
            "avatar-zero",
            "User",
            "AB",
            0.0,
            AvatarShape::Circle,
        );
        let frame = context.produce().unwrap();
        assert_eq!(small.size, 48.0);
        assert_eq!(large.size, 96.0);
        assert_eq!(zero.size, 0.0);
        let small_node = frame
            .access_nodes
            .iter()
            .find(|node| node.label == "User" && node.bounds.width == 48.0)
            .expect("small avatar label");
        assert_eq!(small_node.role, Role::Label);
        let large_node = frame
            .access_nodes
            .iter()
            .find(|node| node.label == "User" && node.bounds.width == 96.0)
            .expect("large avatar label");
        assert_eq!(large_node.role, Role::Label);
        assert!(frame.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Circle { radius, .. } if *radius == 24.0)
        ));
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Text { text, .. } if text == "AB")));
    }

    #[test]
    fn avatar_image_scales_pixels_within_the_target() {
        let pixels = [
            255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let data = ImageData::new(&pixels, 2, 2);
        let size = Size::new(120.0, 120.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let state = avatar_image(
            &mut context,
            "avatar-image",
            "Photo",
            data,
            40.0,
            AvatarShape::Circle,
        );
        let frame = context.produce().unwrap();
        assert!(state.has_image);
        assert!(state.drawn);
        assert!(frame
            .display
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Rect { color, .. } if *color == crate::ui::paint::Color::rgb(255, 0, 0))));
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Label
                && node.label == "Photo"
                && node.value.as_deref() == Some("image")
        }));
    }
}
