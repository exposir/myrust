//! Shared text layout, measurement, and line wrapping.
//!
//! Native layout is shaped once and reused by CPU rasterization, the GPU glyph
//! atlas, and damage bounds. It applies OpenType kerning/ligatures, Unicode
//! bidi ordering, UAX #14 line breaks, and ordered font fallback. The
//! deterministic [`ApproximateMeasurer`] remains only for environments with
//! no font at all.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use unicode_bidi::ParagraphBidiInfo;
use unicode_linebreak::{linebreaks, BreakOpportunity};
use unicode_segmentation::UnicodeSegmentation;

use super::font::{default_font_stack, FontFace, FontStack, ShapedGlyph};

/// Text metrics in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TextMetrics {
    /// Width of the widest measured line.
    pub width: f32,
    /// Total height of all measured lines.
    pub height: f32,
    /// Height allocated to one line.
    pub line_height: f32,
}

/// One font/bidi run in a laid-out line.
#[derive(Debug, Clone)]
pub struct LayoutRun {
    /// Face used for this run.
    pub font: Arc<FontFace>,
    /// Resolved bidi level.
    pub level: u8,
    /// Run text in logical order.
    pub text: String,
    /// Byte start within the line.
    pub start: usize,
    /// Byte end within the line.
    pub end: usize,
    /// Shaped glyphs in visual order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Run advance width.
    pub width: f32,
}

/// One visual line produced by shaping and wrapping.
#[derive(Debug, Clone)]
pub struct LayoutLine {
    /// Source text for this line, without the terminating newline.
    pub text: String,
    /// Byte start in the original string.
    pub source_start: usize,
    /// Byte end in the original string.
    pub source_end: usize,
    /// Visual advance width.
    pub width: f32,
    /// Horizontal offset selected by alignment.
    pub x_offset: f32,
    /// Font/bidi runs in visual left-to-right order.
    pub runs: Vec<LayoutRun>,
}

/// Shaped, bidi-ordered, wrapped text shared by every renderer.
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// CSS font size used to shape and rasterize.
    pub font_size: f32,
    /// Visual lines.
    pub lines: Vec<LayoutLine>,
    /// Width of the widest line.
    pub width: f32,
    /// Total line-box height.
    pub height: f32,
    /// One line-box height.
    pub line_height: f32,
    /// Primary face ascent.
    pub ascent: f32,
    /// Primary face descent.
    pub descent: f32,
    /// Left edge of the aligned visual content relative to the draw origin.
    pub content_x: f32,
    /// Width from `content_x` through the furthest aligned line end.
    pub content_width: f32,
    /// Characters for which no face in the stack had a glyph.
    pub unsupported_glyphs: usize,
}

impl TextLayout {
    /// Returns the baseline for a zero-based visual line.
    pub fn baseline(&self, line: usize) -> f32 {
        line as f32 * self.line_height + self.ascent
    }

    /// Maps a point in layout-local coordinates to a source byte offset.
    pub fn source_offset(&self, x: f32, y: f32) -> usize {
        let line = ((y / self.line_height.max(f32::EPSILON)).floor() as usize)
            .min(self.lines.len().saturating_sub(1));
        self.lines
            .get(line)
            .map(|line| line.source_offset_at_x(x))
            .unwrap_or(0)
    }

    /// Returns the visual caret x for an absolute source byte offset.
    pub fn caret_x(&self, source: usize) -> f32 {
        self.lines
            .iter()
            .find(|line| source >= line.source_start && source <= line.source_end)
            .or_else(|| self.lines.last())
            .map_or(0.0, |line| line.caret_x(source))
    }
}

impl LayoutLine {
    /// Maps a line-local x coordinate to an absolute source byte offset.
    pub fn source_offset_at_x(&self, x: f32) -> usize {
        let mut intervals = self.visual_glyph_intervals();
        if intervals.is_empty() {
            return self.source_start;
        }
        intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (left, right, start, end) in &intervals {
            if x <= *left {
                return self.source_start + *start;
            }
            if x < *right {
                return self.source_start
                    + if x < (*left + *right) / 2.0 {
                        *start
                    } else {
                        *end
                    };
            }
        }
        self.source_end
    }

