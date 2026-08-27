// SPDX-License-Identifier: GPL-3.0-or-later

//! # The bounded buffer-replacement workflow
//!
//! What happens when something replaces a document's **whole** buffer: draft
//! recovery installing a restored body, a local-history restore or its undo,
//! save formatting mirroring rewritten text back, or memory eviction emptying an
//! inactive tab. One workflow, one stage order, and one promise — **a
//! half-replaced document is never left visible.**
//!
//! This directory is a **per-workflow role home**: `ui/editor_page/` hosts eight
//! workflows and the role file names `policy.rs` and `evidence.rs` are fixed at
//! one each per workflow, so `save/` and `load/` already spend two of the three.
//! A prefixed `buffer_replacement_policy.rs` was not available: pure policy is
//! reached by the mutation scope through `ui/**/policy.rs`, and a prefixed name
//! would leave that scope.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | the four entry points and this narration |
//! | `execution` | coordination | the session, the projection guard, body ownership, the scheduled turns, supersession, exactly-once terminal cleanup, and the four actuation seams |
//! | [`policy`] | pure policy | the seam value types, the phase and cancellation decisions, terminal classification, and bounded-turn metrics accounting |
//! | [`evidence`] | evidence | [`BufferReplacementEvidence`], the one typed value observers read |
//!
//! ## Stages
//!
//! 1. **Accept, or supersede.** [`LushtextEditorPage::replace_buffer_bounded`]
//!    is the only way in. With no live session the request begins immediately.
//!    With one, that session is cancelled as `Superseded` first; if its
//!    cancellation reached its terminal in the same turn the newcomer starts now,
//!    otherwise it is parked as the **latest** intent and any request it displaces
//!    gets its terminal immediately rather than waiting for one that never comes.
//! 2. **Begin.** The cross-cutting `model::buffer_replacement` plan classifies
//!    direct versus sliced from **both** the existing character count and the
//!    incoming byte length. Editability, cursor visibility, syntax highlighting,
//!    minimap tracking, local-history capture, projection, and the file monitor
//!    are captured and suspended as one guard, and the edit opens as a single
//!    irreversible action so the user cannot undo into a partial state.
//! 3. **Replace directly**, for a document small enough on both sides: one turn,
//!    revalidate, `set_text`, finish.
//! 4. **Clear in bounded turns**, otherwise. Each turn deletes a slice ending at
//!    a **line start**, because GTK validates whole paragraphs and a deletion
//!    stopping mid-line would re-lay-out the shrinking remainder every turn.
//! 5. **Install in bounded turns.** Each slice ends just after a newline, from
//!    the same cross-cutting arithmetic — this is the contract that keeps
//!    recovering a 33 MB single-line draft linear instead of quadratic. A
//!    paragraph larger than the budget installs in one turn, because GTK cannot
//!    lay it out incrementally anyway.
//! 6. **Cancel.** Stale, superseded, or disposed. The uninstalled body goes back
//!    to its owner with its disposal reservation intact. A session that has not
//!    yet mutated the buffer, or whose page is being disposed, finishes here.
//!    **A session that has already mutated must empty the partial buffer in
//!    bounded turns first** — this is the whole reason the workflow has a fourth
//!    phase.
//! 7. **Reach the terminal, exactly once.** The irreversible action closes, the
//!    guard is restored unless the page is being disposed, the caller's callback
//!    fires with `Complete` or `Cancelled` and its metrics, the terminal
//!    diagnostic is recorded for observers, and ownership passes to any parked
//!    request.
//!
//! ## Where control leaves, and where it comes back
//!
//! One deferred mechanism, three resume points:
//!
//! - **Stages 4, 5, and 6 all yield through one `glib::timeout_add_local_once`
//!   of one millisecond.** Control resumes in `execution`'s `run_turn`, which
//!   dispatches by phase to the clear turn, the install turn, or the
//!   cancelled-clear turn. A one-millisecond timeout rather than an idle source
//!   is deliberate: an idle source can starve behind higher-priority work while
//!   the buffer is still half-mutated.
//!
//! Three further points hand *ownership* out without control coming back: the
//! caller's terminal callback, the guarded body's cancel return, and the guarded
//! body's completion return. Stage 1's eviction of a displaced pending request
//! fires that request's terminal synchronously from the new request's entry.
//!
//! ## Its callers, and the two that are not this change's to restructure
//!
//! `BufferReplacementWorkflow`'s variants name the caller set exactly. Five call
//! sites across four owning workflows:
//!
//! | Variant | Caller | Owning workflow |
//! | --- | --- | --- |
//! | `DraftRecovery` | `ui/window/drafts/restore_execution.rs` | `WFR-DRAFT-RECOVERY` |
//! | `LocalHistoryRestore`, `LocalHistoryUndo` | `ui/window/local_history/restore_execution.rs`, twice | `WFR-LOCAL-HISTORY` |
//! | `MemoryEviction` | `ui/editor_page/mod.rs` | `WFR-EDITOR-MEMORY` — **exempt, no slot** |
//! | `SaveFormatting` | `ui/editor_page/save/execution.rs` | `WFR-DOCUMENT-SAVE` — **migrated** |
//!
//! Replace All undo is **not** a caller: `LocalHistoryUndo` is local history's
//! own undo affordance.
//!
//! ## State this workflow shares with others
//!
//! | State | Ownership from this workflow's side |
//! | --- | --- |
//! | `model::buffer_replacement` — the direct/sliced threshold, the clear slice budget, and `next_replacement_boundary` | **cross-cutting and called, never copied.** Also consumed directly by `WFR-DOCUMENT-LOAD` (`load/policy.rs`, and `load/execution.rs` through the `model::file_load::next_install_boundary` synonym) and by `WFR-LOCAL-HISTORY`'s preview installer. Forking a shared limit lets it drift while both copies still read as correct |
//! | `imp().minimap.tracking_suspended`, `imp().local_history.automatic_capture_suppressed` | **suspended and exactly restored** by this workflow's guard; owned by the minimap and local-history workflows |
//! | `imp().monitor.file_monitor` | stopped and conditionally restarted by the guard; owned by the external-file-monitor workflow |
//! | `imp().load.projection_suspended` | never written here. `load_projection_suspended` reads **both** flags, because a projection must stand down for either workflow's suspension |
//! | the search bar's search context | detached and re-attached by the guard; owned by `WFR-EDITOR-FIND` |
//! | `ui::plain_disposal` and the `DisposalOwned` bodies callers hand in | cross-cutting (slot 7). This workflow moves guarded bodies and never releases their reservations itself |

