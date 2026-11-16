//! Deterministic, CPU-only text measurement and line wrapping.
//!
//! [`ApproximateMeasurer`] estimates advance widths from Unicode ranges. It
//! does not perform font shaping, kerning, glyph substitution, bidi layout,
//! or system-font fallback. Those operations require a platform font backend
//! in a later rendering batch. [`crate::RealFontMeasurer`]
//! can supply actual single-face TTF advances to the same layout pipeline.

use serde::{Deserialize, Serialize};

/// Approximate text metrics in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TextMetrics {
    /// Width of the widest measured line.
    pub width: f32,
    /// Total height of all measured lines.
    pub height: f32,
    /// Height allocated to one line.
    pub line_height: f32,
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn myrust_measure_text(ptr: *const u8, len: usize, size: f32) -> f32;
}

/// Measures one line of text with the host's canvas, when the host exports
/// `myrust_measure_text`.
///
/// On the web build the browser owns the font, and its canvas is the same
/// one that draws the text, so its `measureText` is the only advance width
/// that keeps layout and glyphs on the same pixels. Native builds return
/// `None` and measure with the embedded face instead.
#[cfg(target_arch = "wasm32")]
pub fn host_text_width(text: &str, font_size: f32) -> Option<f32> {
    if text.is_empty() {
        return None;
    }
    let width = unsafe { myrust_measure_text(text.as_ptr(), text.len(), font_size) };
    width.is_finite().then_some(width.max(0.0))
}

/// Measures with the host canvas in a style: `flags` bit 0 italic, bit 1
/// monospace, bit 2 bold.
#[cfg(target_arch = "wasm32")]
pub fn host_text_width_styled(
    text: &str,
    font_size: f32,
    italic: bool,
    mono: bool,
    bold: bool,
) -> Option<f32> {
    host_text_width_weighted(text, font_size, italic, mono, if bold { 700 } else { 400 })
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn myrust_measure_text_weighted(
        ptr: *const u8,
        len: usize,
        size: f32,
        italic: u32,
        mono: u32,
        weight: u32,
    ) -> f32;
}

/// Measures with the host canvas at a CSS numeric weight. MyGo styles text
/// with CSS weights (600 for strong, 650 and 700 for headings), and the
/// browser resolves each to its own face, so a bold boolean would round
/// them all to the same advance.
#[cfg(target_arch = "wasm32")]
pub fn host_text_width_weighted(
    text: &str,
    font_size: f32,
    italic: bool,
    mono: bool,
    weight: u16,
) -> Option<f32> {
    if text.is_empty() {
        return None;
    }
    let width = unsafe {
        myrust_measure_text_weighted(
            text.as_ptr(),
            text.len(),
            font_size,
            u32::from(italic),
            u32::from(mono),
            u32::from(weight),
        )
    };
    width.is_finite().then_some(width.max(0.0))
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn myrust_measure_text_family(
        ptr: *const u8,
        len: usize,
        size: f32,
        italic: u32,
        mono: u32,
        weight: u32,
        family_ptr: *const u8,
        family_len: usize,
    ) -> f32;
}

/// Measures with the host canvas in a named family, as MyGo's `Font(name)`
/// resolves it. An empty family falls back to the default stack.
#[cfg(target_arch = "wasm32")]
pub fn host_text_width_family(
    text: &str,
    font_size: f32,
    italic: bool,
    mono: bool,
    weight: u16,
    family: &str,
) -> Option<f32> {
    if text.is_empty() {
        return None;
    }
    let width = unsafe {
        myrust_measure_text_family(
            text.as_ptr(),
            text.len(),
            font_size,
            u32::from(italic),
            u32::from(mono),
            u32::from(weight),
            family.as_ptr(),
            family.len(),
        )
    };
    width.is_finite().then_some(width.max(0.0))
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn myrust_text_ascent(size: f32, flags: u32) -> f32;
}

