//! A counter in native UI: a window MyRust draws itself, with no web page.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{button, column, row, text, NativeUi, Theme};

struct Counter {
    value: i32,
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-counter-native")?;
    let state = Rc::new(RefCell::new(Counter { value: 0 }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let theme = Theme::system();
        let mut counter = view_state.borrow_mut();
        column(context)
            .fill()
            .center()
            .gap(16.0)
            .children(|context| {
                text(context, &counter.value.to_string())
                    .font_size(56.0)
                    .bold();
                row(context).gap(8.0).children(|context| {
                    if button(context, "−").clicked {
                        counter.value -= 1;
                    }
                    if button(context, "Reset").clicked {
                        counter.value = 0;
                    }
                    if button(context, "+").clicked {
                        counter.value += 1;
                    }
                });
                text(context, "A native window, drawn by MyRust.")
                    .font_size(12.0)
                    .color(theme.muted);
            });
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Counter".to_string(),
            width: 360.0,
            height: 260.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
