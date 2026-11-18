//! The native 2048 window.

use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::PathBuf,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use example_2048::{Direction, Game, Pcg32, SavedGame, Tile};
use myrust_core::{
    app::{App, WindowConfig, WindowContent},
    ui::{
        column, container, row, text, AlignItems, Color, Context, Instant, JustifyContent, Key,
        NativeUi, Point, Transform,
    },
};
use serde::{Deserialize, Serialize};

const PAGE_WIDTH: f32 = 500.0;
const PAGE_HEIGHT: f32 = 790.0;
const SLIDE_MILLIS: f32 = 100.0;
const DRAG_MIN: f32 = 10.0;

const PAGE_BG: Color = Color::rgb(250, 248, 239);
const INK: Color = Color::rgb(119, 110, 101);
const BOARD_BG: Color = Color::rgb(187, 173, 160);
const CELL_BG: Color = Color::rgba(238, 228, 218, 89);
const WHITE: Color = Color::rgb(255, 255, 255);
const LABEL_INK: Color = Color::rgb(238, 228, 218);
const LIGHT_INK: Color = Color::rgb(249, 246, 242);
const BUTTON_BG: Color = Color::rgb(143, 122, 102);

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    best: usize,
    #[serde(default)]
    game: Option<StoredGame>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGame {
    tiles: Vec<StoredTile>,
    score: usize,
    won: bool,
    keep_playing: bool,
    next_id: usize,
}

#[derive(Serialize, Deserialize)]
struct StoredTile {
    id: usize,
    value: usize,
    row: usize,
    col: usize,
}

impl StoredGame {
    fn from_game(game: &Game) -> Self {
        let saved = game.save();
        Self {
            tiles: saved
                .tiles
                .into_iter()
                .map(|tile| StoredTile {
                    id: tile.id,
                    value: tile.value,
                    row: tile.row,
                    col: tile.col,
                })
                .collect(),
            score: saved.score,
            won: saved.won,
            keep_playing: saved.keep_playing,
            next_id: saved.next_id,
        }
    }

    fn to_saved_game(&self) -> SavedGame {
        SavedGame {
            tiles: self
                .tiles
                .iter()
                .map(|tile| Tile {
                    id: tile.id,
                    value: tile.value,
                    row: tile.row,
                    col: tile.col,
                    from_merge: false,
                })
                .collect(),
            score: self.score,
            won: self.won,
            keep_playing: self.keep_playing,
            next_id: self.next_id,
        }
    }
}

fn state_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("myrust-2048")
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
        eprintln!("saving 2048: {error}");
        return;
    }
    let text = match serde_json::to_string(saved) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("saving 2048: {error}");
            return;
        }
    };
    let temporary = parent.join("state.json.tmp");
    if let Err(error) = fs::write(&temporary, text).and_then(|_| fs::rename(&temporary, &path)) {
        eprintln!("saving 2048: {error}");
        let _ = fs::remove_file(temporary);
    }
}

fn seeded_rng() -> Pcg32 {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x2048);
    Pcg32::new(seed, 0x9e37_79b9_7f4a_7c15)
}

struct Game2048 {
    game: Game,
    best: usize,
    pointer_was_down: bool,
    press_start: Option<Point>,
    scale: f32,
    move_time: Option<Instant>,
    previous_positions: HashMap<usize, (usize, usize)>,
}

impl Game2048 {
    fn new() -> Self {
        let saved = load_saved();
        let rng = seeded_rng();
        let game = saved
            .game
            .as_ref()
            .and_then(|game| Game::restore(&game.to_saved_game(), rng.clone()))
            .unwrap_or_else(|| Game::new(rng));
        Self {
            best: saved.best.max(game.score),
            game,
            pointer_was_down: false,
            press_start: None,
            scale: 1.0,
            move_time: None,
            previous_positions: HashMap::new(),
        }
    }

    fn save(&self) {
        let game = (!self.game.terminated()).then(|| StoredGame::from_game(&self.game));
        save_saved(&Saved {
            best: self.best,
            game,
        });
    }

    fn move_tiles(&mut self, direction: Direction) {
        if self.game.terminated() {
            return;
        }
        let mut previous = HashMap::new();
        for tile in &self.game.tiles {
            previous.insert(tile.id, (tile.row, tile.col));
        }
        let (moved, _) = self.game.move_dir(direction);
        if !moved {
            return;
        }
        self.previous_positions = previous;
        self.move_time = Some(Instant::now());
        self.best = self.best.max(self.game.score);
        self.save();
    }

    fn restart(&mut self) {
        self.game = Game::new(seeded_rng());
        self.press_start = None;
        self.move_time = None;
        self.previous_positions.clear();
        self.save();
    }

