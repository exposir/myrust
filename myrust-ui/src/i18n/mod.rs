//! Locale matching, plural-aware message catalogs, and CLDR number/date data.
//!
//! [`I18n`] resolves the system locale list against an application's
//! supported locales, maintains a catalog lookup chain, and tracks a
//! generation counter so a view can rebuild localized labels after a
//! language change. ICU4X compiled data supplies CLDR plural rules, decimal
//! formatting, and short date formatting without runtime downloads.

mod format;
mod locale;
mod message;

use std::collections::BTreeMap;

pub use locale::{
    resolve_locale, resolve_system_locales, system_locales, LocaleId, LocaleResolution,
};
pub use message::{MessageArguments, MessageValue};

pub(crate) use format::{format_date, format_number, DateFormat, FormatFailure};

/// The fallback locale used when no application locale matches.
pub const DEFAULT_LOCALE: &str = "en-US";

/// An i18n failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum I18nError {
    /// A locale code was not valid BCP 47.
    #[error("invalid locale `{code}`: {reason}")]
    InvalidLocale { code: String, reason: String },
    /// A requested catalog locale is not in the supported set.
    #[error("locale `{locale}` is not supported")]
    UnsupportedLocale { locale: String },
    /// The same catalog was added twice.
    #[error("a catalog for locale `{locale}` is already loaded")]
    DuplicateCatalog { locale: String },
    /// A message failed to parse.
    #[error("invalid message `{key}` ({message}): {reason}")]
    InvalidMessage {
        key: String,
        message: String,
        reason: String,
    },
    /// No catalog in the fallback chain contained the key.
    #[error("missing message `{key}`; tried locales {locales:?}")]
    MissingKey { key: String, locales: Vec<String> },
    /// A placeholder had no corresponding argument.
    #[error("message `{key}` is missing argument `{argument}`")]
    MissingArgument { key: String, argument: String },
    /// A placeholder received an argument of the wrong kind.
    #[error("message `{key}` argument `{argument}` expected {expected}, got {actual}")]
    WrongArgumentType {
        key: String,
        argument: String,
        expected: String,
        actual: String,
    },
    /// An argument value or style was invalid.
    #[error("message `{key}` argument `{argument}` is invalid: {reason}")]
    InvalidArgument {
        key: String,
        argument: String,
        reason: String,
    },
    /// ICU4X could not format a value, usually because locale data is absent.
    #[error("message `{key}` formatting failed: {reason}")]
    Formatting { key: String, reason: String },
}

/// Parsed messages for one locale.
#[derive(Clone, Debug)]
pub struct MessageCatalog {
    locale: LocaleId,
    messages: BTreeMap<String, message::ParsedMessage>,
}

impl MessageCatalog {
    /// Creates a catalog from `(key, ICU MessageFormat)` pairs.
    pub fn try_new<I, K, V>(locale: LocaleId, messages: I) -> Result<Self, I18nError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut catalog = Self {
            locale,
            messages: BTreeMap::new(),
        };
        for (key, source) in messages {
            catalog.insert(key.as_ref(), source.as_ref())?;
        }
        Ok(catalog)
    }

    /// Returns the catalog locale.
    pub fn locale(&self) -> &LocaleId {
        &self.locale
    }

    /// Parses and inserts one message, rejecting duplicate keys.
    pub fn insert(&mut self, key: &str, source: &str) -> Result<(), I18nError> {
        if key.is_empty() {
            return Err(I18nError::InvalidMessage {
                key: key.to_string(),
                message: source.to_string(),
                reason: "message key is empty".to_string(),
            });
        }
        if self.messages.contains_key(key) {
            return Err(I18nError::InvalidMessage {
                key: key.to_string(),
                message: source.to_string(),
                reason: "duplicate message key".to_string(),
            });
        }
        let message =
            message::ParsedMessage::parse(source).map_err(|reason| I18nError::InvalidMessage {
                key: key.to_string(),
                message: source.to_string(),
                reason,
            })?;
        self.messages.insert(key.to_string(), message);
        Ok(())
    }

    /// Returns whether this catalog contains a key.
    pub fn has_key(&self, key: &str) -> bool {
        self.messages.contains_key(key)
    }

    fn get(&self, key: &str) -> Option<&message::ParsedMessage> {
        self.messages.get(key)
    }
}

