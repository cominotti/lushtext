// SPDX-License-Identifier: GPL-3.0-or-later

//! # The Protected View workflow (`long-line-protected-view`, C2)
//!
//! What happens to a document whose longest paragraph exceeds
//! `L_PROTECT_EDIT`: it is shown view-only, caret-free and folded, read through
//! an immutable text source, and left only through a reload from disk or the
//! one bounded entry undo of the edit that made it protected (design D2).
//!
//! This directory is a **per-workflow role home**, like `save/` and `load/`:
//! `ui/editor_page/` hosts several workflows, and the fixed role names
//! `policy.rs` and `evidence.rs` are one each per workflow.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | the stage order below |
//! | `policy` | pure policy | viewport steps, press and action dispositions, the session transition, navigation targets, inspector windows, find batches, notice text |
//! | `seams` | seam value objects | `ProtectedViewTicket`, `ProtectedViewFacts`, `protected_view_is_current`, and `ProtectedTextSource` (the retained install body find, the inspector, copy and saves read) |
//! | `execution` | coordination | the editor's session, the install and edit-growth entries, the entry undo |
//! | `evidence` | evidence | `ProtectedViewEvidence`, the workflow's one observable surface |
//! | `capture_execution` | coordination | the edit-entry source capture: one iterator kept across bounded dispatches, re-positioned by line and byte index after a mark or tag change, joined and owned for disposal on a worker |
//! | `find_execution` | coordination | the worker-find stage: the search bar's routed query and navigation, one worker search over the source at a time, matches published as paragraph, column and length |
//! | `interaction_execution` | coordination | the caret-free interaction stage: stopped keybinding signals, the `move-cursor` intercept, the capture key controller and gestures, drop targets |
//!
//! Called presentation surfaces outside this directory: `ui/protected_fold.rs`
//! (the fold tag and the folded insert the install turns call), the search
//! bar's external-find routing, and the automation snapshot's tab fields.
//!
//! ## Stages and resumption points
//!
//! 1. **Enter at install.** A whole-content install (load, eviction reload,
//!    draft recovery, local-history restore and its undo) whose body profile
//!    says protected calls `begin_protected_install` before its first byte;
//!    the install's own turns fold each paragraph as they insert it, and the
//!    install terminal hands the body over as the [`seams::ProtectedTextSource`]
//!    (`retain_protected_source`). No resumption point of its own: the install
//!    workflow's turns are the inversion.
//! 2. **Enter from an edit.** C1's edit watch, inside the same `insert-text`
//!    emission, calls `enter_protected_view_from_edit`: the length and density
//!    folds over the edited lines, then the caret-free properties. Synchronous.
//!    An idle then starts the source capture (`capture_execution`), which
//!    resumes once per chunk dispatch and, after a worker joins the chunks, in
//!    `accept_captured_source` while the session generation holds.
//! 3. **Stay caret-free** (`interaction_execution`): stopped key-binding
//!    signals, the `move-cursor` intercept (`policy::viewport_step`), the
//!    capture key controller and gestures, drop targets off, PRIMARY off. The
//!    `realize`/`unrealize` handlers resume the PRIMARY balance.
//! 4. **Serve reads from the source.** Worker find (`find_execution`) resumes in
//!    its `spawn_blocking_then` completion, which publishes only for the
//!    current find generation while the editor is still protected. Copy resumes
//!    in its completion and sets the clipboard with the bytes selected at the
//!    request, so it needs no freshness check. Save writes the source verbatim
//!    (`save/execution.rs`).
//! 5. **Leave.** A reload from disk re-derives the session; the entry undo
//!    (`undo_protected_entry`) ends an edit-growth session and restores the
//!    scale held before entry.

pub(crate) mod capture_execution;
pub(crate) mod evidence;
pub(crate) mod execution;
pub(crate) mod find_execution;
pub(crate) mod interaction_execution;
pub(crate) mod policy;
pub(crate) mod seams;

pub use evidence::ProtectedViewEvidence;
