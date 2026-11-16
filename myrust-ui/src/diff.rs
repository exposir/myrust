//! Line diff viewer.
//!
//! The comparison is git's: the histogram algorithm (`xhistogram.c`),
//! which anchors on lines that occur rarely, falling back to Myers's
//! O(ND) with xdiff's cost limit, then xdiff's indent heuristic slides
//! each group of changed lines to where it reads as a block of code. The
//! inline highlights compare words the same way, and the two sides of a
//! split view are paired line by line as Pierre's diffs pair them.

use std::collections::HashMap;

use super::context::Context;

/// Kind of one diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Unchanged line.
    Same,
    /// Added line.
    Added,
    /// Removed line.
    Removed,
}

/// One line in a line diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Same/Added/Removed.
    pub kind: DiffKind,
    /// Line text without the newline.
    pub text: String,
    /// One-based old line number.
    pub old_line: Option<usize>,
    /// One-based new line number.
    pub new_line: Option<usize>,
}

/// A change: lines `a0..a1` of the old text replaced by `b0..b1` of the
/// new one, as MyGo's `diff.Hunk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    /// First old line changed.
    pub a0: usize,
    /// One past the last old line changed.
    pub a1: usize,
    /// First new line changed.
    pub b0: usize,
    /// One past the last new line changed.
    pub b1: usize,
}

/// Diff rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffLayout {
    /// One column with `+`/`-` prefixes.
    #[default]
    Unified,
    /// Two aligned columns.
    SideBySide,
}

/// Caller-owned diff viewer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffViewerState {
    /// Current rendering mode.
    pub layout: DiffLayout,
}

/// Result of constructing a diff viewer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffViewerResponse {
    /// Full diff lines.
    pub lines: Vec<DiffLine>,
    /// Added line count.
    pub added: usize,
    /// Removed line count.
    pub removed: usize,
    /// Unchanged line count.
    pub same: usize,
    /// Layout used for this frame.
    pub layout: DiffLayout,
}

/// Splits text into lines as MyGo's `ropeLines` does: none for an empty
/// text, else `strings.Split`'s lines, the empty one after a trailing
/// newline included.
pub fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n').map(str::to_string).collect()
}

/// Compares two texts and returns the hunks turning `a` into `b`.
pub fn diff_hunks(a: &str, b: &str) -> Vec<Hunk> {
    lines_diff(&split_lines(a), &split_lines(b))
}

/// Compares two line sequences, as MyGo's `diff.Lines`.
pub fn lines_diff(a: &[String], b: &[String]) -> Vec<Hunk> {
    let (ia, ib) = intern(a, b);
    let mut differ = Differ::new(ia, ib);
    differ.histogram(0, differ.a.len(), 0, differ.b.len(), 0);
    if differ.da.is_empty() {
        return Vec::new();
    }
    let (mut da, mut db) = (differ.da.clone(), differ.db.clone());
    compact(&mut da, &db, &differ.a, a);
    compact(&mut db, &da, &differ.b, b);
    hunks_of(&da, &db)
}

fn hunks_of(da: &[bool], db: &[bool]) -> Vec<Hunk> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < da.len() || j < db.len() {
        if i < da.len() && j < db.len() && !da[i] && !db[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (a0, b0) = (i, j);
        while i < da.len() && da[i] {
            i += 1;
        }
        while j < db.len() && db[j] {
            j += 1;
        }
        if a0 == i && b0 == j {
            if i < da.len() {
                i += 1;
            }
            if j < db.len() {
                j += 1;
            }
            continue;
        }
        out.push(Hunk { a0, a1: i, b0, b1: j });
    }
    out
}

/// Numbers the distinct strings of `a` and `b`, equal strings sharing a
/// number.
fn intern(a: &[String], b: &[String]) -> (Vec<i32>, Vec<i32>) {
    let mut ids: HashMap<&str, i32> = HashMap::with_capacity(a.len() + b.len());
    let mut next = 0;
    let ia = intern_side(a, &mut ids, &mut next);
    let ib = intern_side(b, &mut ids, &mut next);
    (ia, ib)
}

fn intern_side<'a>(
    s: &'a [String],
    ids: &mut HashMap<&'a str, i32>,
    next: &mut i32,
) -> Vec<i32> {
    s.iter()
        .map(|v| {
            if let Some(id) = ids.get(v.as_str()) {
                return *id;
            }
            let id = *next;
            *next += 1;
            ids.insert(v.as_str(), id);
            id
        })
        .collect()
}

