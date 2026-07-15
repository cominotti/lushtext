// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain policy for bounded document-sized GTK buffer replacement.
//!
//! GTK adapters own sources, widgets, and terminal callbacks. This module owns
//! the calibrated direct-versus-sliced decision and UTF-8-safe per-turn bounds.

/// Maximum old/new document size that may be replaced synchronously.
pub const SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES: usize = 1024 * 1024;

/// Maximum existing buffer characters deleted in one GTK turn.
pub const REPLACEMENT_CLEAR_SLICE_CHARS: i32 = 64 * 1024;

/// Maximum replacement UTF-8 bytes considered in one GTK insertion turn.
pub const REPLACEMENT_INSERT_SLICE_BYTES: usize = 256 * 1024;

/// Execution shape for one whole-buffer replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferReplacementMode {
    /// Clear and install in the current GTK turn.
    Direct,
    /// Clear and install through bounded scheduled turns.
    Sliced,
}

/// Immutable policy decision for one clear-only or clear-and-insert request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferReplacementPlan {
    /// Direct or scheduled execution selected from both old and new sizes.
    pub mode: BufferReplacementMode,
    /// Whether the terminal buffer must remain empty.
    pub clear_only: bool,
}

impl BufferReplacementPlan {
    /// Classify one replacement from the conservative existing character charge
    /// and exact incoming UTF-8 byte length.
    #[must_use]
    pub fn for_sizes(existing_chars: i32, replacement_bytes: usize) -> Self {
        let existing_bytes = usize::try_from(existing_chars.max(0))
            .unwrap_or(usize::MAX)
            .saturating_mul(4);
        let mode = if existing_bytes <= SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES
            && replacement_bytes <= SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES
        {
            BufferReplacementMode::Direct
        } else {
            BufferReplacementMode::Sliced
        };
        Self {
            mode,
            clear_only: replacement_bytes == 0,
        }
    }
}

/// Return the number of leading characters one clear turn may delete.
#[must_use]
pub fn next_clear_char_count(remaining_chars: i32) -> i32 {
    remaining_chars.clamp(0, REPLACEMENT_CLEAR_SLICE_CHARS)
}

/// Return the next UTF-8 boundary without exceeding the insertion byte budget.
#[must_use]
pub fn next_replacement_boundary(text: &str, start: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    let mut end = start
        .saturating_add(REPLACEMENT_INSERT_SLICE_BYTES)
        .min(text.len());
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == start {
        text[start..]
            .char_indices()
            .nth(1)
            .map_or(text.len(), |(offset, _)| start.saturating_add(offset))
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_uses_both_existing_and_incoming_sizes() {
        let direct = BufferReplacementPlan::for_sizes(12, 12);
        assert_eq!(direct.mode, BufferReplacementMode::Direct);
        assert!(!direct.clear_only);

        let large_old = i32::try_from(SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES / 4 + 1)
            .expect("threshold fits i32");
        assert_eq!(
            BufferReplacementPlan::for_sizes(large_old, 0),
            BufferReplacementPlan {
                mode: BufferReplacementMode::Sliced,
                clear_only: true,
            }
        );
        assert_eq!(
            BufferReplacementPlan::for_sizes(0, SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES + 1).mode,
            BufferReplacementMode::Sliced
        );
    }

    #[test]
    fn clear_policy_handles_empty_and_exact_boundaries() {
        assert_eq!(next_clear_char_count(-1), 0);
        assert_eq!(next_clear_char_count(0), 0);
        assert_eq!(
            next_clear_char_count(REPLACEMENT_CLEAR_SLICE_CHARS),
            REPLACEMENT_CLEAR_SLICE_CHARS
        );
        assert_eq!(
            next_clear_char_count(REPLACEMENT_CLEAR_SLICE_CHARS + 1),
            REPLACEMENT_CLEAR_SLICE_CHARS
        );
    }

    #[test]
    fn insertion_policy_never_splits_awkward_unicode() {
        let text = format!(
            "{}🙂e\u{301}tail",
            "a".repeat(REPLACEMENT_INSERT_SLICE_BYTES - 1)
        );
        let mut start = 0;
        let mut rebuilt = String::new();
        while start < text.len() {
            let end = next_replacement_boundary(&text, start);
            assert!(end > start);
            assert!(end - start <= REPLACEMENT_INSERT_SLICE_BYTES);
            assert!(text.is_char_boundary(end));
            rebuilt.push_str(&text[start..end]);
            start = end;
        }
        assert_eq!(rebuilt, text);
    }
}
