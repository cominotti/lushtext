// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content-search services.
//!
//! This service layer stays GTK-free and splits the two main use cases into
//! separate modules: streaming search execution and on-disk replace/undo flows.

mod replace;
mod search;

pub use replace::{
    ApplyReplacementsOutcome, MAX_REPLACE_FILE_BYTES, MAX_REPLACE_UNDO_BYTES,
    MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceConstructionMetrics, ReplaceUndoBackup,
    ReplaceUndoEntry, UndoReplaceOutcome, apply_replacements, replace_undo_retained_byte_weight,
    undo_replacements, undo_replacements_for_open_identities,
};
#[cfg(feature = "test-utils")]
pub use replace::{
    ReplaceBeforeRenameFailureGuard, UndoAfterMetadataHookGuard,
    fail_next_replace_before_rename_for_path_for_test, register_undo_after_metadata_hook_for_test,
    replace_before_rename_failure_is_armed_for_test, set_max_replace_undo_bytes_for_test,
    undo_after_metadata_hook_is_registered_for_test,
    undo_after_metadata_hook_registry_is_empty_for_test,
};
pub(crate) use replace::{ReplaceJournalFreshness, apply_replacements_if_current};
#[cfg(feature = "property-tests")]
pub use replace::{
    apply_replacements_to_text_for_property_test,
    apply_replacements_to_text_reference_for_property_test,
};
pub use search::{search, search_with_plan};
