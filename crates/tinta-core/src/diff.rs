//! Two texts lined up row by row, for the side-by-side view.

use similar::{ChangeTag, DiffTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Equal,
    Removed,
    Added,
    Changed,
}

/// One row on screen: a line from each side, or a gap on the side that doesn't have it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub kind: RowKind,
    /// The parts of a changed line that actually differ, as char columns.
    pub left_spans: Vec<(u32, u32)>,
    pub right_spans: Vec<(u32, u32)>,
}

/// Past this length, working out what changed inside a line isn't worth the wait.
const MAX_INLINE: usize = 2000;

fn inline_changes(a: &str, b: &str) -> (Vec<(u32, u32)>, Vec<(u32, u32)>) {
    if a.len() > MAX_INLINE || b.len() > MAX_INLINE {
        return (Vec::new(), Vec::new());
    }
    let diff = TextDiff::from_chars(a, b);
    // Two lines with little in common read better as a whole than as confetti.
    if diff.ratio() < 0.35 {
        return (Vec::new(), Vec::new());
    }
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let (mut la, mut rb) = (0u32, 0u32);
    let push = |list: &mut Vec<(u32, u32)>, at: u32| match list.last_mut() {
        Some(last) if last.1 == at => last.1 = at + 1,
        _ => list.push((at, at + 1)),
    };
    for change in diff.iter_all_changes() {
        let n = change.value().chars().count() as u32;
        match change.tag() {
            ChangeTag::Equal => {
                la += n;
                rb += n;
            }
            ChangeTag::Delete => {
                for i in 0..n {
                    push(&mut left, la + i);
                }
                la += n;
            }
            ChangeTag::Insert => {
                for i in 0..n {
                    push(&mut right, rb + i);
                }
                rb += n;
            }
        }
    }
    (left, right)
}

pub fn side_by_side(old: &str, new: &str) -> Vec<DiffRow> {
    let diff = TextDiff::from_lines(old, new);
    let olds = diff.old_slices();
    let news = diff.new_slices();
    let mut rows = Vec::new();
    let plain = |left: Option<usize>, right: Option<usize>, kind| DiffRow {
        left: left.map(|l| l as u32),
        right: right.map(|l| l as u32),
        kind,
        left_spans: Vec::new(),
        right_spans: Vec::new(),
    };
    for op in diff.ops() {
        let (o, n) = (op.old_range(), op.new_range());
        match op.tag() {
            DiffTag::Equal => rows.extend(o.zip(n).map(|(a, b)| plain(Some(a), Some(b), RowKind::Equal))),
            DiffTag::Delete => rows.extend(o.map(|a| plain(Some(a), None, RowKind::Removed))),
            DiffTag::Insert => rows.extend(n.map(|b| plain(None, Some(b), RowKind::Added))),
            DiffTag::Replace => {
                let paired = o.len().min(n.len());
                for i in 0..paired {
                    let (a, b) = (o.start + i, n.start + i);
                    let (ls, rs) = inline_changes(olds[a].trim_end_matches('\n'), news[b].trim_end_matches('\n'));
                    rows.push(DiffRow { left: Some(a as u32), right: Some(b as u32), kind: RowKind::Changed, left_spans: ls, right_spans: rs });
                }
                rows.extend((o.start + paired..o.end).map(|a| plain(Some(a), None, RowKind::Removed)));
                rows.extend((n.start + paired..n.end).map(|b| plain(None, Some(b), RowKind::Added)));
            }
        }
    }
    rows
}

/// Rows where a run of changes starts, for jumping between them.
pub fn hunk_starts(rows: &[DiffRow]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut prev_equal = true;
    for (i, r) in rows.iter().enumerate() {
        let equal = r.kind == RowKind::Equal;
        if !equal && prev_equal {
            out.push(i);
        }
        prev_equal = equal;
    }
    out
}

/// How much changed: lines added, lines removed.
pub fn stats(rows: &[DiffRow]) -> (usize, usize) {
    let added = rows.iter().filter(|r| matches!(r.kind, RowKind::Added | RowKind::Changed)).count();
    let removed = rows.iter().filter(|r| matches!(r.kind, RowKind::Removed | RowKind::Changed)).count();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(rows: &[DiffRow]) -> Vec<RowKind> {
        rows.iter().map(|r| r.kind).collect()
    }

    #[test]
    fn lines_up_equal_added_removed_and_changed() {
        let rows = side_by_side("a\nb\nc\nd\n", "a\nB\nc\nx\nd\n");
        assert_eq!(kinds(&rows), vec![RowKind::Equal, RowKind::Changed, RowKind::Equal, RowKind::Added, RowKind::Equal]);
        assert_eq!((rows[3].left, rows[3].right), (None, Some(3)));
        assert_eq!((rows[4].left, rows[4].right), (Some(3), Some(4)));
        let rows = side_by_side("a\nb\n", "a\n");
        assert_eq!((rows[1].left, rows[1].right, rows[1].kind), (Some(1), None, RowKind::Removed));
    }

    #[test]
    fn a_replacement_of_unequal_length_pairs_what_it_can() {
        let rows = side_by_side("x\ny\nz\n", "1\n");
        assert_eq!(kinds(&rows), vec![RowKind::Changed, RowKind::Removed, RowKind::Removed]);
        let rows = side_by_side("x\n", "1\n2\n");
        assert_eq!(kinds(&rows), vec![RowKind::Changed, RowKind::Added]);
    }

    #[test]
    fn inline_spans_mark_what_differs_in_chars() {
        let rows = side_by_side("let ação = 1;\n", "let ação = 22;\n");
        assert_eq!(rows[0].kind, RowKind::Changed);
        assert_eq!(rows[0].left_spans, vec![(11, 12)]);
        assert_eq!(rows[0].right_spans, vec![(11, 13)]);
    }

    #[test]
    fn unrelated_lines_get_no_confetti() {
        let rows = side_by_side("abcdefgh\n", "12345678\n");
        assert!(rows[0].left_spans.is_empty() && rows[0].right_spans.is_empty());
    }

    #[test]
    fn hunks_and_stats() {
        let rows = side_by_side("a\nb\nc\nd\ne\n", "a\nX\nc\nd\nY\nZ\n");
        assert_eq!(hunk_starts(&rows), vec![1, 4]);
        assert_eq!(stats(&rows), (3, 2));
        assert!(hunk_starts(&side_by_side("a\n", "a\n")).is_empty());
    }

    #[test]
    fn empty_sides_and_no_final_newline() {
        assert_eq!(kinds(&side_by_side("", "a\nb\n")), vec![RowKind::Added, RowKind::Added]);
        assert_eq!(kinds(&side_by_side("a\n", "")), vec![RowKind::Removed]);
        assert_eq!(kinds(&side_by_side("a", "a\n")), vec![RowKind::Changed]);
    }
}
