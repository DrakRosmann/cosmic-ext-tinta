//! Maps document lines to rows on screen: folds hide lines, word wrap splits them.

use crate::buffer::{ChangeRec, Document};
use ropey::RopeSlice;
use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

/// Cells a char takes when it starts at visual column `vcol`.
pub fn char_width(ch: char, vcol: usize, tab: usize) -> usize {
    match ch {
        '\t' => tab - vcol % tab,
        c => c.width().unwrap_or(1),
    }
}

/// Visual column of char column `col` in a line.
pub fn char_to_vcol(line: RopeSlice<'_>, col: usize, tab: usize) -> usize {
    let mut v = 0;
    for ch in line.chars().take(col) {
        v += char_width(ch, v, tab);
    }
    v
}

/// Char column nearest to visual column `target`, with `round` clicks past a cell's middle go right.
pub fn vcol_to_char(line: RopeSlice<'_>, target: usize, tab: usize) -> usize {
    let mut v = 0;
    for (i, ch) in line.chars().enumerate() {
        let w = char_width(ch, v, tab);
        if ch == '\n' || v + w > target {
            return i;
        }
        v += w;
    }
    line.len_chars()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Row {
    pub line: u32,
    /// Char column where the row starts.
    pub start: u32,
    /// Char column where the row ends (exclusive).
    pub end: u32,
    /// Blank cells before the text in continuation rows of a wrapped line.
    pub pad: u16,
}

/// Lines hidden by folding, as (header line, last hidden line).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folds {
    pub ranges: Vec<(u32, u32)>,
    #[serde(skip)]
    pub generation: u64,
}

impl Folds {
    pub fn is_folded(&self, line: usize) -> bool {
        self.ranges.iter().any(|r| r.0 as usize == line)
    }

    pub fn is_hidden(&self, line: usize) -> bool {
        self.ranges.iter().any(|r| (r.0 as usize) < line && line <= r.1 as usize)
    }

    pub fn add(&mut self, start: usize, end: usize) {
        if end <= start {
            return;
        }
        self.ranges.retain(|r| !(r.0 as usize >= start && r.1 as usize <= end));
        self.ranges.push((start as u32, end as u32));
        self.ranges.sort_unstable();
        self.generation += 1;
    }

    pub fn remove(&mut self, line: usize) -> bool {
        let n = self.ranges.len();
        self.ranges.retain(|r| r.0 as usize != line);
        self.generation += 1;
        n != self.ranges.len()
    }

    /// Opens every fold that hides `line`.
    pub fn reveal(&mut self, line: usize) -> bool {
        let n = self.ranges.len();
        self.ranges.retain(|r| !((r.0 as usize) < line && line <= r.1 as usize));
        if n != self.ranges.len() {
            self.generation += 1;
            return true;
        }
        false
    }

    pub fn clear(&mut self) {
        self.ranges.clear();
        self.generation += 1;
    }

    pub fn on_change(&mut self, c: &ChangeRec) {
        if c.removed_lines == 0 && c.inserted_lines == 0 {
            // Edits within one line only matter if they touch hidden text, which cannot be typed.
            return;
        }
        let first = c.start_line as u32;
        let last_removed = first + c.removed_lines as u32;
        let delta = c.inserted_lines as i64 - c.removed_lines as i64;
        self.ranges.retain_mut(|r| {
            if r.1 < first {
                return true;
            }
            if r.0 > last_removed || (r.0 > first && c.removed_lines == 0) {
                r.0 = (r.0 as i64 + delta) as u32;
                r.1 = (r.1 as i64 + delta) as u32;
                return true;
            }
            // The edit reaches into the fold: open it rather than guess.
            false
        });
        self.generation += 1;
    }
}

/// Last line of the block a header at `line` opens, by indentation; `None` if not a header.
pub fn fold_range(doc: &Document, line: usize, tab: usize) -> Option<usize> {
    if let Some(st) = &doc.structure
        && st.has_tree()
    {
        return st.fold_end(line).filter(|e| *e < doc.len_lines());
    }
    if line + 1 >= doc.len_lines() || doc.line_is_blank(line) {
        return None;
    }
    let base = line_indent_vcols(doc, line, tab);
    let mut last = None;
    let mut l = line + 1;
    let total = doc.len_lines();
    while l < total {
        if !doc.line_is_blank(l) {
            if line_indent_vcols(doc, l, tab) <= base {
                break;
            }
            last = Some(l);
        }
        l += 1;
    }
    last
}

