//! Clipboard copies a note as plain text and pastes it back, all in native UI.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::desktop::Clipboard;
use myrust_core::ui::{button, column, row, text, text_field, NativeUi, Theme};

struct Note {
    title: String,
    body: String,
    status: String,
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-clipboard")?;
    let state = Rc::new(RefCell::new(Note {
        title: "A portable note".to_string(),
        body: "Written in a Rust native window.".to_string(),
        status: "Copy the note, then paste it back.".to_string(),
    }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let theme = Theme::system();
        let mut note = view_state.borrow_mut();
        column(context)
            .fill()
            .padding(20.0)
            .gap(12.0)
            .children(|context| {
                text(context, "Clipboard").font_size(24.0).bold();
                text(
                    context,
                    "One note, copied and pasted as plain text through the system clipboard.",
                )
                .font_size(12.0)
                .color(theme.muted);

                text(context, "Title").font_size(12.0).color(theme.muted);
                text_field(context, "title", &mut note.title);
                text(context, "Body").font_size(12.0).color(theme.muted);
                text_field(context, "body", &mut note.body);

                row(context).gap(8.0).children(|context| {
                    if button(context, "Copy note").clicked {
                        let payload = format!("{}\n{}", note.title, note.body);
                        note.status = match Clipboard::new()
                            .and_then(|mut clipboard| clipboard.write_text(payload))
                        {
                            Ok(()) => "Copied to the system clipboard.".to_string(),
                            Err(error) => format!("Copy failed: {error}"),
                        };
                    }
                    if button(context, "Paste").clicked {
                        note.status = match Clipboard::new().and_then(|mut clipboard| clipboard.read_text()) {
                            Ok(pasted) => {
                                let mut lines = pasted.splitn(2, '\n');
                                note.title = lines.next().unwrap_or_default().to_string();
                                note.body = lines.next().unwrap_or_default().to_string();
                                "Pasted into the note.".to_string()
                            }
                            Err(error) => format!("Paste failed: {error}"),
                        };
                    }
                    if button(context, "Clear").clicked {
                        note.title.clear();
                        note.body.clear();
                        note.status = "Cleared.".to_string();
                    }
                });

                text(context, &note.status).font_size(12.0).color(theme.muted);
            });
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Clipboard".to_string(),
            width: 640.0,
            height: 420.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
