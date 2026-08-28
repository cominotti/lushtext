// SPDX-License-Identifier: GPL-3.0-or-later

//! Change how a document reads and writes text bytes.
//!
//! Three related user operations over one document's format policy, entered from
//! `win.show-encoding-controls`, `win.show-line-ending-controls`, and
//! `win.show-file-health` plus the status-bar controls and the
//! invisible-characters shortcut. Each is a **picker followed by an apply**, and
//! each hands its actual write to a workflow that already owns it: the reopen
//! goes to `WFR-DOCUMENT-LOAD`, the byte encoding goes to `WFR-DOCUMENT-SAVE`,
//! and the invisible-character default goes to GSettings. This row decides
//! *policy*, never bytes.
//!
//! ## Stage orders
//!
//! **A. Reopen with encoding** — present the decoding picker → the user chooses
//! → if the document is modified, route through the discard confirmation and
//! *(resume)* in its response handler → hand the path and encoding to the load
//! workflow. No bytes are read here.
//!
//! **B. Change save encoding** — present the save picker → the user chooses →
//! **capture the buffer**, chunked when it is large, *(resume)* in the capture
//! completion → **analyse on a worker** for characters the target encoding
//! cannot represent, *(resume)* in the worker completion → if the conversion is
//! lossy, present the confirmation and *(resume)* again in its response handler
//! → write the new save policy. **Three resumption points**, all guarded by the
//! same freshness triple (analysis generation, content generation, still the
//! active tab), because the user can keep typing throughout. Large-file mode
//! skips the analysis and defers the check to the save path rather than scanning
//! the whole buffer on the GTK thread.
//!
//! **C. Invisible characters and line endings** — present the picker → the user
//! chooses → write the policy synchronously. Choosing a line ending on a
//! **mixed** document also retires the mixed-line-ending finding, so the warning
//! the user just answered does not come back. No inversion.
//!
//! A fourth entry, **File Health**, is a read-only report with no stages.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — every word shown to the user, and every selected/activatable decision |
//! | `execution` | coordination — the apply paths, the analysis worker, and the freshness triple |
//! | `dialogs` | **called presentation surface**, not a role: the six grouped-row dialogs and the shared row chrome |
//! | `test_policy` | test policy — one `test-utils` configuration override (the analysis delay); compiles only under `test-utils` |
//!
//! `ui/editor_page/invisibles.rs` is this workflow's other **called presentation
//! surface**: it applies an invisible-character mode to one editor's
//! `GtkSourceView` drawing flags. It is recorded in the matrix row rather than
//! given a role name here, because it belongs to the editor page's widget tree.
//! `services/editor_io.rs` owns the byte policy and is shared with the save and
//! load rows, so it is not this row's and is not a role. `model/encoding.rs` is
//! the domain vocabulary, shared by 15 consumers, and stays in `model/`.
//!
//! ## What a test reads
//!
//! Production API plus **one** configuration override. This row has **no
//! evidence surface**: probing found nothing to consolidate, because its two
//! gated declarations were both halves of the same timing override rather than
//! inspections of live state, and the observable results of every stage are
//! already read through the editor's own production accessors (`save_encoding`,
//! `opened_encoding`, `save_line_ending`, `detected_line_ending`,
//! `invisible_characters_mode`, `file_health`) and the status lane. The row's
//! write crosses into `WFR-DOCUMENT-SAVE`, which owns that seam.

pub mod policy;

mod dialogs;
mod execution;
#[cfg(feature = "test-utils")]
mod test_policy;

use crate::model::encoding::{DocumentEncoding, InvisibleCharactersMode, LineEnding};
use crate::services::editor_io::LossyEncodingPreview;
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

#[cfg(feature = "test-utils")]
pub use test_policy::set_lossy_encoding_analysis_delay_for_test;

/// Sleep the configured analysis delay, or nothing in a production build.
///
/// Called from inside the analysis worker. Kept here so `execution` has one
/// call site regardless of feature configuration, rather than a `cfg` block in
/// the middle of the worker body.
#[inline]
fn test_policy_delay() {
    #[cfg(feature = "test-utils")]
    test_policy::delay_lossy_encoding_analysis();
}

impl LushtextWindow {
    /// Stage A/B/C entry — present the summary encoding surface.
    pub(super) fn show_encoding_controls_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        // Reopen needs bytes on disk to reinterpret, so an unsaved document gets
        // an explained, insensitive row rather than a missing one.
        dialogs::present_encoding_controls(self, editor.file_path().is_some());
    }

    /// Stage C entry — present line-ending controls.
    pub(super) fn show_line_ending_controls_dialog(&self) {
        dialogs::present_line_ending_controls(self);
    }

    /// Read-only report of the active document's file-health findings.
    pub(super) fn show_file_health_dialog(&self) {
        dialogs::present_file_health(self);
    }

    /// Stage A — the user chose a decoding for the bytes on disk.
    pub(super) fn begin_reopen_with_encoding(&self, encoding: DocumentEncoding) {
        execution::begin_reopen(self, encoding);
    }

    /// Stage B — the user chose a next-save encoding.
    pub(super) fn begin_save_encoding_change(&self, encoding: DocumentEncoding) {
        execution::begin_save_encoding_change(self, encoding);
    }

    /// Stage C — the user chose a line-ending style.
    pub(super) fn apply_line_ending_choice(&self, line_ending: LineEnding) {
        execution::apply_line_ending(self, line_ending);
    }

    /// Stage C — the user chose an invisible-character display mode.
    pub(super) fn apply_invisible_characters_mode(&self, mode: InvisibleCharactersMode) {
        execution::apply_invisible_mode_to_active(self, mode);
    }

    /// Stage C — advance the invisible-character mode in shortcut order.
    pub(super) fn cycle_invisible_characters(&self) {
        execution::cycle_invisible_characters(self);
    }

    /// Ask the user whether to proceed with a lossy save, once.
    ///
    /// Called by the save workflow when its own write found unrepresentable
    /// characters, which is the reverse direction from stage B: there the
    /// analysis runs before the policy changes, here the policy is already set
    /// and the bytes are about to be written.
    pub(super) fn confirm_lossy_save(
        &self,
        editor: &LushtextEditorPage,
        preview: &LossyEncodingPreview,
        retry_save: impl FnOnce() + 'static,
    ) {
        execution::confirm_lossy_save(self, editor, preview, retry_save);
    }
}
