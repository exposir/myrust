//! The rules of a Tetris-style falling-block game, with no rendering or
//! platform code.
//!
//! The pieces are the seven tetrominoes, each rotated inside its own square
//! matrix. Rotation uses the simple offset kicks that the original game
//! popularised: try turning in place, then nudge one cell either way.

pub const COLS: usize = 10;
pub const ROWS: usize = 20;
/// Lines cleared per level; each level makes the piece fall faster.
pub const LINES_PER_LEVEL: usize = 10;
pub const BASE_DROP_MS: f32 = 800.0;
pub const MIN_DROP_MS: f32 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::I,
        Kind::O,
        Kind::T,
        Kind::S,
        Kind::Z,
        Kind::J,
        Kind::L,
    ];

    /// The piece's cells inside its 4×4 matrix, at rotation 0.
    fn cells(self) -> &'static [(usize, usize)] {
        match self {
            // Each matrix is the piece's smallest square box, so rotating
            // stays inside the declared cells and the kicks are the same
            // size for every piece.
            Kind::I => &[(1, 0), (1, 1), (1, 2), (1, 3)],
            Kind::O => &[(0, 0), (0, 1), (1, 0), (1, 1)],
            Kind::T => &[(0, 1), (1, 0), (1, 1), (1, 2)],
            Kind::S => &[(0, 1), (0, 2), (1, 0), (1, 1)],
            Kind::Z => &[(0, 0), (0, 1), (1, 1), (1, 2)],
            Kind::J => &[(0, 0), (1, 0), (1, 1), (1, 2)],
            Kind::L => &[(0, 2), (1, 0), (1, 1), (1, 2)],
        }
    }

    fn matrix_size(self) -> usize {
        match self {
            Kind::I => 4,
            Kind::O => 2,
            _ => 3,
        }
    }

    /// The cells at one of the four rotations, clockwise.
    pub fn rotation_cells(self, rotation: u8) -> Vec<(usize, usize)> {
        let size = self.matrix_size();
        let mut cells = self.cells().to_vec();
        for _ in 0..(rotation % 4) {
            // (row, col) -> (col, size - 1 - row)
            cells = cells
                .into_iter()
                .map(|(row, col)| (col, size - 1 - row))
                .collect();
        }
        cells
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub rotation: u8,
    pub row: isize,
    pub col: isize,
}

impl Piece {
    fn new(kind: Kind) -> Self {
        let size = kind.matrix_size() as isize;
        Self {
            kind,
            rotation: 0,
            row: 0,
            col: (COLS as isize - size) / 2,
        }
    }

    pub fn cells(&self) -> Vec<(isize, isize)> {
        self.kind
            .rotation_cells(self.rotation)
            .into_iter()
            .map(|(row, col)| (self.row + row as isize, self.col + col as isize))
            .collect()
    }
}

/// A deterministic Permuted Congruential Generator (PCG32), the same one the
/// other games use so a seed plays out identically.
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

/// What a move did, for score flashes and redraw decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub moved: bool,
    pub rotated: bool,
    pub locked: bool,
    pub lines: usize,
    pub over: bool,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub board: [[Option<Kind>; COLS]; ROWS],
    pub piece: Piece,
    pub next: Kind,
    /// The following pieces, so the next box can show more than one.
    pub queue: Vec<Kind>,
    pub score: usize,
    pub lines: usize,
    pub level: usize,
    pub over: bool,
    pub paused: bool,
    rng: Pcg32,
}

impl Game {
    pub fn new(rng: Pcg32) -> Self {
        let mut game = Self {
            board: [[None; COLS]; ROWS],
            piece: Piece::new(Kind::T),
            next: Kind::I,
            queue: Vec::new(),
            score: 0,
            lines: 0,
            level: 0,
            over: false,
            paused: false,
            rng,
        };
        game.refill_queue();
        game.spawn();
        game
    }

    fn refill_queue(&mut self) {
        while self.queue.len() < 4 {
            self.queue.push(Kind::ALL[self.rng.gen_range(Kind::ALL.len())]);
        }
    }

    fn take_next(&mut self) -> Kind {
        let kind = self.queue.remove(0);
        self.refill_queue();
        kind
    }

    fn spawn(&mut self) {
        let kind = self.next;
        self.next = self.take_next();
        let piece = Piece::new(kind);
        if self.collides(piece) {
            self.over = true;
        }
        self.piece = piece;
    }

