//! Showcase landing page — hero, feature-card grid, live chart, toast feedback.
//! Demonstrates myrust-ui immediate-mode controls with no external dependencies.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    button, button_primary, column, container, grid, icon, line_chart, row, text, toasts, Color,
    Context, NativeUi, Toast, ToastQueue,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
struct State {
    toasts: ToastQueue,
    clicks: u32,
    tick: u64,
}

// ---------------------------------------------------------------------------
// Chart data — 30 smoothly-animated sine-wave samples driven by tick.
// ---------------------------------------------------------------------------

fn chart_values(tick: u64) -> Vec<f32> {
    (0..30)
        .map(|i| {
            let phase = (tick as f32 * 0.04 + i as f32 * 0.28).sin();
            (0.5_f32 + 0.42 * phase).clamp(0.0, 1.0)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

fn view(context: &mut Context, state: &mut State) {
    state.tick = state.tick.wrapping_add(1);

    let theme = context.theme().clone();
    let accent = theme.accent;
    let muted = theme.muted;
    let surface = theme.surface;
    let border = theme.border;

    column(context)
        .fill()
        .padding(40.0)
        .gap(40.0)
        .children(|context| {
            // ----------------------------------------------------------------
            // Hero
            // ----------------------------------------------------------------
            column(context).gap(16.0).children(|context| {
                text(context, "Build native apps in Rust.")
                    .font_size(38.0)
                    .bold();
                text(
                    context,
                    "An immediate-mode UI engine — no HTML, no JavaScript, no WebView.",
                )
                .font_size(15.0)
                .color(muted);

                row(context).gap(12.0).children(|context| {
                    if button_primary(context, "Get started").clicked {
                        state.clicks += 1;
                        state
                            .toasts
                            .push(context, Toast::new("Opening the docs…"));
                    }
                    if button(context, "View on GitHub").clicked {
                        state.clicks += 1;
                        state
                            .toasts
                            .push(context, Toast::new("Opening GitHub repository…"));
                    }
                });
            });

            // ----------------------------------------------------------------
            // 2×3 feature-card grid (3 columns, 2 rows)
            // ----------------------------------------------------------------
            grid(context).columns(3).gap(16.0).children(|context| {
                let features: &[(&str, &str, &str)] = &[
                    (
                        "home",
                        "Native UI",
                        "Self-drawn windows: flex layout, TTF glyphs, hover, focus.",
                    ),
                    (
                        "code",
                        "Typed IPC",
                        "Call Rust from TypeScript with auto-generated bindings.",
                    ),
                    (
                        "settings",
                        "Desktop APIs",
                        "Menus, tray, clipboard, global shortcuts.",
                    ),
                    (
                        "star",
                        "No unsafe",
                        "myrust-core enforces #![forbid(unsafe_code)].",
                    ),
                    (
                        "loader",
                        "Small binary",
                        "Ships as a single stripped executable under 2 MB.",
                    ),
                    (
                        "terminal",
                        "Testable",
                        "NativeUi::render() runs views headlessly in CI.",
                    ),
                ];

                for (idx, (icon_name, title, desc)) in features.iter().enumerate() {
                    container(context)
                        .padding(18.0)
                        .background(surface, 10.0)
                        .border(border, 10.0)
                        .children(|context| {
                            column(context).gap(10.0).children(|context| {
                                icon(
                                    context,
                                    &format!("showcase-icon-{idx}"),
                                    icon_name,
                                    20.0,
                                    accent,
                                );
                                text(context, *title).font_size(14.0).bold();
                                text(context, *desc).font_size(12.0).color(muted);
                            });
                        });
                }
            });

            // ----------------------------------------------------------------
            // Metrics strip + live line chart
            // ----------------------------------------------------------------
            row(context).gap(32.0).children(|context| {
                // Three headline metrics
                column(context).gap(20.0).children(|context| {
                    for (num, label) in [
                        ("1.9 MB", "Binary, stripped"),
                        ("6 MB", "Memory, idle UI"),
                        ("0%", "CPU at idle"),
                    ] {
                        column(context).gap(2.0).children(|context| {
                            text(context, num).font_size(28.0).bold().color(accent);
                            text(context, label).font_size(11.0).color(muted);
                        });
                    }
                });

                // Live animated chart
                let values = chart_values(state.tick);
                container(context)
                    .width(420.0)
                    .height(120.0)
                    .children(|context| {
                        line_chart(context, "showcase-chart", &values, 120.0, Some(0.0), Some(1.0));
                    });
            });

            // Toast overlay — must be last in the column so it renders above other content
            toasts(context, "showcase-toasts", &mut state.toasts);
        });
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let mut app = App::new("myrust-showcase")?;

    let state = Rc::new(RefCell::new(State::default()));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let mut s = view_state.borrow_mut();
        view(context, &mut s);
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Showcase".to_string(),
            width: 760.0,
            height: 700.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
