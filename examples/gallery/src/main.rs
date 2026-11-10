//! The widget gallery: text, buttons, checkboxes, sliders, text fields, and
//! a custom display list, all in one native window.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    button, checkbox, column, container, row, slider, text, text_field, DrawCommand,
    NativeUi, Radius, TextAlignment, Theme,
};

#[derive(Default)]
struct State {
    checked: bool,
    volume: f32,
    name: String,
    clicks: u32,
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-gallery")?;
    let state = Rc::new(RefCell::new(State {
        volume: 40.0,
        ..State::default()
    }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let theme = Theme::system();
        let mut state = view_state.borrow_mut();
        column(context)
            .fill()
            .padding(24.0)
            .gap(14.0)
            .children(|context| {
                text(context, "Widget gallery").font_size(26.0).bold();
                text(
                    context,
                    "Built-in controls, drawn by the immediate-mode native runtime.",
                )
                .font_size(12.0)
                .color(theme.muted);

                row(context).gap(8.0).children(|context| {
                    if button(context, "Primary").clicked {
                        state.clicks += 1;
                    }
                    if button(context, "Secondary").clicked {
                        state.clicks += 1;
                    }
                    if button(context, "Tertiary").clicked {
                        state.clicks += 1;
                    }
                    text(context, &format!("{} clicks", state.clicks))
                        .font_size(12.0)
                        .color(theme.muted);
                });

                row(context).gap(10.0).children(|context| {
                    checkbox(context, "enabled", &mut state.checked);
                    text(
                        context,
                        if state.checked { "Checked" } else { "Unchecked" },
                    )
                    .font_size(13.0)
                    .color(theme.foreground);
                });

                row(context).gap(10.0).children(|context| {
                    text(context, "Volume").font_size(12.0).color(theme.muted);
                    slider(context, "volume", &mut state.volume, 0.0, 100.0);
                    text(context, &format!("{:.0}%", state.volume))
                        .font_size(12.0)
                        .color(theme.foreground);
                });

                text(context, "Name").font_size(12.0).color(theme.muted);
                text_field(context, "name", &mut state.name);

                let mut drawing = vec![
                    DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: 560.0,
                        height: 120.0,
                        radius: Radius::all(14.0),
                        color: theme.surface,
                    },
                    DrawCommand::Circle {
                        x: 70.0,
                        y: 60.0,
                        radius: 34.0,
                        color: theme.accent,
                    },
                    DrawCommand::Line {
                        x1: 130.0,
                        y1: 60.0,
                        x2: 520.0,
                        y2: 60.0,
                        width: 2.0,
                        color: theme.border,
                    },
                    DrawCommand::Text {
                        x: 130.0,
                        y: 34.0,
                        text: "Display list: rounded rect, circle, line, text".to_string(),
                        font_size: 14.0,
                        weight: 500,
                        color: theme.foreground,
                        max_width: Some(400.0),
                        alignment: TextAlignment::Start,
                    },
                ];
                drawing.push(DrawCommand::Text {
                    x: 130.0,
                    y: 70.0,
                    text: format!("{:.0}% accent hue", 210.0 - state.volume * 1.8),
                    font_size: 12.0,
                    weight: 400,
                    color: theme.muted,
                    max_width: None,
                    alignment: TextAlignment::Start,
                });
                container(context).size(560.0, 120.0).draw(drawing);

                text(
                    context,
                    "Font sizes: 26 bold heading, 14 body, 12 muted caption.",
                )
                .font_size(12.0)
                .color(theme.muted);
            });
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Gallery".to_string(),
            width: 680.0,
            height: 640.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