const MAX_CHAIN: usize = 64;
const MAX_DEPTH: usize = 256;
const MAX_COST_MIN: i32 = 256;
const LINE_MAX: i32 = i32::MAX / 4;

struct Differ {
    a: Vec<i32>,
    b: Vec<i32>,
    da: Vec<bool>,
    db: Vec<bool>,
    next: Vec<isize>,
    cnt: Vec<usize>,
    recs: HashMap<i32, (isize, usize)>,
    myers_v: Vec<i32>,
    myers_extra: i32,
}

impl Differ {
    fn new(a: Vec<i32>, b: Vec<i32>) -> Self {
        Differ {
            a,
            b,
            da: Vec::new(),
            db: Vec::new(),
            next: Vec::new(),
            cnt: Vec::new(),
            recs: HashMap::new(),
            myers_v: Vec::new(),
            myers_extra: 0,
        }
    }

    fn mark(&mut self, a0: usize, a1: usize, b0: usize, b1: usize) {
        if a0 == a1 && b0 == b1 {
            return;
        }
        if self.da.is_empty() {
            self.da = vec![false; self.a.len()];
            self.db = vec![false; self.b.len()];
        }
        for slot in &mut self.da[a0..a1] {
            *slot = true;
        }
        for slot in &mut self.db[b0..b1] {
            *slot = true;
        }
    }

    fn histogram(
        &mut self,
        mut a0: usize,
        mut a1: usize,
        mut b0: usize,
        mut b1: usize,
        depth: usize,
    ) {
        loop {
            while a0 < a1 && b0 < b1 && self.a[a0] == self.b[b0] {
                a0 += 1;
                b0 += 1;
            }
            while a0 < a1 && b0 < b1 && self.a[a1 - 1] == self.b[b1 - 1] {
                a1 -= 1;
                b1 -= 1;
            }
            if a0 == a1 || b0 == b1 {
                self.mark(a0, a1, b0, b1);
                return;
            }
            if depth > MAX_DEPTH {
                self.myers(a0, a1, b0, b1);
                return;
            }
            let Some((la0, la1, lb0, lb1)) = self.lcs(a0, a1, b0, b1) else {
                self.myers(a0, a1, b0, b1);
                return;
            };
            if la0 == la1 {
                self.mark(a0, a1, b0, b1);
                return;
            }
            if la0 - a0 + lb0 - b0 < a1 - la1 + b1 - lb1 {
                self.histogram(a0, la0, b0, lb0, depth + 1);
                a0 = la1;
                b0 = lb1;
            } else {
                self.histogram(la1, a1, lb1, b1, depth + 1);
                a1 = la0;
                b1 = lb0;
            }
        }
    }

    fn lcs(
        &mut self,
        a0: usize,
        a1: usize,
        b0: usize,
        b1: usize,
    ) -> Option<(usize, usize, usize, usize)> {
        let n = a1 - a0;
        if self.next.len() < n {
            self.next = vec![-1; n];
            self.cnt = vec![0; n];
        }
        if self.recs.capacity() > 8 * n + 64 {
            self.recs = HashMap::with_capacity(n);
        } else {
            self.recs.clear();
        }
        for i in (a0..a1).rev() {
            let entry = self.recs.entry(self.a[i]).or_insert((-1, 0));
            let first = entry.0;
            self.next[i - a0] = first;
            entry.0 = i as isize;
            entry.1 += 1;
        }
        for i in a0..a1 {
            self.cnt[i - a0] = self.recs[&self.a[i]].1;
        }
        let mut best = MAX_CHAIN + 1;
        let mut common = false;
        let (mut la0, mut la1, mut lb0, mut lb1) = (a0, a0, b0, b0);
        let mut bi = b0;
        while bi < b1 {
            let mut bnext = bi + 1;
            let Some(&(first, count)) = self.recs.get(&self.b[bi]) else {
                bi = bnext;
                continue;
            };
            common = true;
            if count > best {
                bi = bnext;
                continue;
            }
            let mut at = first;
            while at >= 0 {
                let mut s = at as usize;
                let mut bs = bi;
                let mut e = s + 1;
                let mut be = bi + 1;
                let mut rc = self.cnt[s - a0];
                while s > a0 && bs > b0 && self.a[s - 1] == self.b[bs - 1] {
                    s -= 1;
                    bs -= 1;
                    if rc > 1 {
                        rc = rc.min(self.cnt[s - a0]);
                    }
                }
                while e < a1 && be < b1 && self.a[e] == self.b[be] {
                    if rc > 1 {
                        rc = rc.min(self.cnt[e - a0]);
                    }
                    e += 1;
                    be += 1;
                }
                if bnext < be {
                    bnext = be;
                }
                if la1 - la0 < e - s || rc < best {
                    la0 = s;
                    la1 = e;
                    lb0 = bs;
                    lb1 = be;
                    best = rc;
                }
                let mut np = self.next[at as usize - a0];
                while np >= 0 && (np as usize) < e {
                    np = self.next[np as usize - a0];
                }
                at = np;
            }
            bi = bnext;
        }
        if common && best > MAX_CHAIN {
            return None;
        }
        Some((la0, la1, lb0, lb1))
    }

