pub const SIZE: usize = 4;
pub const WIN_VALUE: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    pub fn vector(&self) -> (isize, isize) {
        match self {
            Direction::Up => (-1, 0),
            Direction::Right => (0, 1),
            Direction::Down => (1, 0),
            Direction::Left => (0, -1),
        }
    }
}

/// A lightweight, deterministic Permuted Congruential Generator (PCG32)
/// that requires no external dependencies or libc.
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

    pub fn next_f64(&mut self) -> f64 {
        let v = self.next_u32();
        (v as f64) / (u32::MAX as f64 + 1.0)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub id: usize,
    pub value: usize,
    pub row: usize,
    pub col: usize,
    pub from_merge: bool,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub tiles: Vec<Tile>,
    pub ghosts: Vec<Tile>,
    pub score: usize,
    pub won: bool,
    pub keep_playing: bool,
    pub over: bool,
    pub next_id: usize,
    pub rng: Pcg32,
}

impl Game {
    pub fn new(rng: Pcg32) -> Self {
        let mut g = Self {
            tiles: Vec::with_capacity(16),
            ghosts: Vec::new(),
            score: 0,
            won: false,
            keep_playing: false,
            over: false,
            next_id: 0,
            rng,
        };
        g.add_random_tile();
        g.add_random_tile();
        g
    }

    pub fn terminated(&self) -> bool {
        self.over || (self.won && !self.keep_playing)
    }

    pub fn grid(&self) -> [[Option<Tile>; SIZE]; SIZE] {
        let mut cells: [[Option<Tile>; SIZE]; SIZE] = Default::default();
        for t in &self.tiles {
            if t.row < SIZE && t.col < SIZE {
                cells[t.row][t.col] = Some(t.clone());
            }
        }
        cells
    }

    pub fn empty_cells(&self) -> Vec<(usize, usize)> {
        let cells = self.grid();
        let mut empty = Vec::new();
        for r in 0..SIZE {
            for c in 0..SIZE {
                if cells[r][c].is_none() {
                    empty.push((r, c));
                }
            }
        }
        empty
    }

    pub fn add_random_tile(&mut self) {
        let empty = self.empty_cells();
        if empty.is_empty() {
            return;
        }
        let cell = empty[self.rng.gen_range(empty.len())];
        let value = if self.rng.next_f64() >= 0.9 { 4 } else { 2 };
        self.next_id += 1;
        self.tiles.push(Tile {
            id: self.next_id,
            value,
            row: cell.0,
            col: cell.1,
            from_merge: false,
        });
    }

    pub fn move_dir(&mut self, d: Direction) -> (bool, usize) {
        if self.terminated() {
            return (false, 0);
        }
        let mut cells = self.grid();
        let mut ghosts = Vec::new();
        let mut joined = [[false; SIZE]; SIZE];
        let (dr, dc) = d.vector();

        let order = [0, 1, 2, 3];
        let reversed = [3, 2, 1, 0];
        let rows = if dr == 1 { reversed } else { order };
        let cols = if dc == 1 { reversed } else { order };

        let mut moved = false;
        let mut gained = 0;

        for &r in &rows {
            for &c in &cols {
                let Some(mut t) = cells[r][c].take() else {
                    continue;
                };
                let (mut fr, mut fc) = (r as isize, c as isize);
                loop {
                    let (nr, nc) = (fr + dr, fc + dc);
                    if !Self::inside(nr, nc) || cells[nr as usize][nc as usize].is_some() {
                        break;
                    }
                    fr = nr;
                    fc = nc;
                }
                let (nr, nc) = (fr + dr, fc + dc);
                if Self::inside(nr, nc) {
                    let nru = nr as usize;
                    let ncu = nc as usize;
                    if let Some(next) = &cells[nru][ncu] {
                        if next.value == t.value && !joined[nru][ncu] {
                            self.next_id += 1;
                            let merged_value = t.value * 2;
                            let next_tile = cells[nru][ncu].take().unwrap();
                            cells[nru][ncu] = Some(Tile {
                                id: self.next_id,
                                value: merged_value,
                                row: nru,
                                col: ncu,
                                from_merge: true,
                            });
                            joined[nru][ncu] = true;
                            t.row = nru;
                            t.col = ncu;
                            ghosts.push(next_tile);
                            ghosts.push(t);
                            gained += merged_value;
                            if merged_value == WIN_VALUE {
                                self.won = true;
                            }
                            moved = true;
                            continue;
                        }
                    }
                }
                let fru = fr as usize;
                let fcu = fc as usize;
                if fru != r || fcu != c {
                    t.row = fru;
                    t.col = fcu;
                    moved = true;
                }
                cells[fru][fcu] = Some(t);
            }
        }

        if !moved {
            return (false, 0);
        }

        self.ghosts = ghosts;
        self.tiles.clear();
        for r in 0..SIZE {
            for c in 0..SIZE {
                if let Some(t) = cells[r][c].take() {
                    self.tiles.push(t);
                }
            }
        }
        self.score += gained;
        self.add_random_tile();
        if !self.moves_available() {
            self.over = true;
        }
        (true, gained)
    }