/// Returns the host canvas ascent of the styled font, when exported.
#[cfg(target_arch = "wasm32")]
pub fn host_text_ascent(font_size: f32, italic: bool, mono: bool, bold: bool) -> Option<f32> {
    let flags = u32::from(italic) | (u32::from(mono) << 1) | (u32::from(bold) << 2);
    let ascent = unsafe { myrust_text_ascent(font_size, flags) };
    ascent.is_finite().then_some(ascent.max(0.0))
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn myrust_text_x_height(size: f32, flags: u32) -> f32;
}

/// Returns the host canvas height of a lowercase x, the reference MyGo
/// strikes text through, when exported.
#[cfg(target_arch = "wasm32")]
pub fn host_text_x_height(font_size: f32, italic: bool, mono: bool, bold: bool) -> Option<f32> {
    let flags = u32::from(italic) | (u32::from(mono) << 1) | (u32::from(bold) << 2);
    let height = unsafe { myrust_text_x_height(font_size, flags) };
    height.is_finite().then_some(height.max(0.0))
}

/// Native builds have no host canvas; see the wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_x_height(
    _font_size: f32,
    _italic: bool,
    _mono: bool,
    _bold: bool,
) -> Option<f32> {
    None
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn myrust_text_descent(size: f32, flags: u32) -> f32;
}

/// Returns the host canvas descent of the styled font, when exported.
#[cfg(target_arch = "wasm32")]
pub fn host_text_descent(font_size: f32, italic: bool, mono: bool, bold: bool) -> Option<f32> {
    let flags = u32::from(italic) | (u32::from(mono) << 1) | (u32::from(bold) << 2);
    let descent = unsafe { myrust_text_descent(font_size, flags) };
    descent.is_finite().then_some(descent.max(0.0))
}

/// Native builds have no host canvas; see the wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_ascent(
    _font_size: f32,
    _italic: bool,
    _mono: bool,
    _bold: bool,
) -> Option<f32> {
    None
}

/// Native builds have no host canvas; see the wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_descent(
    _font_size: f32,
    _italic: bool,
    _mono: bool,
    _bold: bool,
) -> Option<f32> {
    None
}

/// Native builds have no host canvas; see the wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_width_styled(
    _text: &str,
    _font_size: f32,
    _italic: bool,
    _mono: bool,
    _bold: bool,
) -> Option<f32> {
    None
}

/// Native builds have no host canvas; see the wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_width_weighted(
    _text: &str,
    _font_size: f32,
    _italic: bool,
    _mono: bool,
    _weight: u16,
) -> Option<f32> {
    None
}

/// Native builds measure with their embedded face; see the wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_width(_text: &str, _font_size: f32) -> Option<f32> {
    None
}

/// Native builds have no host canvas to resolve a named family; see the
/// wasm variant.
#[cfg(not(target_arch = "wasm32"))]
pub const fn host_text_width_family(
    _text: &str,
    _font_size: f32,
    _italic: bool,
    _mono: bool,
    _weight: u16,
    _family: &str,
) -> Option<f32> {
    None
}

/// CPU-side text measurement interface.
pub trait TextMeasurer {
    /// Measures one or more newline-separated lines.
    fn measure(&self, text: &str, font_size: f32) -> TextMetrics;
}

/// Deterministic width estimator based on character classes.
///
/// ASCII text uses `0.6 * font_size` per character, while CJK/full-width
/// characters and common emoji use one full `font_size`. Whitespace uses
/// `0.3 * font_size`. This deliberately avoids host font APIs.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproximateMeasurer;

impl ApproximateMeasurer {
    /// Creates an approximate measurer.
    pub const fn new() -> Self {
        Self
    }

    /// Returns the deterministic advance width for one character.
    pub fn char_width(character: char, font_size: f32) -> f32 {
        let font_size = normalize_font_size(font_size);
        if is_full_width(character) {
            font_size
        } else if character == '\t' {
            font_size * 2.0
        } else if character.is_whitespace() {
            font_size * 0.3
        } else {
            font_size * 0.6
        }
    }
}