    /// Returns the line-local visual x for an absolute source byte offset.
    pub fn caret_x(&self, source: usize) -> f32 {
        let local = source
            .saturating_sub(self.source_start)
            .min(self.text.len());
        for (left, right, start, end) in self.visual_glyph_intervals() {
            if local >= start && local <= end {
                if local <= start {
                    return left;
                }
                if local >= end {
                    return right;
                }
                let fraction = (local - start) as f32 / (end - start).max(1) as f32;
                return left + (right - left) * fraction;
            }
        }
        if local >= self.text.len() {
            self.x_offset + self.width
        } else {
            self.x_offset
        }
    }

    fn visual_glyph_intervals(&self) -> Vec<(f32, f32, usize, usize)> {
        let mut intervals = Vec::new();
        let mut visual_pen = self.x_offset;
        for run in &self.runs {
            let mut pen = visual_pen;
            for glyph in &run.glyphs {
                let start = glyph.cluster.min(run.text.len());
                let end = run
                    .text
                    .get(start..)
                    .and_then(|tail| tail.chars().next())
                    .map(|character| start + character.len_utf8())
                    .unwrap_or(run.text.len());
                intervals.push((
                    pen,
                    pen + glyph.advance.max(0.0),
                    run.start + start,
                    run.start + end,
                ));
                pen += glyph.advance;
            }
            visual_pen += run.width;
        }
        intervals
    }
}

#[derive(Hash, PartialEq, Eq)]
struct TextLayoutKey {
    stack_id: u64,
    text: String,
    font_size: u32,
    max_width: Option<u32>,
    alignment: u8,
    mono: bool,
}

struct TextLayoutCache {
    entries: HashMap<TextLayoutKey, Arc<TextLayout>>,
}

static TEXT_LAYOUT_CACHE: OnceLock<Mutex<TextLayoutCache>> = OnceLock::new();
static TEXT_LAYOUT_HITS: AtomicU64 = AtomicU64::new(0);
static TEXT_LAYOUT_MISSES: AtomicU64 = AtomicU64::new(0);

/// Bounded shared text-layout cache counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextLayoutCacheStats {
    /// Layouts reused from the cache.
    pub hits: u64,
    /// Layouts shaped after a cache miss.
    pub misses: u64,
    /// Current entry count.
    pub entries: usize,
}

/// Returns current shared text-layout cache counters.
pub fn text_layout_cache_stats() -> TextLayoutCacheStats {
    let entries = TEXT_LAYOUT_CACHE
        .get()
        .and_then(|cache| cache.lock().ok().map(|cache| cache.entries.len()))
        .unwrap_or(0);
    TextLayoutCacheStats {
        hits: TEXT_LAYOUT_HITS.load(Ordering::Relaxed),
        misses: TEXT_LAYOUT_MISSES.load(Ordering::Relaxed),
        entries,
    }
}

/// Shapes, bidi-reorders and wraps text with the process-wide font stack.
///
/// This is the single layout used by CPU rasterization, the GPU glyph atlas,
/// hit testing and damage bounds. `None` means no font is available and a
/// caller must use its last-resort bitmap text path.
pub fn layout_text_shaped(
    text: &str,
    font_size: f32,
    max_width: Option<f32>,
    alignment: super::paint::TextAlignment,
    mono: bool,
) -> Option<Arc<TextLayout>> {
    let stack = default_font_stack()?;
    Some(cached_layout(
        &stack, text, font_size, max_width, alignment, mono,
    ))
}

