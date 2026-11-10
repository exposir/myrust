//! A loading spinner drawn with the display list while an animation keeps
//! the frame loop alive.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    column, container, text, Animation, Color, DrawCommand, Easing, NativeUi, Theme,
};

struct State {
    start: Instant,
    /// Which end of the pump animation to run toward next.
    pump: f32,
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-loading")?;
    let state = Rc::new(RefCell::new(State {
        start: Instant::now(),
        pump: 1.0,
    }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let theme = Theme::system();
        let mut state = view_state.borrow_mut();
        let elapsed = context.frame_time().duration_since(state.start).as_secs_f32();

        // A scalar ping-pong keeps `needs_animation_frame` true; the spinner
        // itself is drawn from the frame clock.
        let pump = Animation::new(Duration::from_millis(400), Easing::Linear);
        let target = state.pump;
        let value = context.animate_with("spin-pump", target, pump);
        if (value - target).abs() < 0.001 {
            let next = 1.0 - target;
            context.animate_with("spin-pump", next, pump);
            state.pump = next;
        }

        let mut spinner = Vec::new();
        let center = 32.0_f32;
        let angle = elapsed * std::f32::consts::TAU;
        for index in 0..12 {
            let a = angle - index as f32 * std::f32::consts::TAU / 12.0;
            let alpha = 30 + ((11 - index) as u16 * 18) as u8;
            let color = Color::rgba(theme.accent.r, theme.accent.g, theme.accent.b, alpha);
            spinner.push(DrawCommand::Line {
                x1: center + a.cos() * 10.0,
                y1: center + a.sin() * 10.0,
                x2: center + a.cos() * 26.0,
                y2: center + a.sin() * 26.0,
                width: 3.0,
                color,
            });
        }

        column(context)
            .fill()
            .center()
            .gap(14.0)
            .children(|context| {
                container(context).size(64.0, 64.0).draw(spinner);
                text(context, "Loading…").font_size(13.0).color(theme.muted);
            });
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Loading".to_string(),
            width: 360.0,
            height: 260.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