impl TextMeasurer for ApproximateMeasurer {
    fn measure(&self, text: &str, font_size: f32) -> TextMetrics {
        let font_size = normalize_font_size(font_size);
        let line_height = font_size * 1.25;
        let lines = text.split('\n').collect::<Vec<_>>();
        let line_count = lines.len().max(1);
        let width = lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|character| Self::char_width(character, font_size))
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max);

        TextMetrics {
            width,
            height: line_count as f32 * line_height,
            line_height,
        }
    }
}

/// One laid-out line of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    /// UTF-8 line content.
    pub text: String,
    /// Measured line width.
    pub width: f32,
    /// Line height supplied by the measurer.
    pub line_height: f32,
}

/// Wraps text into deterministic lines.
///
/// Explicit `\n` boundaries are always preserved. With a finite `max_width`,
/// Latin words wrap at whitespace, oversized words split between characters,
/// and CJK/full-width characters wrap individually. ASCII whitespace inside
/// a bounded paragraph is collapsed; an unbounded layout preserves paragraph
/// contents. `max_width` accepts either a number or `Option<f32>`. This is
/// layout only and does not shape glyphs.
pub fn layout_text(
    text: &str,
    font_size: f32,
    max_width: impl Into<Option<f32>>,
    measurer: &dyn TextMeasurer,
) -> Vec<TextLine> {
    let max_width = max_width.into();
    let font_size = normalize_font_size(font_size);
    let line_height = measurer.measure("", font_size).line_height;
    let bounded_width = max_width.and_then(|width| {
        if width.is_finite() {
            Some(width.max(0.0))
        } else {
            None
        }
    });

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        if let Some(max_width) = bounded_width {
            wrap_paragraph(
                paragraph,
                font_size,
                max_width,
                line_height,
                measurer,
                &mut lines,
            );
        } else {
            lines.push(measured_line(paragraph, font_size, line_height, measurer));
        }
    }
    lines
}

fn wrap_paragraph(
    paragraph: &str,
    font_size: f32,
    max_width: f32,
    line_height: f32,
    measurer: &dyn TextMeasurer,
    lines: &mut Vec<TextLine>,
) {
    let mut line = String::new();
    let mut pending_space = false;

    for token in tokenize(paragraph) {
        match token {
            TextToken::Space => {
                if !line.is_empty() {
                    pending_space = true;
                }
            }
            TextToken::Word(word) => {
                push_atom(
                    &mut line,
                    &word,
                    &mut pending_space,
                    font_size,
                    max_width,
                    line_height,
                    measurer,
                    lines,
                );
            }
            TextToken::Wide(character) => {
                push_atom(
                    &mut line,
                    &character.to_string(),
                    &mut pending_space,
                    font_size,
                    max_width,
                    line_height,
                    measurer,
                    lines,
                );
            }
        }
    }

    lines.push(measured_line(&line, font_size, line_height, measurer));
}

#[allow(clippy::too_many_arguments)]
fn push_atom(
    line: &mut String,
    atom: &str,
    pending_space: &mut bool,
    font_size: f32,
    max_width: f32,
    line_height: f32,
    measurer: &dyn TextMeasurer,
    lines: &mut Vec<TextLine>,
) {
    let spaced = if *pending_space && !line.is_empty() {
        format!(" {atom}")
    } else {
        atom.to_owned()
    };
    *pending_space = false;

    let candidate = format!("{line}{spaced}");
    if measurer.measure(&candidate, font_size).width <= max_width {
        line.push_str(&spaced);
        return;
    }

    if !line.is_empty() {
        lines.push(measured_line(line, font_size, line_height, measurer));
        line.clear();
    }

    if measurer.measure(atom, font_size).width <= max_width {
        line.push_str(atom);
        return;
    }

    for character in atom.chars() {
        let candidate = format!("{line}{character}");
        if !line.is_empty() && measurer.measure(&candidate, font_size).width > max_width {
            lines.push(measured_line(line, font_size, line_height, measurer));
            line.clear();
        }
        line.push(character);
    }
}

