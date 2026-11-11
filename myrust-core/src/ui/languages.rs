//! Minimal locale-aware formatting for en-US and zh-CN.

/// Supported locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// English (United States).
    #[default]
    EnUs,
    /// Simplified Chinese (China).
    ZhCn,
}

impl Locale {
    /// Parses a supported BCP-47-style code.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en-US" => Some(Self::EnUs),
            "zh-CN" => Some(Self::ZhCn),
            _ => None,
        }
    }

    /// Returns the locale code.
    pub fn code(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }
}

/// Date formatting style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateStyle {
    /// Canonical `YYYY-MM-DD`.
    #[default]
    Iso,
    /// Locale short form.
    Short,
}

/// Formatting failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// Number was NaN or infinite.
    #[error("number must be finite")]
    NonFinite,
    /// More than 18 fraction digits were requested.
    #[error("decimals must be between 0 and 18")]
    InvalidDecimals,
    /// Input was not an ISO `YYYY-MM-DD` date.
    #[error("date must be ISO YYYY-MM-DD")]
    InvalidDate,
    /// Calendar fields did not form a real date.
    #[error("date fields are out of range")]
    InvalidDateFields,
}

/// Formats a number with thousands separators and fixed fraction digits.
///
/// Both supported locales use comma grouping and a dot decimal separator in
/// this minimal subset.
pub fn format_number(value: f64, locale: Locale, decimals: usize) -> Result<String, FormatError> {
    if !value.is_finite() {
        return Err(FormatError::NonFinite);
    }
    if decimals > 18 {
        return Err(FormatError::InvalidDecimals);
    }
    let _ = locale;
    let negative = value.is_sign_negative() && value != 0.0;
    let absolute = value.abs();
    let formatted = format!("{absolute:.decimals$}");
    let mut parts = formatted.split('.');
    let integer = parts.next().unwrap_or("0");
    let fraction = parts.next();
    let mut grouped = String::new();
    for (index, character) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(character);
    }
    if negative {
        grouped.insert(0, '-');
    }
    if let Some(fraction) = fraction {
        grouped.push('.');
        grouped.push_str(fraction);
    }
    Ok(grouped)
}

/// Formats an ISO `YYYY-MM-DD` date.
pub fn format_date(date: &str, locale: Locale, style: DateStyle) -> Result<String, FormatError> {
    let (year, month, day) = parse_iso_date(date)?;
    if !valid_date(year, month, day) {
        return Err(FormatError::InvalidDateFields);
    }
    match style {
        DateStyle::Iso => Ok(format!("{year:04}-{month:02}-{day:02}")),
        DateStyle::Short => match locale {
            Locale::EnUs => Ok(format!("{month:02}/{day:02}/{year:04}")),
            Locale::ZhCn => Ok(format!("{year}年{month}月{day}日")),
        },
    }
}

fn parse_iso_date(date: &str) -> Result<(i32, u32, u32), FormatError> {
    let mut parts = date.split('-');
    let year = parts.next().ok_or(FormatError::InvalidDate)?;
    let month = parts.next().ok_or(FormatError::InvalidDate)?;
    let day = parts.next().ok_or(FormatError::InvalidDate)?;
    if parts.next().is_some()
        || year.len() != 4
        || month.len() != 2
        || day.len() != 2
        || !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
        || !day.chars().all(|c| c.is_ascii_digit())
    {
        return Err(FormatError::InvalidDate);
    }
    Ok((
        year.parse().map_err(|_| FormatError::InvalidDate)?,
        month.parse().map_err(|_| FormatError::InvalidDate)?,
        day.parse().map_err(|_| FormatError::InvalidDate)?,
    ))
}

fn valid_date(year: i32, month: u32, day: u32) -> bool {
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => return false,
    };
    day <= days
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_for_en_and_zh_subsets() {
        assert_eq!(
            format_number(1_234_567.891, Locale::EnUs, 2).unwrap(),
            "1,234,567.89"
        );
        assert_eq!(format_number(-1234.5, Locale::ZhCn, 1).unwrap(), "-1,234.5");
        assert_eq!(format_number(0.0, Locale::EnUs, 0).unwrap(), "0");
        assert_eq!(format_number(42.0, Locale::ZhCn, 3).unwrap(), "42.000");
        assert_eq!(
            format_number(f64::NAN, Locale::EnUs, 2),
            Err(FormatError::NonFinite)
        );
        assert_eq!(
            format_number(1.0, Locale::EnUs, 19),
            Err(FormatError::InvalidDecimals)
        );
        assert_eq!(Locale::from_code("zh-CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::from_code("fr-FR"), None);
    }

    #[test]
    fn formats_iso_dates_and_rejects_invalid_calendar_fields() {
        assert_eq!(
            format_date("2024-02-29", Locale::EnUs, DateStyle::Iso).unwrap(),
            "2024-02-29"
        );
        assert_eq!(
            format_date("2024-02-29", Locale::EnUs, DateStyle::Short).unwrap(),
            "02/29/2024"
        );
        assert_eq!(
            format_date("2024-02-29", Locale::ZhCn, DateStyle::Short).unwrap(),
            "2024年2月29日"
        );
        assert_eq!(
            format_date("2023-02-29", Locale::EnUs, DateStyle::Iso),
            Err(FormatError::InvalidDateFields)
        );
        assert_eq!(
            format_date("2024-2-29", Locale::EnUs, DateStyle::Iso),
            Err(FormatError::InvalidDate)
        );
        assert_eq!(
            format_date("not-a-date", Locale::ZhCn, DateStyle::Iso),
            Err(FormatError::InvalidDate)
        );
    }
}
