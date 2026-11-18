//! The rules of the snake game, with no rendering or platform code.

use std::collections::VecDeque;

pub const DEFAULT_COLS: usize = 20;
pub const DEFAULT_ROWS: usize = 20;
/// The snake starts this long, laid out left to right and heading right.
pub const START_LENGTH: usize = 3;
pub const BASE_TICK_MS: f32 = 140.0;
pub const FASTEST_TICK_MS: f32 = 70.0;
/// Every this many foods the tick shortens by `SPEED_STEP_MS`.
pub const FOODS_PER_SPEEDUP: usize = 4;
pub const SPEED_STEP_MS: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    pub fn vector(self) -> (isize, isize) {
        match self {
            Direction::Up => (-1, 0),
            Direction::Right => (0, 1),
            Direction::Down => (1, 0),
            Direction::Left => (0, -1),
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Direction::Up => Direction::Down,
            Direction::Right => Direction::Left,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
}

impl Cell {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

/// A lightweight, deterministic Permuted Congruential Generator (PCG32)
/// that needs no external dependencies or libc.
///
/// The same generator as the 2048 example, so a seed plays out the same way
/// in both games.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub fn new(state: u64, inc: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (inc << 1) | 1,
        };
        rng.state = rng.state.wrapping_add(state);
        rng.step();
        rng
    }

    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(self.inc);
    }

    pub fn next_u32(&mut self) -> u32 {
        let old_state = self.state;
        self.step();
        let xor_shifted = (((old_state >> 18) ^ old_state) >> 27) as u32;
        let rot = (old_state >> 59) as u32;
        (xor_shifted >> rot) | (xor_shifted << ((!rot).wrapping_add(1) & 31))
    }

    pub fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u32() as usize) % upper
    }
}

impl Default for Pcg32 {
    fn default() -> Self {
        Self::new(1, 2)
    }
}

/// What one tick did, so a caller can drive sound, score flashes or redraws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// The snake moved at all; false while the game waits for its first turn.
    pub moved: bool,
    pub ate: bool,
    pub died: bool,
    pub won: bool,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub cols: usize,
    pub rows: usize,
    /// Head first, tail last.
    pub snake: VecDeque<Cell>,
    pub direction: Direction,
    /// The turn the player asked for since the last tick, applied then.
    queued: Option<Direction>,
    pub food: Cell,
    pub score: usize,
    pub over: bool,
    pub won: bool,
    /// False until the player picks a direction, so the board holds still
    /// while they get their bearings.
    pub started: bool,
    pub rng: Pcg32,
}

impl Game {
    /// Builds a game on a `cols` by `rows` board. The board is widened to fit
    /// the starting snake rather than panicking on a silly size.
    pub fn new(cols: usize, rows: usize, rng: Pcg32) -> Self {
        let cols = cols.max(START_LENGTH + 2);
        let rows = rows.max(START_LENGTH + 2);
        let row = rows / 2;
        let head_col = cols / 2;
        let mut snake = VecDeque::with_capacity(START_LENGTH);
        for step in 0..START_LENGTH {
            snake.push_back(Cell::new(row, head_col - step));
        }
        let mut game = Self {
            cols,
            rows,
            snake,
            direction: Direction::Right,
            queued: None,
            food: Cell::new(0, 0),
            score: 0,
            over: false,
            won: false,
            started: false,
            rng,
        };
        game.spawn_food();
        game
    }

    pub fn tick_interval_ms(&self) -> f32 {
        let speedups = (self.score / FOODS_PER_SPEEDUP) as f32;
        (BASE_TICK_MS - speedups * SPEED_STEP_MS).max(FASTEST_TICK_MS)
    }

    pub fn occupied(&self, cell: Cell) -> bool {
        self.snake.contains(&cell)
    }

