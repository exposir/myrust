//! Locale, appearance, accessibility, and system proxy integration.
//!
//! Locale and dark-mode detection use native platform data where available.
//! macOS and Windows also read accent color, reduced motion and high contrast
//! from their native appearance APIs. Linux leaves those fields as `None`
//! rather than guessing. Proxy detection reads only standard environment
//! variables; native macOS/Windows proxy stores are not queried.

use std::sync::{Arc, Mutex};

use myrust_ui::{Color, Theme};
use serde::{Deserialize, Serialize};

/// System locale information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemLocale {
    /// Preferred locale in BCP 47 form, when detected.
    pub locale: Option<String>,
    /// Preferred locales in descending priority.
    pub locales: Vec<String>,
}

impl SystemLocale {
    /// Reads the system locale from the OS.
    pub fn detect() -> Self {
        let locales = sys_locale::get_locales().collect::<Vec<_>>();
        let locale = locales.first().cloned();
        Self { locale, locales }
    }
}

/// Application locale override and selection.
#[derive(Debug, Clone, Default)]
pub struct AppLocale {
    chosen: Arc<Mutex<Option<String>>>,
}

impl AppLocale {
    /// Creates an app locale with no explicit override.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an explicit application locale.
    pub fn set_locale(&self, locale: Option<String>) {
        *self.chosen.lock().expect("locale lock poisoned") = locale;
    }

    /// Returns the explicit locale, or the system's preferred locale.
    pub fn chosen_locale(&self) -> Option<String> {
        self.chosen
            .lock()
            .expect("locale lock poisoned")
            .clone()
            .or_else(|| SystemLocale::detect().locale)
    }
}

/// System appearance preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearancePreferences {
    /// Whether the system prefers dark mode, when detectable.
    pub dark_mode: Option<bool>,
    /// System accent color as RGB, when a platform API is available.
    ///
    /// macOS reads `NSColor.controlAccentColor`; Windows reads the system UI
    /// accent color. Linux leaves this as `None`.
    pub accent_color: Option<(u8, u8, u8)>,
    /// Whether the system requests reduced motion.
    ///
    /// macOS reads `NSWorkspace`'s accessibility display option; Windows reads
    /// the animation setting. Linux leaves this as `None`.
    pub reduced_motion: Option<bool>,
    /// Whether the system requests increased contrast.
    ///
    /// macOS reads `NSWorkspace`'s accessibility display option; Windows reads
    /// the high-contrast system parameter. Linux leaves this as `None`.
    pub high_contrast: Option<bool>,
}

impl AppearancePreferences {
    /// Reads available system appearance and accessibility preferences.
    pub fn detect() -> Self {
        let dark_mode = match dark_light::detect() {
            Ok(dark_light::Mode::Dark) => Some(true),
            Ok(dark_light::Mode::Light) => Some(false),
            Ok(dark_light::Mode::Unspecified) | Err(_) => None,
        };

        let mut preferences = Self {
            dark_mode,
            accent_color: None,
            reduced_motion: None,
            high_contrast: None,
        };
        apply_platform_preferences(&mut preferences);
        preferences
    }

    /// Builds a theme from the detected light/dark and accent preferences.
    ///
    /// A missing dark-mode value falls back to [`Theme::system`]. A detected
    /// accent replaces `accent`, derives hover/pressed shades and picks a
    /// contrasting `on_accent`. Reduced-motion and high-contrast values are
    /// available on the preference value; `Theme` has no fields for them.
    pub fn theme(&self) -> Theme {
        let mut theme = match self.dark_mode {
            Some(true) => Theme::dark(),
            Some(false) => Theme::light(),
            None => Theme::system(),
        };
        if let Some((red, green, blue)) = self.accent_color {
            let accent = Color::rgb(red, green, blue);
            let hover_target = if theme.background.r > 140 {
                Color::rgb(0, 0, 0)
            } else {
                Color::rgb(255, 255, 255)
            };
            theme.accent = accent;
            theme.accent_hover = mix_color(accent, hover_target, 0.12);
            theme.accent_pressed = mix_color(accent, Color::rgb(0, 0, 0), 0.22);
            theme.on_accent = if red as u16 + green as u16 + blue as u16 > 3 * 140 {
                Color::rgb(0, 0, 0)
            } else {
                Color::rgb(255, 255, 255)
            };
        }
        theme
    }
}

/// Returns the native light/dark theme with a system accent when available.
pub fn system_theme() -> Theme {
    AppearancePreferences::detect().theme()
}

fn mix_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::rgb(
        mix_channel(from.r, to.r, amount),
        mix_channel(from.g, to.g, amount),
        mix_channel(from.b, to.b, amount),
    )
}

fn mix_channel(from: u8, to: u8, amount: f32) -> u8 {
    ((f32::from(from) * (1.0 - amount) + f32::from(to) * amount).round()) as u8
}