    fn myers(&mut self, a0: usize, a1: usize, b0: usize, b1: usize) {
        let n = self.a.len() + self.b.len() + 3;
        if self.myers_v.len() < 2 * n {
            self.myers_v = vec![0; 2 * n];
        }
        let mut cost = 1;
        while (cost * cost) < n as i32 {
            cost <<= 1;
        }
        self.myers_extra = cost.max(MAX_COST_MIN);
        self.recs_cmp(a0, a1, b0, b1, false);
    }

    fn recs_cmp(
        &mut self,
        mut off1: usize,
        mut lim1: usize,
        mut off2: usize,
        mut lim2: usize,
        need_min: bool,
    ) {
        while off1 < lim1 && off2 < lim2 && self.a[off1] == self.b[off2] {
            off1 += 1;
            off2 += 1;
        }
        while off1 < lim1 && off2 < lim2 && self.a[lim1 - 1] == self.b[lim2 - 1] {
            lim1 -= 1;
            lim2 -= 1;
        }
        if off1 == lim1 || off2 == lim2 {
            self.mark(off1, lim1, off2, lim2);
            return;
        }
        let (i1, i2, min_lo, min_hi) = self.split(off1, lim1, off2, lim2, need_min);
        self.recs_cmp(off1, i1, off2, i2, min_lo);
        self.recs_cmp(i1, lim1, i2, lim2, min_hi);
    }

