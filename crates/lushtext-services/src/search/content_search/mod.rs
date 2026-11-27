// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content-search services.
//!
//! This service layer stays GTK-free and splits the two main use cases into
//! separate modules: streaming search execution and on-disk replace/undo flows.

mod preview;
mod replace;
mod search;

pub use preview::{
    generate_replacement_preview, generate_replacement_preview_with_budget,
    generate_replacement_preview_with_budget_and_cancel,
};

pub use replace::{
    ApplyReplacementsOutcome, ContentDigest, MAIN_MAX_REPLACE_FILE_BYTES, MAIN_REPLACE_FILE_BYTES,
    MAIN_REPLACE_UNDO_BYTES, MAX_REPLACE_FILE_BYTES, MAX_REPLACE_UNDO_RETAINED_BYTES,
    REPLACE_DISK_RESERVE_BYTES, REPLACE_STREAM_CHUNK_BYTES, ReplaceConstructionMetrics,
    ReplaceJournalFreshness, ReplacePhase, ReplaceProgress, ReplaceRunRequest, ReplaceUndoBackup,
    ReplaceUndoEntry, UndoOriginal, UndoReplaceOutcome, apply_replacements,
    apply_replacements_with, apply_request_if_current, replace_undo_retained_byte_weight,
    undo_replacements, undo_replacements_for_open_identities,
};
#[cfg(feature = "test-utils")]
pub use replace::{
    ReplaceBeforeRenameFailureGuard, UndoAfterMetadataHookGuard,
    fail_next_replace_before_rename_for_path_for_test, register_after_pass_a_hook_for_test,
    register_undo_after_metadata_hook_for_test, replace_before_rename_failure_is_armed_for_test,
    set_max_replace_file_bytes_for_test, set_max_replace_undo_bytes_for_test,
    undo_after_metadata_hook_is_registered_for_test,
    undo_after_metadata_hook_registry_is_empty_for_test,
};
pub(crate) use replace::{ShrinkAction, shrink_step};
#[cfg(feature = "property-tests")]
pub use replace::{
    apply_replacements_to_text_for_property_test,
    apply_replacements_to_text_in_chunks_for_property_test,
    apply_replacements_to_text_reference_for_property_test,
    apply_replacements_to_text_reference_outcome_for_property_test,
};
pub use search::{ContentSearchRequest, search, search_with_plan};