#[cfg(any(target_os = "macos", test))]
fn unit_rgb_to_u8(red: f64, green: f64, blue: f64) -> Option<(u8, u8, u8)> {
    let channel = |value: f64| {
        value
            .is_finite()
            .then(|| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
    };
    Some((channel(red)?, channel(green)?, channel(blue)?))
}

fn apply_platform_preferences(_preferences: &mut AppearancePreferences) {
    #[cfg(target_os = "linux")]
    let _ = ();
    #[cfg(target_os = "macos")]
    platform::apply(_preferences);
    #[cfg(target_os = "windows")]
    platform::apply(_preferences);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod platform {
    use objc2_app_kit::{NSColor, NSColorSpace, NSWorkspace};

    use super::{unit_rgb_to_u8, AppearancePreferences};

    pub(super) fn apply(preferences: &mut AppearancePreferences) {
        let workspace = unsafe { NSWorkspace::sharedWorkspace() };
        preferences.reduced_motion =
            Some(unsafe { workspace.accessibilityDisplayShouldReduceMotion() });
        preferences.high_contrast =
            Some(unsafe { workspace.accessibilityDisplayShouldIncreaseContrast() });

        let color = unsafe { NSColor::controlAccentColor() };
        let space = unsafe { NSColorSpace::sRGBColorSpace() };
        if let Some(color) = unsafe { color.colorUsingColorSpace(&space) } {
            preferences.accent_color = unit_rgb_to_u8(
                unsafe { color.redComponent() },
                unsafe { color.greenComponent() },
                unsafe { color.blueComponent() },
            );
        }
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod platform {
    use std::mem::size_of;

    use windows::{
        Win32::UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{
                SystemParametersInfoW, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            },
        },
        UI::{
            Color as WindowsColor,
            ViewManagement::{UIColorType, UISettings},
        },
    };

    use super::AppearancePreferences;

    pub(super) fn apply(preferences: &mut AppearancePreferences) {
        if let Ok(settings) = UISettings::new() {
            if let Ok(animations_enabled) = settings.AnimationsEnabled() {
                preferences.reduced_motion = Some(!animations_enabled);
            }
            if let Ok(color) = settings.GetColorValue(UIColorType::Accent) {
                preferences.accent_color = Some(color_channels(color));
            }
        }
        preferences.high_contrast = read_high_contrast();
    }

    fn color_channels(color: WindowsColor) -> (u8, u8, u8) {
        (color.R, color.G, color.B)
    }

    fn read_high_contrast() -> Option<bool> {
        let mut data = HIGHCONTRASTW {
            cbSize: size_of::<HIGHCONTRASTW>() as u32,
            ..HIGHCONTRASTW::default()
        };
        let result = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                0,
                Some((&mut data as *mut HIGHCONTRASTW).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        result
            .ok()
            .map(|()| data.dwFlags.contains(HCF_HIGHCONTRASTON))
    }
}

/// System proxy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy URL, for example `http://127.0.0.1:7890`.
    pub url: String,
    /// Optional proxy bypass list.
    pub no_proxy: Vec<String>,
}

/// Reads proxy settings from standard environment variables.
///
/// This does not query macOS SystemConfiguration or Windows Internet Options.
/// Those native proxy readers are not implemented in batch C. `None` means no
/// standard environment proxy variable was present. The returned configuration
/// is informational only; callers must apply it to their own HTTP client.
pub fn system_proxy() -> Option<ProxyConfig> {
    let url = std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .or_else(|_| std::env::var("http_proxy"))
        .ok()?;
    let no_proxy = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(ProxyConfig { url, no_proxy })
}

/// Number and date formatting abstraction.
///
/// Implementations are intentionally not provided in batch C; this trait marks
/// the boundary without pulling in a full ICU data set.
pub trait LocaleFormatter {
    /// Formats a number for a BCP 47 locale.
    fn format_number(&self, value: f64, locale: &str) -> String;
    /// Formats a Unix timestamp for a BCP 47 locale.
    fn format_date(&self, unix_seconds: i64, locale: &str) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_locale_prefers_explicit_override() {
        let locale = AppLocale::new();
        locale.set_locale(Some("zh-Hans".to_string()));
        assert_eq!(locale.chosen_locale().as_deref(), Some("zh-Hans"));
    }

    #[test]
    fn appearance_returns_optional_values() {
        let preferences = AppearancePreferences::detect();
        let _ = preferences.dark_mode;
        #[cfg(target_os = "linux")]
        {
            assert_eq!(preferences.accent_color, None);
            assert_eq!(preferences.reduced_motion, None);
            assert_eq!(preferences.high_contrast, None);
        }
    }

    #[test]
    fn appearance_builds_a_theme_from_neutral_values() {
        let preferences = AppearancePreferences {
            dark_mode: Some(false),
            accent_color: Some((250, 240, 20)),
            reduced_motion: Some(true),
            high_contrast: Some(true),
        };
        let theme = preferences.theme();
        assert_eq!(theme.background, Theme::light().background);
        assert_eq!(theme.accent, Color::rgb(250, 240, 20));
        assert_eq!(theme.on_accent, Color::rgb(0, 0, 0));
        assert_ne!(theme.accent_hover, theme.accent);
        assert_eq!(preferences.reduced_motion, Some(true));
        assert_eq!(preferences.high_contrast, Some(true));
    }

    #[test]
    fn missing_accent_keeps_the_built_in_theme() {
        let preferences = AppearancePreferences {
            dark_mode: Some(false),
            ..AppearancePreferences::default()
        };
        assert_eq!(preferences.theme().accent, Theme::light().accent);
    }

    #[test]
    fn native_color_components_are_clamped_and_finite() {
        assert_eq!(unit_rgb_to_u8(0.0, 0.5, 1.0), Some((0, 128, 255)));
        assert_eq!(unit_rgb_to_u8(-0.2, 1.4, 0.25), Some((0, 255, 64)));
        assert_eq!(unit_rgb_to_u8(f64::NAN, 0.5, 1.0), None);
    }

    #[test]
    fn proxy_reader_uses_environment_shape() {
        if let Some(proxy) = system_proxy() {
            assert!(!proxy.url.is_empty());
        }
    }
}
