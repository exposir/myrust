//! Built-in SVG path icons and CPU-side icon scaling.
//!
//! Icon data is exposed as serializable path commands. This module does not
//! parse SVG path syntax or rasterize pixels; those operations belong to the
//! later renderer.

use serde::{Deserialize, Serialize};

use super::context::Context;
use super::direction::Direction;
use super::layout::Point;
use super::paint::{Color, DrawCommand, Rect, SvgPath, Transform};

/// Result of placing an icon in the immediate-mode view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IconState {
    /// Requested icon name.
    pub name: String,
    /// Logical size.
    pub size: f32,
    /// Whether the name resolved to a built-in icon.
    pub found: bool,
    /// Number of generated display-list commands.
    pub commands: usize,
}

/// Draws a built-in icon as a fixed-size widget.
pub fn icon(context: &mut Context, id: &str, name: &str, size: f32, color: Color) -> IconState {
    context.icon(id, name, size, color)
}

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
            Self::chevron_down(),
            Self::chevron_left(),
            Self::chevron_right(),
            Self::dots(),
            Self::bold(),
            Self::italic(),
            Self::underline(),
            Self::list(),
            Self::grid(),
            Self::lock(),
            Self::pin(),
            Self::close(),
            Self::check(),
            Self::search(),
            Self::magnifier(),
            Self::menu(),
            Self::settings(),
            Self::home(),
            Self::ruler(),
            Self::folder(),
            Self::file(),
            Self::save(),
            Self::trash(),
            Self::image(),
            Self::code(),
            Self::audio(),
            Self::inbox(),
            Self::send(),
            Self::draft(),
            Self::tag(),
            Self::star(),
            Self::bell(),
            Self::heart(),
            Self::loader(),
            Self::terminal(),
            Self::share(),
            Self::bookmark(),
            Self::gear(),
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
        self.to_commands_with_direction(size, color, Direction::Ltr)
    }

    /// Produces transform-wrapped SVG commands for a layout direction.
    ///
    /// RTL mirrors the icon horizontally within its target box.
    pub fn to_commands_with_direction(
        &self,
        size: f32,
        color: Color,
        direction: Direction,
    ) -> Vec<DrawCommand> {
        let target_size = if size.is_finite() { size.max(0.0) } else { 0.0 };
        let view_width = self.view_box.width.max(f32::EPSILON);
        let view_height = self.view_box.height.max(f32::EPSILON);
        let mut scale = Point::new(target_size / view_width, target_size / view_height);
        let mut translation = Point::new(-self.view_box.x * scale.x, -self.view_box.y * scale.y);
        if direction.is_rtl() {
            scale.x = -scale.x;
            translation.x = target_size - self.view_box.x * scale.x;
        }
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

    /// Returns an inbox tray icon, as MyGo's navigation demo draws.
    pub fn inbox() -> Self {
        Self::stroked(
            "inbox",
            "M22 12h-6l-2 3h-4l-2-3H2 M5.5 5.1 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1z",
            2.0,
        )
    }

    /// Returns a paper-plane icon.
    pub fn send() -> Self {
        Self::stroked("send", "m22 2-7 20-4-9-9-4z M22 2 11 13", 2.0)
    }

    /// Returns a pen icon.
    pub fn draft() -> Self {
        Self::stroked(
            "draft",
            "M12 20h9 M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z",
            2.0,
        )
    }

    /// Returns a tag icon.
    pub fn tag() -> Self {
        Self::stroked(
            "tag",
            "M12.6 2.6A2 2 0 0 0 11.2 2H4a2 2 0 0 0-2 2v7.2a2 2 0 0 0 .6 1.4l8.7 8.7a2.4 2.4 0 0 0 3.4 0l6.6-6.6a2.4 2.4 0 0 0 0-3.4z M7.5 7.5h.01",
            2.0,
        )
    }

    /// Returns a five-point star icon.
    pub fn star() -> Self {
        Self::stroked(
            "star",
            "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1-5.4-2.9-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z",
            2.0,
        )
    }

    /// Returns a bell icon.
    pub fn bell() -> Self {
        Self::stroked(
            "bell",
            "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9 M10.3 21a1.9 1.9 0 0 0 3.4 0",
            2.0,
        )
    }

    /// Returns a heart icon.
    pub fn heart() -> Self {
        Self::stroked(
            "heart",
            "M19 14c1.5-1.5 3-3.2 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.8 0-3 .5-4.5 2-1.5-1.5-2.7-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4 3 5.5l7 7z",
            2.0,
        )
    }

    /// Returns a terminal prompt icon, as MyGo's containers demo draws.
    pub fn terminal() -> Self {
        Self::stroked("terminal", "m4 17 6-6-6-6 M12 19h8", 2.0)
    }

    /// Returns a share icon, as MyGo's dialogs demo draws beside "Share".
    pub fn share() -> Self {
        Self::stroked(
            "share",
            "M15 5a3 3 0 1 0 6 0a3 3 0 1 0-6 0 M3 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0 M15 19a3 3 0 1 0 6 0a3 3 0 1 0-6 0 M8.6 13.5l6.8 4 M15.4 6.5l-6.8 4",
            2.0,
        )
    }

    /// Returns a bookmark icon, as MyGo's dialogs demo draws.
    pub fn bookmark() -> Self {
        Self::stroked("bookmark", "m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z", 2.0)
    }

    /// Returns a cog icon, as MyGo's dialogs demo draws beside "Settings".
    ///
    /// Distinct from [`Icon::settings`], whose sliders MyGo's own settings
    /// demo uses; this one is the gear the dialogs demo names `iconGear`.
    pub fn gear() -> Self {
        Self::stroked(
            "gear",
            "M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6 M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-2.9-1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.7 1.7 0 0 0 3 15.4H3a2 2 0 1 1 0-4h.1A1.7 1.7 0 0 0 4.6 9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9.3A1.7 1.7 0 0 0 10.4 5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
            2.0,
        )
    }

    /// Returns a loader arc icon.
    pub fn loader() -> Self {
        Self::stroked("loader", "M21 12a9 9 0 1 1-6.2-8.6", 2.0)
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

    /// Returns a bold-style "B" mark for a toggle group.
    pub fn bold() -> Self {
        Self::stroked("bold", "M7 5h6a3.5 3.5 0 0 1 0 7H7z M7 12h7a3.5 3.5 0 0 1 0 7H7z", 2.0)
    }

    /// Returns an italic-style slanted mark.
    pub fn italic() -> Self {
        Self::stroked("italic", "M19 4h-9 M14 20H5 M15 4 9 20", 2.0)
    }

    /// Returns an underline mark.
    pub fn underline() -> Self {
        Self::stroked("underline", "M6 4v6a6 6 0 0 0 12 0V4 M4 20h16", 2.0)
    }

    /// Returns a list icon with three bulleted lines.
    pub fn list() -> Self {
        Self::stroked(
            "list",
            "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01",
            2.0,
        )
    }

    /// Returns a two-by-two grid icon.
    pub fn grid() -> Self {
        Self::stroked(
            "grid",
            "M4 3h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z \
             M15 3h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z \
             M4 14h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z \
             M15 14h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-5a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z",
            2.0,
        )
    }

    /// Returns a padlock icon.
    pub fn lock() -> Self {
        Self::stroked(
            "lock",
            "M6 11h12a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2z M8 11V7a4 4 0 0 1 8 0v4",
            2.0,
        )
    }

    /// Returns a pushpin icon.
    pub fn pin() -> Self {
        Self::stroked("pin", "M12 17v5 M9 10.8V4h6v6.8l2 3.2H7z", 2.0)
    }

    /// Returns a chevron pointing left, as a history button's arrow.
    ///
    /// MyGo's history arrow is no SVG: it strokes three points in a
    /// `Space(4)` box, `s` the box's fifth, `1.6` wide with round ends and
    /// joins (`ui/router.go` historyButton). In the 24-unit view box that
    /// is these points and a 2.4 stroke.
    pub fn chevron_left() -> Self {
        Self::stroked("chevron_left", "!round!M14.64 3.552 6.72 12 14.64 20.448", 2.4)
    }

    /// Returns a chevron pointing right, as a forward button's arrow.
    pub fn chevron_right() -> Self {
        Self::stroked("chevron_right", "!round!M9.36 3.552 17.28 12 9.36 20.448", 2.4)
    }

    /// Returns a chevron pointing down, as a menu button's arrow.
    ///
    /// MyGo draws this one at 10x10 with a 1.5px stroke (`ui/widgets.go`
    /// chevron); in the 24-unit view box that is a 3.6 stroke.
    pub fn chevron_down() -> Self {
        Self::stroked("chevron_down", "M2.4 7.2 12 16.8 21.6 7.2", 3.6)
    }

    /// Returns three dots, as a more button's icon.
    pub fn dots() -> Self {
        Self::stroked(
            "dots",
            "M5 11a1 1 0 1 0 0 2a1 1 0 1 0 0-2 M12 11a1 1 0 1 0 0 2a1 1 0 1 0 0-2 M19 11a1 1 0 1 0 0 2a1 1 0 1 0 0-2",
            2.0,
        )
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
        Self::stroked("search", "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14 M20 20 16 16", 2.0)
    }

    /// Returns the magnifier MyGo's search field and find bar show, whose
    /// lens is MingCute's: a circle of radius 6 at (10, 10) and a handle
    /// from (14.5, 14.5) to (20, 20) (`ui/combobox.go` searchIcon).
    pub fn magnifier() -> Self {
        Self::stroked(
            "magnifier",
            "M14.5 14.5 20 20 M16 10a6 6 0 1 1-12 0a6 6 0 0 1 12 0Z",
            2.0,
        )
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

    /// Returns a ruler icon, as MyGo's dialogs demo draws beside
    /// "Dimensions" (`website/docs-demo/dialogs.go`).
    pub fn ruler() -> Self {
        Self::stroked(
            "ruler",
            "M21.3 15.3 8.7 2.7a1 1 0 0 0-1.4 0L2.7 7.3a1 1 0 0 0 0 1.4l12.6 12.6a1 1 0 0 0 1.4 0l4.6-4.6a1 1 0 0 0 0-1.4z M7.5 10.5l2-2 M10.5 13.5l2-2 M13.5 16.5l2-2",
            2.0,
        )
    }



    /// Returns a floppy/save icon.
    pub fn save() -> Self {
        Self::stroked("save", "M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z M17 21v-8H7v8 M7 3v5h8", 2.0)
    }

    /// Returns a trash icon.
    pub fn trash() -> Self {
        Self::stroked("trash", "M3 6h18 M8 6V4h8v2 M19 6l-1 14H6L5 6", 2.0)
    }

    /// Returns a document/file outline icon.
    pub fn file() -> Self {
        Self::stroked(
            "file",
            "M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z M14 3v6h6",
            2.0,
        )
    }

    /// Returns Lucide's `image` icon, the one MyGo's docs demos mark a
    /// picture with (`website/docs-demo/collections.go` collectionsIconImage).
    pub fn image() -> Self {
        Self::stroked(
            "image",
            "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z \
             M9 11a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z \
             M21 15l-3.1-3.1a2 2 0 0 0-2.8 0L6 21",
            2.0,
        )
    }

    /// Returns a code brackets icon.
    pub fn code() -> Self {
        Self::stroked("code", "M8 8l-4 4 4 4 M16 8l4 4-4 4", 1.8)
    }

    /// Returns an audio note icon.
    pub fn audio() -> Self {
        Self::stroked("audio", "M9 18V6l10-2v12 M9 18a2 2 0 1 1-4 0 2 2 0 0 1 4 0 M19 16a2 2 0 1 1-4 0 2 2 0 0 1 4 0", 2.0)
    }

    /// Returns a folder icon.
    pub fn folder() -> Self {
        Self::stroked(
            "folder",
            "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
            2.0,
        )
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
    fn builtin_set_contains_all_icons_with_path_data() {
        let icons = Icon::all();
        assert_eq!(icons.len(), 42);
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
    fn icon_control_emits_svg_commands_and_access_semantics() {
        let size = crate::layout::Size::new(200.0, 100.0);
        let mut context = Context::new(size);
        context.begin_frame(size);
        let state = icon(
            &mut context,
            "search-icon",
            "search",
            32.0,
            Color::rgb(1, 2, 3),
        );
        let missing = icon(
            &mut context,
            "missing-icon",
            "does-not-exist",
            32.0,
            Color::WHITE,
        );
        let frame = context.produce().unwrap();
        assert!(state.found);
        assert_eq!(state.commands, 3);
        assert!(!missing.found);
        assert_eq!(missing.commands, 0);
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.label == "search" && node.value.as_deref() == Some("icon")));
    }

    #[test]
    fn search_icon_strokes_one_path_and_applies_color() {
        let icon = Icon::search();
        assert_eq!(icon.paths.len(), 1);
        let commands = icon.to_commands(24.0, Color::rgb(1, 2, 3));
        assert_eq!(commands.len(), 3);
        let DrawCommand::SvgPath { stroke, .. } = &commands[1] else {
            panic!("expected svg path");
        };
        assert_eq!(*stroke, Some(Color::rgb(1, 2, 3)));
    }
}