/// A locale switch result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageChange {
    /// Locale selected before the change.
    pub previous: LocaleId,
    /// Locale selected after the change.
    pub current: LocaleId,
    /// Monotonic counter for consumers that rebuild localized interface text.
    pub generation: u64,
    /// Whether the selected locale changed.
    pub changed: bool,
}

/// A rendered message and the catalog locale that supplied it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Translation {
    /// Rendered message text.
    pub text: String,
    /// Locale of the catalog that actually contained the message.
    pub used_locale: LocaleId,
    /// The selected locale when the message came from a fallback catalog.
    pub fallback_from: Option<LocaleId>,
    /// Whether the result was transformed for the `en-XA` layout locale.
    pub pseudo_localized: bool,
}

/// Locale state, catalogs, fallback lookup, and language-change tracking.
#[derive(Clone, Debug)]
pub struct I18n {
    supported: Vec<LocaleId>,
    catalogs: BTreeMap<String, MessageCatalog>,
    resolution: LocaleResolution,
    generation: u64,
}

impl I18n {
    /// Creates locale state for the supported set, initially selecting `en-US`.
    pub fn new<I>(supported: I) -> Self
    where
        I: IntoIterator<Item = LocaleId>,
    {
        let supported = normalize_supported(supported.into_iter().collect());
        let request = locale::default_locale_id().clone();
        let resolution = resolve_locale(&request, &supported);
        Self {
            supported,
            catalogs: BTreeMap::new(),
            resolution,
            generation: 0,
        }
    }

    /// Creates locale state selected from the operating system's locale list.
    pub fn from_system<I>(supported: I) -> Self
    where
        I: IntoIterator<Item = LocaleId>,
    {
        let mut state = Self::new(supported);
        state.refresh_system_locale();
        state
    }

    /// Supported locales, including the implicit English fallback.
    pub fn supported_locales(&self) -> &[LocaleId] {
        &self.supported
    }

    /// Returns the active locale resolution.
    pub fn resolution(&self) -> &LocaleResolution {
        &self.resolution
    }

    /// Returns the selected locale.
    pub fn current_locale(&self) -> &LocaleId {
        self.resolution.selected()
    }

    /// Returns the generation counter used to invalidate localized UI text.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Loads a catalog. Every catalog locale except `en-XA` must be supported.
    pub fn add_catalog(&mut self, catalog: MessageCatalog) -> Result<(), I18nError> {
        let locale = catalog.locale().clone();
        if locale.is_pseudo()
            || !self
                .supported
                .iter()
                .any(|supported| supported.code() == locale.code())
        {
            return Err(I18nError::UnsupportedLocale {
                locale: locale.code().to_string(),
            });
        }
        if self.catalogs.contains_key(locale.code()) {
            return Err(I18nError::DuplicateCatalog {
                locale: locale.code().to_string(),
            });
        }
        self.catalogs.insert(locale.code().to_string(), catalog);
        Ok(())
    }

    /// Switches locale, resolving through the supported set and English fallback.
    pub fn set_locale(&mut self, code: &str) -> Result<LanguageChange, I18nError> {
        let requested = LocaleId::try_new(code)?;
        if requested.is_pseudo()
            && !self
                .supported
                .iter()
                .any(|supported| supported.is_pseudo() && supported == &requested)
        {
            return Err(I18nError::UnsupportedLocale {
                locale: requested.code().to_string(),
            });
        }
        let next = resolve_locale(&requested, &self.supported);
        Ok(self.apply_resolution(next))
    }

    /// Re-resolves against an explicit system locale list.
    pub fn set_system_locales(&mut self, system: &[String]) -> LanguageChange {
        let next = resolve_system_locales(system, &self.supported);
        self.apply_resolution(next)
    }