/// Shared layout against an explicit stack, for tests and custom hosts.
pub fn layout_text_with_stack(
    stack: &FontStack,
    text: &str,
    font_size: f32,
    max_width: Option<f32>,
    alignment: super::paint::TextAlignment,
    mono: bool,
) -> Option<Arc<TextLayout>> {
    Some(Arc::new(shape_and_wrap(
        stack, text, font_size, max_width, alignment, mono,
    )))
}

fn cached_layout(
    stack: &FontStack,
    text: &str,
    font_size: f32,
    max_width: Option<f32>,
    alignment: super::paint::TextAlignment,
    mono: bool,
) -> Arc<TextLayout> {
    let font_size = if font_size.is_finite() {
        font_size.max(1.0)
    } else {
        1.0
    };
    let maximum = max_width
        .filter(|width| width.is_finite() && *width > 0.0)
        .map(|width| width.max(0.0));
    let key = TextLayoutKey {
        stack_id: stack.id(),
        text: text.to_owned(),
        font_size: font_size.to_bits(),
        max_width: maximum.map(f32::to_bits),
        alignment: match alignment {
            super::paint::TextAlignment::Start => 0,
            super::paint::TextAlignment::Center => 1,
            super::paint::TextAlignment::End => 2,
        },
        mono,
    };
    let cache = TEXT_LAYOUT_CACHE.get_or_init(|| {
        Mutex::new(TextLayoutCache {
            entries: HashMap::new(),
        })
    });
    if let Ok(cache) = cache.lock() {
        if let Some(layout) = cache.entries.get(&key) {
            TEXT_LAYOUT_HITS.fetch_add(1, Ordering::Relaxed);
            return layout.clone();
        }
    }
    TEXT_LAYOUT_MISSES.fetch_add(1, Ordering::Relaxed);
    let layout = Arc::new(shape_and_wrap(
        stack, text, font_size, maximum, alignment, mono,
    ));
    if let Ok(mut cache) = cache.lock() {
        if cache.entries.len() >= 2048 {
            cache.entries.clear();
        }
        cache.entries.insert(key, layout.clone());
    }
    layout
}

fn shape_and_wrap(
    stack: &FontStack,
    text: &str,
    font_size: f32,
    max_width: Option<f32>,
    alignment: super::paint::TextAlignment,
    mono: bool,
) -> TextLayout {
    let metrics = stack.metrics_face(mono);
    let line_height = metrics.line_height(font_size);
    let ascent = metrics.ascent(font_size);
    let descent = metrics.descent(font_size);
    let mut lines = Vec::new();
    let mut unsupported_glyphs = 0;
    let mut source = 0;
    for paragraph in text.split('\n') {
        let paragraph_len = paragraph.len();
        if let Some(maximum) = max_width {
            wrap_paragraph(
                stack,
                paragraph,
                source,
                font_size,
                maximum,
                mono,
                &mut lines,
                &mut unsupported_glyphs,
            );
        } else {
            let (line, unsupported) =
                layout_line(stack, paragraph, source, font_size, mono);
            unsupported_glyphs += unsupported;
            lines.push(line);
        }
        source += paragraph_len + 1;
    }
    if lines.is_empty() {
        lines.push(empty_line(0, 0));
    }
    let width = lines
        .iter()
        .map(|line| line.width)
        .fold(0.0_f32, f32::max);
    let maximum = max_width.unwrap_or(width);
    for line in &mut lines {
        line.x_offset = match alignment {
            super::paint::TextAlignment::Start => 0.0,
            super::paint::TextAlignment::Center => (maximum - line.width).max(0.0) / 2.0,
            super::paint::TextAlignment::End => (maximum - line.width).max(0.0),
        };
    }
    let content_x = lines
        .iter()
        .map(|line| line.x_offset)
        .fold(f32::INFINITY, f32::min);
    let content_end = lines
        .iter()
        .map(|line| line.x_offset + line.width)
        .fold(f32::NEG_INFINITY, f32::max);
    let (content_x, content_end) = if content_x.is_finite() && content_end.is_finite() {
        (content_x, content_end)
    } else {
        (0.0, width)
    };
    TextLayout {
        font_size,
        height: lines.len() as f32 * line_height,
        lines,
        width,
        line_height,
        ascent,
        descent,
        content_x,
        content_width: (content_end - content_x).max(0.0),
        unsupported_glyphs,
    }
}

