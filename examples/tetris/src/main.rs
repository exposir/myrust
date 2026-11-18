//! The native falling-block window.

use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use example_tetris::{Game, Kind, Pcg32, SavedGame, Step, COLS, ROWS};
use myrust_core::{
    app::{App, WindowConfig, WindowContent},
    ui::{
        column, container, row, text, AlignItems, Color, Context, DrawCommand, Instant,
        JustifyContent, Key, NativeUi, Radius,
    },
};
use serde::{Deserialize, Serialize};

const PAGE_WIDTH: f32 = 500.0;
const PAGE_HEIGHT: f32 = 1190.0;
const CELL_GAP: f32 = 2.0;

const PAGE_BG: Color = Color::rgb(250, 248, 239);
const INK: Color = Color::rgb(119, 110, 101);
const BOARD_BG: Color = Color::rgb(47, 43, 38);
const CELL_BG: Color = Color::rgb(59, 54, 48);
const GHOST: Color = Color::rgb(78, 71, 62);
const BUTTON_BG: Color = Color::rgb(143, 122, 102);
const LIGHT_INK: Color = Color::rgb(249, 246, 242);
const LABEL_INK: Color = Color::rgb(238, 228, 218);
const WHITE: Color = Color::rgb(255, 255, 255);

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    best: usize,
    #[serde(default)]
    game: Option<StoredGame>,
}

#[derive(Serialize, Deserialize)]
struct StoredGame {
    /// Row-major cells; 0 is empty, 1..7 are the seven piece kinds.
    #[serde(rename = "board")]
    board: Vec<u8>,
    piece: StoredPiece,
    next: u8,
    queue: Vec<u8>,
    score: usize,
    lines: usize,
    level: usize,
}

#[derive(Serialize, Deserialize)]
struct StoredPiece {
    kind: u8,
    rotation: u8,
    row: isize,
    col: isize,
}

fn kind_code(kind: Kind) -> u8 {
    match kind {
        Kind::I => 1,
        Kind::O => 2,
        Kind::T => 3,
        Kind::S => 4,
        Kind::Z => 5,
        Kind::J => 6,
        Kind::L => 7,
    }
}

fn kind_from(code: u8) -> Option<Kind> {
    Some(match code {
        1 => Kind::I,
        2 => Kind::O,
        3 => Kind::T,
        4 => Kind::S,
        5 => Kind::Z,
        6 => Kind::J,
        7 => Kind::L,
        _ => return None,
    })
}

impl StoredGame {
    fn from_game(game: &Game) -> Self {
        let saved = game.save();
        let mut board = Vec::with_capacity(ROWS * COLS);
        for row in saved.board {
            for cell in row {
                board.push(cell.map(kind_code).unwrap_or(0));
            }
        }
        Self {
            board,
            piece: StoredPiece {
                kind: kind_code(saved.piece.kind),
                rotation: saved.piece.rotation,
                row: saved.piece.row,
                col: saved.piece.col,
            },
            next: kind_code(saved.next),
            queue: saved.queue.into_iter().map(kind_code).collect(),
            score: saved.score,
            lines: saved.lines,
            level: saved.level,
        }
    }

    fn to_saved_game(&self) -> Option<SavedGame> {
        if self.board.len() != ROWS * COLS || self.queue.len() < 4 {
            return None;
        }
        let mut board = [[None; COLS]; ROWS];
        for (index, code) in self.board.iter().enumerate() {
            board[index / COLS][index % COLS] = match code {
                0 => None,
                other => Some(kind_from(*other)?),
            };
        }
        Some(SavedGame {
            board,
            piece: example_tetris::Piece {
                kind: kind_from(self.piece.kind)?,
                rotation: self.piece.rotation % 4,
                row: self.piece.row,
                col: self.piece.col,
            },
            next: kind_from(self.next)?,
            queue: self.queue.iter().map(|code| kind_from(*code)).collect::<Option<Vec<_>>>()?,
            score: self.score,
            lines: self.lines,
            level: self.level,
        })
    }
}