    /// Re-resolves against the operating system's current locale list.
    pub fn refresh_system_locale(&mut self) -> LanguageChange {
        let system = system_locales();
        self.set_system_locales(&system)
    }

    /// Looks up and formats a message through the active fallback chain.
    pub fn translate(
        &self,
        key: &str,
        arguments: &MessageArguments<'_>,
    ) -> Result<Translation, I18nError> {
        let selected = self.current_locale();
        for locale in self.resolution.lookup() {
            let Some(catalog) = self.catalogs.get(locale.code()) else {
                continue;
            };
            let Some(message) = catalog.get(key) else {
                continue;
            };
            let mut text = message.render(key, locale, arguments)?;
            let pseudo_localized = selected.is_pseudo();
            if pseudo_localized {
                text = pseudo_localize(&text);
            }
            return Ok(Translation {
                text,
                used_locale: locale.clone(),
                fallback_from: (locale != selected).then(|| selected.clone()),
                pseudo_localized,
            });
        }
        Err(I18nError::MissingKey {
            key: key.to_string(),
            locales: self
                .resolution
                .lookup()
                .iter()
                .map(|locale| locale.code().to_string())
                .collect(),
        })
    }

    fn apply_resolution(&mut self, next: LocaleResolution) -> LanguageChange {
        let previous = self.current_locale().clone();
        let current = next.selected().clone();
        let changed = previous != current;
        if changed {
            self.generation = self.generation.saturating_add(1);
        }
        self.resolution = next;
        LanguageChange {
            previous,
            current,
            generation: self.generation,
            changed,
        }
    }
}

/// Transforms text into the deterministic `en-XA` layout-test form.
///
/// Vowels are accented and doubled, brackets mark the transformed run, and
/// the result is padded to at least 150% of the source length. Digits and
/// punctuation are left alone so formatted numbers remain recognizable.
pub fn pseudo_localize(text: &str) -> String {
    let mut output = String::with_capacity(text.len() * 2);
    output.push('[');
    for character in text.chars() {
        match character {
            'a' => output.push_str("\u{00e1}a"),
            'e' => output.push_str("\u{00e9}e"),
            'i' => output.push_str("\u{00ed}i"),
            'o' => output.push_str("\u{00f3}o"),
            'u' => output.push_str("\u{00fa}u"),
            'A' => output.push_str("\u{00c1}A"),
            'E' => output.push_str("\u{00c9}E"),
            'I' => output.push_str("\u{00cd}I"),
            'O' => output.push_str("\u{00d3}O"),
            'U' => output.push_str("\u{00da}U"),
            other => output.push(other),
        }
    }
    output.push(']');
    let target = text.chars().count().saturating_mul(3) / 2 + 2;
    while output.chars().count() < target {
        output.push('~');
    }
    output
}