fn measured_line(
    text: &str,
    font_size: f32,
    line_height: f32,
    measurer: &dyn TextMeasurer,
) -> TextLine {
    TextLine {
        text: text.to_owned(),
        width: measurer.measure(text, font_size).width,
        line_height,
    }
}

enum TextToken {
    Word(String),
    Wide(char),
    Space,
}

fn tokenize(text: &str) -> Vec<TextToken> {
    let mut tokens = Vec::new();
    let mut word = String::new();

    let flush_word = |tokens: &mut Vec<TextToken>, word: &mut String| {
        if !word.is_empty() {
            tokens.push(TextToken::Word(std::mem::take(word)));
        }
    };

    for character in text.chars() {
        if character.is_whitespace() {
            flush_word(&mut tokens, &mut word);
            tokens.push(TextToken::Space);
        } else if is_full_width(character) {
            flush_word(&mut tokens, &mut word);
            tokens.push(TextToken::Wide(character));
        } else {
            word.push(character);
        }
    }
    flush_word(&mut tokens, &mut word);
    tokens
}

fn normalize_font_size(font_size: f32) -> f32 {
    if font_size.is_finite() {
        font_size.max(0.0)
    } else {
        0.0
    }
}

fn is_full_width(character: char) -> bool {
    matches!(
        character as u32,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn measures_ascii_with_deterministic_widths() {
        let metrics = ApproximateMeasurer.measure("hello", 10.0);
        close(metrics.width, 30.0);
        close(metrics.line_height, 12.5);
        close(metrics.height, 12.5);
    }

    #[test]
    fn measures_cjk_as_full_width_and_preserves_newlines() {
        let metrics = ApproximateMeasurer.measure("你好\nab", 10.0);
        close(metrics.width, 20.0);
        close(metrics.height, 25.0);
    }

    #[test]
    fn explicit_newlines_produce_empty_lines() {
        let lines = layout_text("a\n\nb", 10.0, Some(20.0), &ApproximateMeasurer);
        assert_eq!(lines.len(), 3);
        close(lines[0].width, 6.0);
        close(lines[1].width, 0.0);
        close(lines[2].width, 6.0);
    }

    #[test]
    fn wraps_ascii_at_word_boundaries() {
        let lines = layout_text("hello world", 10.0, Some(40.0), &ApproximateMeasurer);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[1].text, "world");
        close(lines[0].width, 30.0);
        close(lines[1].width, 30.0);
    }

    #[test]
    fn accepts_a_numeric_max_width() {
        let lines = layout_text("hello world", 10.0, 40.0, &ApproximateMeasurer);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[1].text, "world");
    }

    #[test]
    fn splits_oversized_words_between_characters() {
        let lines = layout_text("abcdef", 10.0, Some(20.0), &ApproximateMeasurer);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "abc");
        assert_eq!(lines[1].text, "def");
        close(lines[0].width, 18.0);
        close(lines[1].width, 18.0);
    }

    #[test]
    fn wraps_cjk_one_character_at_a_time() {
        let lines = layout_text("你好世界", 10.0, Some(20.0), &ApproximateMeasurer);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "你好");
        assert_eq!(lines[1].text, "世界");
        close(lines[0].width, 20.0);
        close(lines[1].width, 20.0);
    }

    #[test]
    fn unbounded_layout_keeps_paragraph_on_one_line() {
        let lines = layout_text("hello world", 10.0, None, &ApproximateMeasurer);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "hello world");
        close(lines[0].width, 63.0);
    }

    #[test]
    fn empty_text_still_produces_one_line() {
        let lines = layout_text("", 10.0, Some(10.0), &ApproximateMeasurer);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.is_empty());
        close(lines[0].width, 0.0);
    }
}