fn state_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("myrust-tetris")
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
        eprintln!("saving tetris: {error}");
        return;
    }
    let text = match serde_json::to_string(saved) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("saving tetris: {error}");
            return;
        }
    };
    let temporary = parent.join("state.json.tmp");
    if let Err(error) = fs::write(&temporary, text).and_then(|_| fs::rename(&temporary, &path)) {
        eprintln!("saving tetris: {error}");
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

fn piece_color(kind: Kind) -> Color {
    match kind {
        Kind::I => Color::rgb(95, 191, 226),
        Kind::O => Color::rgb(237, 194, 46),
        Kind::T => Color::rgb(178, 122, 216),
        Kind::S => Color::rgb(124, 196, 108),
        Kind::Z => Color::rgb(232, 106, 90),
        Kind::J => Color::rgb(86, 126, 214),
        Kind::L => Color::rgb(238, 160, 66),
    }
}

struct Tetris {
    game: Game,
    best: usize,
    last_drop: Instant,
    scale: f32,
    /// The last line-clear count, shown as a flash of text.
    flash: Option<(usize, Instant)>,
}

impl Tetris {
    fn new() -> Self {
        let saved = load_saved();
        let rng = seeded_rng();
        let game = saved
            .game
            .as_ref()
            .and_then(|game| game.to_saved_game())
            .and_then(|game| Game::restore(&game, rng.clone()))
            .unwrap_or_else(|| Game::new(rng));
        let best = saved.best.max(game.score);
        Self {
            game,
            best,
            last_drop: Instant::now(),
            scale: 1.0,
            flash: None,
        }
    }

    fn save(&self) {
        let game = (!self.game.over).then(|| StoredGame::from_game(&self.game));
        save_saved(&Saved {
            best: self.best,
            game,
        });
    }

    fn restart(&mut self) {
        self.game = Game::new(seeded_rng());
        self.last_drop = Instant::now();
        self.flash = None;
        self.save();
    }

    fn record(&mut self, step: Step) {
        if step.locked || step.lines > 0 || step.over {
            self.best = self.best.max(self.game.score);
            if step.lines > 0 {
                self.flash = Some((step.lines, Instant::now()));
            }
            self.save();
        }
    }

    fn advance(&mut self, now: Instant) {
        if self.game.over || self.game.paused {
            self.last_drop = now;
            return;
        }
        let interval = self.game.drop_interval_ms();
        let mut steps = 0;
        while steps < 4 {
            if now.duration_since(self.last_drop).as_secs_f32() * 1000.0 < interval {
                break;
            }
            self.last_drop = now;
            let step = self.game.step();
            self.record(step);
            steps += 1;
            if step.over {
                break;
            }
        }
    }
}

fn game_button(context: &mut Context, scale: f32, label: &str) -> bool {
    let button = context.clickable(&format!("tetris-button-{label}"));
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
        .padding_xy(14.0 * scale, 8.0 * scale)
        .background(BOARD_BG, 3.0 * scale)
        .align(AlignItems::Center)
        .column()
        .children(|context| {
            text(context, label)
                .font_size(12.0 * scale)
                .bold()
                .color(LABEL_INK)
                .single_line();
            text(context, &value.to_string())
                .font_size(22.0 * scale)
                .bold()
                .color(WHITE)
                .single_line();
        });
}

/// The small preview of the coming piece, drawn in its own box.
fn next_piece(context: &mut Context, scale: f32, kind: Kind) {
    let cell = 14.0 * scale;
    let gap = 2.0 * scale;
    container(context)
        .padding(6.0 * scale)
        .background(BOARD_BG, 4.0 * scale)
        .column()
        .gap(gap)
        .children(|context| {
            for row_index in 0..3 {
                row(context).gap(gap).children(|context| {
                    for col_index in 0..3 {
                        let filled = if kind == Kind::O {
                            row_index < 2 && col_index < 2
                        } else if kind == Kind::I {
                            row_index == 1
                        } else {
                            kind.rotation_cells(0)
                                .contains(&(row_index, col_index))
                        };
                        let color = if filled {
                            piece_color(kind)
                        } else {
                            Color::rgba(59, 54, 48, 140)
                        };
                        container(context)
                            .size(cell, cell)
                            .background(color, 2.0 * scale);
                    }
                });
            }
        });
}

fn view(context: &mut Context, state: &mut Tetris) {
    let canvas = context.available_size();
    let width = canvas.width.max(1.0);
    let height = canvas.height.max(1.0);
    let scale = ((width - 32.0) / PAGE_WIDTH)
        .min((height - 32.0) / PAGE_HEIGHT)
        .clamp(0.45, 1.6);
    state.scale = scale;

    state.advance(context.frame_time());

    if context.key_pressed(Key::ArrowLeft) {
        state.game.shift(-1);
    }
    if context.key_pressed(Key::ArrowRight) {
        state.game.shift(1);
    }
    if context.key_pressed(Key::ArrowDown) {
        let step = state.game.step();
        state.record(step);
        state.last_drop = context.frame_time();
    }
    if context.key_pressed(Key::ArrowUp) {
        state.game.rotate();
    }
    if context.key_pressed(Key::Space) {
        let step = state.game.hard_drop();
        state.record(step);
        state.last_drop = context.frame_time();
    }
    let typed = context.text_input();
    for character in typed.chars() {
        match character.to_ascii_lowercase() {
            'x' => {
                state.game.rotate();
            }
            'p' => state.game.toggle_pause(),
            'r' => state.restart(),
            _ => {}
        }
    }

    if !state.game.over && !state.game.paused {
        context.request_animation_frame();
    }

    let board_width = PAGE_WIDTH * scale;
    let gap = CELL_GAP * scale;
    let cell_width = (board_width - gap * (COLS as f32 + 1.0)) / COLS as f32;
    let board_height = cell_width * ROWS as f32 + gap * (ROWS as f32 + 1.0);
    let board_radius = (cell_width * 0.2).max(2.0);

    let ghost = state.game.ghost();
    let ghost_cells: Vec<(isize, isize)> = if state.game.over {
        Vec::new()
    } else {
        ghost.cells()
    };
    let active_cells: Vec<(isize, isize)> = state.game.piece.cells();

    let status = if state.game.over {
        "Stack topped out"
    } else if state.game.paused {
        "Paused — P to resume"
    } else if let Some((lines, at)) = state.flash {
        if at.elapsed().as_secs_f32() < 1.2 {
            match lines {
                1 => "Line clear!",
                2 => "Double!",
                3 => "Triple!",
                _ => "Tetris!",
            }
        } else {
            "Arrows to move, Up/X to turn, Space to drop"
        }
    } else {
        "Arrows to move, Up/X to turn, Space to drop"
    };

    container(context)
        .size(width, height)
        .align(AlignItems::Center)
        .justify(JustifyContent::Center)
        .background(PAGE_BG, 0.0)
        .children(|context| {
            column(context)
                .width(board_width)
                .gap(14.0 * scale)
                .children(|context| {
                    row(context)
                        .justify(JustifyContent::SpaceBetween)
                        .align(AlignItems::Center)
                        .children(|context| {
                            text(context, "Tetris")
                                .font_size(52.0 * scale)
                                .bold()
                                .color(INK)
                                .single_line();
                            row(context).gap(5.0 * scale).children(|context| {
                                score_box(context, scale, "SCORE", state.game.score);
                                score_box(context, scale, "BEST", state.best);
                                score_box(context, scale, "LINES", state.game.lines);
                                score_box(context, scale, "LEVEL", state.game.level);
                            });
                        });

                    // Controls sit on their own line and the hint sits under
                    // the board: sharing one row overflowed the column as soon
                    // as the hint wrapped.
                    row(context)
                        .justify(JustifyContent::FlexEnd)
                        .align(AlignItems::Center)
                        .gap(8.0 * scale)
                        .children(|context| {
                            next_piece(context, scale, state.game.next);
                            let pause_label = if state.game.paused { "Resume" } else { "Pause" };
                            if game_button(context, scale, pause_label) {
                                state.game.toggle_pause();
                            }
                            if game_button(context, scale, "New Game") {
                                state.restart();
                            }
                        });

                    // Same reasoning as the snake board: the grid is
                    // pixels, not 200 layout nodes per frame.
                    let mut cells = Vec::with_capacity(ROWS * COLS);
                    for row_index in 0..ROWS {
                        for col_index in 0..COLS {
                            let at = (row_index as isize, col_index as isize);
                            let settled = state.game.board[row_index][col_index];
                            let active = active_cells.contains(&at);
                            let ghosted = !active
                                && ghost_cells.contains(&at)
                                && state.game.piece.kind != Kind::O;
                            let color = if let Some(kind) = settled {
                                piece_color(kind)
                            } else if active {
                                piece_color(state.game.piece.kind)
                            } else if ghosted {
                                GHOST
                            } else {
                                CELL_BG
                            };
                            cells.push(DrawCommand::RoundedRect {
                                x: gap + col_index as f32 * (cell_width + gap),
                                y: gap + row_index as f32 * (cell_width + gap),
                                width: cell_width,
                                height: cell_width,
                                radius: Radius::all(board_radius),
                                color,
                            });
                        }
                    }
                    container(context)
                        .size(board_width, board_height)
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
    let mut app = App::new("myrust-tetris")?;
    let state = Rc::new(RefCell::new(Tetris::new()));
    let view_state = Rc::clone(&state);
    let native = NativeUi::new(move |context| {
        let mut state = view_state.borrow_mut();
        view(context, &mut state);
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Tetris".to_string(),
            width: 620.0,
            height: 900.0,
            min_size: Some((360.0, 620.0)),
            window_key: Some("main".to_string()),
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });
    Ok(app.run()?)
}
