// SPDX-License-Identifier: GPL-3.0-or-later

//! File size thresholds for graceful degradation with large files.
//!
//! These constants encode domain policy: at what sizes should LushText
//! disable features to maintain responsiveness and avoid OOM. They live
//! in the services layer (not UI) because they are data-path policy,
//! not presentation decisions.

/// Above this size, show an informational toast to set user expectations.
/// GtkSourceView handles 1MB fine but undo history grows fast.
pub const LARGE_FILE_TOAST: u64 = 1_000_000;

/// Above this size, disable syntax highlighting.
/// GtkSourceView's regex-based syntax engine scans the full buffer for context.
/// Above 10MB, the initial highlight pass exceeds 500ms.
pub const DISABLE_SYNTAX_HIGHLIGHTING: u64 = 10_000_000;

/// Above this size, keep undo history permanently disabled.
/// Each edit creates undo entries that roughly double memory for the buffer.
pub const DISABLE_UNDO_HISTORY: u64 = 50_000_000;

/// Above this size, refuse to open the file entirely.
/// `buffer.set_text()` for 500MB allocates ~1GB and blocks the main thread
/// for 5-10 seconds even with syntax highlighting off.
pub const REFUSE_TO_OPEN: u64 = 500_000_000;

/// Result of checking a file's size against thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSizeCheck {
    /// File is small enough for all features.
    Normal,
    /// File is large — show a toast, all features enabled.
    LargeFileToast,
    /// File is very large — disable syntax highlighting.
    DisableSyntax,
    /// File is huge — disable syntax highlighting and undo history.
    DisableUndoAndSyntax,
    /// File is too large to open.
    TooLarge,
}

impl FileSizeCheck {
    /// Classify a file size into the appropriate threshold category.
    pub fn classify(size: u64) -> Self {
        if size > REFUSE_TO_OPEN {
            Self::TooLarge
        } else if size > DISABLE_UNDO_HISTORY {
            Self::DisableUndoAndSyntax
        } else if size > DISABLE_SYNTAX_HIGHLIGHTING {
            Self::DisableSyntax
        } else if size > LARGE_FILE_TOAST {
            Self::LargeFileToast
        } else {
            Self::Normal
        }
    }

    /// Whether syntax highlighting should be enabled.
    pub fn syntax_enabled(self) -> bool {
        matches!(self, Self::Normal | Self::LargeFileToast)
    }

    /// Whether undo history should be enabled.
    pub fn undo_enabled(self) -> bool {
        matches!(
            self,
            Self::Normal | Self::LargeFileToast | Self::DisableSyntax
        )
    }

    /// Approximate GtkTextBuffer memory multiplier for eviction decisions.
    ///
    /// Undo history is the dominant extra overhead, so we use a higher
    /// estimate while undo is enabled and a lower one once large-file mode
    /// disables it.
    pub fn estimated_buffer_multiplier(self) -> u64 {
        if self.undo_enabled() {
            3
        } else {
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_normal() {
        assert_eq!(FileSizeCheck::classify(0), FileSizeCheck::Normal);
        assert_eq!(FileSizeCheck::classify(999_999), FileSizeCheck::Normal);
        assert_eq!(FileSizeCheck::classify(1_000_000), FileSizeCheck::Normal);
    }

    #[test]
    fn test_classify_large_file_toast() {
        assert_eq!(
            FileSizeCheck::classify(1_000_001),
            FileSizeCheck::LargeFileToast
        );
        assert_eq!(
            FileSizeCheck::classify(10_000_000),
            FileSizeCheck::LargeFileToast
        );
    }

    #[test]
    fn test_classify_disable_syntax() {
        assert_eq!(
            FileSizeCheck::classify(10_000_001),
            FileSizeCheck::DisableSyntax
        );
        assert_eq!(
            FileSizeCheck::classify(50_000_000),
            FileSizeCheck::DisableSyntax
        );
    }

    #[test]
    fn test_classify_disable_undo_and_syntax() {
        assert_eq!(
            FileSizeCheck::classify(50_000_001),
            FileSizeCheck::DisableUndoAndSyntax
        );
        assert_eq!(
            FileSizeCheck::classify(500_000_000),
            FileSizeCheck::DisableUndoAndSyntax
        );
    }

    #[test]
    fn test_classify_too_large() {
        assert_eq!(
            FileSizeCheck::classify(500_000_001),
            FileSizeCheck::TooLarge
        );
        assert_eq!(FileSizeCheck::classify(u64::MAX), FileSizeCheck::TooLarge);
    }

    #[test]
    fn test_syntax_enabled() {
        assert!(FileSizeCheck::Normal.syntax_enabled());
        assert!(FileSizeCheck::LargeFileToast.syntax_enabled());
        assert!(!FileSizeCheck::DisableSyntax.syntax_enabled());
        assert!(!FileSizeCheck::DisableUndoAndSyntax.syntax_enabled());
    }

    #[test]
    fn test_undo_enabled() {
        assert!(FileSizeCheck::Normal.undo_enabled());
        assert!(FileSizeCheck::LargeFileToast.undo_enabled());
        assert!(FileSizeCheck::DisableSyntax.undo_enabled());
        assert!(!FileSizeCheck::DisableUndoAndSyntax.undo_enabled());
    }

    #[test]
    fn test_threshold_ordering() {
        assert!(LARGE_FILE_TOAST < DISABLE_SYNTAX_HIGHLIGHTING);
        assert!(DISABLE_SYNTAX_HIGHLIGHTING < DISABLE_UNDO_HISTORY);
        assert!(DISABLE_UNDO_HISTORY < REFUSE_TO_OPEN);
    }

    #[test]
    fn test_estimated_buffer_multiplier() {
        assert_eq!(FileSizeCheck::Normal.estimated_buffer_multiplier(), 3);
        assert_eq!(
            FileSizeCheck::LargeFileToast.estimated_buffer_multiplier(),
            3
        );
        assert_eq!(
            FileSizeCheck::DisableSyntax.estimated_buffer_multiplier(),
            3
        );
        assert_eq!(
            FileSizeCheck::DisableUndoAndSyntax.estimated_buffer_multiplier(),
            2
        );
    }
}
