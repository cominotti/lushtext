// SPDX-License-Identifier: GPL-3.0-or-later

//! Print the active document.
//!
//! One user operation, one ordered stage sequence, and **no inversion**: the
//! whole workflow runs inside the `win.print` activation, because
//! `gtk4::PrintOperation::run` blocks on the native dialog and returns the
//! result to the same call. There is no worker completion, no timer, and no
//! deferred drain, so no stage resumes elsewhere and nothing needs a generation
//! counter to prove freshness — the only request that can be in flight is this
//! one.
//!
//! ## Stages
//!
//! Entry point: `win.print`, the only one. The action is registered disabled and
//! the window's content-stack refresh enables it once a tab exists, so the
//! stages below always run against a live editor.
//!
//! 1. **Choose the document.** Read the workflow's evidence surface for the
//!    selected editor page. A window with no tabs yields no document and the
//!    request stops here.
//! 2. **Run the request.** Under `test-utils` a probe may stand in for the
//!    native operation; otherwise `execution` builds the GtkSourceView
//!    compositor and runs the native print dialog. Either way the stage ends in
//!    the workflow's own `PrintOutcome` vocabulary rather than GTK's.
//! 3. **Report.** `policy` decides whether the outcome owes the user a message.
//!    A completed, in-progress, or user-cancelled print is silent; a failure is
//!    logged and published to the status lane.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — the outcome vocabulary and the report decision |
//! | `execution` | coordination — owns the compositor and the GTK operation |
//! | `evidence` | evidence surface — the row's single observable state; `test-utils`-gated, so production never reads it |
//! | `test_policy` | test policy — the one `test-utils` override, a lifecycle probe kept because printing has no production stand-in; `test-utils`-gated |
//!
//! The status lane itself is a called presentation surface reached through
//! `publish_status_message`; this workflow owns no widget.

pub mod policy;

#[cfg(feature = "test-utils")]
pub mod evidence;
mod execution;
#[cfg(feature = "test-utils")]
mod test_policy;

use gtk4::gio;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;
use policy::PrintOutcome;

#[cfg(feature = "test-utils")]
pub use evidence::{PrintDocumentFacts, PrintEvidence, print_evidence};
#[cfg(feature = "test-utils")]
pub use test_policy::with_print_runner_for_test;

/// Register the `win.print` action on the window.
pub(super) fn setup_print_action(window: &LushtextWindow) {
    let action = gio::SimpleAction::new("print", None);
    // Printing needs a document; the content-stack refresh enables this once a
    // tab exists. See the action-enabled-state rule in
    // `.agents/rules/widget-wiring.md`.
    action.set_enabled(false);

    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        // Stage 1.
        let Some(editor) = window.active_editor() else {
            return;
        };
        // Stages 2 and 3.
        report_print_outcome(&window, &run_print_request(&window, &editor));
    });

    window.add_action(&action);
}

/// Stage 2 — run one print request, through the probe when a test installed one.
fn run_print_request(window: &LushtextWindow, editor: &LushtextEditorPage) -> PrintOutcome {
    #[cfg(feature = "test-utils")]
    if let Some(outcome) = test_policy::installed_runner_outcome(&print_evidence(window)) {
        return outcome;
    }

    execution::run_native_print_operation(window, editor)
}

/// Stage 3 — tell the user only when the request actually failed.
fn report_print_outcome(window: &LushtextWindow, outcome: &PrintOutcome) {
    if let Some(message) = policy::print_failure_report(outcome) {
        tracing::error!("{message}");
        window.publish_status_message(&message, MessageKind::Error);
    }
}