    #[allow(clippy::too_many_arguments)]
    fn split(
        &mut self,
        off1: usize,
        lim1: usize,
        off2: usize,
        lim2: usize,
        need_min: bool,
    ) -> (usize, usize, bool, bool) {
        let nd = self.a.len() + self.b.len() + 3;
        let base = (self.b.len() + 1) as i32;
        let (off1, lim1, off2, lim2) = (off1 as i32, lim1 as i32, off2 as i32, lim2 as i32);
        let a = &self.a;
        let b = &self.b;
        let dmin = off1 - lim2;
        let dmax = lim1 - off2;
        let fmid = off1 - off2;
        let bmid = lim1 - lim2;
        let odd = (fmid - bmid) & 1 != 0;
        let (mut fmin, mut fmax) = (fmid, fmid);
        let (mut bmin, mut bmax) = (bmid, bmid);
        let v = &mut self.myers_v;
        let extra = self.myers_extra;
        let (vf, vb) = v.split_at_mut(nd);
        let at = |k: i32| (base + k) as usize;
        vf[at(fmid)] = off1;
        vb[at(bmid)] = lim1;
        let mut ec = 0;
        loop {
            ec += 1;
            if fmin > dmin {
                fmin -= 1;
                vf[at(fmin - 1)] = -1;
            } else {
                fmin += 1;
            }
            if fmax < dmax {
                fmax += 1;
                vf[at(fmax + 1)] = -1;
            } else {
                fmax -= 1;
            }
            let mut k = fmax;
            while k >= fmin {
                let mut i1 = if vf[at(k - 1)] >= vf[at(k + 1)] {
                    vf[at(k - 1)] + 1
                } else {
                    vf[at(k + 1)]
                };
                let mut i2 = i1 - k;
                while i1 < lim1 && i2 < lim2 && a[i1 as usize] == b[i2 as usize] {
                    i1 += 1;
                    i2 += 1;
                }
                vf[at(k)] = i1;
                if odd && bmin <= k && k <= bmax && vb[at(k)] <= i1 {
                    return (i1 as usize, i2 as usize, true, true);
                }
                k -= 2;
            }
            if bmin > dmin {
                bmin -= 1;
                vb[at(bmin - 1)] = LINE_MAX;
            } else {
                bmin += 1;
            }
            if bmax < dmax {
                bmax += 1;
                vb[at(bmax + 1)] = LINE_MAX;
            } else {
                bmax -= 1;
            }
            let mut k = bmax;
            while k >= bmin {
                let mut i1 = if vb[at(k - 1)] < vb[at(k + 1)] {
                    vb[at(k - 1)]
                } else {
                    vb[at(k + 1)] - 1
                };
                let mut i2 = i1 - k;
                while i1 > off1 && i2 > off2 && a[i1 as usize - 1] == b[i2 as usize - 1] {
                    i1 -= 1;
                    i2 -= 1;
                }
                vb[at(k)] = i1;
                if !odd && fmin <= k && k <= fmax && i1 <= vf[at(k)] {
                    return (i1 as usize, i2 as usize, true, true);
                }
                k -= 2;
            }
            if need_min || ec < extra {
                continue;
            }
            let mut fbest = -1;
            let mut fbest1 = -1;
            let mut k = fmax;
            while k >= fmin {
                let mut i1 = vf[at(k)].min(lim1);
                let mut i2 = i1 - k;
                if lim2 < i2 {
                    i1 = lim2 + k;
                    i2 = lim2;
                }
                if fbest < i1 + i2 {
                    fbest = i1 + i2;
                    fbest1 = i1;
                }
                k -= 2;
            }
            let mut bbest = LINE_MAX;
            let mut bbest1 = LINE_MAX;
            let mut k = bmax;
            while k >= bmin {
                let mut i1 = vb[at(k)].max(off1);
                let mut i2 = i1 - k;
                if i2 < off2 {
                    i1 = off2 + k;
                    i2 = off2;
                }
                if i1 + i2 < bbest {
                    bbest = i1 + i2;
                    bbest1 = i1;
                }
                k -= 2;
            }
            if lim1 + lim2 - bbest < fbest - (off1 + off2) {
                return (fbest1 as usize, (fbest - fbest1) as usize, true, false);
            }
            return (bbest1 as usize, (bbest - bbest1) as usize, false, true);
        }
    }
}

// The weights of git's indent heuristic (xdiffi.c), tuned on a corpus of
// real changes.
const MAX_INDENT: i32 = 200;
const MAX_BLANKS: i32 = 20;
const START_OF_FILE_PENALTY: i32 = 1;
const END_OF_FILE_PENALTY: i32 = 21;
const TOTAL_BLANK_WEIGHT: i32 = -30;
const POST_BLANK_WEIGHT: i32 = 6;
const RELATIVE_INDENT_PENALTY: i32 = -4;
const RELATIVE_INDENT_WITH_BLANK_PENALTY: i32 = 10;
const RELATIVE_OUTDENT_PENALTY: i32 = 24;
const RELATIVE_OUTDENT_WITH_BLANK: i32 = 17;
const RELATIVE_DEDENT_PENALTY: i32 = 23;
const RELATIVE_DEDENT_WITH_BLANK: i32 = 17;
const INDENT_WEIGHT: i32 = 60;
const MAX_SLIDING: usize = 100;

