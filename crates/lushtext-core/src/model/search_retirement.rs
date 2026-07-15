// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust per-turn ownership budget for retired workspace-search state.

/// Saturating counter that lets one GTK adapter turn release at most its row budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchRetirementSliceBudget {
    remaining: usize,
    retired: usize,
}

impl SearchRetirementSliceBudget {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            retired: 0,
        }
    }

    /// Reserve bounded work from the next deterministic ownership category.
    pub fn take(&mut self, available: usize) -> usize {
        let count = self.remaining.min(available);
        self.remaining = self.remaining.saturating_sub(count);
        self.retired = self.retired.saturating_add(count);
        count
    }

    #[must_use]
    pub fn exhausted(self) -> bool {
        self.remaining == 0
    }

    #[must_use]
    pub fn retired(self) -> usize {
        self.retired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_result_cap_retires_over_bounded_slices() {
        let mut categories = [1usize, 10_000, 1, 10_000, 1];
        let mut slices = 0usize;
        while categories.iter().any(|count| *count > 0) {
            let mut budget = SearchRetirementSliceBudget::new(250);
            for count in &mut categories {
                let retired = budget.take(*count);
                *count = count.saturating_sub(retired);
            }
            assert!(budget.retired() <= 250);
            assert!(budget.retired() > 0);
            slices = slices.saturating_add(1);
        }
        assert!(slices > 1);
    }

    #[test]
    fn zero_budget_and_large_available_counts_saturate_safely() {
        let mut empty = SearchRetirementSliceBudget::new(0);
        assert_eq!(empty.take(usize::MAX), 0);
        assert!(empty.exhausted());

        let mut bounded = SearchRetirementSliceBudget::new(250);
        assert_eq!(bounded.take(usize::MAX), 250);
        assert_eq!(bounded.retired(), 250);
        assert!(bounded.exhausted());
    }
}
