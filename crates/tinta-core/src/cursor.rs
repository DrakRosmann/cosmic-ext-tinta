//! Selections are pairs of char offsets. A cursor is a selection with `anchor == head`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
    /// Visual column kept while moving up and down through shorter lines.
    #[serde(skip)]
    pub goal: Option<u32>,
}

impl Selection {
    pub fn cursor(pos: usize) -> Self {
        Self { anchor: pos, head: pos, goal: None }
    }
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head, goal: None }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

/// One or more selections, always sorted and without overlaps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursors {
    sels: Vec<Selection>,
    primary: usize,
}

impl Default for Cursors {
    fn default() -> Self {
        Self::single(Selection::cursor(0))
    }
}

impl Cursors {
    pub fn single(sel: Selection) -> Self {
        Self { sels: vec![sel], primary: 0 }
    }

    pub fn from_vec(sels: Vec<Selection>, primary: usize) -> Self {
        let mut c = Self { sels, primary };
        if c.sels.is_empty() {
            c.sels.push(Selection::cursor(0));
        }
        c.normalize();
        c
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Selection> + ExactSizeIterator {
        self.sels.iter()
    }
    pub fn as_slice(&self) -> &[Selection] {
        &self.sels
    }
    pub fn len(&self) -> usize {
        self.sels.len()
    }
    pub fn is_empty(&self) -> bool {
        false
    }
    pub fn primary(&self) -> Selection {
        self.sels[self.primary]
    }
    pub fn primary_index(&self) -> usize {
        self.primary
    }
    pub fn head(&self) -> usize {
        self.primary().head
    }
    pub fn is_multi(&self) -> bool {
        self.sels.len() > 1
    }

    pub fn set_single(&mut self, sel: Selection) {
        self.sels.clear();
        self.sels.push(sel);
        self.primary = 0;
    }

    /// Adds a selection and makes it primary.
    pub fn add(&mut self, sel: Selection) {
        self.sels.push(sel);
        self.primary = self.sels.len() - 1;
        self.normalize();
    }

    /// Removes the selection holding `pos` when there are others; true if one was removed.
    pub fn remove_at(&mut self, pos: usize) -> bool {
        if self.sels.len() < 2 {
            return false;
        }
        if let Some(i) = self.sels.iter().position(|s| s.start() <= pos && pos <= s.end()) {
            self.sels.remove(i);
            self.primary = self.primary.min(self.sels.len() - 1);
            return true;
        }
        false
    }

    pub fn keep_primary(&mut self) {
        let p = self.primary();
        self.set_single(p);
    }

    pub fn map(&mut self, mut f: impl FnMut(Selection) -> Selection) {
        for s in &mut self.sels {
            *s = f(*s);
        }
        self.normalize();
    }

    /// Sorts and merges. Two cursors at the same offset, or overlapping selections, become one.
    pub fn normalize(&mut self) {
        let primary = self.sels[self.primary.min(self.sels.len() - 1)];
        self.sels.sort_by_key(|s| (s.start(), s.end()));
        let mut out: Vec<Selection> = Vec::with_capacity(self.sels.len());
        let mut new_primary = 0;
        for s in self.sels.drain(..) {
            let is_primary = s == primary;
            if let Some(last) = out.last_mut() {
                let touching = s.start() < last.end()
                    || (s.start() == last.end() && (s.is_empty() || last.is_empty()));
                if touching {
                    let start = last.start().min(s.start());
                    let end = last.end().max(s.end());
                    let rev = last.reversed();
                    *last = if rev { Selection::new(end, start) } else { Selection::new(start, end) };
                    if is_primary {
                        new_primary = out.len() - 1;
                    }
                    continue;
                }
            }
            out.push(s);
            if is_primary {
                new_primary = out.len() - 1;
            }
        }
        self.sels = out;
        self.primary = new_primary;
    }

    pub fn clamp(&mut self, max: usize) {
        self.map(|s| Selection { anchor: s.anchor.min(max), head: s.head.min(max), goal: s.goal });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_overlaps_and_tracks_primary() {
        let c = Cursors::from_vec(vec![Selection::new(0, 5), Selection::new(3, 8), Selection::cursor(20)], 2);
        assert_eq!(c.len(), 2);
        assert_eq!(c.primary().head, 20);
        assert_eq!(c.as_slice()[0], Selection::new(0, 8));
    }

    #[test]
    fn duplicate_cursors_collapse() {
        let c = Cursors::from_vec(vec![Selection::cursor(4), Selection::cursor(4)], 0);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn adjacent_selections_stay_apart() {
        let c = Cursors::from_vec(vec![Selection::new(0, 3), Selection::new(3, 6)], 0);
        assert_eq!(c.len(), 2);
    }
}
