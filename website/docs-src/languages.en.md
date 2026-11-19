`myrust-ui::i18n` now provides locale matching, fallback lookup, ICU MessageFormat rendering, and CLDR number and date formats. CLDR data is compiled into the binary through ICU4X; the runtime never downloads locale packs.

```rust
use myrust_ui::{I18n, LocaleId, MessageArguments, MessageCatalog, MessageValue};

let mut i18n = I18n::from_system([
    LocaleId::try_new("en-US")?,
    LocaleId::try_new("zh-CN")?,
]);
i18n.add_catalog(MessageCatalog::try_new(
    LocaleId::try_new("en-US")?,
    [
        ("file-count", "{count, plural, =0 {No files} one {# file} other {# files}}"),
        ("greeting", "Hello, {name}"),
    ],
)?)?;

let text = i18n.translate(
    "file-count",
    &MessageArguments::new().with("count", MessageValue::Number(2.0)),
)?;
println!("{}", text.text); // "2 files"
```

## Locale resolution

`system_locales()` reads the operating system's preferred locale list on native targets. `resolve_system_locales()` walks that list in order, matches exact locales first, then related language, script and region variants, and finally falls back to English. The default is `en-US`; if an application exposes another English catalog, that catalog becomes the fallback tail.

`LocaleId` validates and normalizes BCP 47 identifiers. `I18n` always includes an English fallback in its supported set, even if the caller's array contains only non-English locales.

`en-XA` is the layout test locale. Add it to the supported set, select it with `set_locale("en-XA")`, and message output is accent-expanded to at least 150% of its source length. The base text still comes from the English catalog.

## Catalogs and fallback

`MessageCatalog::try_new(locale, [(key, message), ...])` parses every message before accepting the catalog. Syntax errors are reported with the key, source message and byte position. Duplicate keys are rejected.

`I18n::translate` tries each locale in the active fallback chain. A message found in a fallback catalog still renders successfully, but `Translation` records `used_locale`, `fallback_from` and `pseudo_localized`, so the caller can log or surface the fallback. If every catalog lacks the key, `I18nError::MissingKey` lists the locales that were tried.

Catalogs are authored data, not a replacement for programming-language strings. A missing translation is never invented silently: either a fallback catalog supplies it or translation returns an error.

## Message syntax

The implemented ICU MessageFormat subset supports:

- interpolation: `{name}`
- plural selection: `{count, plural, =0 {...} one {...} other {...}}`, with `#`, `offset:N`, exact selectors and CLDR plural categories
- select selection: `{kind, select, a {...} b {...} other {...}}`
- number arguments: `{amount, number}` and `{amount, number, integer}`
- date arguments: `{day, date}`, `{day, date, short}` and `{day, date, iso}`
- quoted literal syntax with `''` and apostrophe-quoted runs

Plural categories are selected by `icu_plurals`, not by hard-coded `one`/`other` checks. This matters for languages such as Russian, Polish and Arabic. Both `select` and `plural` must include an `other` branch.

Values are passed as `MessageValue::Text`, `MessageValue::Number` or `MessageValue::Date`. A missing argument returns `MissingArgument`; a number supplied to `select`, or text supplied to `plural`, returns `WrongArgumentType`. Invalid ISO dates and non-finite numbers are errors rather than empty output.

## Language changes

`I18n::set_locale`, `set_system_locales` and `refresh_system_locale` return a `LanguageChange`. `changed` says whether the selected locale changed, and `generation` is a monotonic per-instance counter. A view can retain the generation it rendered and rebuild its translated labels when it changes.

This is the API-level integration point. `Context` and the built-in controls do not yet read an ambient `I18n` value, so framework-owned English strings such as `"text field"` and `"checkbox"` remain fixed until controls accept translated labels.

## Formatting numbers and dates

The compatibility functions `format_number(value, locale, decimals)` and `format_date(date, locale, style)` now delegate to ICU4X. The existing `en-US` and `zh-CN` output is preserved, including the US `M/D/YYYY` short date and the Chinese short date with localized year, month and day markers, while other locales use their CLDR patterns through messages.

```rust
let n = format_number(1234567.89, Locale::EnUs, 2)?; // "1,234,567.89"
let d = format_date("2024-02-29", Locale::ZhCn, DateStyle::Short)?;
```

Decimal grouping, decimal separators, minus signs and date ordering come from CLDR. Formats outside the implemented decimal/date subset still need dedicated APIs.

## Limits

There is no currency, percent, measurement, unit, relative-time or time-zone formatter. Message parsing implements the subset above, not the full MessageFormat 2 grammar. Built-in UI chrome is not automatically translated, and changing `I18n` does not by itself repaint a view; the generation counter tells the application when to rebuild it.

## In Go

The reference implementation gets its catalog and formatting data from Go's i18n packages. MyRust uses the equivalent ICU4X components directly, with compiled CLDR data and no runtime data service.

## Package i18n

The package-level API is `myrust-ui::i18n`: `LocaleId`, `I18n`, `MessageCatalog`, `MessageArguments`, `MessageValue`, `Translation`, `I18nError`, locale resolvers, `system_locales` and `pseudo_localize`. `myrust-ui::languages` remains as the stable compatibility facade for `Locale`, `DateStyle`, `format_number` and `format_date`.
