//! Line diff viewer.

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

/// Computes a line diff using a suffix LCS table.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines = old.lines().collect::<Vec<_>>();
    let new_lines = new.lines().collect::<Vec<_>>();
    let old_count = old_lines.len();
    let new_count = new_lines.len();
    if old_count.saturating_mul(new_count) > 4_000_000 {
        let mut lines = Vec::new();
        for (index, text) in old_lines.iter().enumerate() {
            lines.push(DiffLine {
                kind: DiffKind::Removed,
                text: text.to_string(),
                old_line: Some(index + 1),
                new_line: None,
            });
        }
        for (index, text) in new_lines.iter().enumerate() {
            lines.push(DiffLine {
                kind: DiffKind::Added,
                text: text.to_string(),
                old_line: None,
                new_line: Some(index + 1),
            });
        }
        return lines;
    }
    let stride = new_count + 1;
    let mut table = vec![0_u32; (old_count + 1) * stride];
    for i in (0..old_count).rev() {
        for j in (0..new_count).rev() {
            table[i * stride + j] = if old_lines[i] == new_lines[j] {
                table[(i + 1) * stride + j + 1] + 1
            } else {
                table[(i + 1) * stride + j].max(table[i * stride + j + 1])
            };
        }
    }
    let mut lines = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old_count && j < new_count {
        if old_lines[i] == new_lines[j] {
            lines.push(DiffLine {
                kind: DiffKind::Same,
                text: old_lines[i].to_string(),
                old_line: Some(i + 1),
                new_line: Some(j + 1),
            });
            i += 1;
            j += 1;
        } else if table[(i + 1) * stride + j] >= table[i * stride + j + 1] {
            lines.push(DiffLine {
                kind: DiffKind::Removed,
                text: old_lines[i].to_string(),
                old_line: Some(i + 1),
                new_line: None,
            });
            i += 1;
        } else {
            lines.push(DiffLine {
                kind: DiffKind::Added,
                text: new_lines[j].to_string(),
                old_line: None,
                new_line: Some(j + 1),
            });
            j += 1;
        }
    }
    while i < old_count {
        lines.push(DiffLine {
            kind: DiffKind::Removed,
            text: old_lines[i].to_string(),
            old_line: Some(i + 1),
            new_line: None,
        });
        i += 1;
    }
    while j < new_count {
        lines.push(DiffLine {
            kind: DiffKind::Added,
            text: new_lines[j].to_string(),
            old_line: None,
            new_line: Some(j + 1),
        });
        j += 1;
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

pub(crate) fn paired_rows(lines: &[DiffLine]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind == DiffKind::Same {
            rows.push((Some(index), Some(index)));
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && lines[index].kind != DiffKind::Same {
            index += 1;
        }
        let removed = (start..index)
            .filter(|line| lines[*line].kind == DiffKind::Removed)
            .collect::<Vec<_>>();
        let added = (start..index)
            .filter(|line| lines[*line].kind == DiffKind::Added)
            .collect::<Vec<_>>();
        for offset in 0..removed.len().max(added.len()) {
            rows.push((removed.get(offset).copied(), added.get(offset).copied()));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lines: &[DiffLine]) -> Vec<DiffKind> {
        lines.iter().map(|line| line.kind).collect()
    }

    #[test]
    fn line_diff_handles_same_add_remove_and_mixed_inputs() {
        let same = diff_lines("a\nb", "a\nb");
        assert_eq!(kinds(&same), vec![DiffKind::Same, DiffKind::Same]);

        let added = diff_lines("a", "a\nb");
        assert_eq!(kinds(&added), vec![DiffKind::Same, DiffKind::Added]);
        assert_eq!(added[1].new_line, Some(2));

        let removed = diff_lines("a\nb", "a");
        assert_eq!(kinds(&removed), vec![DiffKind::Same, DiffKind::Removed]);
        assert_eq!(removed[1].old_line, Some(2));

        let mixed = diff_lines("a\nb\nc", "a\nx\nc");
        assert_eq!(
            kinds(&mixed),
            vec![
                DiffKind::Same,
                DiffKind::Removed,
                DiffKind::Added,
                DiffKind::Same
            ]
        );
    }
}
