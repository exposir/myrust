use icu_decimal::input::FloatPrecision;
use icu_plurals::{PluralCategory, PluralRules};

use super::format::{
    data_locale, decimal_from_f64, format_date, format_number, DateFormat, FormatFailure,
};
use super::locale::LocaleId;
use super::I18nError;

const MAX_MESSAGE_DEPTH: usize = 32;

type PluralBranches = Vec<(PluralSelector, Vec<Part>)>;
type SelectBranches = Vec<(String, Vec<Part>)>;

/// A value passed to a message placeholder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MessageValue<'a> {
    /// Literal text, also used by `select` placeholders.
    Text(&'a str),
    /// A finite numeric value.
    Number(f64),
    /// An ISO `YYYY-MM-DD` date.
    Date(&'a str),
}

impl MessageValue<'_> {
    fn type_name(self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Number(_) => "number",
            Self::Date(_) => "date",
        }
    }
}

/// Named message arguments in insertion order.
#[derive(Clone, Debug, Default)]
pub struct MessageArguments<'a> {
    values: Vec<(&'a str, MessageValue<'a>)>,
}

impl<'a> MessageArguments<'a> {
    /// Creates an empty argument list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces an argument.
    pub fn with(mut self, name: &'a str, value: MessageValue<'a>) -> Self {
        self.insert(name, value);
        self
    }

    /// Adds or replaces an argument in place.
    pub fn insert(&mut self, name: &'a str, value: MessageValue<'a>) {
        if let Some((_, slot)) = self.values.iter_mut().find(|(key, _)| *key == name) {
            *slot = value;
        } else {
            self.values.push((name, value));
        }
    }

