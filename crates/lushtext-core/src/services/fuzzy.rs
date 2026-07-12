// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared GTK-free fuzzy query state for application search surfaces.

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Reusable nucleo fuzzy query state for scoring many candidates.
pub(crate) struct FuzzyQuery {
    matcher: Matcher,
    atom: Atom,
    buf: Vec<char>,
}

impl FuzzyQuery {
    /// Build one matcher/atom pair with the application-wide fuzzy configuration.
    pub(crate) fn new(query: &str) -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            atom: Atom::new(
                query,
                CaseMatching::Ignore,
                Normalization::Smart,
                AtomKind::Fuzzy,
                false,
            ),
            buf: Vec::new(),
        }
    }

    /// Score one candidate while reusing the matcher and UTF-32 conversion buffer.
    pub(crate) fn score(&mut self, candidate: &str) -> Option<u32> {
        self.buf.clear();
        let haystack = Utf32Str::new(candidate, &mut self.buf);
        self.atom.score(haystack, &mut self.matcher).map(u32::from)
    }
}