    /// Queues a turn for the next tick.
    ///
    /// The turn is checked against the direction actually in effect, not the
    /// queued one, so two presses inside one tick cannot add up to a reversal:
    /// moving right, "up" then "left" leaves the snake turning up.
    ///
    /// Pressing the direction it already faces starts the game without a
    /// turn — the player's first press should not be swallowed.
    pub fn turn(&mut self, direction: Direction) -> bool {
        if self.over || self.won {
            return false;
        }
        if direction == self.direction.opposite() {
            return false;
        }
        if direction != self.direction {
            self.queued = Some(direction);
        }
        self.started = true;
        true
    }

    /// Advances the snake one cell. Does nothing until the player starts.
    pub fn tick(&mut self) -> Tick {
        if self.over || self.won || !self.started {
            return Tick::default();
        }
        if let Some(direction) = self.queued.take() {
            self.direction = direction;
        }

        let head = *self.snake.front().expect("the snake never empties");
        let (dr, dc) = self.direction.vector();
        let row = head.row as isize + dr;
        let col = head.col as isize + dc;
        if row < 0 || col < 0 || row >= self.rows as isize || col >= self.cols as isize {
            self.over = true;
            return Tick {
                moved: true,
                died: true,
                ..Tick::default()
            };
        }

        let next = Cell::new(row as usize, col as usize);
        let eating = next == self.food;
        // The tail moves away on this tick, so its cell is free to enter.
        // Food never sits on the snake, so an eating move cannot be that case.
        let tail = *self.snake.back().expect("the snake never empties");
        if !eating && next != tail && self.occupied(next) {
            self.over = true;
            return Tick {
                moved: true,
                died: true,
                ..Tick::default()
            };
        }

        self.snake.push_front(next);
        if eating {
            self.score += 1;
            self.spawn_food();
            return Tick {
                moved: true,
                ate: true,
                won: self.won,
                ..Tick::default()
            };
        }
        self.snake.pop_back();
        Tick {
            moved: true,
            ..Tick::default()
        }
    }

    /// Places the food on a free cell, or declares the board full.
    fn spawn_food(&mut self) {
        let mut free = Vec::with_capacity(self.cols * self.rows);
        for row in 0..self.rows {
            for col in 0..self.cols {
                let cell = Cell::new(row, col);
                if !self.occupied(cell) {
                    free.push(cell);
                }
            }
        }
        match free.len() {
            0 => self.won = true,
            count => self.food = free[self.rng.gen_range(count)],
        }
    }

    pub fn save(&self) -> SavedGame {
        SavedGame {
            cols: self.cols,
            rows: self.rows,
            snake: self.snake.iter().copied().collect(),
            direction: self.direction,
            food: self.food,
            score: self.score,
            started: self.started,
        }
    }