    fn keep_going(&mut self) {
        self.game.keep_playing = true;
        self.save();
    }

    fn handle_pointer(&mut self, context: &Context, over_board: bool) {
        let down = context.pointer_down();
        let position = context.pointer_position();
        if !down && self.pointer_was_down {
            if let Some(start) = self.press_start.take() {
                if let Some(end) = position {
                    if let Some(direction) = swipe_direction(
                        end.x - start.x,
                        end.y - start.y,
                        DRAG_MIN * self.scale,
                    ) {
                        self.move_tiles(direction);
                    }
                }
            }
        } else if down && !self.pointer_was_down && over_board {
            self.press_start = position;
        }
        self.pointer_was_down = down;
    }

    fn handle_keyboard(&mut self, context: &Context) {
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
            self.move_tiles(direction);
            return;
        }

        let typed = context.text_input();
        if typed
            .chars()
            .any(|character| character.eq_ignore_ascii_case(&'r'))
        {
            self.restart();
            return;
        }
        for character in typed.chars() {
            let direction = match character.to_ascii_lowercase() {
                'w' | 'k' => Direction::Up,
                'd' | 'l' => Direction::Right,
                's' | 'j' => Direction::Down,
                'a' | 'h' => Direction::Left,
                _ => continue,
            };
            self.move_tiles(direction);
        }
    }
}

fn swipe_direction(dx: f32, dy: f32, minimum: f32) -> Option<Direction> {
    let ax = dx.abs();
    let ay = dy.abs();
    if ax.max(ay) < minimum {
        return None;
    }
    Some(if ax > ay {
        if dx > 0.0 {
            Direction::Right
        } else {
            Direction::Left
        }
    } else if dy > 0.0 {
        Direction::Down
    } else {
        Direction::Up
    })
}

fn tile_colors(value: usize) -> (Color, Color) {
    let background = match value {
        2 => Color::rgb(238, 228, 218),
        4 => Color::rgb(237, 224, 200),
        8 => Color::rgb(242, 177, 121),
        16 => Color::rgb(245, 149, 99),
        32 => Color::rgb(246, 124, 95),
        64 => Color::rgb(246, 94, 59),
        128 => Color::rgb(237, 207, 114),
        256 => Color::rgb(237, 204, 97),
        512 => Color::rgb(237, 200, 80),
        1024 => Color::rgb(237, 197, 63),
        2048 => Color::rgb(237, 194, 46),
        _ => Color::rgb(60, 58, 50),
    };
    let foreground = if value <= 4 { INK } else { LIGHT_INK };
    (background, foreground)
}

fn tile_font_size(digits: usize) -> f32 {
    match digits {
        0..=2 => 55.0,
        3 => 45.0,
        4 => 35.0,
        5 => 30.0,
        _ => 24.0,
    }
}