    fn get(&self, name: &str) -> Option<MessageValue<'a>> {
        self.values
            .iter()
            .find_map(|(key, value)| (*key == name).then_some(*value))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedMessage {
    parts: Vec<Part>,
}

#[derive(Clone, Debug)]
enum Part {
    Literal(String),
    Pound,
    Argument {
        name: String,
        format: ArgumentFormat,
    },
    Plural {
        name: String,
        offset: i64,
        branches: PluralBranches,
    },
    Select {
        name: String,
        branches: SelectBranches,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArgumentFormat {
    Text,
    Number(NumberStyle),
    Date(DateFormat),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumberStyle {
    Decimal,
    Integer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PluralSelector {
    Exact(i64),
    Category(PluralCategory),
}

impl ParsedMessage {
    pub(crate) fn parse(source: &str) -> Result<Self, String> {
        let mut parser = Parser {
            source,
            index: 0,
            depth: 0,
        };
        let parts = parser.parse_parts(None, false)?;
        if !parser.at_end() {
            return Err(parser.error("unexpected trailing input"));
        }
        Ok(Self { parts })
    }

    pub(crate) fn render(
        &self,
        key: &str,
        locale: &LocaleId,
        arguments: &MessageArguments<'_>,
    ) -> Result<String, I18nError> {
        let mut output = String::new();
        render_parts(&self.parts, key, locale, arguments, None, &mut output)?;
        Ok(output)
    }
}

fn render_parts(
    parts: &[Part],
    key: &str,
    locale: &LocaleId,
    arguments: &MessageArguments<'_>,
    pound: Option<&str>,
    output: &mut String,
) -> Result<(), I18nError> {
    for part in parts {
        match part {
            Part::Literal(text) => output.push_str(text),
            Part::Pound => output.push_str(pound.unwrap_or("#")),
            Part::Argument { name, format } => {
                let value = required_argument(key, arguments, name)?;
                output.push_str(&render_argument(key, locale, name, *format, value)?);
            }
            Part::Plural {
                name,
                offset,
                branches,
            } => {
                let value = required_argument(key, arguments, name)?;
                let MessageValue::Number(value) = value else {
                    return Err(wrong_type(key, name, "number", value));
                };
                if !value.is_finite() {
                    return Err(FormatFailure::NonFinite.into_error(key));
                }
                let adjusted = value - *offset as f64;
                let category =
                    plural_category(locale, adjusted).map_err(|error| error.into_error(key))?;
                let selected = branches
                    .iter()
                    .find(|(selector, _)| {
                        matches!(selector, PluralSelector::Exact(number) if adjusted == *number as f64)
                    })
                    .or_else(|| {
                        branches.iter().find(|(selector, _)| {
                            matches!(selector, PluralSelector::Category(value) if *value == category)
                        })
                    })
                    .or_else(|| {
                        branches.iter().find(|(selector, _)| {
                            matches!(selector, PluralSelector::Category(PluralCategory::Other))
                        })
                    })
                    .ok_or_else(|| I18nError::Formatting {
                        key: key.to_string(),
                        reason: "plural message has no matching branch".to_string(),
                    })?;
                let number =
                    format_number(adjusted, locale, None).map_err(|error| error.into_error(key))?;
                render_parts(&selected.1, key, locale, arguments, Some(&number), output)?;
            }
            Part::Select { name, branches } => {
                let value = required_argument(key, arguments, name)?;
                let MessageValue::Text(value) = value else {
                    return Err(wrong_type(key, name, "text", value));
                };
                let selected = branches
                    .iter()
                    .find(|(selector, _)| selector == value)
                    .or_else(|| {
                        branches
                            .iter()
                            .find(|(selector, _)| selector.as_str() == "other")
                    })
                    .ok_or_else(|| I18nError::Formatting {
                        key: key.to_string(),
                        reason: "select message has no matching branch".to_string(),
                    })?;
                render_parts(&selected.1, key, locale, arguments, pound, output)?;
            }
        }
    }
    Ok(())
}

fn render_argument(
    key: &str,
    locale: &LocaleId,
    name: &str,
    format: ArgumentFormat,
    value: MessageValue<'_>,
) -> Result<String, I18nError> {
    match format {
        ArgumentFormat::Text => match value {
            MessageValue::Text(text) => Ok(text.to_string()),
            MessageValue::Number(value) => {
                format_number(value, locale, None).map_err(|error| error.into_error(key))
            }
            MessageValue::Date(value) => format_date(value, locale, DateFormat::LocaleShort)
                .map_err(|error| error.into_error(key)),
        },
        ArgumentFormat::Number(NumberStyle::Decimal) => {
            let MessageValue::Number(value) = value else {
                return Err(wrong_type(key, name, "number", value));
            };
            format_number(value, locale, None).map_err(|error| error.into_error(key))
        }
        ArgumentFormat::Number(NumberStyle::Integer) => {
            let MessageValue::Number(value) = value else {
                return Err(wrong_type(key, name, "number", value));
            };
            format_number(value, locale, Some(0)).map_err(|error| error.into_error(key))
        }
        ArgumentFormat::Date(style) => {
            let MessageValue::Date(value) = value else {
                return Err(wrong_type(key, name, "date", value));
            };
            format_date(value, locale, style).map_err(|error| error.into_error(key))
        }
    }
}

fn required_argument<'a>(
    key: &str,
    arguments: &MessageArguments<'a>,
    name: &str,
) -> Result<MessageValue<'a>, I18nError> {
    arguments
        .get(name)
        .ok_or_else(|| I18nError::MissingArgument {
            key: key.to_string(),
            argument: name.to_string(),
        })
}

fn wrong_type(
    key: &str,
    argument: &str,
    expected: &'static str,
    value: MessageValue<'_>,
) -> I18nError {
    I18nError::WrongArgumentType {
        key: key.to_string(),
        argument: argument.to_string(),
        expected: expected.to_string(),
        actual: value.type_name().to_string(),
    }
}

fn plural_category(locale: &LocaleId, value: f64) -> Result<PluralCategory, FormatFailure> {
    let decimal = decimal_from_f64(value, FloatPrecision::RoundTrip)?;
    let locale = data_locale(locale);
    let rules = PluralRules::try_new(locale.into(), Default::default())
        .map_err(|error| FormatFailure::Data(error.to_string()))?;
    Ok(rules.category_for(&decimal))
}

struct Parser<'a> {
    source: &'a str,
    index: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn parse_parts(
        &mut self,
        terminator: Option<char>,
        in_plural: bool,
    ) -> Result<Vec<Part>, String> {
        let mut parts = Vec::new();
        let mut literal = String::new();

        loop {
            let Some(character) = self.peek() else {
                if let Some(terminator) = terminator {
                    return Err(self.error(&format!("unclosed message, expected `{terminator}`")));
                }
                break;
            };
            if Some(character) == terminator {
                self.advance();
                break;
            }
            match character {
                '}' => return Err(self.error("unmatched `}`")),
                '{' => {
                    flush_literal(&mut parts, &mut literal);
                    parts.push(self.parse_argument(in_plural)?);
                }
                '\'' => {
                    self.advance();
                    if self.peek() == Some('\'') {
                        self.advance();
                        literal.push('\'');
                    } else if matches!(self.peek(), Some('{' | '}' | '#')) {
                        loop {
                            match self.peek() {
                                Some('\'') => {
                                    self.advance();
                                    if self.peek() == Some('\'') {
                                        self.advance();
                                        literal.push('\'');
                                        continue;
                                    }
                                    break;
                                }
                                Some(character) => {
                                    self.advance();
                                    literal.push(character);
                                }
                                None => return Err(self.error("unclosed quoted literal")),
                            }
                        }
                    } else {
                        literal.push('\'');
                    }
                }
                '#' if in_plural => {
                    flush_literal(&mut parts, &mut literal);
                    self.advance();
                    parts.push(Part::Pound);
                }
                _ => {
                    self.advance();
                    literal.push(character);
                }
            }
        }
        flush_literal(&mut parts, &mut literal);
        Ok(parts)
    }

    fn parse_argument(&mut self, in_plural: bool) -> Result<Part, String> {
        self.advance();
        if self.depth >= MAX_MESSAGE_DEPTH {
            return Err(self.error("message nesting is too deep"));
        }
        self.depth += 1;
        let result = self.parse_argument_inner(in_plural);
        self.depth -= 1;
        result
    }

    fn parse_argument_inner(&mut self, in_plural: bool) -> Result<Part, String> {
        let name = self.read_until(&[',', '}']).trim().to_string();
        if name.is_empty() {
            return Err(self.error("argument name is empty"));
        }
        if !valid_identifier(&name) {
            return Err(self.error("argument name contains invalid characters"));
        }
        match self.peek() {
            Some('}') => {
                self.advance();
                return Ok(Part::Argument {
                    name,
                    format: ArgumentFormat::Text,
                });
            }
            Some(',') => {
                self.advance();
            }
            _ => return Err(self.error("expected `}` after argument")),
        }

        let kind = self.read_until(&[',', '}']).trim().to_string();
        match kind.as_str() {
            "plural" => {
                self.expect(',')?;
                let (offset, branches) = self.parse_plural_branches()?;
                Ok(Part::Plural {
                    name,
                    offset,
                    branches,
                })
            }
            "select" => {
                self.expect(',')?;
                let branches = self.parse_select_branches(in_plural)?;
                Ok(Part::Select { name, branches })
            }
            "number" => {
                let style = self.parse_optional_style()?;
                let style = match style.as_deref().unwrap_or("decimal") {
                    "decimal" => NumberStyle::Decimal,
                    "integer" => NumberStyle::Integer,
                    other => return Err(self.error(&format!("unknown number style `{other}`"))),
                };
                Ok(Part::Argument {
                    name,
                    format: ArgumentFormat::Number(style),
                })
            }
            "date" => {
                let style = self.parse_optional_style()?;
                let style = match style.as_deref().unwrap_or("short") {
                    "short" => DateFormat::LocaleShort,
                    "iso" => DateFormat::Iso,
                    other => return Err(self.error(&format!("unknown date style `{other}`"))),
                };
                Ok(Part::Argument {
                    name,
                    format: ArgumentFormat::Date(style),
                })
            }
            other => Err(self.error(&format!("unknown argument type `{other}`"))),
        }
    }

    fn parse_optional_style(&mut self) -> Result<Option<String>, String> {
        match self.peek() {
            Some('}') => {
                self.advance();
                Ok(None)
            }
            Some(',') => {
                self.advance();
                let style = self.read_until(&['}']).trim().to_string();
                self.expect('}')?;
                Ok(Some(style))
            }
            _ => Err(self.error("expected `}` after argument type")),
        }
    }

    fn parse_plural_branches(&mut self) -> Result<(i64, PluralBranches), String> {
        let mut offset = 0;
        let mut branches = Vec::new();
        let mut saw_other = false;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.advance();
                    break;
                }
                None => return Err(self.error("unclosed plural select")),
                _ => {}
            }
            let selector = if self.source[self.index..]
                .trim_start()
                .starts_with("offset:")
            {
                self.read_until(&[' ', '\t', '\r', '\n', '{', '}'])
                    .trim()
                    .to_string()
            } else {
                self.read_until(&['{', '}']).trim().to_string()
            };
            if selector.is_empty() {
                return Err(self.error("plural selector is empty"));
            }
            if let Some(value) = selector.strip_prefix("offset:") {
                if !branches.is_empty() {
                    return Err(self.error("plural offset must precede branches"));
                }
                offset = value
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| self.error("plural offset must be a non-negative integer"))?;
                if offset < 0 {
                    return Err(self.error("plural offset must be a non-negative integer"));
                }
                continue;
            }
            self.expect('{')?;
            let body = self.parse_parts(Some('}'), true)?;
            let selector =
                parse_plural_selector(&selector).map_err(|reason| self.error(&reason))?;
            if branches.iter().any(|(existing, _)| *existing == selector) {
                return Err(self.error("duplicate plural selector"));
            }
            saw_other |= selector == PluralSelector::Category(PluralCategory::Other);
            branches.push((selector, body));
        }
        if !saw_other {
            return Err(self.error("plural select must have an `other` branch"));
        }
        Ok((offset, branches))
    }

    fn parse_select_branches(&mut self, in_plural: bool) -> Result<SelectBranches, String> {
        let mut branches = Vec::new();
        let mut saw_other = false;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.advance();
                    break;
                }
                None => return Err(self.error("unclosed select message")),
                _ => {}
            }
            let selector = self.read_until(&['{', '}']).trim().to_string();
            if selector.is_empty() {
                return Err(self.error("select selector is empty"));
            }
            if !valid_identifier(&selector) {
                return Err(self.error("select selector contains invalid characters"));
            }
            self.expect('{')?;
            let body = self.parse_parts(Some('}'), in_plural)?;
            if branches.iter().any(|(existing, _)| existing == &selector) {
                return Err(self.error("duplicate select selector"));
            }
            saw_other |= selector == "other";
            branches.push((selector, body));
        }
        if !saw_other {
            return Err(self.error("select message must have an `other` branch"));
        }
        Ok(branches)
    }

    fn read_until(&mut self, delimiters: &[char]) -> String {
        let start = self.index;
        while let Some(character) = self.peek() {
            if delimiters.contains(&character) {
                break;
            }
            self.advance();
        }
        self.source[start..self.index].to_string()
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        match self.peek() {
            Some(actual) if actual == expected => {
                self.advance();
                Ok(())
            }
            _ => Err(self.error(&format!("expected `{expected}`"))),
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.advance();
        }
    }

    fn at_end(&self) -> bool {
        self.index == self.source.len()
    }

    fn peek(&self) -> Option<char> {
        self.source[self.index..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.index += character.len_utf8();
        Some(character)
    }

    fn error(&self, reason: &str) -> String {
        format!("{reason} at byte {}", self.index)
    }
}