    pub fn drop_interval_ms(&self) -> f32 {
        (BASE_DROP_MS - self.level as f32 * 70.0).max(MIN_DROP_MS)
    }

    /// Whether the piece would overlap a wall, the floor, or a settled block.
    pub fn collides(&self, piece: Piece) -> bool {
        piece.cells().into_iter().any(|(row, col)| {
            if col < 0 || col >= COLS as isize || row >= ROWS as isize {
                return true;
            }
            if row < 0 {
                return false;
            }
            self.board[row as usize][col as usize].is_some()
        })
    }

    /// Moves the active piece one cell sideways. Returns whether it moved.
    pub fn shift(&mut self, direction: isize) -> bool {
        if self.over || self.paused {
            return false;
        }
        let moved = Piece {
            col: self.piece.col + direction,
            ..self.piece
        };
        if self.collides(moved) {
            return false;
        }
        self.piece = moved;
        true
    }

    /// Turns the piece clockwise, nudging it when the turn would clip.
    pub fn rotate(&mut self) -> bool {
        if self.over || self.paused {
            return false;
        }
        let rotation = (self.piece.rotation + 1) % 4;
        // Turn in place first, then one cell right, left, up, down — the
        // small kick set that keeps a piece against a wall or a stack able
        // to turn at all.
        for (drow, dcol) in [(0, 0), (0, 1), (0, -1), (-1, 0), (1, 0)] {
            let candidate = Piece {
                rotation,
                row: self.piece.row + drow,
                col: self.piece.col + dcol,
                ..self.piece
            };
            if !self.collides(candidate) {
                self.piece = candidate;
                return true;
            }
        }
        false
    }

    /// Advances the piece one row, locking it when it cannot fall.
    pub fn step(&mut self) -> Step {
        if self.over || self.paused {
            return Step::default();
        }
        let down = Piece {
            row: self.piece.row + 1,
            ..self.piece
        };
        if !self.collides(down) {
            self.piece = down;
            return Step {
                moved: true,
                ..Step::default()
            };
        }
        self.lock()
    }

    /// Drops the piece to the floor and locks it, scoring the distance.
    pub fn hard_drop(&mut self) -> Step {
        if self.over || self.paused {
            return Step::default();
        }
        let mut distance = 0;
        loop {
            let down = Piece {
                row: self.piece.row + 1,
                ..self.piece
            };
            if self.collides(down) {
                break;
            }
            self.piece = down;
            distance += 1;
        }
        self.score += distance as usize * 2;
        let mut result = self.lock();
        result.moved = true;
        result
    }

    fn lock(&mut self) -> Step {
        for (row, col) in self.piece.cells() {
            if row >= 0 {
                self.board[row as usize][col as usize] = Some(self.piece.kind);
            }
        }
        let lines = self.clear_lines();
        if lines > 0 {
            // The original scoring: 100, 300, 500, 800 times the level.
            let base = [0, 100, 300, 500, 800][lines.min(4)];
            self.score += base * (self.level + 1);
            self.lines += lines;
            self.level = self.lines / LINES_PER_LEVEL;
        }
        self.spawn();
        Step {
            locked: true,
            lines,
            over: self.over,
            ..Step::default()
        }
    }

    fn clear_lines(&mut self) -> usize {
        let mut kept: Vec<[Option<Kind>; COLS]> = Vec::with_capacity(ROWS);
        let mut cleared = 0;
        for row in 0..ROWS {
            if self.board[row].iter().all(|cell| cell.is_some()) {
                cleared += 1;
            } else {
                kept.push(self.board[row]);
            }
        }
        if cleared == 0 {
            return 0;
        }
        while kept.len() < ROWS {
            kept.insert(0, [None; COLS]);
        }
        self.board.copy_from_slice(&kept);
        cleared
    }

    /// The cells the active piece would occupy if it fell to the floor, for
    /// the landing shadow.
    pub fn ghost(&self) -> Piece {
        let mut ghost = self.piece;
        loop {
            let down = Piece {
                row: ghost.row + 1,
                ..ghost
            };
            if self.collides(down) {
                return ghost;
            }
            ghost = down;
        }
    }

    pub fn toggle_pause(&mut self) {
        if !self.over {
            self.paused = !self.paused;
        }
    }

