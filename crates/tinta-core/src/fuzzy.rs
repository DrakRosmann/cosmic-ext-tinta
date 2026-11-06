//! Fuzzy matching for the command palette and quick open.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub struct Hit {
    pub index: usize,
    pub score: u32,
    /// Char positions that matched, for highlighting.
    pub positions: Vec<u32>,
}

/// Best matches for `query`, best first. An empty query keeps everything in order.
pub fn filter<'a>(
    items: impl Iterator<Item = &'a str>,
    query: &str,
    path_mode: bool,
    limit: usize,
) -> Vec<Hit> {
    if query.trim().is_empty() {
        return items.enumerate().take(limit).map(|(index, _)| Hit { index, score: 0, positions: Vec::new() }).collect();
    }
    let config = if path_mode { Config::DEFAULT.match_paths() } else { Config::DEFAULT };
    let mut matcher = Matcher::new(config);
    let pattern = Pattern::new(query, CaseMatching::Smart, Normalization::Smart, AtomKind::Fuzzy);
    let mut buf = Vec::new();
    let mut hits = Vec::new();
    for (index, item) in items.enumerate() {
        let hay = Utf32Str::new(item, &mut buf);
        let mut positions = Vec::new();
        if let Some(score) = pattern.indices(hay, &mut matcher, &mut positions) {
            positions.sort_unstable();
            positions.dedup();
            hits.push(Hit { index, score, positions });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_match_comes_first() {
        let items = ["src/main.rs", "src/editor/view.rs", "README.md", "docs/main_notes.md"];
        let hits = filter(items.iter().copied(), "mainrs", true, 10);
        assert_eq!(items[hits[0].index], "src/main.rs");
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].positions.is_empty());
    }

    #[test]
    fn empty_query_keeps_order_up_to_limit() {
        let items = ["b", "a", "c"];
        let hits = filter(items.iter().copied(), " ", false, 2);
        assert_eq!(hits.iter().map(|h| h.index).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn accents_and_case_are_forgiven() {
        let items = ["Configurações", "Outra"];
        let hits = filter(items.iter().copied(), "configuracoes", false, 10);
        assert_eq!(hits.len(), 1);
    }
}
