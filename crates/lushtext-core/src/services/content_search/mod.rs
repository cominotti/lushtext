// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content-search services.
//!
//! This service layer stays GTK-free and splits the two main use cases into
//! separate modules: streaming search execution and on-disk replace/undo flows.

mod replace;
mod search;

#[cfg(feature = "property-tests")]
pub use replace::apply_replacements_to_text_for_property_test;
pub use replace::{
    MAX_REPLACE_FILE_BYTES, MAX_REPLACE_UNDO_BYTES, ReplaceUndoBackup, ReplaceUndoEntry,
    UndoReplaceOutcome, apply_replacements, undo_replacements,
};
pub use search::search;