/// Slides the groups of changed lines of one side up and down where the
/// same lines frame them, as git's `xdl_change_compact`.
fn compact(r: &mut [bool], ro: &[bool], ids: &[i32], lines: &[String]) {
    let mut g = Group::init(r);
    let mut g_other = Group::init(ro);
    loop {
        if g.end != g.start {
            let mut size;
            let mut earliest_end;
            let mut end_matching_other;
            loop {
                size = g.end - g.start;
                end_matching_other = None;
                while g.slide_up(r, ids) {
                    g_other.previous(ro);
                }
                earliest_end = g.end;
                if g_other.end > g_other.start {
                    end_matching_other = Some(g.end);
                }
                while g.slide_down(r, ids) {
                    g_other.next(ro);
                    if g_other.end > g_other.start {
                        end_matching_other = Some(g.end);
                    }
                }
                if size == g.end - g.start {
                    break;
                }
            }
            if g.end == earliest_end {
                // It could not move.
            } else if end_matching_other.is_some() {
                // Back to the last place matching a group of the other side.
                while g_other.end == g_other.start {
                    if !g.slide_up(r, ids) || !g_other.previous(ro) {
                        break;
                    }
                }
            } else {
                let mut shift = earliest_end
                    .max(g.end.saturating_sub(size + 1))
                    .max(g.end.saturating_sub(MAX_SLIDING));
                let mut best = None;
                let mut best_score = Score::default();
                while shift <= g.end {
                    let mut s = Score::default();
                    s.add(measure(lines, shift));
                    s.add(measure(lines, shift - size));
                    if best.is_none() || s.cmp(&best_score) <= 0 {
                        best_score = s;
                        best = Some(shift);
                    }
                    shift += 1;
                }
                let best = best.unwrap_or(g.end);
                while g.end > best {
                    if !g.slide_up(r, ids) || !g_other.previous(ro) {
                        break;
                    }
                }
            }
        }
        if !g.next(r) || !g_other.next(ro) {
            return;
        }
    }
}

/// A run of changed lines, maybe empty, between unchanged ones.
struct Group {
    start: usize,
    end: usize,
}

impl Group {
    fn init(r: &[bool]) -> Self {
        let mut g = Group { start: 0, end: 0 };
        while g.end < r.len() && r[g.end] {
            g.end += 1;
        }
        g
    }

    fn next(&mut self, r: &[bool]) -> bool {
        if self.end >= r.len() {
            return false;
        }
        self.start = self.end + 1;
        self.end = self.start;
        while self.end < r.len() && r[self.end] {
            self.end += 1;
        }
        true
    }

    fn previous(&mut self, r: &[bool]) -> bool {
        if self.start == 0 {
            return false;
        }
        self.end = self.start - 1;
        self.start = self.end;
        while self.start > 0 && r[self.start - 1] {
            self.start -= 1;
        }
        true
    }

    fn slide_down(&mut self, r: &mut [bool], ids: &[i32]) -> bool {
        if self.start < self.end && self.end < r.len() && ids[self.start] == ids[self.end] {
            r[self.start] = false;
            self.start += 1;
            r[self.end] = true;
            self.end += 1;
            while self.end < r.len() && r[self.end] {
                self.end += 1;
            }
            return true;
        }
        false
    }

    fn slide_up(&mut self, r: &mut [bool], ids: &[i32]) -> bool {
        if self.start < self.end && self.start > 0 && ids[self.start - 1] == ids[self.end - 1] {
            self.start -= 1;
            r[self.start] = true;
            self.end -= 1;
            r[self.end] = false;
            while self.start > 0 && r[self.start - 1] {
                self.start -= 1;
            }
            return true;
        }
        false
    }
}

/// How far a line is indented, tabs to multiples of 8, or `-1` for a
/// blank line.
fn indent(s: &str) -> i32 {
    let mut n = 0;
    for byte in s.bytes() {
        match byte {
            b' ' => n += 1,
            b'\t' => n += 8 - n % 8,
            b'\r' | b'\n' | 0x0c | 0x0b => {}
            _ => return n,
        }
        if n >= MAX_INDENT {
            return MAX_INDENT;
        }
    }
    -1
}

#[derive(Default)]
struct Split {
    end_of_file: bool,
    indent: i32,
    pre_blank: i32,
    pre_indent: i32,
    post_blank: i32,
    post_indent: i32,
}

fn measure(lines: &[String], at: usize) -> Split {
    let mut m = Split {
        pre_indent: -1,
        post_indent: -1,
        ..Split::default()
    };
    if at >= lines.len() {
        m.end_of_file = true;
        m.indent = -1;
    } else {
        m.indent = indent(&lines[at]);
    }
    let mut i = at;
    while i > 0 {
        i -= 1;
        m.pre_indent = indent(&lines[i]);
        if m.pre_indent != -1 {
            break;
        }
        m.pre_blank += 1;
        if m.pre_blank == MAX_BLANKS {
            m.pre_indent = 0;
            break;
        }
    }
    let mut i = at + 1;
    while i < lines.len() {
        m.post_indent = indent(&lines[i]);
        if m.post_indent != -1 {
            break;
        }
        m.post_blank += 1;
        if m.post_blank == MAX_BLANKS {
            m.post_indent = 0;
            break;
        }
        i += 1;
    }
    m
}