#[allow(clippy::too_many_arguments)]
fn wrap_paragraph(
    stack: &FontStack,
    paragraph: &str,
    source_start: usize,
    font_size: f32,
    max_width: f32,
    mono: bool,
    lines: &mut Vec<LayoutLine>,
    unsupported_glyphs: &mut usize,
) {
    if paragraph.is_empty() {
        lines.push(empty_line(source_start, source_start));
        return;
    }
    let mut start = 0;
    while start < paragraph.len() {
        let remaining = &paragraph[start..];
        let mut last_fit = None;
        let mut forced = false;
        for (relative, opportunity) in linebreaks(remaining) {
            let end = start + relative;
            if end <= start {
                continue;
            }
            let (line, _) =
                layout_line(stack, &paragraph[start..end], source_start + start, font_size, mono);
            if line.width <= max_width {
                last_fit = Some(end);
                if opportunity == BreakOpportunity::Mandatory {
                    break;
                }
            } else {
                forced = true;
                break;
            }
        }
        let end = if let Some(end) = last_fit {
            end
        } else if forced || max_width <= 0.0 {
            let mut fallback_end = start;
            for (offset, grapheme) in remaining.grapheme_indices(true) {
                let next = start + offset + grapheme.len();
                let (line, _) =
                    layout_line(stack, &paragraph[start..next], source_start + start, font_size, mono);
                if line.width > max_width && fallback_end > start {
                    break;
                }
                fallback_end = next;
                if line.width > max_width {
                    break;
                }
            }
            fallback_end.max(start + remaining.graphemes(true).next().map_or(0, str::len))
        } else {
            start + remaining.len()
        };

        let text = &paragraph[start..end];
        let (line, unsupported) =
            layout_line(stack, text, source_start + start, font_size, mono);
        *unsupported_glyphs += unsupported;
        lines.push(line);
        start = end;
    }
}

fn empty_line(source_start: usize, source_end: usize) -> LayoutLine {
    LayoutLine {
        text: String::new(),
        source_start,
        source_end,
        width: 0.0,
        x_offset: 0.0,
        runs: Vec::new(),
    }
}

fn layout_line(
    stack: &FontStack,
    text: &str,
    source_start: usize,
    font_size: f32,
    mono: bool,
) -> (LayoutLine, usize) {
    if text.is_empty() {
        return (empty_line(source_start, source_start), 0);
    }
    let mut unsupported = 0;
    let bidi = ParagraphBidiInfo::new(text, None);
    let (_, visual_runs) = bidi.visual_runs(0..text.len());
    let mut runs = Vec::new();
    for visual in visual_runs {
        let level = bidi.levels[visual.start].number();
        let mut split = split_by_face(stack, text, visual.clone(), mono, &mut unsupported);
        if level % 2 == 1 {
            split.reverse();
        }
        for (run_start, run_end, font) in split {
            let run_text = &text[run_start..run_end];
            let glyphs = font.shape(run_text, font_size, level % 2 == 1);
            let width = glyphs.iter().map(|glyph| glyph.advance).sum::<f32>();
            runs.push(LayoutRun {
                font,
                level,
                text: run_text.to_owned(),
                start: run_start,
                end: run_end,
                glyphs,
                width,
            });
        }
    }
    let width = runs.iter().map(|run| run.width).sum();
    (
        LayoutLine {
            text: text.to_owned(),
            source_start,
            source_end: source_start + text.len(),
            width,
            x_offset: 0.0,
            runs,
        },
        unsupported,
    )
}