fn normalize_supported(mut supported: Vec<LocaleId>) -> Vec<LocaleId> {
    let mut unique = Vec::new();
    for locale in supported.drain(..) {
        if !unique.iter().any(|existing| existing == &locale) {
            unique.push(locale);
        }
    }
    if !unique
        .iter()
        .any(|locale| locale.language() == "en" && !locale.is_pseudo())
    {
        unique.push(locale::default_locale_id().clone());
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locale(code: &str) -> LocaleId {
        LocaleId::try_new(code).unwrap()
    }

    fn english_catalog() -> MessageCatalog {
        MessageCatalog::try_new(
            locale("en-US"),
            [
                ("hello", "Hello"),
                (
                    "items",
                    "{count, plural, =0 {none} one {# item} other {# items}}",
                ),
                (
                    "pronoun",
                    "{gender, select, female {She} male {He} other {They}}",
                ),
                (
                    "formatted",
                    "Total: {amount, number}; {day, date}; {day, date, iso}",
                ),
                (
                    "offset",
                    "{count, plural, offset:1 =1 {you and one other} one {you and one other} other {you and # others}}",
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn system_locale_matching_uses_language_and_english_fallback() {
        let supported = vec![locale("en-US"), locale("zh-CN")];
        let resolution = resolve_locale(&locale("zh-Hant-TW"), &supported);
        assert_eq!(resolution.selected().code(), "zh-CN");
        assert_eq!(
            resolution
                .lookup()
                .iter()
                .map(LocaleId::code)
                .collect::<Vec<_>>(),
            ["zh-CN", "en-US"]
        );

        let system = vec!["fr-FR".to_string(), "zh-Hans-CN".to_string()];
        let resolution = resolve_system_locales(&system, &supported);
        assert_eq!(resolution.requested().code(), "zh-Hans-CN");
        assert_eq!(resolution.selected().code(), "zh-CN");

        let system = vec!["en-US".to_string(), "zh-CN".to_string()];
        let resolution = resolve_system_locales(&system, &supported);
        assert_eq!(resolution.selected().code(), "en-US");
    }

    #[test]
    fn plural_uses_cldr_categories_and_offsets() {
        let mut i18n = I18n::new([locale("en-US")]);
        i18n.add_catalog(english_catalog()).unwrap();

        let none = i18n
            .translate(
                "items",
                &MessageArguments::new().with("count", MessageValue::Number(0.0)),
            )
            .unwrap();
        assert_eq!(none.text, "none");

        let one = i18n
            .translate(
                "items",
                &MessageArguments::new().with("count", MessageValue::Number(1.0)),
            )
            .unwrap();
        assert_eq!(one.text, "1 item");

        let many = i18n
            .translate(
                "items",
                &MessageArguments::new().with("count", MessageValue::Number(2.0)),
            )
            .unwrap();
        assert_eq!(many.text, "2 items");

        let offset = i18n
            .translate(
                "offset",
                &MessageArguments::new().with("count", MessageValue::Number(3.0)),
            )
            .unwrap();
        assert_eq!(offset.text, "you and 2 others");
    }

    #[test]
    fn select_plural_and_number_date_arguments_render() {
        let mut i18n = I18n::new([locale("en-US")]);
        i18n.add_catalog(english_catalog()).unwrap();

        let pronoun = i18n
            .translate(
                "pronoun",
                &MessageArguments::new().with("gender", MessageValue::Text("female")),
            )
            .unwrap();
        assert_eq!(pronoun.text, "She");

        let formatted = i18n
            .translate(
                "formatted",
                &MessageArguments::new()
                    .with("amount", MessageValue::Number(12_345.5))
                    .with("day", MessageValue::Date("2024-02-29")),
            )
            .unwrap();
        assert_eq!(formatted.text, "Total: 12,345.5; 2/29/2024; 2024-02-29");
    }

    #[test]
    fn missing_keys_fall_back_explicitly_and_then_error() {
        let mut i18n = I18n::new([locale("en-US"), locale("zh-CN")]);
        i18n.add_catalog(english_catalog()).unwrap();
        i18n.set_locale("zh-CN").unwrap();

        let translated = i18n.translate("hello", &MessageArguments::new()).unwrap();
        assert_eq!(translated.text, "Hello");
        assert_eq!(translated.used_locale.code(), "en-US");
        assert_eq!(
            translated.fallback_from.as_ref().map(LocaleId::code),
            Some("zh-CN")
        );

        let error = i18n
            .translate("absent", &MessageArguments::new())
            .unwrap_err();
        assert!(matches!(error, I18nError::MissingKey { .. }));
    }

    #[test]
    fn pseudo_locale_expands_english_text() {
        let mut i18n = I18n::new([locale("en-US"), locale("en-XA")]);
        i18n.add_catalog(english_catalog()).unwrap();
        let change = i18n.set_locale("en-XA").unwrap();
        assert!(change.changed);
        let translated = i18n.translate("hello", &MessageArguments::new()).unwrap();
        assert!(translated.pseudo_localized);
        assert!(translated.text.starts_with('['));
        assert!(translated.text.chars().count() >= "Hello".chars().count() * 3 / 2);
        assert!(translated.text.chars().count() > "Hello".chars().count());
    }

    #[test]
    fn invalid_messages_and_missing_arguments_are_errors() {
        let invalid = MessageCatalog::try_new(locale("en-US"), [("bad", "{name")]).unwrap_err();
        assert!(matches!(invalid, I18nError::InvalidMessage { .. }));

        let mut i18n = I18n::new([locale("en-US")]);
        i18n.add_catalog(
            MessageCatalog::try_new(
                locale("en-US"),
                [
                    ("greeting", "Hello {name}"),
                    ("day", "{day, date}"),
                    ("choice", "{kind, select, yes {Yes} other {No}}"),
                ],
            )
            .unwrap(),
        )
        .unwrap();
        let error = i18n
            .translate("greeting", &MessageArguments::new())
            .unwrap_err();
        assert!(matches!(error, I18nError::MissingArgument { .. }));

        let error = i18n
            .translate(
                "day",
                &MessageArguments::new().with("day", MessageValue::Date("2024-02-30")),
            )
            .unwrap_err();
        assert!(matches!(error, I18nError::InvalidArgument { .. }));

        let error = i18n
            .translate(
                "choice",
                &MessageArguments::new().with("kind", MessageValue::Number(1.0)),
            )
            .unwrap_err();
        assert!(matches!(error, I18nError::WrongArgumentType { .. }));
    }

    #[test]
    fn language_generation_changes_only_when_selected_locale_changes() {
        let mut i18n = I18n::new([locale("en-US"), locale("zh-CN")]);
        let first = i18n.set_locale("zh-CN").unwrap();
        assert!(first.changed);
        assert!(first.generation > 0);
        let same = i18n.set_locale("zh-Hans-CN").unwrap();
        assert!(!same.changed);
        assert_eq!(same.generation, first.generation);
    }

    #[test]
    fn formatting_follows_cldr_for_locales_beyond_legacy_pair() {
        let mut i18n = I18n::new([locale("de-DE"), locale("en-US")]);
        i18n.add_catalog(
            MessageCatalog::try_new(
                locale("de-DE"),
                [("value", "{amount, number} / {day, date}")],
            )
            .unwrap(),
        )
        .unwrap();
        i18n.set_locale("de-DE").unwrap();
        let rendered = i18n
            .translate(
                "value",
                &MessageArguments::new()
                    .with("amount", MessageValue::Number(12_345.5))
                    .with("day", MessageValue::Date("2024-02-29")),
            )
            .unwrap();
        assert_eq!(rendered.text, "12.345,5 / 29.02.2024");
    }

    #[test]
    fn russian_plural_uses_icu_categories() {
        let mut i18n = I18n::new([locale("en-US"), locale("ru-RU")]);
        i18n.add_catalog(
            MessageCatalog::try_new(
                locale("ru-RU"),
                [(
                    "items",
                    "{count, plural, one {# \u{0448}\u{0442}\u{0443}\u{043a}\u{0430}} few {# \u{0448}\u{0442}\u{0443}\u{043a}\u{0438}} many {# \u{0448}\u{0442}\u{0443}\u{043a}} other {# \u{0448}\u{0442}\u{0443}\u{043a}\u{0438}}}",
                )],
            )
            .unwrap(),
        )
        .unwrap();
        i18n.set_locale("ru-RU").unwrap();
        for (count, expected) in [
            (1.0, "1 \u{0448}\u{0442}\u{0443}\u{043a}\u{0430}"),
            (2.0, "2 \u{0448}\u{0442}\u{0443}\u{043a}\u{0438}"),
            (5.0, "5 \u{0448}\u{0442}\u{0443}\u{043a}"),
        ] {
            let value = i18n
                .translate(
                    "items",
                    &MessageArguments::new().with("count", MessageValue::Number(count)),
                )
                .unwrap();
            assert_eq!(value.text, expected);
        }
    }
}