/// Cheap check for the gutter: does the next non-blank line step in?
pub fn is_foldable(doc: &Document, line: usize, tab: usize) -> bool {
    if let Some(st) = &doc.structure
        && st.has_tree()
    {
        return st.fold_end(line).is_some();
    }
    if doc.line_is_blank(line) {
        return false;
    }
    let base = line_indent_vcols(doc, line, tab);
    let total = doc.len_lines();
    let mut l = line + 1;
    let limit = (line + 200).min(total);
    while l < limit {
        if !doc.line_is_blank(l) {
            return line_indent_vcols(doc, l, tab) > base;
        }
        l += 1;
    }
    false
}

pub fn line_indent_vcols(doc: &Document, line: usize, tab: usize) -> usize {
    let mut v = 0;
    for ch in doc.line_slice(line).chars() {
        match ch {
            ' ' => v += 1,
            '\t' => v += tab - v % tab,
            _ => break,
        }
    }
    v
}

/// Header lines enclosing `line`, outermost first, for sticky scroll.
pub fn enclosing_headers(doc: &Document, line: usize, tab: usize, max: usize) -> Vec<usize> {
    if let Some(st) = &doc.structure
        && st.has_tree()
    {
        let mut out = st.enclosing(line);
        if out.len() > max {
            out.drain(..out.len() - max);
        }
        return out;
    }
    let mut out = Vec::new();
    if line >= doc.len_lines() {
        return out;
    }
    let mut l = line;
    while l > 0 && doc.line_is_blank(l) {
        l -= 1;
    }
    let mut indent = line_indent_vcols(doc, l, tab);
    let mut scanned = 0;
    while l > 0 && indent > 0 && scanned < 5000 {
        l -= 1;
        scanned += 1;
        if doc.line_is_blank(l) {
            continue;
        }
        let i = line_indent_vcols(doc, l, tab);
        if i < indent {
            let text = doc.line_text(l);
            let t = text.trim();
            // Closing braces and such are not much of a header.
            if !t.starts_with(['}', ')', ']']) && !t.starts_with("//") && !t.starts_with('#') {
                out.push(l);
            }
            indent = i;
        }
    }
    out.reverse();
    if out.len() > max {
        out.drain(..out.len() - max);
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct DisplayMap {
    /// Empty when lines and rows are the same thing.
    rows: Vec<Row>,
    identity: bool,
    pub tab: usize,
    pub wrap: Option<usize>,
    built_for: (u64, u64, Option<usize>, usize),
    has_built: bool,
    /// Line → first row, when there is a `rows` table.
    first_row: Vec<u32>,
}

impl DisplayMap {
    pub fn new(doc: &Document, folds: &Folds, wrap: Option<usize>, tab: usize) -> Self {
        let mut m = Self::default();
        m.rebuild(doc, folds, wrap, tab);
        m
    }

    /// Rebuilds if anything it depends on changed.
    pub fn update(&mut self, doc: &Document, folds: &Folds, wrap: Option<usize>, tab: usize) {
        let key = (doc.revision, folds.generation, wrap, tab);
        if !self.has_built || self.built_for != key {
            self.rebuild(doc, folds, wrap, tab);
        }
    }

    fn rebuild(&mut self, doc: &Document, folds: &Folds, wrap: Option<usize>, tab: usize) {
        self.tab = tab;
        self.wrap = wrap;
        self.built_for = (doc.revision, folds.generation, wrap, tab);
        self.has_built = true;
        self.rows.clear();
        self.first_row.clear();
        self.identity = wrap.is_none() && folds.ranges.is_empty();
        if self.identity {
            return;
        }
        let total = doc.len_lines();
        self.first_row.resize(total, 0);
        let mut f = folds.ranges.iter().peekable();
        let mut line = 0;
        while line < total {
            self.first_row[line] = self.rows.len() as u32;
            match wrap {
                None => self.rows.push(Row {
                    line: line as u32,
                    start: 0,
                    end: doc.line_len(line) as u32,
                    pad: 0,
                }),
                Some(w) => wrap_line(doc, line, w.max(8), tab, &mut self.rows),
            }
            while f.peek().is_some_and(|r| (r.1 as usize) <= line) {
                f.next();
            }
            if let Some(r) = f.peek()
                && r.0 as usize == line
            {
                // Hidden lines map to the row of their header.
                let header_row = self.rows.len() as u32 - 1;
                for l in line + 1..=(r.1 as usize).min(total - 1) {
                    self.first_row[l] = header_row;
                }
                line = r.1 as usize + 1;
                continue;
            }
            line += 1;
        }
    }

    pub fn row_count(&self, doc: &Document) -> usize {
        if self.identity { doc.len_lines() } else { self.rows.len() }
    }

    pub fn row(&self, doc: &Document, i: usize) -> Row {
        if self.identity {
            let i = i.min(doc.len_lines() - 1);
            Row { line: i as u32, start: 0, end: doc.line_len(i) as u32, pad: 0 }
        } else {
            self.rows.get(i).or(self.rows.last()).copied().unwrap_or_default()
        }
    }

    pub fn line_to_row(&self, line: usize) -> usize {
        if self.identity { line } else { self.first_row.get(line).or(self.first_row.last()).copied().unwrap_or(0) as usize }
    }

    /// (row, visual column within the row, not counting the row's pad).
    pub fn pos_to_row(&self, doc: &Document, pos: usize) -> (usize, usize) {
        let (line, col) = doc.line_col(pos);
        if self.identity {
            return (line, char_to_vcol(doc.line_slice(line), col, self.tab));
        }
        let first = self.line_to_row(line);
        let mut r = first;
        // A hidden line lands on its fold header.
        if self.rows.get(first).is_some_and(|row| row.line as usize != line) {
            return (first, 0);
        }
        while let Some(next) = self.rows.get(r + 1) {
            if next.line as usize != line || (next.start as usize) > col {
                break;
            }
            r += 1;
        }
        let row = self.rows[r.min(self.rows.len() - 1)];
        let slice = doc.line_slice(line);
        let v0 = char_to_vcol(slice, row.start as usize, self.tab);
        (r, char_to_vcol(slice, col, self.tab) - v0)
    }

    /// Document offset at a visual position.
    pub fn row_to_pos(&self, doc: &Document, row: usize, vcol: usize) -> usize {
        let r = self.row(doc, row);
        let line = r.line as usize;
        let slice = doc.line_slice(line);
        let v0 = char_to_vcol(slice, r.start as usize, self.tab);
        let col = vcol_to_char(slice, v0 + vcol, self.tab);
        let wrapped_more = self.rows.get(row + 1).is_some_and(|n| n.line == r.line);
        // A wrapped row's end is the next row's start: stay on this row.
        let max = if wrapped_more { (r.end as usize).saturating_sub(1).max(r.start as usize) } else { r.end as usize };
        doc.line_start(line) + col.clamp(r.start as usize, max)
    }
}

fn wrap_line(doc: &Document, line: usize, width: usize, tab: usize, out: &mut Vec<Row>) {
    let slice = doc.line_slice(line);
    let len = slice.len_chars();
    let indent = line_indent_vcols(doc, line, tab);
    let pad = if indent < width / 2 { indent } else { 0 };
    let mut start = 0usize;
    let mut v = 0usize; // visual column in the line
    let mut row_v0 = 0usize;
    let mut last_break: Option<(usize, usize)> = None; // (char, vcol at it)
    let mut first = true;
    let mut i = 0usize;
    for ch in slice.chars() {
        let w = char_width(ch, v, tab);
        let avail = if first { width } else { width - pad };
        if v + w - row_v0 > avail && i > start {
            let (brk, brk_v) = match last_break {
                Some((b, bv)) if b > start => (b, bv),
                _ => (i, v),
            };
            out.push(Row { line: line as u32, start: start as u32, end: brk as u32, pad: if first { 0 } else { pad as u16 } });
            start = brk;
            row_v0 = brk_v;
            first = false;
            last_break = None;
        }
        v += w;
        i += 1;
        if ch == ' ' || ch == '\t' {
            last_break = Some((i, v));
        }
    }
    out.push(Row { line: line as u32, start: start as u32, end: len as u32, pad: if first { 0 } else { pad as u16 } });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_map() {
        let d = Document::new("abc\n\tde\n");
        let m = DisplayMap::new(&d, &Folds::default(), None, 4);
        assert_eq!(m.row_count(&d), 3);
        assert_eq!(m.pos_to_row(&d, 5), (1, 4));
        assert_eq!(m.row_to_pos(&d, 1, 5), 6);
        assert_eq!(m.row_to_pos(&d, 0, 99), 3);
    }

    #[test]
    fn wide_chars_take_two_cells() {
        let d = Document::new("日本語x\n");
        let m = DisplayMap::new(&d, &Folds::default(), None, 4);
        assert_eq!(m.pos_to_row(&d, 3), (0, 6));
        assert_eq!(m.row_to_pos(&d, 0, 6), 3);
        assert_eq!(m.row_to_pos(&d, 0, 3), 1);
    }

    #[test]
    fn wrap_breaks_at_spaces_and_positions_round_trip() {
        let d = Document::new("aaaa bbbb cccc dddd\nshort\n");
        let m = DisplayMap::new(&d, &Folds::default(), Some(10), 4);
        assert_eq!(m.row_count(&d), 4);
        assert_eq!(m.row(&d, 0), Row { line: 0, start: 0, end: 10, pad: 0 });
        assert_eq!(m.row(&d, 1), Row { line: 0, start: 10, end: 19, pad: 0 });
        assert_eq!(m.row(&d, 2).line, 1);
        for pos in 0..d.len_chars() {
            let (r, v) = m.pos_to_row(&d, pos);
            assert_eq!(m.row_to_pos(&d, r, v), pos, "pos {pos}");
        }
    }

    #[test]
    fn wrap_hard_breaks_long_words_and_keeps_indent() {
        let d = Document::new("    xxxxxxxxxxxxxxxxxxxxxxxxx");
        let m = DisplayMap::new(&d, &Folds::default(), Some(12), 4);
        assert!(m.row_count(&d) >= 3);
        assert_eq!(m.row(&d, 1).pad, 4);
        let last = m.row(&d, m.row_count(&d) - 1);
        assert_eq!(last.end as usize, d.line_len(0));
    }

    #[test]
    fn folds_hide_lines_and_cursors_map_to_the_header() {
        let d = Document::new("fn a() {\n    x\n    y\n}\nend\n");
        assert!(is_foldable(&d, 0, 4));
        assert!(!is_foldable(&d, 1, 4));
        assert_eq!(fold_range(&d, 0, 4), Some(2));
        let mut folds = Folds::default();
        folds.add(0, 2);
        let m = DisplayMap::new(&d, &folds, None, 4);
        assert_eq!(m.row_count(&d), 4);
        assert_eq!(m.row(&d, 1).line, 3);
        assert_eq!(m.line_to_row(2), 0);
        assert_eq!(m.pos_to_row(&d, d.pos(1, 2)), (0, 0));
        assert!(folds.is_hidden(2) && !folds.is_hidden(3));
    }

    #[test]
    fn folds_follow_line_changes() {
        let mut f = Folds::default();
        f.add(10, 20);
        // Line added above: shifts down.
        f.on_change(&ChangeRec { at: 0, removed: 0, inserted: 1, start_line: 2, removed_lines: 0, inserted_lines: 1 });
        assert_eq!(f.ranges, vec![(11, 21)]);
        // Below: stays.
        f.on_change(&ChangeRec { at: 0, removed: 0, inserted: 1, start_line: 30, removed_lines: 0, inserted_lines: 1 });
        assert_eq!(f.ranges, vec![(11, 21)]);
        // Deleting across it: opens.
        f.on_change(&ChangeRec { at: 0, removed: 9, inserted: 0, start_line: 15, removed_lines: 9, inserted_lines: 0 });
        assert!(f.ranges.is_empty());
    }

    #[test]
    fn sticky_headers_are_the_enclosing_blocks() {
        let d = Document::new("mod m {\n    fn f() {\n        if x {\n            y\n        }\n    }\n}\n");
        assert_eq!(enclosing_headers(&d, 3, 4, 5), vec![0, 1, 2]);
        assert_eq!(enclosing_headers(&d, 3, 4, 2), vec![1, 2]);
        assert_eq!(enclosing_headers(&d, 0, 4, 5), Vec::<usize>::new());
    }
}