fn split_by_face(
    stack: &FontStack,
    line: &str,
    range: std::ops::Range<usize>,
    mono: bool,
    unsupported: &mut usize,
) -> Vec<(usize, usize, Arc<FontFace>)> {
    let mut out = Vec::new();
    let mut current: Option<(usize, Arc<FontFace>)> = None;
    for (relative, grapheme) in line[range.clone()].grapheme_indices(true) {
        let start = range.start + relative;
        let font = grapheme
            .chars()
            .find_map(|character| stack.face_for(character, mono));
        let font = match font {
            Some(font) => font,
            None => {
                *unsupported += grapheme.chars().count();
                stack.metrics_face(mono).clone()
            }
        };
        match &current {
            Some((_, current_font)) if current_font.id() == font.id() => {}
            Some((run_start, current_font)) => {
                out.push((*run_start, start, current_font.clone()));
                current = Some((start, font));
            }
            None => current = Some((start, font)),
        }
    }
    if let Some((start, font)) = current {
        out.push((start, range.end, font));
    }
    out
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
            wrap_paragraph_measured(
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

fn wrap_paragraph_measured(
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
    use crate::font::{FontRegistry, FontStack};
    use crate::paint::TextAlignment;
    use std::path::Path;

    const DEJAVU: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
    const DEJAVU_MONO: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf";
    const DROID_FALLBACK: &str =
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf";

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

    fn load(path: &str) -> Option<Arc<FontFace>> {
        FontRegistry::load_bytes(std::fs::read(Path::new(path)).ok()?).ok().map(Arc::new)
    }

    fn dejavu_stack() -> Option<Arc<FontStack>> {
        let face = load(DEJAVU)?;
        Some(Arc::new(FontStack::new(face, None, Vec::new())))
    }

    #[test]
    fn shaping_applies_kerning_and_ligatures() {
        let Some(font) = load(DEJAVU) else {
            return;
        };
        let unshaped = font.glyph_advance('A', 24.0) + font.glyph_advance('V', 24.0);
        let shaped = font.shape("AV", 24.0, false);
        let shaped_width = shaped.iter().map(|glyph| glyph.advance).sum::<f32>();
        assert!(
            shaped_width < unshaped - 0.2,
            "kerning should reduce AV from {unshaped} to {shaped_width}"
        );

        let ligature = font.shape("fi", 24.0, false);
        assert_eq!(
            ligature.len(),
            1,
            "the DejaVu OpenType face should substitute an fi ligature"
        );
    }

    #[test]
    fn missing_glyphs_use_the_next_face_in_the_stack() {
        let Some(primary) = load(DEJAVU_MONO) else {
            return;
        };
        let Some(fallback) = load(DEJAVU) else {
            return;
        };
        let stack = FontStack::new(
            primary.clone(),
            Some(primary.clone()),
            vec![fallback.clone()],
        );
        let layout = layout_text_with_stack(
            &stack,
            "\u{01c4}",
            20.0,
            None,
            TextAlignment::Start,
            false,
        )
        .expect("an explicit stack always lays out");
        assert_eq!(layout.unsupported_glyphs, 0);
        assert_eq!(layout.lines[0].runs[0].font.id(), fallback.id());
        assert_ne!(layout.lines[0].runs[0].font.id(), primary.id());

        if let Some(cjk_fallback) = load(DROID_FALLBACK) {
            let stack = FontStack::new(
                primary.clone(),
                Some(primary),
                vec![cjk_fallback.clone()],
            );
            let layout = layout_text_with_stack(
                &stack,
                "\u{4f60}\u{597d}",
                20.0,
                None,
                TextAlignment::Start,
                false,
            )
            .unwrap();
            assert_eq!(layout.unsupported_glyphs, 0);
            assert!(layout.lines[0]
                .runs
                .iter()
                .all(|run| run.font.id() == cjk_fallback.id()));
        }
    }

    #[test]
    fn bidi_reorders_rtl_runs_and_glyphs() {
        let Some(stack) = dejavu_stack() else {
            return;
        };
        let layout = layout_text_with_stack(
            &stack,
            "abc \u{05d0}\u{05d1}\u{05d2}",
            20.0,
            None,
            TextAlignment::Start,
            false,
        )
        .unwrap();
        let line = &layout.lines[0];
        assert_eq!(line.runs.len(), 2);
        assert_eq!(line.runs[0].level, 0);
        assert_eq!(line.runs[1].level, 1);
        assert_eq!(line.runs[0].text, "abc ");
        assert_eq!(line.runs[1].text, "\u{05d0}\u{05d1}\u{05d2}");
        let clusters = line.runs[1]
            .glyphs
            .iter()
            .map(|glyph| glyph.cluster)
            .collect::<Vec<_>>();
        assert!(
            clusters.windows(2).all(|pair| pair[0] >= pair[1]),
            "RTL glyphs should be laid out in visual order: {clusters:?}"
        );
    }

    #[test]
    fn wrapping_uses_the_shaped_width_and_honours_max_width() {
        let Some(stack) = dejavu_stack() else {
            return;
        };
        let layout = layout_text_with_stack(
            &stack,
            "hello shaped world",
            16.0,
            Some(72.0),
            TextAlignment::Start,
            false,
        )
        .unwrap();
        assert!(layout.lines.len() > 1);
        assert!(layout.lines.iter().all(|line| line.width <= 72.0));
        assert_eq!(layout.lines.iter().map(|line| line.text.as_str()).collect::<String>(), "hello shaped world");
    }

    #[test]
    fn alignment_offsets_are_included_in_the_shared_bounds() {
        let Some(stack) = dejavu_stack() else {
            return;
        };
        let layout = layout_text_with_stack(
            &stack,
            "A",
            20.0,
            Some(100.0),
            TextAlignment::Center,
            false,
        )
        .unwrap();
        assert!(layout.lines[0].x_offset > 40.0);
        assert!(layout.content_x > 40.0);
        assert!(layout.content_width < 20.0);
    }

    #[test]
    fn caret_mapping_uses_shaped_clusters_for_combining_and_emoji() {
        let Some(stack) = dejavu_stack() else {
            return;
        };
        let text = "A\u{0301}\u{1f469}\u{200d}\u{1f4bb}B";
        let layout = layout_text_with_stack(
            &stack,
            text,
            24.0,
            None,
            TextAlignment::Start,
            false,
        )
        .unwrap();
        for x in 0..20 {
            let offset = layout.source_offset(x as f32 * layout.width / 20.0, 0.0);
            assert!(text.is_char_boundary(offset), "x={x} offset={offset}");
        }
        assert_eq!(layout.source_offset(0.0, 0.0), 0);
        assert_eq!(layout.caret_x(0), 0.0);
        let combining = layout.caret_x("A".len());
        assert!((0.0..=layout.width).contains(&combining));
    }

    #[test]
    fn shared_layout_cache_hits_and_invalidates_by_stack() {
        let Some(stack) = dejavu_stack() else {
            return;
        };
        let before = text_layout_cache_stats();
        let first = cached_layout(
            &stack,
            "cache hit probe",
            18.0,
            Some(180.0),
            TextAlignment::Start,
            false,
        );
        let second = cached_layout(
            &stack,
            "cache hit probe",
            18.0,
            Some(180.0),
            TextAlignment::Start,
            false,
        );
        assert!(Arc::ptr_eq(&first, &second));
        let after_hit = text_layout_cache_stats();
        assert!(after_hit.hits > before.hits);

        let Some(other_stack) = dejavu_stack() else {
            return;
        };
        let after_stack = cached_layout(
            &other_stack,
            "cache hit probe",
            18.0,
            Some(180.0),
            TextAlignment::Start,
            false,
        );
        assert!(!Arc::ptr_eq(&first, &after_stack));
        assert!(text_layout_cache_stats().misses > after_hit.misses);
    }
}
