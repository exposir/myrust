//! Built-in SVG path icons and CPU-side icon scaling.
//!
//! Icon data is exposed as serializable path commands. This module does not
//! parse SVG path syntax or rasterize pixels; those operations belong to the
//! later renderer.

use serde::{Deserialize, Serialize};

use super::layout::Point;
use super::paint::{Color, DrawCommand, Rect, SvgPath, Transform};

/// A named icon defined inside a rectangular SVG view box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    /// Stable icon name.
    pub name: String,
    /// SVG coordinate system containing the paths.
    pub view_box: Rect,
    /// Styled SVG path definitions.
    pub paths: Vec<SvgPath>,
}

impl Icon {
    /// Creates an icon from its name, view box, and paths.
    pub fn new(name: impl Into<String>, view_box: Rect, paths: Vec<SvgPath>) -> Self {
        Self {
            name: name.into(),
            view_box,
            paths,
        }
    }

    /// Returns all built-in icons.
    pub fn all() -> Vec<Self> {
        vec![
            Self::arrow_left(),
            Self::arrow_right(),
            Self::arrow_up(),
            Self::arrow_down(),
            Self::close(),
            Self::check(),
            Self::search(),
            Self::menu(),
            Self::settings(),
            Self::home(),
        ]
    }

    /// Returns a built-in icon by name.
    pub fn builtin(name: &str) -> Option<Self> {
        Self::all().into_iter().find(|icon| icon.name == name)
    }

    /// Produces a transform-wrapped SVG path command sequence.
    ///
    /// Path coordinates remain in the icon's view box. The returned
    /// transform scales and offsets them into a `size` by `size` target box.
    /// The requested color replaces the built-in path stroke color; this
    /// method does not perform rasterization.
    pub fn to_commands(&self, size: f32, color: Color) -> Vec<DrawCommand> {
        let target_size = if size.is_finite() { size.max(0.0) } else { 0.0 };
        let view_width = self.view_box.width.max(f32::EPSILON);
        let view_height = self.view_box.height.max(f32::EPSILON);
        let scale = Point::new(target_size / view_width, target_size / view_height);
        let translation = Point::new(-self.view_box.x * scale.x, -self.view_box.y * scale.y);
        let mut commands = vec![DrawCommand::TransformPush {
            transform: Transform {
                translation,
                scale,
                rotation: 0.0,
            },
        }];

        for path in &self.paths {
            commands.push(DrawCommand::SvgPath {
                x: self.view_box.x,
                y: self.view_box.y,
                width: self.view_box.width,
                height: self.view_box.height,
                path: path.path.clone(),
                fill: path.fill.map(|_| color),
                stroke: path.stroke.map(|_| color),
                stroke_width: path.stroke_width,
            });
        }
        commands.push(DrawCommand::TransformPop);
        commands
    }

    /// Returns a left arrow icon.
    pub fn arrow_left() -> Self {
        Self::stroked("arrow_left", "M19 12H5 M11 6 5 12 11 18", 2.0)
    }

    /// Returns a right arrow icon.
    pub fn arrow_right() -> Self {
        Self::stroked("arrow_right", "M5 12H19 M13 6 19 12 13 18", 2.0)
    }

    /// Returns an upward arrow icon.
    pub fn arrow_up() -> Self {
        Self::stroked("arrow_up", "M12 19V5 M6 11 12 5 18 11", 2.0)
    }

    /// Returns a downward arrow icon.
    pub fn arrow_down() -> Self {
        Self::stroked("arrow_down", "M12 5V19 M6 13 12 19 18 13", 2.0)
    }

    /// Returns a close icon.
    pub fn close() -> Self {
        Self::stroked("close", "M6 6 18 18 M18 6 6 18", 2.0)
    }

    /// Returns a checkmark icon.
    pub fn check() -> Self {
        Self::stroked("check", "M5 12 10 17 19 7", 2.0)
    }

    /// Returns a search icon with a separate lens and handle.
    pub fn search() -> Self {
        let mut icon = Self::stroked(
            "search",
            "M10.5 4A6.5 6.5 0 1 0 10.5 17A6.5 6.5 0 1 0 10.5 4",
            2.0,
        );
        icon.paths
            .push(SvgPath::stroked("M15.5 15.5 21 21", 2.0, Color::WHITE));
        icon
    }

    /// Returns a hamburger menu icon.
    pub fn menu() -> Self {
        Self::stroked("menu", "M4 7H20 M4 12H20 M4 17H20", 2.0)
    }

    /// Returns a simple settings/sliders icon.
    pub fn settings() -> Self {
        Self::stroked(
            "settings",
            "M4 6H20 M4 12H20 M4 18H20 M8 3V9 M16 9V15 M10 15V21",
            2.0,
        )
    }

    /// Returns a home icon.
    pub fn home() -> Self {
        Self::stroked("home", "M3 11 12 3 21 11 M5 10V21H19V10", 2.0)
    }

    fn stroked(name: &str, path: &str, width: f32) -> Self {
        Self::new(
            name,
            Rect::new(0.0, 0.0, 24.0, 24.0),
            vec![SvgPath::stroked(path, width, Color::WHITE)],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_set_contains_ten_icons_with_path_data() {
        let icons = Icon::all();
        assert_eq!(icons.len(), 10);
        assert!(icons.iter().all(|icon| !icon.paths.is_empty()));
        assert!(icons
            .iter()
            .flat_map(|icon| &icon.paths)
            .all(|path| !path.path.is_empty()));
    }

    #[test]
    fn icon_scales_view_box_to_target_size() {
        let icon = Icon::close();
        let commands = icon.to_commands(48.0, Color::rgb(10, 20, 30));
        assert_eq!(commands.len(), 3);
        let DrawCommand::TransformPush { transform } = &commands[0] else {
            panic!("expected transform push");
        };
        assert_eq!(transform.scale, Point::new(2.0, 2.0));
        assert_eq!(transform.translation, Point::new(0.0, 0.0));
        assert!(matches!(commands[2], DrawCommand::TransformPop));
    }

    #[test]
    fn search_icon_preserves_two_paths_and_applies_color() {
        let icon = Icon::search();
        assert_eq!(icon.paths.len(), 2);
        let commands = icon.to_commands(24.0, Color::rgb(1, 2, 3));
        assert_eq!(commands.len(), 4);
        let DrawCommand::SvgPath { stroke, .. } = &commands[1] else {
            panic!("expected svg path");
        };
        assert_eq!(*stroke, Some(Color::rgb(1, 2, 3)));
    }
}
