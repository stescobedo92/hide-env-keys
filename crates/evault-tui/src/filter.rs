//! Fuzzy filter state — backs the Ctrl+F overlay.
//!
//! The filter has two modes:
//!
//! - **Input active** — the user is typing the needle. Every printable
//!   character is appended; Backspace pops one; Enter accepts the
//!   needle and switches to *applied* mode; Esc cancels the filter
//!   entirely.
//! - **Applied** — the input box is hidden but the row buffer is still
//!   filtered. Action keys (`j`/`k`, `s`, `?`, …) work normally again.
//!   Ctrl+F re-opens the input.
//!
//! Matching is delegated to [`nucleo_matcher`]; we keep one [`Matcher`]
//! instance alive across keystrokes so its internal allocations are
//! re-used.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};

/// Filter state owned by [`crate::AppState`].
///
/// `visible_indices` lists the indices into the underlying row buffer
/// in match-score order (best score first). When the needle is empty
/// every row is visible in its original order.
#[allow(clippy::redundant_pub_crate)]
#[derive(Debug)]
pub(crate) struct FilterState {
    needle: String,
    visible_indices: Vec<usize>,
    input_active: bool,
    matcher: Matcher,
}

impl FilterState {
    pub(crate) fn new(row_count: usize) -> Self {
        Self {
            needle: String::new(),
            visible_indices: (0..row_count).collect(),
            input_active: true,
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    pub(crate) fn needle(&self) -> &str {
        &self.needle
    }

    pub(crate) const fn input_active(&self) -> bool {
        self.input_active
    }

    pub(crate) fn visible_indices(&self) -> &[usize] {
        &self.visible_indices
    }

    pub(crate) const fn commit(&mut self) {
        self.input_active = false;
    }

    pub(crate) const fn reopen_input(&mut self) {
        self.input_active = true;
    }

    /// Append `c` to the needle and re-rank the visible rows.
    ///
    /// `haystacks` must be the canonical names of every row in
    /// [`crate::AppState::rows`], in the same order.
    pub(crate) fn push(&mut self, c: char, haystacks: &[&str]) {
        self.needle.push(c);
        self.rerank(haystacks);
    }

    /// Pop the last character from the needle.
    pub(crate) fn pop(&mut self, haystacks: &[&str]) {
        self.needle.pop();
        self.rerank(haystacks);
    }

    fn rerank(&mut self, haystacks: &[&str]) {
        if self.needle.is_empty() {
            self.visible_indices = (0..haystacks.len()).collect();
            return;
        }
        // Use the higher-level `Pattern` API rather than calling
        // `Matcher::fuzzy_match` directly. `Pattern` overrides the
        // matcher's `ignore_case` / `normalize` config per-atom, which
        // is the only safe entry point when feeding mixed-case needles
        // against uppercase haystacks (`Matcher::fuzzy_match` panics
        // inside its prefilter when case mismatch trips an invariant).
        let pattern = Pattern::new(
            &self.needle,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut scored: Vec<(usize, u32)> = Vec::with_capacity(haystacks.len());
        for (idx, haystack) in haystacks.iter().enumerate() {
            let haystack_buf = Utf32String::from(*haystack);
            if let Some(score) = pattern.score(haystack_buf.slice(..), &mut self.matcher) {
                scored.push((idx, score));
            }
        }
        // Best score first; tie-breaker by original index for stability.
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        self.visible_indices = scored.into_iter().map(|(idx, _)| idx).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needle_shows_every_row() {
        let mut f = FilterState::new(3);
        f.rerank(&["A", "B", "C"]);
        assert_eq!(f.visible_indices(), &[0, 1, 2]);
    }

    #[test]
    fn typed_needle_ranks_matches_highest() {
        let mut f = FilterState::new(4);
        let rows = ["DATABASE_URL", "API_KEY", "DB_HOST", "NODE_ENV"];
        f.push('D', &rows);
        f.push('B', &rows);
        // Both DATABASE_URL and DB_HOST contain D and B; API_KEY does not.
        let visible = f.visible_indices();
        assert!(!visible.contains(&1), "API_KEY must not match `DB`");
        assert!(visible.contains(&0)); // DATABASE_URL
        assert!(visible.contains(&2)); // DB_HOST
    }

    #[test]
    fn pop_restores_previously_filtered_out_rows() {
        let mut f = FilterState::new(2);
        let rows = ["DATABASE_URL", "API_KEY"];
        f.push('X', &rows);
        assert!(f.visible_indices().is_empty());
        f.pop(&rows);
        assert_eq!(f.visible_indices(), &[0, 1]);
    }

    #[test]
    fn commit_and_reopen_input() {
        let mut f = FilterState::new(0);
        assert!(f.input_active());
        f.commit();
        assert!(!f.input_active());
        f.reopen_input();
        assert!(f.input_active());
    }
}
