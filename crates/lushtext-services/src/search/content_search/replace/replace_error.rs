// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed errors for workspace Replace All execution.

use crate::search::search_backup::SearchBackupError;

/// Typed errors returned when preparing, running, or rolling back Replace All.
#[derive(Debug, thiserror::Error)]
pub enum ReplaceApplyError {
    /// Preview selection exceeded the maximum number of replacements in one run.
    #[error("Replace All selection exceeds the {limit}-replacement limit")]
    SelectionLimitExceeded {
        /// Number of replacements supplied by the caller.
        count: usize,
        /// Maximum allowed replacements per run.
        limit: usize,
    },

    /// Run was cancelled before acquiring the journal lock or after full rollback.
    #[error("Replace cancelled")]
    Cancelled,

    /// Run was cancelled and cleanup of the rolled-back undo journal also failed.
    #[error("Replace cancelled; undo backup cleanup failed: {source}")]
    CancelledCleanupFailed {
        /// Underlying journal deletion error.
        #[source]
        source: SearchBackupError,
    },

    /// Unguarded Replace All lost currency before completing.
    #[error("unguarded Replace All became stale")]
    StaleGeneration,

    /// Every candidate file failed without replacing or skipping any file.
    #[error("{summary}")]
    AllFilesFailed {
        /// Bounded failure summary from the run's diagnostic sample.
        summary: String,
    },

    /// Acquiring or updating the Replace All undo journal failed.
    #[error(transparent)]
    Journal(#[from] SearchBackupError),
}