#[derive(Default)]
struct Score {
    effective_indent: i32,
    penalty: i32,
}

impl Score {
    fn add(&mut self, m: Split) {
        if m.pre_indent == -1 && m.pre_blank == 0 {
            self.penalty += START_OF_FILE_PENALTY;
        }
        if m.end_of_file {
            self.penalty += END_OF_FILE_PENALTY;
        }
        let mut post_blank = 0;
        if m.indent == -1 {
            post_blank = 1 + m.post_blank;
        }
        let total_blank = m.pre_blank + post_blank;
        self.penalty += TOTAL_BLANK_WEIGHT * total_blank + POST_BLANK_WEIGHT * post_blank;
        let mut inn = m.indent;
        if inn == -1 {
            inn = m.post_indent;
        }
        let blanks = total_blank != 0;
        self.effective_indent += inn;
        match () {
            _ if inn == -1 || m.pre_indent == -1 || inn == m.pre_indent => {}
            _ if inn > m.pre_indent => {
                self.penalty += pick(
                    blanks,
                    RELATIVE_INDENT_WITH_BLANK_PENALTY,
                    RELATIVE_INDENT_PENALTY,
                )
            }
            _ if m.post_indent != -1 && m.post_indent > inn => {
                self.penalty += pick(blanks, RELATIVE_OUTDENT_WITH_BLANK, RELATIVE_OUTDENT_PENALTY)
            }
            _ => self.penalty += pick(blanks, RELATIVE_DEDENT_WITH_BLANK, RELATIVE_DEDENT_PENALTY),
        }
    }

    fn cmp(&self, o: &Score) -> i32 {
        let c = match self.effective_indent.cmp(&o.effective_indent) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
        };
        INDENT_WEIGHT * c + self.penalty - o.penalty
    }
}

fn pick(c: bool, a: i32, b: i32) -> i32 {
    if c {
        a
    } else {
        b
    }
}

/// A range of bytes, `start` to `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

/// Compares two lines word by word, as review tools highlight what
/// changed within lines, and returns the byte ranges of each that
/// changed.
pub fn word_ranges(a: &str, b: &str) -> (Vec<ByteRange>, Vec<ByteRange>) {
    tokens(a, b, word_tokens)
}

fn tokens(a: &str, b: &str, split: fn(&str) -> Vec<ByteRange>) -> (Vec<ByteRange>, Vec<ByteRange>) {
    let ta = split(a);
    let tb = split(b);
    let mut ids: HashMap<&str, i32> = HashMap::with_capacity(ta.len() + tb.len());
    let mut next = 0;
    let ia = intern_tokens(a, &ta, &mut ids, &mut next);
    let ib = intern_tokens(b, &tb, &mut ids, &mut next);
    let hunks = ints_diff(&ia, &ib);
    let mut ra = Vec::new();
    let mut rb = Vec::new();
    for h in hunks {
        if h.a0 < h.a1 {
            join(&mut ra, ByteRange { start: ta[h.a0].start, end: ta[h.a1 - 1].end }, a);
        }
        if h.b0 < h.b1 {
            join(&mut rb, ByteRange { start: tb[h.b0].start, end: tb[h.b1 - 1].end }, b);
        }
    }
    (ra, rb)
}

fn intern_tokens<'a>(
    s: &'a str,
    t: &[ByteRange],
    ids: &mut HashMap<&'a str, i32>,
    next: &mut i32,
) -> Vec<i32> {
    t.iter()
        .map(|r| {
            let token = &s[r.start..r.end];
            if let Some(id) = ids.get(token) {
                return *id;
            }
            let id = *next;
            *next += 1;
            ids.insert(token, id);
            id
        })
        .collect()
}

/// Compares two interned sequences without the indent heuristic, as
/// MyGo's `diff.Ints`.
fn ints_diff(a: &[i32], b: &[i32]) -> Vec<Hunk> {    let mut differ = Differ::new(a.to_vec(), b.to_vec());
    differ.histogram(0, differ.a.len(), 0, differ.b.len(), 0);
    if differ.da.is_empty() {
        return Vec::new();
    }
    hunks_of(&differ.da, &differ.db)
}

