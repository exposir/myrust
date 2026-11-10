//! Three rolling waveforms drawn into a native display list, with the frame
//! clock driving the newest samples.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    column, container, text, Animation, Color, DrawCommand, Easing, NativeUi, Theme,
};

const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 420.0;
const POINTS: usize = 180;

struct State {
    start: Instant,
    pump: f32,
}

fn series_color(index: usize) -> Color {
    match index {
        0 => Color::rgb(40, 140, 240),
        1 => Color::rgb(240, 100, 70),
        _ => Color::rgb(50, 190, 110),
    }
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-chart-streaming")?;
    let state = Rc::new(RefCell::new(State {
        start: Instant::now(),
        pump: 1.0,
    }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let theme = Theme::system();
        let mut state = view_state.borrow_mut();
        let elapsed = context.frame_time().duration_since(state.start).as_secs_f32();

        let pump = Animation::new(Duration::from_millis(400), Easing::Linear);
        let target = state.pump;
        let value = context.animate_with("chart-pump", target, pump);
        if (value - target).abs() < 0.001 {
            let next = 1.0 - target;
            context.animate_with("chart-pump", next, pump);
            state.pump = next;
        }

        let mut commands = vec![DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: WIDTH,
            height: HEIGHT,
            color: theme.surface,
        }];
        for level in 0..=4 {
            let y = level as f32 * HEIGHT / 4.0;
            commands.push(DrawCommand::Line {
                x1: 0.0,
                y1: y,
                x2: WIDTH,
                y2: y,
                width: 1.0,
                color: theme.border,
            });
        }
        for series in 0..3 {
            let color = series_color(series);
            let mut previous: Option<(f32, f32)> = None;
            for point in 0..POINTS {
                let x = point as f32 / (POINTS - 1) as f32 * WIDTH;
                let t = elapsed + point as f32 * 0.02;
                let value = 0.5
                    + 0.25 * (t * (1.1 + series as f32 * 0.33) + series as f32).sin()
                    + 0.05 * (t * 6.0).sin();
                let y = (1.0 - value.clamp(0.0, 1.0)) * HEIGHT;
                if let Some((px, py)) = previous {
                    commands.push(DrawCommand::Line {
                        x1: px,
                        y1: py,
                        x2: x,
                        y2: y,
                        width: 2.0,
                        color,
                    });
                }
                previous = Some((x, y));
            }
        }

        column(context)
            .fill()
            .padding(30.0)
            .gap(10.0)
            .children(|context| {
                text(context, "Streaming chart").font_size(20.0).bold();
                text(
                    context,
                    "Three deterministic series, advanced from the frame clock.",
                )
                .font_size(12.0)
                .color(theme.muted);
                container(context).size(WIDTH, HEIGHT).draw(commands);
            });
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Streaming chart".to_string(),
            width: 1000.0,
            height: 560.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