fn flush_literal(parts: &mut Vec<Part>, literal: &mut String) {
    if !literal.is_empty() {
        parts.push(Part::Literal(std::mem::take(literal)));
    }
}

fn parse_plural_selector(selector: &str) -> Result<PluralSelector, String> {
    if let Some(number) = selector.strip_prefix('=') {
        let number = number
            .parse::<i64>()
            .map_err(|_| format!("invalid exact plural selector `{selector}`"))?;
        return Ok(PluralSelector::Exact(number));
    }
    let category = match selector {
        "zero" => PluralCategory::Zero,
        "one" => PluralCategory::One,
        "two" => PluralCategory::Two,
        "few" => PluralCategory::Few,
        "many" => PluralCategory::Many,
        "other" => PluralCategory::Other,
        _ => return Err(format!("unknown plural selector `{selector}`")),
    };
    Ok(PluralSelector::Category(category))
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_escaping_and_nested_arguments() {
        let message = ParsedMessage::parse("Use '{value}' for {name}.").unwrap();
        let args = MessageArguments::new().with("name", MessageValue::Text("Ada"));
        let locale = LocaleId::try_new("en-US").unwrap();
        assert_eq!(
            message.render("key", &locale, &args).unwrap(),
            "Use {value} for Ada."
        );
    }

    #[test]
    fn rejects_unclosed_messages() {
        assert!(ParsedMessage::parse("{name").is_err());
        assert!(ParsedMessage::parse("{count, plural, one {one}}").is_err());
        assert!(ParsedMessage::parse("{value, wat}").is_err());
    }
}
