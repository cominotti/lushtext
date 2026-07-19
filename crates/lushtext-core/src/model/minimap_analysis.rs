// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust line analysis shared by sliced minimap layout and marker work.

/// Character and retained-marker policy for one minimap analysis generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimapAnalysisPolicy {
    /// Strict character count above which a line receives a warning marker.
    pub warning_line_chars: usize,
    /// Strict character count above which wrapped minimap layout is rejected.
    pub wrapped_line_chars: usize,
    /// Maximum warning-line identities retained for projection.
    pub marker_limit: usize,
}

/// Accepted GTK-free result from one complete content generation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MinimapAnalysisResult {
    /// Whether any logical line exceeded the wrapped-layout character budget.
    pub wrapped_layout_too_large: bool,
    /// Bounded warning-line identities in source order.
    pub long_line_lines: Vec<u32>,
    /// Characters examined across every bounded slice.
    pub characters_examined: u64,
    /// Logical lines reached by the complete scan.
    pub lines_examined: u64,
}

/// Incremental logical-line accumulator independent of GTK iterator ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinimapAnalysisAccumulator {
    policy: MinimapAnalysisPolicy,
    collect_markers: bool,
    current_line: u32,
    current_line_chars: usize,
    current_line_marked: bool,
    wrapped_layout_too_large: bool,
    long_line_lines: Vec<u32>,
    characters_examined: u64,
}

impl MinimapAnalysisAccumulator {
    /// Start one content scan, optionally retaining long-line marker identities.
    #[must_use]
    pub fn new(policy: MinimapAnalysisPolicy, collect_markers: bool) -> Self {
        Self {
            policy,
            collect_markers,
            current_line: 0,
            current_line_chars: 0,
            current_line_marked: false,
            wrapped_layout_too_large: false,
            long_line_lines: Vec::with_capacity(policy.marker_limit.min(64)),
            characters_examined: 0,
        }
    }

    /// Inspect at most `character_limit` scalars from one caller-owned iterator.
    pub fn inspect_slice(
        &mut self,
        characters: impl IntoIterator<Item = char>,
        character_limit: usize,
    ) -> usize {
        let mut inspected = 0usize;
        for character in characters.into_iter().take(character_limit) {
            self.inspect_char(character);
            inspected = inspected.saturating_add(1);
        }
        inspected
    }

    /// Inspect one scalar from a GTK-owned cursor without retaining GTK state.
    pub fn inspect_char(&mut self, character: char) {
        self.characters_examined = self.characters_examined.saturating_add(1);
        if character == '\n' {
            self.current_line = self.current_line.saturating_add(1);
            self.current_line_chars = 0;
            self.current_line_marked = false;
            return;
        }

        self.current_line_chars = self.current_line_chars.saturating_add(1);
        if self.current_line_chars > self.policy.wrapped_line_chars {
            self.wrapped_layout_too_large = true;
        }
        if self.collect_markers
            && !self.current_line_marked
            && self.current_line_chars > self.policy.warning_line_chars
        {
            self.current_line_marked = true;
            if self.long_line_lines.len() < self.policy.marker_limit {
                self.long_line_lines.push(self.current_line);
            }
        }
    }

    /// Return whether layout evidence already found an extreme logical line.
    #[must_use]
    pub fn wrapped_layout_too_large(&self) -> bool {
        self.wrapped_layout_too_large
    }

    /// Return the number of examined characters.
    #[must_use]
    pub fn characters_examined(&self) -> u64 {
        self.characters_examined
    }

    /// Finish a complete scan and transfer its bounded accepted evidence.
    #[must_use]
    pub fn finish(self) -> MinimapAnalysisResult {
        MinimapAnalysisResult {
            wrapped_layout_too_large: self.wrapped_layout_too_large,
            long_line_lines: self.long_line_lines,
            characters_examined: self.characters_examined,
            lines_examined: u64::from(self.current_line).saturating_add(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: MinimapAnalysisPolicy = MinimapAnalysisPolicy {
        warning_line_chars: 4,
        wrapped_line_chars: 8,
        marker_limit: 2,
    };

    #[test]
    fn slices_preserve_line_state_and_strict_thresholds() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        assert_eq!(analysis.inspect_slice("short\nlo".chars(), 8), 8);
        assert_eq!(analysis.inspect_slice("ng-line\nend".chars(), 64), 11);
        let result = analysis.finish();

        assert!(result.wrapped_layout_too_large);
        assert_eq!(result.long_line_lines, vec![0, 1]);
        assert_eq!(result.characters_examined, 19);
        assert_eq!(result.lines_examined, 3);
    }

    #[test]
    fn marker_cap_does_not_stop_wrapped_layout_evidence() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        analysis.inspect_slice("12345\n67890\nabcdefghijkl\n".chars(), usize::MAX);
        let result = analysis.finish();

        assert_eq!(result.long_line_lines, vec![0, 1]);
        assert!(result.wrapped_layout_too_large);
        assert_eq!(result.lines_examined, 4);
    }

    #[test]
    fn marker_disabled_scan_retains_only_shared_layout_evidence() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, false);
        analysis.inspect_slice("abcdefghijkl".chars(), usize::MAX);
        let result = analysis.finish();

        assert!(result.wrapped_layout_too_large);
        assert!(result.long_line_lines.is_empty());
    }

    #[test]
    fn many_short_lines_require_multiple_bounded_caller_slices() {
        let text = "x\n".repeat(10_000);
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        let mut characters = text.chars();
        let mut slices = 0usize;
        loop {
            let inspected = analysis.inspect_slice(characters.by_ref(), 257);
            if inspected == 0 {
                break;
            }
            assert!(inspected <= 257);
            slices = slices.saturating_add(1);
        }
        let result = analysis.finish();

        assert!(slices > 1);
        assert_eq!(result.characters_examined, 20_000);
        assert!(!result.wrapped_layout_too_large);
        assert!(result.long_line_lines.is_empty());
    }
}
