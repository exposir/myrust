//! Text input, checkbox, and slider, all owned by application state.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    button, checkbox, column, row, slider, text, text_field, NativeUi, Theme,
};

struct Form {
    name: String,
    subscribe: bool,
    volume: f32,
    status: String,
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-text-input")?;
    let state = Rc::new(RefCell::new(Form {
        name: "Linus".to_string(),
        subscribe: false,
        volume: 62.0,
        status: String::new(),
    }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let theme = Theme::system();
        let mut form = view_state.borrow_mut();
        column(context)
            .fill()
            .padding(24.0)
            .gap(14.0)
            .children(|context| {
                text(context, "Application-owned input").font_size(22.0).bold();
                text(
                    context,
                    "Every control edits the same Rust state the view reads each frame.",
                )
                .font_size(12.0)
                .color(theme.muted);

                text(context, "Name").font_size(12.0).color(theme.muted);
                text_field(context, "name", &mut form.name);

                row(context).gap(8.0).children(|context| {
                    checkbox(context, "subscribe", &mut form.subscribe);
                    text(context, "Subscribe to release notes")
                        .font_size(13.0)
                        .color(theme.foreground);
                });

                row(context).gap(10.0).children(|context| {
                    text(context, "Volume").font_size(12.0).color(theme.muted);
                    slider(context, "volume", &mut form.volume, 0.0, 100.0);
                    text(context, &format!("{:.0}%", form.volume))
                        .font_size(12.0)
                        .color(theme.foreground);
                });

                text(
                    context,
                    &format!(
                        "{} · {} · {:.0}%",
                        if form.name.is_empty() { "(no name)" } else { &form.name },
                        if form.subscribe { "subscribed" } else { "not subscribed" },
                        form.volume
                    ),
                )
                .font_size(12.0)
                .color(theme.muted);

                if button(context, "Submit").clicked {
                    form.status = format!(
                        "Submitted {} with volume {:.0}%.",
                        if form.name.is_empty() { "an unnamed form" } else { &form.name },
                        form.volume
                    );
                }
                text(context, &form.status).font_size(12.0).color(theme.accent);
            });
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Text input".to_string(),
            width: 640.0,
            height: 420.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
