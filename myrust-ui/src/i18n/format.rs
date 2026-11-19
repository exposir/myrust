use icu_datetime::fieldsets::YMD;
use icu_datetime::input::Date;
use icu_datetime::options::YearStyle;
use icu_datetime::FixedCalendarDateTimeFormatter;
use icu_decimal::input::{Decimal, FloatPrecision};
use icu_decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu_decimal::DecimalFormatter;
use icu_locale::Locale as IcuLocale;
use writeable::Writeable;

use super::locale::LocaleId;
use super::I18nError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FormatFailure {
    NonFinite,
    InvalidDecimals,
    InvalidDate,
    InvalidDateFields,
    Data(String),
}

impl FormatFailure {
    pub(crate) fn into_error(self, key: &str) -> I18nError {
        match self {
            Self::NonFinite => I18nError::InvalidArgument {
                key: key.to_string(),
                argument: "number".to_string(),
                reason: "number must be finite".to_string(),
            },
            Self::InvalidDecimals => I18nError::InvalidArgument {
                key: key.to_string(),
                argument: "number".to_string(),
                reason: "decimals must be between 0 and 18".to_string(),
            },
            Self::InvalidDate => I18nError::InvalidArgument {
                key: key.to_string(),
                argument: "date".to_string(),
                reason: "date must be ISO YYYY-MM-DD".to_string(),
            },
            Self::InvalidDateFields => I18nError::InvalidArgument {
                key: key.to_string(),
                argument: "date".to_string(),
                reason: "date fields are out of range".to_string(),
            },
            Self::Data(reason) => I18nError::Formatting {
                key: key.to_string(),
                reason,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DateFormat {
    LocaleShort,
    Iso,
}

pub(crate) fn parse_iso_date(date: &str) -> Result<(i32, u32, u32), FormatFailure> {
    let mut parts = date.split('-');
    let year = parts.next().ok_or(FormatFailure::InvalidDate)?;
    let month = parts.next().ok_or(FormatFailure::InvalidDate)?;
    let day = parts.next().ok_or(FormatFailure::InvalidDate)?;
    if parts.next().is_some()
        || year.len() != 4
        || month.len() != 2
        || day.len() != 2
        || !year.chars().all(|character| character.is_ascii_digit())
        || !month.chars().all(|character| character.is_ascii_digit())
        || !day.chars().all(|character| character.is_ascii_digit())
    {
        return Err(FormatFailure::InvalidDate);
    }
    Ok((
        year.parse().map_err(|_| FormatFailure::InvalidDate)?,
        month.parse().map_err(|_| FormatFailure::InvalidDate)?,
        day.parse().map_err(|_| FormatFailure::InvalidDate)?,
    ))
}

pub(crate) fn format_number(
    value: f64,
    locale: &LocaleId,
    decimals: Option<usize>,
) -> Result<String, FormatFailure> {
    if !value.is_finite() {
        return Err(FormatFailure::NonFinite);
    }
    if decimals.is_some_and(|decimals| decimals > 18) {
        return Err(FormatFailure::InvalidDecimals);
    }

    let precision = match decimals {
        Some(decimals) => FloatPrecision::Magnitude(-(decimals as i16)),
        None => FloatPrecision::RoundTrip,
    };
    // Decimal formatting keeps the sign of a value that rounds to zero, but
    // the public formatter has historically treated negative zero as zero.
    let value = if value == 0.0 { value.abs() } else { value };
    let decimal = Decimal::try_from_f64(value, precision)
        .map_err(|error| FormatFailure::Data(error.to_string()))?;
    format_decimal(&decimal, locale)
}

pub(crate) fn format_decimal(
    decimal: &Decimal,
    locale: &LocaleId,
) -> Result<String, FormatFailure> {
    let locale = data_locale(locale);
    let mut options = DecimalFormatterOptions::default();
    options.grouping_strategy = Some(GroupingStrategy::Always);
    let formatter = DecimalFormatter::try_new(locale.into(), options)
        .map_err(|error| FormatFailure::Data(error.to_string()))?;
    Ok(formatter.format_to_string(decimal))
}

pub(crate) fn decimal_from_f64(
    value: f64,
    precision: FloatPrecision,
) -> Result<Decimal, FormatFailure> {
    if !value.is_finite() {
        return Err(FormatFailure::NonFinite);
    }
    let value = if value == 0.0 { value.abs() } else { value };
    Decimal::try_from_f64(value, precision).map_err(|error| FormatFailure::Data(error.to_string()))
}

pub(crate) fn format_date(
    date: &str,
    locale: &LocaleId,
    format: DateFormat,
) -> Result<String, FormatFailure> {
    let (year, month, day) = parse_iso_date(date)?;
    let month = u8::try_from(month).map_err(|_| FormatFailure::InvalidDateFields)?;
    let day = u8::try_from(day).map_err(|_| FormatFailure::InvalidDateFields)?;
    let value =
        Date::try_new_gregorian(year, month, day).map_err(|_| FormatFailure::InvalidDateFields)?;

    if format == DateFormat::Iso {
        return Ok(format!("{year:04}-{month:02}-{day:02}"));
    }

    let locale = data_locale(locale);
    let field_set = if locale.id.language.as_str() == "zh" {
        // zh-CN's short CLDR pattern is numeric with slashes. MyRust's existing
        // public contract uses the localized medium form, so retain it.
        YMD::medium().with_year_style(YearStyle::Full)
    } else {
        // Short patterns in CLDR abbreviate the year. The existing API always
        // writes the full year, hence the explicit year style.
        YMD::short().with_year_style(YearStyle::Full)
    };
    let formatter = FixedCalendarDateTimeFormatter::try_new(locale.into(), field_set)
        .map_err(|error| FormatFailure::Data(error.to_string()))?;
    Ok(formatter.format(&value).write_to_string().into_owned())
}

pub(crate) fn data_locale(locale: &LocaleId) -> IcuLocale {
    if locale.is_pseudo() {
        super::locale::default_locale_id().icu().clone()
    } else {
        locale.icu().clone()
    }
}