/// Appends `r` to `rs`, merged with the last range when one character is
/// between them, as MyGo's `joinWordAlt` does: "a b c" changed reads as
/// one highlight rather than three.
fn join(rs: &mut Vec<ByteRange>, r: ByteRange, s: &str) {
    if let Some(last) = rs.last_mut() {
        let gap = &s[last.end..r.start];
        if gap.chars().count() <= 1 {
            last.end = r.end;
            return;
        }
    }
    rs.push(r);
}

fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

fn word_tokens(s: &str) -> Vec<ByteRange> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let first = s[i..].chars().next().expect("a char");
        let mut j = i + first.len_utf8();
        if is_word(first) {
            while j < s.len() {
                let c = s[j..].chars().next().expect("a char");
                if !is_word(c) {
                    break;
                }
                j += c.len_utf8();
            }
        } else if first != '\n' && first.is_whitespace() {
            while j < s.len() {
                let c = s[j..].chars().next().expect("a char");
                if c == '\n' || !c.is_whitespace() {
                    break;
                }
                j += c.len_utf8();
            }
        }
        out.push(ByteRange { start: i, end: j });
        i = j;
    }
    out
}

/// How far to push the lines of the shorter side of a hunk down so that
/// they pair with the most similar lines of the longer, as Pierre's
/// diffs realign change blocks.
pub fn align_by_similarity(a: &[String], b: &[String]) -> usize {
    let (short, long) = if a.len() > b.len() { (b, a) } else { (a, b) };
    if short.len() == long.len()
        || short.is_empty()
        || short.len() * (long.len() - short.len() + 1) > 4096
    {
        return 0;
    }
    let score = |off: usize| -> f64 {
        short
            .iter()
            .enumerate()
            .map(|(i, x)| similarity(x, &long[i + off]))
            .sum()
    };
    let mut best = 0;
    let mut best_score = score(0);
    for off in 1..=long.len() - short.len() {
        let s = score(off);
        if s > best_score + 0.5 * short.len() as f64 {
            best = off;
            best_score = s;
        }
    }
    best
}

fn similarity(a: &str, b: &str) -> f64 {
    let squeeze = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    let a = squeeze(a);
    let b = squeeze(b);
    if a == b {
        return 1.0;
    }
    let n = a.len().max(b.len());
    if n == 0 {
        return 1.0;
    }
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut p = 0;
    while p < ab.len() && p < bb.len() && ab[p] == bb[p] {
        p += 1;
    }
    let mut s = 0;
    while s < ab.len() - p && s < bb.len() - p && ab[ab.len() - 1 - s] == bb[bb.len() - 1 - s] {
        s += 1;
    }
    (p + s) as f64 / n as f64
}

/// One row of a split view: the old line and the new line it pairs with,
/// either missing where that side has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffRow {
    /// Old line index, zero-based.
    pub old: Option<usize>,
    /// New line index, zero-based.
    pub new: Option<usize>,
}

/// Pairs the lines of the two texts row by row as MyGo's split view does:
/// unchanged lines together, then each hunk's lines with the shorter side
/// pushed down by [`align_by_similarity`]'s offset.
pub fn split_rows(old: &[String], new: &[String], hunks: &[Hunk]) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let (mut pa, mut pb) = (0, 0);
    for h in hunks {
        for i in 0..h.a0 - pa {
            rows.push(DiffRow { old: Some(pa + i), new: Some(pb + i) });
        }
        pa = h.a1;
        pb = h.b1;
        let (na, nb) = (h.a1 - h.a0, h.b1 - h.b0);
        let count = na.max(nb);
        let offset = if na > 0 && nb > 0 && na != nb {
            align_by_similarity(&old[h.a0..h.a1], &new[h.b0..h.b1])
        } else {
            0
        };
        for r in 0..count {
            let at = |n0: usize, n: usize| -> Option<usize> {
                let mut k = r;
                if n < count {
                    k = r.checked_sub(offset)?;
                }
                (k < n).then_some(n0 + k)
            };
            rows.push(DiffRow { old: at(h.a0, na), new: at(h.b0, nb) });
        }
    }
    let rest = (old.len() - pa).min(new.len() - pb);
    for i in 0..rest {
        rows.push(DiffRow { old: Some(pa + i), new: Some(pb + i) });
    }
    rows
}

