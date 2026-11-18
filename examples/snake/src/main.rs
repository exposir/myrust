//! The native snake window.

use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    rc::Rc,
};

use example_snake::{Cell, Direction, Game, Pcg32, SavedGame};
use myrust_core::{
    app::{App, WindowConfig, WindowContent},
    ui::{
        column, container, row, text, AlignItems, Color, Context, DrawCommand, Instant,
        JustifyContent, Key, NativeUi, Radius,
    },
};
use serde::{Deserialize, Serialize};

const PAGE_WIDTH: f32 = 500.0;
const PAGE_HEIGHT: f32 = 690.0;
const CELL_GAP: f32 = 6.0;

const PAGE_BG: Color = Color::rgb(250, 248, 239);
const INK: Color = Color::rgb(119, 110, 101);
const BOARD_BG: Color = Color::rgb(47, 43, 38);
const CELL_BG: Color = Color::rgb(59, 54, 48);
const SNAKE_BODY: Color = Color::rgb(143, 122, 102);
const SNAKE_HEAD: Color = Color::rgb(238, 228, 218);
const FOOD: Color = Color::rgb(246, 94, 59);
const BUTTON_BG: Color = Color::rgb(143, 122, 102);
const LIGHT_INK: Color = Color::rgb(249, 246, 242);
const LABEL_INK: Color = Color::rgb(238, 228, 218);

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    best: usize,
    #[serde(default)]
    game: Option<StoredGame>,
}

#[derive(Serialize, Deserialize)]
struct StoredGame {
    cols: usize,
    rows: usize,
    #[serde(rename = "snake")]
    snake: Vec<StoredCell>,
    direction: String,
    food: StoredCell,
    score: usize,
    started: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredCell {
    row: usize,
    col: usize,
}

fn stored_cell(cell: Cell) -> StoredCell {
    StoredCell {
        row: cell.row,
        col: cell.col,
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Right => "right",
        Direction::Down => "down",
        Direction::Left => "left",
    }
}

fn direction_from(name: &str) -> Option<Direction> {
    match name {
        "up" => Some(Direction::Up),
        "right" => Some(Direction::Right),
        "down" => Some(Direction::Down),
        "left" => Some(Direction::Left),
        _ => None,
    }
}

impl StoredGame {
    fn from_game(game: &Game) -> Self {
        let saved = game.save();
        Self {
            cols: saved.cols,
            rows: saved.rows,
            snake: saved.snake.into_iter().map(stored_cell).collect(),
            direction: direction_name(saved.direction).to_string(),
            food: stored_cell(saved.food),
            score: saved.score,
            started: saved.started,
        }
    }

    fn to_saved_game(&self) -> Option<SavedGame> {
        Some(SavedGame {
            cols: self.cols,
            rows: self.rows,
            snake: self
                .snake
                .iter()
                .map(|cell| Cell::new(cell.row, cell.col))
                .collect(),
            direction: direction_from(&self.direction)?,
            food: Cell::new(self.food.row, self.food.col),
            score: self.score,
            started: self.started,
        })
    }
}

fn state_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("myrust-snake")
        .join("state.json")
}

fn load_saved() -> Saved {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_saved(saved: &Saved) {
    let path = state_path();
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        eprintln!("saving snake: {error}");
        return;
    }
    let text = match serde_json::to_string(saved) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("saving snake: {error}");
            return;
        }
    };
    let temporary = parent.join("state.json.tmp");
    if let Err(error) = fs::write(&temporary, text).and_then(|_| fs::rename(&temporary, &path)) {
        eprintln!("saving snake: {error}");
        let _ = fs::remove_file(temporary);
    }
}

fn seeded_rng() -> Pcg32 {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1);
    Pcg32::new(seed, 0x9e37_79b9_7f4a_7c15)
}

struct Snake {
    game: Game,
    best: usize,
    paused: bool,
    last_tick: Instant,
    scale: f32,
}

impl Snake {
    fn new() -> Self {
        let saved = load_saved();
        let rng = seeded_rng();
        let game = saved
            .game
            .as_ref()
            .and_then(|game| game.to_saved_game())
            .and_then(|game| Game::restore(&game, rng.clone()))
            .unwrap_or_else(|| Game::new(example_snake::DEFAULT_COLS, example_snake::DEFAULT_ROWS, rng));
        let best = saved.best.max(game.score);
        Self {
            game,
            best,
            paused: false,
            last_tick: Instant::now(),
            scale: 1.0,
        }
    }

