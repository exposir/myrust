//! Locale, appearance, accessibility, and system proxy integration.
//!
//! Locale and dark-mode detection use native platform data where available.
//! Accent color, reduced motion, high contrast, and system proxy reading are
//! not implemented by the selected cross-platform crates in this batch and
//! return `None` rather than guessing. Proxy detection reads only standard
//! environment variables; native macOS/Windows proxy stores are not queried.

use std::sync::{Arc, Mutex};

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
    /// Currently always `None`: no selected dependency exposes a portable
    /// accent-color API.
    pub accent_color: Option<(u8, u8, u8)>,
    /// Whether the system requests reduced motion.
    ///
    /// Currently always `None`: no selected dependency exposes this setting.
    pub reduced_motion: Option<bool>,
    /// Whether the system requests increased contrast.
    ///
    /// Currently always `None`.
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

        Self {
            dark_mode,
            accent_color: None,
            reduced_motion: None,
            high_contrast: None,
        }
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
        assert_eq!(preferences.accent_color, None);
    }

    #[test]
    fn proxy_reader_uses_environment_shape() {
        if let Some(proxy) = system_proxy() {
            assert!(!proxy.url.is_empty());
        }
    }
}