/// Computes a line diff as MyGo's `DiffView` reports it.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let a = split_lines(old);
    let b = split_lines(new);
    let hunks = lines_diff(&a, &b);
    let mut lines = Vec::with_capacity(a.len().max(b.len()));
    let (mut pa, mut pb) = (0, 0);
    for h in &hunks {
        for i in 0..h.a0 - pa {
            lines.push(DiffLine {
                kind: DiffKind::Same,
                text: a[pa + i].clone(),
                old_line: Some(pa + i + 1),
                new_line: Some(pb + i + 1),
            });
        }
        for i in h.a0..h.a1 {
            lines.push(DiffLine {
                kind: DiffKind::Removed,
                text: a[i].clone(),
                old_line: Some(i + 1),
                new_line: None,
            });
        }
        for j in h.b0..h.b1 {
            lines.push(DiffLine {
                kind: DiffKind::Added,
                text: b[j].clone(),
                old_line: None,
                new_line: Some(j + 1),
            });
        }
        pa = h.a1;
        pb = h.b1;
    }
    let rest = (a.len() - pa).min(b.len() - pb);
    for i in 0..rest {
        lines.push(DiffLine {
            kind: DiffKind::Same,
            text: a[pa + i].clone(),
            old_line: Some(pa + i + 1),
            new_line: Some(pb + i + 1),
        });
    }
    lines
}

/// Creates a diff viewer.
pub fn diff_viewer(
    context: &mut Context,
    id: &str,
    old: &str,
    new: &str,
    state: &mut DiffViewerState,
) -> DiffViewerResponse {
    context.diff_viewer(id, old, new, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_diff_handles_same_add_remove_and_mixed_inputs() {
        let same = diff_lines("a\nb", "a\nb");
        assert_eq!(same.len(), 2);
        assert!(same.iter().all(|l| l.kind == DiffKind::Same));

        let added = diff_lines("a", "a\nb");
        assert_eq!(added[1].kind, DiffKind::Added);
        assert_eq!(added[1].new_line, Some(2));

        let removed = diff_lines("a\nb", "a");
        assert_eq!(removed[1].kind, DiffKind::Removed);
        assert_eq!(removed[1].old_line, Some(2));

        let mixed = diff_lines("a\nb\nc", "a\nx\nc");
        assert_eq!(
            mixed.iter().map(|l| l.kind).collect::<Vec<_>>(),
            vec![
                DiffKind::Same,
                DiffKind::Removed,
                DiffKind::Added,
                DiffKind::Same
            ]
        );
    }

    #[test]
    fn a_trailing_newline_adds_an_empty_last_line() {
        assert_eq!(split_lines("a\n").len(), 2);
        assert_eq!(split_lines("").len(), 0);
        let hunks = diff_hunks("package greeter\n\nfunc G() {\n}\n", "package greeter\n\nimport \"strings\"\n\nfunc G() {\n}\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].b1 - hunks[0].b0, 2);
    }

    #[test]
    fn the_greeter_edit_reports_five_added_and_three_removed() {
        let old = concat!(
            "package greeter\n",
            "\n",
            "// Greet greets name, or the world.\n",
            "func Greet(name string) string {\n",
            "\t// TODO: trim spaces.\n",
            "\tif name == \"\" {\n",
            "\t\tname = \"world\"\n",
            "\t}\n",
            "\treturn \"Hello, \" + name + \"!\"\n",
            "}\n",
            "\n"
        );
        let new = concat!(
            "package greeter\n",
            "\n",
            "import \"strings\"\n",
            "\n",
            "// Greet greets name, or everyone.\n",
            "func Greet(name string) string {\n",
            "\tname = strings.TrimSpace(name)\n",
            "\tif name == \"\" {\n",
            "\t\tname = \"everyone\"\n",
            "\t}\n",
            "\treturn \"Hello, \" + name + \"!\"\n",
            "}\n",
            "\n"
        );
        let hunks = diff_hunks(old, new);
        let added: usize = hunks.iter().map(|h| h.b1 - h.b0).sum();
        let removed: usize = hunks.iter().map(|h| h.a1 - h.a0).sum();
        assert_eq!((added, removed), (5, 3));
    }

    #[test]
    fn word_ranges_mark_only_what_changed() {
        let (a, b) = word_ranges("name = \"world\"", "name = \"everyone\"");
        assert_eq!(&"name = \"world\""[a[0].start..a[0].end], "world");
        assert_eq!(&"name = \"everyone\""[b[0].start..b[0].end], "everyone");
    }
}
