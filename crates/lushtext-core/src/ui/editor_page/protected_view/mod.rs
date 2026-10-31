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
//! | `find_execution` | coordination | the worker-find stage: the search bar's routed query and navigation, one worker search over the source at a time, matches published as paragraph, column and length |
//! | `interaction_execution` | coordination | the caret-free interaction stage: stopped keybinding signals, the `move-cursor` intercept, the capture key controller and gestures, drop targets |
//!
//! The executor milestone (tasks 4.3–6.x) adds the coordination modules
//! (`execution`, `inspector_execution`, `find_execution`), the evidence surface
//! and the called presentation surfaces (`inspector`, `fold_gutter`).
//!
//! ## Stages (filled by the executor milestone)
//!
//! 1. **Enter.** On a whole-content install whose body profile says protected,
//!    before its first byte, or from C1's edit watch in the same emission:
//!    `ProtectedSession::on_event` advances the session, and the ticket is
//!    built once.
//! 2. **Make the view caret-free.** Properties, the `move-cursor` intercept
//!    (`viewport_step`), the capture gestures (`press_disposition`), DnD off,
//!    key-binding pre-emption (`action_disposition`), minimap off.
//! 3. **Serve reads from the source.** Navigation (`navigation_target`), the
//!    Line Inspector (`inspector_window*`), whole-line copy
//!    (`COPY_CHUNK_BYTES`), worker find (`admit_find_record`,
//!    `next_find_batch`). Each resumes from a worker completion or an idle
//!    dispatch and publishes only while `protected_view_is_current` holds.
//! 4. **Leave.** A reload from disk re-derives; the entry undo ends an
//!    edit-growth session.

pub(crate) mod evidence;
pub(crate) mod execution;
pub(crate) mod find_execution;
pub(crate) mod interaction_execution;
pub(crate) mod policy;
pub(crate) mod seams;

pub use evidence::ProtectedViewEvidence;