pub mod evidence;
mod execution;
pub mod policy;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use super::LushtextEditorPage;

#[cfg(feature = "test-utils")]
pub use evidence::BufferReplacementEvidence;
pub(crate) use execution::BufferReplacementState;
pub use execution::BufferReplacementTerminalDiagnostic;
pub use execution::{BufferReplacementOutcome, BufferReplacementRequest};
pub use policy::{
    BufferReplacementCancelReason, BufferReplacementMetrics, BufferReplacementTicket,
    BufferReplacementWorkflow, ReplacementPhase,
};

#[cfg(feature = "test-utils")]
pub use execution::BufferReplacementTestOutcome;

impl LushtextEditorPage {
    /// Replace this editor's whole buffer, superseding any live replacement.
    ///
    /// Stage 1, and the only way in. The caller keeps the workflow semantics: its
    /// ticket identifies the request, its freshness check decides at every turn
    /// whether the editor is still the one it asked about, and its terminal
    /// callback is what learns the outcome. Everything between — GTK source
    /// lifetime, the projection guard, body ownership, bounded turns, and exact
    /// cleanup — belongs here.
    pub(crate) fn replace_buffer_bounded(&self, request: BufferReplacementRequest) {
        execution::accept_request(self, request);
    }

    /// Release every replacement this editor owns, without publishing widgets.
    ///
    /// Stage 6 as the widget hierarchy reaches it. Called from `dispose()`, so it
    /// must not restore the guard: the page is leaving the hierarchy and touching
    /// its projections would resurrect them against a dying buffer.
    pub(crate) fn cancel_buffer_replacement_for_dispose(&self) {
        execution::dispose(self);
    }

    /// Whether a replacement currently owns this editor's buffer.
    ///
    /// A cheap accessor over the one cell rather than a whole
    /// [`BufferReplacementEvidence`] read: callers consult it inside guards on
    /// hot paths (save admission, load admission, memory eviction), and it is
    /// identical by construction because both read the same slot.
    #[must_use]
    pub(crate) fn buffer_replacement_in_progress(&self) -> bool {
        self.imp().replacement.active.borrow().is_some()
    }

    /// Whether editor projections are suspended for a replacement.
    ///
    /// The same cheap-accessor reasoning: `load_projection_suspended` calls this
    /// from every buffer signal handler.
    #[must_use]
    pub(crate) fn buffer_replacement_projection_suspended(&self) -> bool {
        self.imp().replacement.projection_suspended.get()
    }
}
