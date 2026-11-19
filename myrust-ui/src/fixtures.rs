//! Repeatable benchmark fixtures for native UI.
//!
//! The ignored tests use the existing test harness and report wall-clock
//! `FrameStats`; they add no benchmark dependency and do not run in ordinary
//! `cargo test`.

/// Row count used by the million-row virtual-list fixture.
pub const MILLION_ROWS: usize = 1_000_000;

/// Returns one thousand distinct labels for an all-visible layout stress case.
pub fn thousand_label_fixture() -> Vec<String> {
    (0..1000).map(|index| format!("Label {index:04}")).collect()
}

/// Returns one million row id/label pairs for the virtual-list fixture.
///
/// Construction allocates the corpus eagerly; only call it from an ignored
/// benchmark or another explicit stress test.
pub fn million_row_fixture() -> Vec<(String, String)> {
    (0..MILLION_ROWS)
        .map(|index| {
            let label = format!("Row {:06} - virtualized benchmark entry", index);
            (format!("row-{index}"), label)
        })
        .collect()
}

/// Returns a large deterministic Markdown document with headings, paragraphs,
/// lists, code and a table.
pub fn large_markdown_fixture() -> String {
    let mut markdown = String::with_capacity(512 * 1024);
    markdown.push_str("# Large Markdown benchmark\n\n");
    for section in 0..400 {
        markdown.push_str(&format!(
            "## Section {section}\n\n\
             Paragraph {section} exercises inline **bold**, *italic*, \
             `code`, and a [link](https://example.com/{section}).\n\n\
             - first item {section}\n\
             - second item {section}\n\
             - third item {section}\n\n\
             ```rust\n\
             fn section_{section}() -> usize {{ {section} }}\n\
             ```\n\n"
        ));
        if section % 25 == 0 {
            markdown.push_str(
                "| value | state |\n| --- | --- |\n| one | ready |\n| two | done |\n\n",
            );
        }
    }
    markdown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{column, text};
    use crate::list::{list, ListResponse, ListRow, ListState};
    use crate::markdown::markdown;
    use crate::native::NativeUi;
    use crate::layout::Size;
    use crate::paint::{Color, DisplayList, DrawCommand};
    use crate::performance::FrameStats;
    use crate::present::{glyph_cache_stats, CpuRasterizer};
    use crate::render::{diff, RenderSize};
    use crate::theme::Theme;
    use std::{cell::RefCell, rc::Rc, time::Instant};

    fn measure(ui: NativeUi, size: Size, frames: usize) -> FrameStats {
        let mut total = FrameStats::default();
        for _ in 0..frames {
            let frame = ui.render(size).unwrap();
            total.accumulate(frame.frame_stats);
        }
        total
    }

    #[test]
    #[ignore = "benchmark fixture"]
    fn bench_one_thousand_labels() {
        let labels = thousand_label_fixture();
        let ui = NativeUi::new(move |context| {
            column(context)
                .fill()
                .padding(8.0)
                .gap(1.0)
                .children(|context| {
                    for label in &labels {
                        text(context, label).font_size(11.0);
                    }
                });
        })
        .with_theme(Theme::light());
        let stats = measure(ui, Size::new(420.0, 300.0), 5);
        eprintln!("1000 labels: {stats:?}");
    }

    #[test]
    #[ignore = "benchmark fixture"]
    fn bench_million_row_virtual_list() {
        let started = Instant::now();
        let rows = million_row_fixture();
        let fixture_ms = started.elapsed().as_secs_f32() * 1000.0;
        let state = Rc::new(RefCell::new(ListState::new()));
        let response = Rc::new(RefCell::new(ListResponse::default()));
        let view_state = Rc::clone(&state);
        let view_response = Rc::clone(&response);
        let ui = NativeUi::new(move |context| {
            column(context).fill().padding(12.0).children(|context| {
                let entries = rows
                    .iter()
                    .map(|(id, label)| ListRow::new(id.as_str(), label.as_str()))
                    .collect::<Vec<_>>();
                let mut state = view_state.borrow_mut();
                *view_response.borrow_mut() = list(
                    context,
                    "million",
                    &mut state,
                    &entries,
                    24.0,
                    240.0,
                );
            });
        })
        .with_theme(Theme::light());
        let stats = measure(ui, Size::new(420.0, 300.0), 3);
        let response = response.borrow();
        let visible = response.visible_end.saturating_sub(response.visible_start);
        assert!(visible <= 16, "only the visible row window is built");
        eprintln!(
            "million rows: fixture {fixture_ms:.1} ms, visible {visible}, stats {stats:?}"
        );
    }

    #[test]
    #[ignore = "benchmark fixture"]
    fn bench_large_markdown_document() {
        let document = large_markdown_fixture();
        let document_len = document.len();
        let ui = NativeUi::new(move |context| {
            column(context)
                .fill()
                .padding(18.0)
                .children(|context| {
                    markdown(context, "large-document", &document);
                });
        })
        .with_theme(Theme::light());
        let stats = measure(ui, Size::new(720.0, 600.0), 3);
        eprintln!("large markdown: {document_len} bytes, stats {stats:?}");
    }

    #[test]
    #[ignore = "benchmark fixture"]
    fn bench_cpu_damage_and_cache_work() {
        let mut previous_commands = Vec::new();
        let mut next_commands = Vec::new();
        for index in 0..2000 {
            let x = (index % 50) as f32 * 12.0;
            let y = (index / 50) as f32 * 12.0;
            if index == 1000 {
                previous_commands.push(DrawCommand::Text {
                    x: 40.0,
                    y: 40.0,
                    text: "damage cache benchmark".to_string(),
                    font_size: 16.0,
                    weight: 600,
                    italic: false,
                    mono: false,
                    family: None,
                    color: Color::rgb(220, 220, 220),
                    max_width: None,
                    alignment: crate::paint::TextAlignment::Start,
                });
                next_commands.push(DrawCommand::Text {
                    x: 40.0,
                    y: 40.0,
                    text: "damage cache benchmark".to_string(),
                    font_size: 16.0,
                    weight: 600,
                    italic: false,
                    mono: false,
                    family: None,
                    color: Color::rgb(120, 220, 120),
                    max_width: None,
                    alignment: crate::paint::TextAlignment::Start,
                });
                continue;
            }
            let command = DrawCommand::Rect {
                x,
                y,
                width: 8.0,
                height: 8.0,
                color: Color::rgb(40, 80, 120),
            };
            previous_commands.push(command.clone());
            next_commands.push(command);
        }
        let previous = DisplayList::from_commands(previous_commands);
        let next = DisplayList::from_commands(next_commands);
        let size = RenderSize::new(640, 480);
        let frame = CpuRasterizer::rasterize(&previous, size, Color::BLACK).unwrap();
        let damage = diff(&previous, &next, size);
        let damaged =
            CpuRasterizer::rasterize_damaged(&frame, &next, size, Color::BLACK, &damage).unwrap();
        let full = CpuRasterizer::rasterize(&next, size, Color::BLACK).unwrap();
        let glyph = glyph_cache_stats();
        let text = crate::text::text_layout_cache_stats();
        eprintln!(
            "cpu damage: full commands {}, full pixels {}, damaged commands {}, damaged pixels {}, glyph {}/{}, text {}/{}",
            full.command_counts.total(),
            full.rasterized_pixels,
            damaged.command_counts.total(),
            damaged.rasterized_pixels,
            glyph.hits,
            glyph.misses,
            text.hits,
            text.misses,
        );
    }
}