    pub fn moves_available(&self) -> bool {
        let cells = self.grid();
        for r in 0..SIZE {
            for c in 0..SIZE {
                let Some(t) = &cells[r][c] else {
                    return true;
                };
                if r + 1 < SIZE {
                    if let Some(down) = &cells[r + 1][c] {
                        if down.value == t.value {
                            return true;
                        }
                    }
                }
                if c + 1 < SIZE {
                    if let Some(right) = &cells[r][c + 1] {
                        if right.value == t.value {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn inside(r: isize, c: isize) -> bool {
        r >= 0 && r < SIZE as isize && c >= 0 && c < SIZE as isize
    }

    pub fn save(&self) -> SavedGame {
        SavedGame {
            tiles: self
                .tiles
                .iter()
                .map(|t| Tile {
                    id: t.id,
                    value: t.value,
                    row: t.row,
                    col: t.col,
                    from_merge: false,
                })
                .collect(),
            score: self.score,
            won: self.won,
            keep_playing: self.keep_playing,
            next_id: self.next_id,
        }
    }

    pub fn restore(saved: &SavedGame, rng: Pcg32) -> Option<Self> {
        if saved.tiles.is_empty() {
            return None;
        }
        let mut seen = [[false; SIZE]; SIZE];
        let mut max_id = saved.next_id;
        let mut tiles = Vec::new();
        for t in &saved.tiles {
            if !Self::inside(t.row as isize, t.col as isize)
                || seen[t.row][t.col]
                || t.value < 2
                || (t.value & (t.value - 1)) != 0
            {
                return None;
            }
            seen[t.row][t.col] = true;
            max_id = max_id.max(t.id);
            tiles.push(t.clone());
        }
        let g = Self {
            tiles,
            ghosts: Vec::new(),
            score: saved.score,
            won: saved.won,
            keep_playing: saved.keep_playing,
            over: false,
            next_id: max_id,
            rng,
        };
        if !g.moves_available() {
            return None;
        }
        Some(g)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedGame {
    pub tiles: Vec<Tile>,
    pub score: usize,
    pub won: bool,
    pub keep_playing: bool,
    pub next_id: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rng() -> Pcg32 {
        Pcg32::new(1, 2)
    }

    fn game_of(rows: [[usize; SIZE]; SIZE]) -> Game {
        let mut g = Game {
            tiles: Vec::new(),
            ghosts: Vec::new(),
            score: 0,
            won: false,
            keep_playing: false,
            over: false,
            next_id: 0,
            rng: test_rng(),
        };
        for r in 0..SIZE {
            for c in 0..SIZE {
                let v = rows[r][c];
                if v != 0 {
                    g.next_id += 1;
                    g.tiles.push(Tile {
                        id: g.next_id,
                        value: v,
                        row: r,
                        col: c,
                        from_merge: false,
                    });
                }
            }
        }
        g
    }

    fn board(g: &Game) -> [[usize; SIZE]; SIZE] {
        let mut rows = [[0; SIZE]; SIZE];
        for t in &g.tiles {
            rows[t.row][t.col] = t.value;
        }
        rows
    }

    fn without_new(g: &Game, mut rows: [[usize; SIZE]; SIZE]) -> [[usize; SIZE]; SIZE] {
        let mut t = &g.tiles[0];
        for u in &g.tiles {
            if u.id > t.id && !u.from_merge {
                t = u;
            }
        }
        rows[t.row][t.col] = 0;
        rows
    }

    fn count_tiles(row: [usize; SIZE]) -> usize {
        row.iter().filter(|&&v| v != 0).count()
    }

    #[test]
    fn test_move_row() {
        struct TestCase {
            name: &'static str,
            input: [usize; SIZE],
            dir: Direction,
            want: [usize; SIZE],
            gained: usize,
        }

        let cases = [
            TestCase {
                name: "slides",
                input: [0, 0, 0, 2],
                dir: Direction::Left,
                want: [2, 0, 0, 0],
                gained: 0,
            },
            TestCase {
                name: "merges",
                input: [2, 0, 0, 2],
                dir: Direction::Left,
                want: [4, 0, 0, 0],
                gained: 4,
            },
            TestCase {
                name: "merges each pair once",
                input: [2, 2, 2, 2],
                dir: Direction::Left,
                want: [4, 4, 0, 0],
                gained: 8,
            },
            TestCase {
                name: "keeps a merged tile",
                input: [2, 2, 4, 0],
                dir: Direction::Left,
                want: [4, 4, 0, 0],
                gained: 4,
            },
            TestCase {
                name: "merges toward the wall first",
                input: [2, 2, 2, 0],
                dir: Direction::Right,
                want: [0, 0, 2, 4],
                gained: 4,
            },
            TestCase {
                name: "merges the far pair",
                input: [0, 2, 2, 2],
                dir: Direction::Left,
                want: [4, 2, 0, 0],
                gained: 4,
            },
            TestCase {
                name: "leaves unequal tiles",
                input: [2, 4, 8, 0],
                dir: Direction::Right,
                want: [0, 2, 4, 8],
                gained: 0,
            },
            TestCase {
                name: "merges big tiles",
                input: [1024, 1024, 0, 0],
                dir: Direction::Right,
                want: [0, 0, 0, 2048],
                gained: 2048,
            },
        ];

        for tc in cases {
            let mut g = game_of([tc.input, [0; SIZE], [0; SIZE], [0; SIZE]]);
            let (moved, gained) = g.move_dir(tc.dir);
            assert!(
                moved && gained == tc.gained,
                "{}: moved {}, gained {}; want true, {}",
                tc.name,
                moved,
                gained,
                tc.gained
            );
            let got = without_new(&g, board(&g));
            assert_eq!(got[0], tc.want, "{}: row {:?}, want {:?}", tc.name, got[0], tc.want);
            assert_eq!(g.score, tc.gained, "{}: score {}, want {}", tc.name, g.score, tc.gained);
            assert_eq!(
                g.tiles.len(),
                count_tiles(tc.want) + 1,
                "{}: {} tiles, want {} and a new one",
                tc.name,
                g.tiles.len(),
                count_tiles(tc.want)
            );
        }
    }

    #[test]
    fn test_move_columns() {
        let mut g = game_of([
            [2, 0, 0, 0],
            [2, 4, 0, 0],
            [0, 4, 0, 0],
            [4, 4, 0, 0],
        ]);
        g.move_dir(Direction::Down);
        let want = [
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            [4, 4, 0, 0],
            [4, 8, 0, 0],
        ];
        let got = without_new(&g, board(&g));
        assert_eq!(got, want, "after down {:?}, want {:?}", got, want);
        assert_eq!(g.score, 12, "score {}, want 12", g.score);
    }

    #[test]
    fn test_move_blocked() {
        let mut g = game_of([[2, 4, 0, 0], [4, 2, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        g.ghosts = vec![Tile {
            id: 99,
            value: 2,
            row: 0,
            col: 0,
            from_merge: false,
        }];
        let (moved, gained) = g.move_dir(Direction::Left);
        assert!(!moved && gained == 0 && g.tiles.len() == 4);
        assert_eq!(g.ghosts.len(), 1, "a blocked move cleared the ghosts");
    }

    #[test]
    fn test_merge_ghosts() {
        let mut g = game_of([[0, 2, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        let a_id = g.tiles[0].id;
        let b_id = g.tiles[1].id;
        g.move_dir(Direction::Left);
        assert_eq!(g.ghosts.len(), 2, "{} ghosts, want 2", g.ghosts.len());
        for gh in &g.ghosts {
            assert!(
                gh.id == a_id || gh.id == b_id,
                "ghost {} is not one of merged tiles",
                gh.id
            );
            assert_eq!(
                (gh.row, gh.col),
                (0, 0),
                "ghost {} at {},{}, want 0,0",
                gh.id,
                gh.row,
                gh.col
            );
        }
        let made = g.tiles.iter().find(|t| t.from_merge);
        assert!(made.is_some(), "no merged tile");
        let made = made.unwrap();
        assert_eq!(made.value, 4);
        assert!(made.id != a_id && made.id != b_id);
    }

    #[test]
    fn test_new_tiles() {
        let mut twos = 0;
        let mut fours = 0;
        let mut rng = test_rng();
        for _ in 0..2000 {
            let g = Game::new(rng.clone());
            // step the parent rng so each game is distinct
            rng.next_u32();
            assert_eq!(g.tiles.len(), 2, "a new game has {} tiles, want 2", g.tiles.len());
            assert!(
                g.tiles[0].row != g.tiles[1].row || g.tiles[0].col != g.tiles[1].col,
                "two tiles in one cell"
            );
            for t in &g.tiles {
                match t.value {
                    2 => twos += 1,
                    4 => fours += 1,
                    _ => panic!("tile other than 2 and 4: {}", t.value),
                }
            }
        }
        assert_eq!(twos + fours, 4000);
        let share = fours as f64 / 4000.0;
        assert!(
            (0.08..=0.12).contains(&share),
            "{:.3} of new tiles are 4s, want about 0.1",
            share
        );
    }

    #[test]
    fn test_win() {
        let mut g = game_of([[1024, 1024, 0, 0], [0; SIZE], [0; SIZE], [0; SIZE]]);
        g.move_dir(Direction::Left);
        assert!(
            g.won && g.terminated(),
            "won {}, terminated {} after making 2048",
            g.won,
            g.terminated()
        );
        let (moved, _) = g.move_dir(Direction::Right);
        assert!(!moved, "a won game took a move before keep going");
        g.keep_playing = true;
        assert!(!g.terminated(), "terminated while keeping going");
        let (moved, _) = g.move_dir(Direction::Right);
        assert!(moved, "no move after keep going");
    }

    #[test]
    fn test_game_over() {
        let mut g = game_of([
            [0, 2, 4, 8],
            [8, 16, 32, 64],
            [128, 256, 512, 1024],
            [32, 128, 64, 256],
        ]);
        g.move_dir(Direction::Left);
        assert!(g.over && g.terminated(), "over {} with {:?}", g.over, board(&g));
        let (moved, _) = g.move_dir(Direction::Right);
        assert!(!moved, "a lost game took a move");

        let g2 = game_of([
            [2, 4, 2, 4],
            [4, 2, 4, 2],
            [2, 4, 2, 4],
            [4, 2, 4, 4],
        ]);
        assert!(g2.moves_available(), "no moves available with two 4s side by side");
    }

    #[test]
    fn test_save_restore() {
        let mut g = game_of([[2, 0, 4, 0], [0, 8, 0, 0], [0; SIZE], [0; SIZE]]);
        g.score = 120;
        g.won = true;
        g.keep_playing = true;
        let mut r = Game::restore(&g.save(), test_rng()).expect("not restored");
        assert_eq!(board(&r), board(&g));
        assert_eq!(r.score, 120);
        assert!(r.won);
        assert!(r.keep_playing);
        assert_eq!(r.next_id, g.next_id);
        let (moved, _) = r.move_dir(Direction::Left);
        assert!(moved, "the restored game takes no move");

        // Invalid saves
        assert!(Game::restore(&SavedGame {
            tiles: vec![],
            score: 0,
            won: false,
            keep_playing: false,
            next_id: 0,
        }, test_rng()).is_none());

        assert!(Game::restore(&SavedGame {
            tiles: vec![Tile { id: 1, value: 2, row: 4, col: 0, from_merge: false }],
            score: 0,
            won: false,
            keep_playing: false,
            next_id: 1,
        }, test_rng()).is_none());

        assert!(Game::restore(&SavedGame {
            tiles: vec![
                Tile { id: 1, value: 2, row: 0, col: 0, from_merge: false },
                Tile { id: 2, value: 4, row: 0, col: 0, from_merge: false },
            ],
            score: 0,
            won: false,
            keep_playing: false,
            next_id: 2,
        }, test_rng()).is_none());

        assert!(Game::restore(&SavedGame {
            tiles: vec![Tile { id: 1, value: 6, row: 0, col: 0, from_merge: false }],
            score: 0,
            won: false,
            keep_playing: false,
            next_id: 1,
        }, test_rng()).is_none());
    }
}
