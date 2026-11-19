use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use icu_locale::fallback::LocaleFallbacker;
use icu_locale::Locale as IcuLocale;

use super::{I18nError, DEFAULT_LOCALE};
use crate::languages::Locale;

static DEFAULT_LOCALE_ID: LazyLock<LocaleId> =
    LazyLock::new(|| LocaleId::try_new(DEFAULT_LOCALE).expect("the default locale is valid"));

/// A validated BCP 47 locale identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LocaleId {
    code: String,
    locale: IcuLocale,
}

impl LocaleId {
    /// Parses a BCP 47 locale identifier.
    pub fn try_new(code: &str) -> Result<Self, I18nError> {
        let locale = IcuLocale::try_from_str(code).map_err(|error| I18nError::InvalidLocale {
            code: code.to_string(),
            reason: error.to_string(),
        })?;
        Ok(Self {
            code: locale.to_string(),
            locale,
        })
    }

    /// Returns the normalized locale code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the language subtag, for example `en`.
    pub fn language(&self) -> &str {
        self.locale.id.language.as_str()
    }

    /// Returns the script subtag, for example `Latn`.
    pub fn script(&self) -> Option<&str> {
        self.locale.id.script.as_ref().map(|script| script.as_str())
    }

    /// Returns the region subtag, for example `US`.
    pub fn region(&self) -> Option<&str> {
        self.locale.id.region.as_ref().map(|region| region.as_str())
    }

    /// Returns whether this is the `en-XA` layout test locale.
    pub fn is_pseudo(&self) -> bool {
        self.language() == "en" && self.region() == Some("XA")
    }

    pub(crate) fn icu(&self) -> &IcuLocale {
        &self.locale
    }

    fn same_language(&self, other: &Self) -> bool {
        self.language() == other.language()
    }
}

impl fmt::Display for LocaleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.code)
    }
}

impl FromStr for LocaleId {
    type Err = I18nError;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        Self::try_new(code)
    }
}

impl From<Locale> for LocaleId {
    fn from(locale: Locale) -> Self {
        Self::try_new(locale.code()).expect("the built-in locale codes are valid BCP 47")
    }
}

/// The result of matching a requested locale to an application's catalog set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleResolution {
    requested: LocaleId,
    selected: LocaleId,
    lookup: Vec<LocaleId>,
}

impl LocaleResolution {
    /// The locale the caller asked for, before matching.
    pub fn requested(&self) -> &LocaleId {
        &self.requested
    }

    /// The locale selected from the supported set.
    pub fn selected(&self) -> &LocaleId {
        &self.selected
    }

    /// Locale identifiers to try when looking up a message, in order.
    pub fn lookup(&self) -> &[LocaleId] {
        &self.lookup
    }
}

/// Returns the system's preferred locales in descending order.
pub fn system_locales() -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        sys_locale::get_locales().collect()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Vec::new()
    }
}

/// Resolves the system locale list against an application's supported locales.
///
/// Invalid or unsupported system entries are skipped. The returned fallback
/// tail is always English, defaulting to `en-US` when a catalog set does not
/// contain another English locale.
pub fn resolve_system_locales(system: &[String], supported: &[LocaleId]) -> LocaleResolution {
    let supported = normalized_supported(supported);
    let default = default_locale(&supported);
    let mut requested = None;
    let mut selected = None;

    for code in system {
        let Ok(candidate) = LocaleId::try_new(code) else {
            continue;
        };
        requested.get_or_insert_with(|| candidate.clone());
        if let Some(matched) = match_candidate(&candidate, &supported) {
            selected = Some(matched);
            requested = Some(candidate);
            break;
        }
    }

    let requested = requested.unwrap_or_else(|| default.clone());
    let selected = selected.unwrap_or(default);
    LocaleResolution {
        lookup: lookup_chain(&selected, &supported),
        requested,
        selected,
    }
}

/// Resolves one requested locale against an application's supported locales.
pub fn resolve_locale(requested: &LocaleId, supported: &[LocaleId]) -> LocaleResolution {
    let supported = normalized_supported(supported);
    let default = default_locale(&supported);
    let selected = match_candidate(requested, &supported).unwrap_or(default);
    LocaleResolution {
        lookup: lookup_chain(&selected, &supported),
        requested: requested.clone(),
        selected,
    }
}

fn normalized_supported(supported: &[LocaleId]) -> Vec<LocaleId> {
    let mut locales = Vec::new();
    for locale in supported {
        if !locales.iter().any(|existing| existing == locale) {
            locales.push(locale.clone());
        }
    }
    if !locales
        .iter()
        .any(|locale| locale.language() == "en" && !locale.is_pseudo())
    {
        locales.push(default_locale_id().clone());
    }
    locales
}

fn default_locale(supported: &[LocaleId]) -> LocaleId {
    supported
        .iter()
        .find(|locale| locale.code() == DEFAULT_LOCALE)
        .or_else(|| {
            supported
                .iter()
                .find(|locale| locale.language() == "en" && !locale.is_pseudo())
        })
        .cloned()
        .unwrap_or_else(|| default_locale_id().clone())
}

fn match_candidate(candidate: &LocaleId, supported: &[LocaleId]) -> Option<LocaleId> {
    if let Some(exact) = supported.iter().find(|locale| locale == &candidate) {
        return Some(exact.clone());
    }
    if candidate.is_pseudo() {
        return None;
    }

    if let Some(matched) = match_related_language(candidate, supported) {
        return Some(matched);
    }

    let fallbacker = LocaleFallbacker::new();
    let mut fallback = fallbacker
        .for_config(Default::default())
        .fallback_for(candidate.icu().clone().into());
    while !fallback.get().is_unknown() {
        let code = fallback.get().to_string();
        if let Ok(candidate) = LocaleId::try_new(&code) {
            if let Some(matched) = match_related_language(&candidate, supported) {
                return Some(matched);
            }
        }
        fallback.step();
    }
    None
}

fn match_related_language(candidate: &LocaleId, supported: &[LocaleId]) -> Option<LocaleId> {
    supported
        .iter()
        .find(|locale| {
            !locale.is_pseudo()
                && locale.same_language(candidate)
                && candidate.script().is_some()
                && locale.script() == candidate.script()
        })
        .or_else(|| {
            supported.iter().find(|locale| {
                !locale.is_pseudo()
                    && locale.same_language(candidate)
                    && candidate.region().is_some()
                    && locale.region() == candidate.region()
            })
        })
        .or_else(|| {
            supported
                .iter()
                .find(|locale| !locale.is_pseudo() && locale.same_language(candidate))
        })
        .cloned()
}

fn lookup_chain(selected: &LocaleId, supported: &[LocaleId]) -> Vec<LocaleId> {
    let default = default_locale(supported);
    if selected.is_pseudo() {
        return vec![default];
    }

    let mut lookup = vec![selected.clone()];
    let fallbacker = LocaleFallbacker::new();
    let mut fallback = fallbacker
        .for_config(Default::default())
        .fallback_for(selected.icu().clone().into());
    fallback.step();
    while !fallback.get().is_unknown() {
        let code = fallback.get().to_string();
        if let Ok(candidate) = LocaleId::try_new(&code) {
            if let Some(matched) = match_related_language(&candidate, supported) {
                if !lookup.iter().any(|locale| locale == &matched) {
                    lookup.push(matched);
                }
            }
        }
        fallback.step();
    }
    if !lookup.iter().any(|locale| locale == &default) {
        lookup.push(default);
    }
    lookup
}

pub(crate) fn default_locale_id() -> &'static LocaleId {
    &DEFAULT_LOCALE_ID
}