    pub fn save(&self) -> SavedGame {
        SavedGame {
            board: self.board,
            piece: self.piece,
            next: self.next,
            queue: self.queue.clone(),
            score: self.score,
            lines: self.lines,
            level: self.level,
        }
    }

    /// Rebuilds a saved game, rejecting boards and pieces that could not
    /// have been played.
    pub fn restore(saved: &SavedGame, rng: Pcg32) -> Option<Self> {
        // The active piece must sit on the board and not overlap the stack.
        let cells = saved.piece.cells();
        if cells.is_empty() {
            return None;
        }
        let mut seen = [[false; COLS]; ROWS];
        for (row, col) in saved.board.iter().enumerate() {
            for (col, cell) in col.iter().enumerate() {
                seen[row][col] = cell.is_some();
            }
        }
        for &(row, col) in &cells {
            if row < 0 || row >= ROWS as isize || col < 0 || col >= COLS as isize {
                return None;
            }
            if seen[row as usize][col as usize] {
                return None;
            }
        }
        let mut game = Self {
            board: saved.board,
            piece: saved.piece,
            next: saved.next,
            queue: saved.queue.clone(),
            score: saved.score,
            lines: saved.lines,
            level: saved.level,
            over: false,
            paused: false,
            rng,
        };
        game.refill_queue();
        Some(game)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedGame {
    pub board: [[Option<Kind>; COLS]; ROWS],
    pub piece: Piece,
    pub next: Kind,
    pub queue: Vec<Kind>,
    pub score: usize,
    pub lines: usize,
    pub level: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rng() -> Pcg32 {
        Pcg32::new(3, 5)
    }

    fn empty_board() -> [[Option<Kind>; COLS]; ROWS] {
        [[None; COLS]; ROWS]
    }

    fn game_with(board: [[Option<Kind>; COLS]; ROWS], kind: Kind) -> Game {
        let mut game = Game::new(test_rng());
        game.board = board;
        game.piece = Piece::new(kind);
        game.over = false;
        game.paused = false;
        game
    }

    /// Fills `rows` rows from the bottom with one column left open.
    fn board_with_rows(rows: usize) -> [[Option<Kind>; COLS]; ROWS] {
        let mut board = empty_board();
        for row in (ROWS - rows)..ROWS {
            for col in 0..COLS {
                board[row][col] = Some(Kind::J);
            }
        }
        board
    }

    #[test]
    fn every_piece_has_four_cells_at_every_rotation() {
        for kind in Kind::ALL {
            for rotation in 0..4 {
                let cells = kind.rotation_cells(rotation);
                assert_eq!(cells.len(), 4, "{kind:?} at {rotation}");
                let mut unique = cells.clone();
                unique.sort_unstable();
                unique.dedup();
                assert_eq!(unique.len(), 4, "{kind:?} at {rotation} has a repeated cell");
            }
        }
    }

    #[test]
    fn rotation_turns_clockwise() {
        // T at rotation 0 points up; one turn points right — its bar in
        // column 1, the nub at (1, 2).
        let cells = Kind::T.rotation_cells(0);
        assert_eq!(cells, vec![(0, 1), (1, 0), (1, 1), (1, 2)]);
        let turned = Kind::T.rotation_cells(1);
        assert_eq!(turned, vec![(1, 2), (0, 1), (1, 1), (2, 1)]);
        // Four turns return to the start.
        assert_eq!(Kind::T.rotation_cells(4), cells);
    }

    #[test]
    fn pieces_spawn_inside_the_board() {
        let game = Game::new(test_rng());
        assert!(!game.collides(game.piece));
        assert!(game.piece.cells().iter().all(|&(row, col)| row >= 0
            && row < ROWS as isize
            && col >= 0
            && col < COLS as isize));
    }

    #[test]
    fn shifting_stops_at_the_wall() {
        let mut game = game_with(empty_board(), Kind::O);
        while game.shift(-1) {}
        let left = game.piece.col;
        assert!(!game.shift(-1));
        assert_eq!(game.piece.col, left);
    }

    #[test]
    fn a_piece_falls_and_locks_on_the_floor() {
        let mut game = game_with(empty_board(), Kind::O);
        let mut steps = 0;
        while !game.step().locked {
            steps += 1;
            assert!(steps < ROWS + 4);
        }
        // Two rows of the O are settled at the bottom.
        assert!(game.board[ROWS - 1].iter().any(|cell| cell.is_some()));
        assert!(game.board[ROWS - 2].iter().any(|cell| cell.is_some()));
    }

    #[test]
    fn hard_drop_lands_on_the_stack() {
        let mut board = empty_board();
        for col in 0..COLS {
            board[ROWS - 1][col] = Some(Kind::L);
        }
        let mut game = game_with(board, Kind::O);
        let result = game.hard_drop();
        assert!(result.locked);
        assert!(game.board[ROWS - 2].iter().any(|cell| cell.is_some()));
        assert!(!game.collides(game.piece));
    }

    #[test]
    fn full_lines_clear_and_score() {
        let mut board = board_with_rows(1);
        // Leave one cell open in the bottom row for a piece to fill.
        board[ROWS - 1][4] = None;
        let mut game = game_with(board, Kind::O);
        game.piece.row = ROWS as isize - 2;
        game.piece.col = 4;
        let result = game.step();
        assert!(result.locked);
        assert_eq!(result.lines, 1);
        assert_eq!(game.score, 100);
        assert_eq!(game.lines, 1);
        assert_eq!(game.level, 0);
    }

    #[test]
    fn four_lines_score_eight_hundred() {
        let mut board = board_with_rows(4);
        for row in (ROWS - 4)..ROWS {
            board[row][4] = None;
        }
        let mut game = game_with(board, Kind::I);
        game.piece.rotation = 1;
        // The vertical I occupies column 2 of its 4×4 matrix.
        game.piece.row = ROWS as isize - 4;
        game.piece.col = 2;
        let result = game.step();
        assert!(result.locked);
        assert_eq!(result.lines, 4);
        assert_eq!(game.score, 800);
    }

    #[test]
    fn level_speeds_up_the_drop() {
        let mut game = Game::new(test_rng());
        assert_eq!(game.drop_interval_ms(), BASE_DROP_MS);
        game.level = 2;
        assert_eq!(game.drop_interval_ms(), BASE_DROP_MS - 140.0);
        game.level = 100;
        assert_eq!(game.drop_interval_ms(), MIN_DROP_MS);
    }

    #[test]
    fn topping_out_ends_the_game() {
        let mut board = empty_board();
        for col in 0..COLS {
            board[0][col] = Some(Kind::T);
        }
        let mut game = game_with(board, Kind::O);
        // Spawn into the filled top rows.
        game.piece = Piece::new(Kind::O);
        game.spawn();
        assert!(game.over);
    }

    #[test]
    fn the_ghost_lands_where_the_piece_would() {
        let game = game_with(empty_board(), Kind::T);
        let ghost = game.ghost();
        let cell = ghost.cells()[0];
        let below = Piece {
            row: ghost.row + 1,
            ..ghost
        };
        assert!(game.collides(below), "the ghost rests on something");
        assert!(cell.0 >= 0);
    }

    #[test]
    fn pause_stops_input_and_falling() {
        let mut game = Game::new(test_rng());
        game.paused = true;
        let before = game.piece;
        assert!(!game.shift(1));
        assert!(!game.rotate());
        assert_eq!(game.step(), Step::default());
        assert_eq!(game.piece, before);
    }

    #[test]
    fn rotation_kicks_off_a_wall() {
        let mut game = game_with(empty_board(), Kind::I);
        game.piece.col = 0;
        // The vertical I at the left wall needs a nudge right to fit.
        assert!(game.rotate());
        assert!(!game.collides(game.piece));
        assert!(game.piece.cells().iter().all(|&(_, col)| col >= 0));
    }

    #[test]
    fn save_and_restore_round_trip() {
        let mut game = Game::new(test_rng());
        game.shift(1);
        game.step();
        game.score = 42;
        let saved = game.save();
        let restored = Game::restore(&saved, test_rng()).expect("a legal position restores");
        assert_eq!(restored.save(), saved);
        assert_eq!(restored.drop_interval_ms(), game.drop_interval_ms());
    }

    #[test]
    fn restore_rejects_overlapping_pieces() {
        let mut game = Game::new(test_rng());
        let mut saved = game.save();
        let cell = saved.piece.cells()[0];
        saved.board[cell.0 as usize][cell.1 as usize] = Some(Kind::O);
        assert!(Game::restore(&saved, test_rng()).is_none());
        game.over = true;
        let _ = game.save();
    }
}