    fn running(&self) -> bool {
        self.game.started && !self.game.over && !self.game.won && !self.paused
    }

    fn save(&self) {
        let game = (self.game.started && !self.game.over && !self.game.won)
            .then(|| StoredGame::from_game(&self.game));
        save_saved(&Saved {
            best: self.best,
            game,
        });
    }

    fn restart(&mut self) {
        self.game = Game::new(
            example_snake::DEFAULT_COLS,
            example_snake::DEFAULT_ROWS,
            seeded_rng(),
        );
        self.paused = false;
        self.last_tick = Instant::now();
        self.save();
    }

    fn turn(&mut self, direction: Direction) {
        if self.game.turn(direction) {
            self.save();
        }
    }

    fn toggle_pause(&mut self) {
        if !self.game.started || self.game.over || self.game.won {
            return;
        }
        self.paused = !self.paused;
        self.last_tick = Instant::now();
    }

    /// Runs the ticks the clock says are due. Catches up after a stall but
    /// stops short of a spiral, so a suspended window does not replay a
    /// minute of moves.
    fn advance(&mut self, now: Instant) {
        if !self.running() {
            self.last_tick = now;
            return;
        }
        let interval = self.game.tick_interval_ms();
        let mut ticks = 0;
        while ticks < 4 {
            let elapsed = now.duration_since(self.last_tick).as_secs_f32() * 1000.0;
            if elapsed < interval {
                break;
            }
            self.last_tick = now;
            let result = self.game.tick();
            if result.ate {
                self.best = self.best.max(self.game.score);
            }
            if result.ate || result.died || result.won {
                self.save();
            }
            ticks += 1;
        }
    }
}

fn game_button(context: &mut Context, scale: f32, label: &str) -> bool {
    let button = context.clickable(&format!("snake-button-{label}"));
    let clicked = button.clicked;
    let background = if button.hovered {
        Color::rgb(151, 132, 113)
    } else {
        BUTTON_BG
    };
    button
        .height(40.0 * scale)
        .padding_x(20.0 * scale)
        .background(background, 3.0 * scale)
        .align(AlignItems::Center)
        .children(|context| {
            text(context, label)
                .font_size(18.0 * scale)
                .bold()
                .color(LIGHT_INK)
                .single_line();
        });
    clicked
}

fn score_box(context: &mut Context, scale: f32, label: &str, value: usize) {
    container(context)
        .padding_xy(16.0 * scale, 8.0 * scale)
        .background(BOARD_BG, 3.0 * scale)
        .align(AlignItems::Center)
        .column()
        .children(|context| {
            text(context, label)
                .font_size(13.0 * scale)
                .bold()
                .color(LABEL_INK)
                .single_line();
            text(context, &value.to_string())
                .font_size(25.0 * scale)
                .bold()
                .color(Color::rgb(255, 255, 255))
                .single_line();
        });
}

