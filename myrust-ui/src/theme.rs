//! Native UI theme tokens.
//!
//! Theme only detects light versus dark appearance. It does not read a system
//! accent color or high-contrast preference.

use super::paint::Color;

/// Spacing scale used by native widgets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    /// Base spacing unit.
    pub base: f32,
    /// Extra-small spacing.
    pub xs: f32,
    /// Small spacing.
    pub sm: f32,
    /// Medium spacing.
    pub md: f32,
    /// Large spacing.
    pub lg: f32,
    /// Extra-large spacing.
    pub xl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            base: 4.0,
            xs: 4.0,
            sm: 8.0,
            md: 16.0,
            lg: 24.0,
            xl: 32.0,
        }
    }
}

impl Spacing {
    /// Creates a 4/8/16/24/32 scale from one base unit.
    pub fn from_base(base: f32) -> Self {
        let base = if base.is_finite() && base > 0.0 {
            base
        } else {
            4.0
        };
        Self {
            base,
            xs: base,
            sm: base * 2.0,
            md: base * 4.0,
            lg: base * 6.0,
            xl: base * 8.0,
        }
    }
}

/// Font-size roles used by native widgets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizes {
    /// Small supporting text.
    pub small: f32,
    /// Body text and form controls.
    pub body: f32,
    /// Large title text.
    pub title: f32,
}

impl Default for FontSizes {
    fn default() -> Self {
        Self {
            small: 12.0,
            body: 14.0,
            title: 28.0,
        }
    }
}

/// Native UI color, spacing, and typography tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Main window background.
    pub background: Color,
    /// Raised surface/control background.
    pub surface: Color,
    /// Primary foreground text.
    pub foreground: Color,
    /// Secondary/muted foreground.
    pub muted: Color,
    /// Accent color used by primary controls and focus.
    pub accent: Color,
    /// Text color drawn on accent fills.
    pub on_accent: Color,
    /// Default border and inactive track color.
    pub border: Color,
    /// Surface color of a hovered control.
    pub surface_hover: Color,
    /// Surface color of a pressed control.
    pub surface_pressed: Color,
    /// Accent color of a hovered primary control.
    pub accent_hover: Color,
    /// Accent color of a pressed primary control.
    pub accent_pressed: Color,
    /// Spacing scale.
    pub spacing: Spacing,
    /// Informational font-family name.
    pub font_family: String,
    /// Font-size roles.
    pub font_sizes: FontSizes,
}

impl Theme {
    /// Built-in dark theme matching the existing native Dashboard.
    pub fn dark() -> Self {
        Self {
            background: Color::rgb(24, 24, 27),
            surface: Color::rgb(39, 39, 42),
            foreground: Color::rgb(244, 244, 245),
            muted: Color::rgb(161, 161, 169),
            accent: Color::rgb(34, 211, 238),
            on_accent: Color::rgb(11, 13, 18),
            border: Color::rgb(63, 63, 69),
            surface_hover: Color::rgb(50, 50, 54),
            surface_pressed: Color::rgb(60, 60, 65),
            accent_hover: Color::rgb(96, 165, 250),
            accent_pressed: Color::rgb(37, 99, 235),
            spacing: Spacing::default(),
            font_family: "DejaVu Sans".to_string(),
            font_sizes: FontSizes::default(),
        }
    }

    /// Built-in light theme.
    pub fn light() -> Self {
        Self {
            background: Color::rgb(255, 255, 255),
            surface: Color::rgb(244, 244, 245),
            foreground: Color::rgb(24, 24, 27),
            muted: Color::rgb(113, 113, 122),
            accent: Color::rgb(37, 99, 235),
            on_accent: Color::rgb(255, 255, 255),
            border: Color::rgb(217, 217, 222),
            surface_hover: Color::rgb(233, 233, 236),
            surface_pressed: Color::rgb(221, 221, 225),
            accent_hover: Color::rgb(29, 78, 216),
            accent_pressed: Color::rgb(30, 64, 175),
            ..Self::dark()
        }
    }

    /// Returns a high-contrast warning color for validation and error text.
    ///
    /// This is an additive method rather than a `Theme` field so existing
    /// struct literals keep compiling. The value is derived from the
    /// background luminance instead of a separately stored token.
    pub fn warn(&self) -> Color {
        let brightness =
            (self.background.r as u16 + self.background.g as u16 + self.background.b as u16) / 3;
        if brightness >= 140 {
            Color::rgb(180, 83, 9)
        } else {
            Color::rgb(251, 191, 36)
        }
    }

    /// Returns the success color MyGo uses for a healthy quantity
    /// (`ui/theme.go` Success, #16a34a in the light theme).
    pub fn success(&self) -> Color {
        let brightness =
            (self.background.r as u16 + self.background.g as u16 + self.background.b as u16) / 3;
        if brightness >= 140 {
            Color::rgb(22, 163, 74)
        } else {
            Color::rgb(34, 197, 94)
        }
    }

    /// Returns the warning color MyGo uses for a quantity that calls for
    /// attention (`ui/theme.go` Warning, #d97706 in the light theme).
    pub fn warning(&self) -> Color {
        let brightness =
            (self.background.r as u16 + self.background.g as u16 + self.background.b as u16) / 3;
        if brightness >= 140 {
            Color::rgb(217, 119, 6)
        } else {
            Color::rgb(245, 158, 11)
        }
    }

    /// Returns the danger color MyGo uses for errors and an invalid
    /// control's border (`ui/theme.go` Danger, #dc2626 in the light theme).
    pub fn danger(&self) -> Color {
        let brightness =
            (self.background.r as u16 + self.background.g as u16 + self.background.b as u16) / 3;
        if brightness >= 140 {
            Color::rgb(220, 38, 38)
        } else {
            Color::rgb(239, 68, 68)
        }
    }

    /// Detects light/dark appearance, falling back to dark when unavailable.
    ///
    /// Accent color and high-contrast preference are not queried.
    pub fn system() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match dark_light::detect() {
                Ok(dark_light::Mode::Light) => Self::light(),
                Ok(dark_light::Mode::Dark) | Ok(dark_light::Mode::Unspecified) | Err(_) => {
                    Self::dark()
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::dark()
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_scale_uses_twenty_four_as_large_step() {
        let spacing = Spacing::default();
        assert_eq!(spacing.xs, 4.0);
        assert_eq!(spacing.sm, 8.0);
        assert_eq!(spacing.md, 16.0);
        assert_eq!(spacing.lg, 24.0);
    }

    #[test]
    fn dark_and_light_themes_have_distinct_backgrounds() {
        assert_ne!(Theme::dark().background, Theme::light().background);
        assert_eq!(Theme::light().font_sizes.body, 14.0);
    }
}