fn game_button(context: &mut Context, scale: f32, label: &str) -> bool {
    let button = context.clickable(&format!("2048-button-{label}"));
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

fn view(context: &mut Context, state: &mut Game2048) {
    let canvas = context.available_size();
    let width = canvas.width.max(1.0);
    let height = canvas.height.max(1.0);
    let scale = ((width - 32.0) / PAGE_WIDTH)
        .min((height - 32.0) / PAGE_HEIGHT)
        .clamp(0.45, 1.6);
    state.scale = scale;

    let over_board = context.hover("game-2048-board");
    state.handle_keyboard(context);
    state.handle_pointer(context, over_board);

    let elapsed = state
        .move_time
        .map_or(SLIDE_MILLIS, |started| started.elapsed().as_secs_f32() * 1000.0);
    let animating = elapsed < SLIDE_MILLIS;
    if animating {
        context.request_animation_frame();
    }
    let progress = if animating {
        let fraction = elapsed / SLIDE_MILLIS;
        fraction * fraction * (3.0 - 2.0 * fraction)
    } else {
        1.0
    };

    container(context)
        .size(width, height)
        .align(AlignItems::Center)
        .justify(JustifyContent::Center)
        .background(PAGE_BG, 0.0)
        .children(|context| {
            column(context)
                .width(PAGE_WIDTH * scale)
                .gap(16.0 * scale)
                .children(|context| {
                    row(context)
                        .justify(JustifyContent::SpaceBetween)
                        .align(AlignItems::Center)
                        .children(|context| {
                            text(context, "2048")
                                .font_size(80.0 * scale)
                                .bold()
                                .color(INK)
                                .single_line();
                            row(context).gap(5.0 * scale).children(|context| {
                                score_box(context, scale, "SCORE", state.game.score);
                                score_box(context, scale, "BEST", state.best);
                            });
                        });

                    row(context)
                        .justify(JustifyContent::SpaceBetween)
                        .align(AlignItems::Center)
                        .gap(12.0 * scale)
                        .children(|context| {
                            text(context, "Join the numbers and get to the 2048 tile!")
                                .font_size(16.0 * scale)
                                .color(INK)
                                .max_width((PAGE_WIDTH * scale - 8.0).max(120.0));
                            if game_button(context, scale, "New Game") {
                                state.restart();
                            }
                        });

                    let board_width = PAGE_WIDTH * scale;
                    let gap = 15.0 * scale;
                    let cell_width = (board_width - gap * 5.0) / 4.0;
                    let cell_step = cell_width + gap;
                    container(context)
                        .size(board_width, board_width)
                        .background(BOARD_BG, 6.0 * scale)
                        .padding(gap)
                        .column()
                        .gap(gap)
                        .interactive("game-2048-board")
                        .children(|context| {
                            let cells = state.game.grid();
                            for row_index in 0..4 {
                                row(context).gap(gap).children(|context| {
                                    for column_index in 0..4 {
                                        container(context)
                                            .size(cell_width, cell_width)
                                            .background(CELL_BG, 3.0 * scale)
                                            .align(AlignItems::Center)
                                            .justify(JustifyContent::Center)
                                            .children(|context| {
                                                let Some(tile) =
                                                    cells[row_index][column_index].as_ref()
                                                else {
                                                    return;
                                                };
                                                let (offset_x, offset_y) = if animating {
                                                    state
                                                        .previous_positions
                                                        .get(&tile.id)
                                                        .map_or((0.0, 0.0), |&(row, column)| {
                                                            (
                                                                (column as f32
                                                                    - column_index as f32)
                                                                    * (1.0 - progress)
                                                                    * cell_step,
                                                                (row as f32 - row_index as f32)
                                                                    * (1.0 - progress)
                                                                    * cell_step,
                                                            )
                                                        })
                                                } else {
                                                    (0.0, 0.0)
                                                };
                                                let (background, foreground) =
                                                    tile_colors(tile.value);
                                                let value = tile.value.to_string();
                                                context.transform(
                                                    Transform::translate(Point::new(
                                                        offset_x, offset_y,
                                                    )),
                                                    |context| {
                                                        container(context)
                                                            .size(cell_width, cell_width)
                                                            .background(
                                                                background,
                                                                3.0 * scale,
                                                            )
                                                            .align(AlignItems::Center)
                                                            .justify(JustifyContent::Center)
                                                            .children(|context| {
                                                                text(context, &value)
                                                                    .font_size(
                                                                        tile_font_size(value.len())
                                                                            * scale,
                                                                    )
                                                                    .bold()
                                                                    .color(foreground)
                                                                    .single_line();
                                                            });
                                                    },
                                                );
                                            });
                                    }
                                });
                            }
                        });

                    if state.game.terminated() {
                        row(context)
                            .justify(JustifyContent::SpaceBetween)
                            .align(AlignItems::Center)
                            .children(|context| {
                                if state.game.over {
                                    text(context, "Game over!")
                                        .font_size(24.0 * scale)
                                        .bold()
                                        .color(INK);
                                    if game_button(context, scale, "Try again") {
                                        state.restart();
                                    }
                                } else {
                                    text(context, "You win!")
                                        .font_size(24.0 * scale)
                                        .bold()
                                        .color(INK);
                                    row(context).gap(8.0 * scale).children(|context| {
                                        if game_button(context, scale, "Keep going") {
                                            state.keep_going();
                                        }
                                        if game_button(context, scale, "Try again") {
                                            state.restart();
                                        }
                                    });
                                }
                            });
                    }

                    text(
                        context,
                        "How to play: Use the arrow keys, or drag, to move the tiles. When two tiles with the same number touch, they merge into one!",
                    )
                    .font_size(15.0 * scale)
                    .color(INK)
                    .max_width(PAGE_WIDTH * scale);
                });
        });
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
                .color(WHITE)
                .single_line();
        });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new("myrust-2048")?;
    let state = Rc::new(RefCell::new(Game2048::new()));
    let view_state = Rc::clone(&state);
    let native = NativeUi::new(move |context| {
        let mut state = view_state.borrow_mut();
        view(context, &mut state);
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "2048".to_string(),
            width: 560.0,
            height: 840.0,
            min_size: Some((320.0, 500.0)),
            window_key: Some("main".to_string()),
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });
    Ok(app.run()?)
}