    /// Rebuilds a saved game, rejecting a board that could not have been
    /// played: an out-of-bounds or doubled segment, or food under the snake.
    pub fn restore(saved: &SavedGame, rng: Pcg32) -> Option<Self> {
        if saved.snake.len() < 2 || saved.cols < START_LENGTH + 2 || saved.rows < START_LENGTH + 2
        {
            return None;
        }
        let mut seen = vec![false; saved.cols * saved.rows];
        for cell in &saved.snake {
            if cell.row >= saved.rows || cell.col >= saved.cols {
                return None;
            }
            let index = cell.row * saved.cols + cell.col;
            if seen[index] {
                return None;
            }
            seen[index] = true;
        }
        if saved.food.row >= saved.rows || saved.food.col >= saved.cols {
            return None;
        }
        if seen[saved.food.row * saved.cols + saved.food.col] {
            return None;
        }
        Some(Self {
            cols: saved.cols,
            rows: saved.rows,
            snake: saved.snake.iter().copied().collect(),
            direction: saved.direction,
            queued: None,
            food: saved.food,
            score: saved.score,
            over: false,
            won: false,
            started: saved.started,
            rng,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedGame {
    pub cols: usize,
    pub rows: usize,
    pub snake: Vec<Cell>,
    pub direction: Direction,
    pub food: Cell,
    pub score: usize,
    pub started: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rng() -> Pcg32 {
        Pcg32::new(7, 11)
    }

    /// A board built by hand, so each test states only what it is about.
    fn game_with(
        cols: usize,
        rows: usize,
        snake: &[(usize, usize)],
        direction: Direction,
        food: (usize, usize),
    ) -> Game {
        let mut game = Game::new(cols, rows, test_rng());
        game.snake = snake
            .iter()
            .map(|&(row, col)| Cell::new(row, col))
            .collect();
        game.direction = direction;
        game.queued = None;
        game.food = Cell::new(food.0, food.1);
        game.score = 0;
        game.over = false;
        game.won = false;
        game.started = true;
        game
    }

    fn cells(game: &Game) -> Vec<(usize, usize)> {
        game.snake.iter().map(|c| (c.row, c.col)).collect()
    }

    #[test]
    fn starts_with_a_snake_and_food_apart() {
        let game = Game::new(DEFAULT_COLS, DEFAULT_ROWS, test_rng());
        assert_eq!(game.snake.len(), START_LENGTH);
        assert!(!game.started);
        assert!(!game.occupied(game.food));
        assert!(game.cols >= START_LENGTH + 2);
    }

    #[test]
    fn waits_for_the_first_turn() {
        let mut game = Game::new(8, 8, test_rng());
        assert_eq!(game.tick(), Tick::default());
        assert_eq!(game.snake.len(), START_LENGTH);
        assert!(game.turn(Direction::Up));
        let tick = game.tick();
        assert!(tick.moved);
        assert_eq!(game.direction, Direction::Up);
    }

    #[test]
    fn a_turn_back_on_itself_is_refused() {
        let mut game = game_with(8, 8, &[(4, 4), (4, 3), (4, 2)], Direction::Right, (0, 0));
        assert!(!game.turn(Direction::Left));
        game.tick();
        assert_eq!(game.direction, Direction::Right);
        assert!(!game.over);
    }

    #[test]
    fn pressing_the_current_direction_just_starts() {
        let mut game = Game::new(8, 8, test_rng());
        assert!(!game.started);
        assert!(game.turn(Direction::Right));
        assert!(game.started);
        let tick = game.tick();
        assert!(tick.moved);
        assert_eq!(game.direction, Direction::Right);
    }

    #[test]
    fn two_presses_in_one_tick_cannot_reverse() {
        // Moving right, "up" is queued, then "left" is checked against the
        // direction in effect (right) and refused — not against "up", which
        // would let the snake turn back into its own neck.
        let mut game = game_with(8, 8, &[(4, 4), (4, 3), (4, 2)], Direction::Right, (0, 0));
        assert!(game.turn(Direction::Up));
        assert!(!game.turn(Direction::Left));
        game.tick();
        assert_eq!(game.direction, Direction::Up);
        assert_eq!(cells(&game), vec![(3, 4), (4, 4), (4, 3)]);
    }

    #[test]
    fn moving_keeps_its_length() {
        let mut game = game_with(8, 8, &[(4, 4), (4, 3), (4, 2)], Direction::Right, (0, 0));
        game.tick();
        assert_eq!(cells(&game), vec![(4, 5), (4, 4), (4, 3)]);
        assert_eq!(game.score, 0);
    }

    #[test]
    fn eating_grows_and_scores() {
        let mut game = game_with(8, 8, &[(4, 4), (4, 3), (4, 2)], Direction::Right, (4, 5));
        let tick = game.tick();
        assert!(tick.ate);
        assert_eq!(game.snake.len(), 4);
        assert_eq!(game.score, 1);
        assert!(!game.occupied(game.food));
    }

    #[test]
    fn walls_kill() {
        let mut game = game_with(8, 8, &[(0, 4), (0, 3), (0, 2)], Direction::Up, (6, 6));
        let tick = game.tick();
        assert!(tick.died);
        assert!(game.over);
    }

    #[test]
    fn the_tail_cell_is_free_to_enter() {
        // A tight U: heading down into the cell the tail is leaving.
        let mut game = game_with(
            8,
            8,
            &[(4, 4), (4, 5), (5, 5), (5, 4)],
            Direction::Down,
            (0, 0),
        );
        let tick = game.tick();
        assert!(!tick.died);
        assert!(!game.over);
        assert_eq!(cells(&game), vec![(5, 4), (4, 4), (4, 5), (5, 5)]);
    }

    #[test]
    fn running_into_the_body_kills() {
        // Heading left into its own middle: the tail is elsewhere.
        let mut game = game_with(
            8,
            8,
            &[(4, 4), (4, 5), (5, 5), (5, 4), (5, 3)],
            Direction::Down,
            (0, 0),
        );
        game.direction = Direction::Down;
        game.tick();
        assert!(game.over);
        assert!(game.snake.len() == 5);
    }

    #[test]
    fn filling_the_board_wins() {
        // Two free cells on a 5×5 board. The first food fills one and moves
        // the next food into the last; the following move fills the board.
        let mut game = game_with(5, 5, &[(0, 2), (0, 1), (0, 0)], Direction::Right, (0, 3));
        // Occupy everything else by hand: rows 1..4 entirely.
        let mut snake: VecDeque<Cell> = VecDeque::new();
        snake.push_back(Cell::new(0, 2));
        snake.push_back(Cell::new(0, 1));
        snake.push_back(Cell::new(0, 0));
        for row in (1..5).rev() {
            for col in (0..5).rev() {
                snake.push_back(Cell::new(row, col));
            }
        }
        game.snake = snake;
        game.food = Cell::new(0, 3);
        assert_eq!(game.snake.len(), 23);

        let first = game.tick();
        assert!(first.ate);
        assert!(!game.won, "one cell is still free");
        assert_eq!(game.food, Cell::new(0, 4));

        let second = game.tick();
        assert!(second.ate);
        assert!(second.won);
        assert!(game.won);
        assert_eq!(game.snake.len(), 25);
    }

    #[test]
    fn speed_rises_with_the_score() {
        let mut game = Game::new(8, 8, test_rng());
        assert_eq!(game.tick_interval_ms(), BASE_TICK_MS);
        game.score = FOODS_PER_SPEEDUP;
        assert_eq!(game.tick_interval_ms(), BASE_TICK_MS - SPEED_STEP_MS);
        game.score = 10_000;
        assert_eq!(game.tick_interval_ms(), FASTEST_TICK_MS);
    }

    #[test]
    fn save_and_restore_round_trip() {
        let mut game = game_with(9, 7, &[(3, 4), (3, 3), (2, 3)], Direction::Left, (0, 0));
        game.score = 3;
        game.started = true;
        let saved = game.save();
        let restored = Game::restore(&saved, test_rng()).expect("a legal board restores");
        assert_eq!(restored.save(), saved);
        assert_eq!(restored.tick_interval_ms(), game.tick_interval_ms());
    }

    #[test]
    fn restore_rejects_impossible_boards() {
        let mut saved = SavedGame {
            cols: 8,
            rows: 8,
            snake: vec![Cell::new(4, 4), Cell::new(4, 4)],
            direction: Direction::Right,
            food: Cell::new(0, 0),
            score: 0,
            started: true,
        };
        assert!(Game::restore(&saved, test_rng()).is_none(), "a doubled segment");
        saved.snake = vec![Cell::new(4, 4), Cell::new(4, 3)];
        saved.food = Cell::new(4, 4);
        assert!(Game::restore(&saved, test_rng()).is_none(), "food under the snake");
        saved.food = Cell::new(8, 0);
        assert!(Game::restore(&saved, test_rng()).is_none(), "food off the board");
        saved.food = Cell::new(0, 0);
        saved.snake = vec![Cell::new(4, 4)];
        assert!(Game::restore(&saved, test_rng()).is_none(), "a one-cell snake");
    }
}