fn view(context: &mut Context, state: &mut Snake) {
    let canvas = context.available_size();
    let width = canvas.width.max(1.0);
    let height = canvas.height.max(1.0);
    let scale = ((width - 32.0) / PAGE_WIDTH)
        .min((height - 32.0) / PAGE_HEIGHT)
        .clamp(0.45, 1.6);
    state.scale = scale;

    state.advance(context.frame_time());

    // Arrow keys and hjkl/wasd; letters arrive as text, not named keys.
    let arrow = if context.key_pressed(Key::ArrowUp) {
        Some(Direction::Up)
    } else if context.key_pressed(Key::ArrowRight) {
        Some(Direction::Right)
    } else if context.key_pressed(Key::ArrowDown) {
        Some(Direction::Down)
    } else if context.key_pressed(Key::ArrowLeft) {
        Some(Direction::Left)
    } else {
        None
    };
    if let Some(direction) = arrow {
        state.turn(direction);
    }
    if context.key_pressed(Key::Space) {
        state.toggle_pause();
    }
    let typed = context.text_input();
    for character in typed.chars() {
        match character.to_ascii_lowercase() {
            'w' | 'k' => state.turn(Direction::Up),
            'd' | 'l' => state.turn(Direction::Right),
            's' | 'j' => state.turn(Direction::Down),
            'a' | 'h' => state.turn(Direction::Left),
            'p' => state.toggle_pause(),
            'r' => state.restart(),
            _ => {}
        }
    }

    if state.running() {
        context.request_animation_frame();
    }

    let board_width = PAGE_WIDTH * scale;
    let gap = CELL_GAP * scale;
    let cols = state.game.cols;
    let rows = state.game.rows;
    let cell_width = (board_width - gap * (cols as f32 + 1.0)) / cols as f32;
    let cell_radius = (cell_width * 0.22).max(2.0);
    let snake: Vec<(usize, usize)> = state
        .game
        .snake
        .iter()
        .map(|cell| (cell.row, cell.col))
        .collect();
    let food = (state.game.food.row, state.game.food.col);

    let status = if state.game.won {
        "You filled the board!"
    } else if state.game.over {
        "Game over"
    } else if state.paused {
        "Paused — Space to resume"
    } else if !state.game.started {
        "Press a direction to start"
    } else {
        "Arrows or WASD to steer, Space to pause, R to restart"
    };

    container(context)
        .size(width, height)
        .align(AlignItems::Center)
        .justify(JustifyContent::Center)
        .background(PAGE_BG, 0.0)
        .children(|context| {
            column(context)
                .width(board_width)
                .gap(16.0 * scale)
                .children(|context| {
                    row(context)
                        .justify(JustifyContent::SpaceBetween)
                        .align(AlignItems::Center)
                        .children(|context| {
                            text(context, "Snake")
                                .font_size(64.0 * scale)
                                .bold()
                                .color(INK)
                                .single_line();
                            row(context).gap(5.0 * scale).children(|context| {
                                score_box(context, scale, "SCORE", state.game.score);
                                score_box(context, scale, "BEST", state.best);
                                score_box(context, scale, "LENGTH", state.game.snake.len());
                            });
                        });

                    // Controls sit on their own line and the hint sits under
                    // the board: sharing one row overflowed the column as soon
                    // as the hint wrapped.
                    row(context)
                        .justify(JustifyContent::FlexEnd)
                        .gap(8.0 * scale)
                        .children(|context| {
                            let pause_label = if state.paused { "Resume" } else { "Pause" };
                            if game_button(context, scale, pause_label) {
                                state.toggle_pause();
                            }
                            if game_button(context, scale, "New Game") {
                                state.restart();
                            }
                        });

                    // The board is pixels, not widgets: 400 layout nodes
                    // per frame would cost far more than the fills.
                    let mut cells = Vec::with_capacity(rows * cols);
                    for row_index in 0..rows {
                        for col_index in 0..cols {
                            let is_food = (row_index, col_index) == (food.0, food.1);
                            let segment = snake
                                .iter()
                                .position(|&cell| cell == (row_index, col_index));
                            let (color, radius) = if is_food {
                                (FOOD, cell_width / 2.0)
                            } else if let Some(index) = segment {
                                if index == 0 {
                                    (SNAKE_HEAD, cell_radius)
                                } else {
                                    (SNAKE_BODY, cell_radius)
                                }
                            } else {
                                (CELL_BG, cell_radius * 0.6)
                            };
                            cells.push(DrawCommand::RoundedRect {
                                x: gap + col_index as f32 * (cell_width + gap),
                                y: gap + row_index as f32 * (cell_width + gap),
                                width: cell_width,
                                height: cell_width,
                                radius: Radius::all(radius),
                                color,
                            });
                        }
                    }
                    container(context)
                        .size(board_width, board_width)
                        .background(BOARD_BG, 6.0 * scale)
                        .draw(cells);

                    text(context, status)
                        .font_size(15.0 * scale)
                        .color(INK)
                        .max_width(board_width);
                });
        });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new("myrust-snake")?;
    let state = Rc::new(RefCell::new(Snake::new()));
    let view_state = Rc::clone(&state);
    let native = NativeUi::new(move |context| {
        let mut state = view_state.borrow_mut();
        view(context, &mut state);
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Snake".to_string(),
            width: 560.0,
            height: 760.0,
            min_size: Some((340.0, 520.0)),
            window_key: Some("main".to_string()),
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });
    Ok(app.run()?)
}
