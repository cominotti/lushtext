// SPDX-License-Identifier: GPL-3.0-or-later

//! The workspace tree workflow: workspaces, their folders, the file tree inside
//! them, and everything that keeps all three agreeing with the disk.
//!
//! This module is the workflow's **narrative facade** and its **canonical role home**.
//! It narrates the ordered stages and delegates every one of them; it owns no timer,
//! no admission bookkeeping, no generation counter, and no widget mutation. The role
//! home is **nested**: the single `policy.rs`, `evidence.rs`, and `seams.rs` live here
//! beside this file, while the per-section coordination roles live in
//! `workspace_section/`, whose own module doc lists them.
//!
//! # Roles at this level
//!
//! | Module | Role |
//! | --- | --- |
//! | `policy.rs` | pure policy — the workflow's only one. Imports no GTK-family crate, which is what keeps it inside the default mutation scope |
//! | `evidence.rs` | evidence — the one typed observation surface, and the only thing `window.workspace` projects from |
//! | `seams.rs` | seam value objects — `WorkspaceWatchTicket`, `FileOperationTicket`, the watch generations, and the window's file-row projection |
//! | `list_execution.rs` | coordination, `execution`: workspace-list load and add / rename / unlist |
//! | `membership_execution.rs` | coordination, `execution`: a workspace's folder add / remove / reorder |
//! | `filter_execution.rs` | coordination, `execution`: the workspace scope filter and its fade |
//! | `persist_execution.rs` | coordination, `execution`: the `workspaces.json` pipeline |
//! | `callbacks.rs`, `dialogs.rs`, `imp.rs` | **called presentation surfaces** — no role |
//! | `width_preset.rs` | **not this workflow's**: `WFR-SHELL-LAYOUT` owns it |
//! | `file_tree_item.rs` | outside this workflow — no coordination tier |
//!
//! # The twelve stage orders, and where control resumes
//!
//! Every one of these is deferred somewhere, so "where control resumes" is the part a
//! reader cannot guess. `⇢` marks a stage boundary control does **not** cross
//! synchronously.
//!
//! | Stage order | Ordered stages | Control resumes |
//! | --- | --- | --- |
//! | workspace-list load | read `workspaces.json` ⇢ adopt and build sections | in the worker completion, **only if** no mutation superseded the request generation captured at dispatch — otherwise the load is discarded rather than reverting the user's newer workspace |
//! | workspace add / rename / unlist | dialog ⇢ response → mutate → request persistence | in the dialog response, then synchronously into `persist_execution` |
//! | folder add / remove / reorder | `Add Folder` dialog, a row remove request, or a **reorder drag-and-drop drop** ⇢ resolve folder identity off the GTK thread ⇢ apply → persist | in the identity worker's completion; that is the only off-GTK stage in the membership family |
//! | persistence | debounce ⇢ worker write ⇢ terminal → retry ladder or settle | in the debounce, then the worker terminal; a close-time flush bypasses the debounce and **aborts the close** if it fails |
//! | scope filter | selection → fade out ⇢ apply visibility ⇢ settle | in the revealer's `child-revealed` notification, with a headless safety-net timer as fallback |
//! | top-level folder rows | seed rows → probe emptiness ⇢ publish | in the empty-probe worker, which is admission-gated and can be refused and retried |
//! | directory scan and expansion | expand → admit ⇢ scan worker ⇢ batched reconcile ⇢ deferred expansion restore | in the scan worker, then per reconcile batch, then in the restore callback — see the warning below |
//! | targeted in-place refresh | coalesce ⇢ debounce ⇢ plan → splice | in the refresh debounce; a pending full refresh releases and dominates queued targeted paths |
//! | watcher install and mailbox | compute targets ⇢ install worker ⇢ poll mailbox ⇢ reconcile | in the install completion, validated as a unit by `WorkspaceWatchTicket`: a stale lifetime **retires**, a stale target generation **restarts** |
//! | file create / rename / delete | inline entry or dialog ⇢ filesystem worker ⇢ project onto the row → migrate sidecars | in the worker completion, gated by `FileOperationTicket` so a recycled row is never rewritten; sidecar migration is a **call** into the notes workflow, after the row updates settle |
//! | `Space` peek | key (captured on the list view, gated against focused controls that own their keys) ⇢ read worker ⇢ popover | in the peek worker, rejected if the request token or path changed |
//! | focused-folder drilldown | activate → reseed rows ⇢ scan ⇢ restore expansion | in the scan for the new root; leaving restores the original folder seeds |
//!
//! # The inversion most easily read wrong
//!
//! The **deferred expansion restore** at the end of the scan order. `expanded_paths`
//! is authoritative live state, and the restore callback must read it **at apply
//! time**, not clone it when it is scheduled. A snapshot taken at schedule time
//! resurrects a collapse the user performed in between — silently, and only for users
//! whose filesystem is slow enough to make the window wide. Its borrow lives inside
//! the deferred closure for exactly that reason.
//!
//! Related, and equally easy to break: a targeted refresh must **not** rewalk the
//! flattened model to rediscover expansion. The full derivation is reserved for
//! bootstrap, pre-replacement capture, and the test oracle, and the evidence surface
//! must not call it at all — it advances the very counters that surface reports.
//!
//! # State this workflow shares with others
//!
//! | Shared with | What, and which direction it flows |
//! | --- | --- |
//! | the cross-cutting startup gate | it calls `load_workspaces()`; this workflow does not decide when startup runs |
//! | `WFR-SHELL-LAYOUT` | the window pushes the open/active file-row projection **down** into the sidebar; the sidebar treats those paths as display identities only. That row also owns the sidebar show/hide animation and the width preset |
//! | `WFR-COMMAND-PALETTE` | the sidebar's structure-changed signal drives the palette's file index; the sidebar does not know the index exists |
//! | `WFR-NOTES-BOOKMARKS` | a rename **calls** `migrate_note_sidecars_after_rename`; a context menu route opens notes. Called, never reached into |
//! | `WFR-LOCAL-HISTORY` | a context menu route calls `show_local_history_for_path` |
//! | `WFR-AUTOMATION-SPINE` | `window.workspace` projects from `evidence.rs`, and three readiness blockers read cheap accessors that are identical to it by construction |

mod callbacks;
mod dialogs;
pub mod evidence;
pub mod file_tree_item;
pub mod policy;
pub(crate) mod seams;
#[cfg(feature = "test-utils")]
mod test_policy;
// Private GObject implementation for the template-backed sidebar shell.
mod imp;
// Cross-cutting, and NOT this workflow's: the workspace sidebar width preset is
// `WFR-SHELL-LAYOUT`'s value, consumed by Preferences and the window shell. It
// lives here because it names a sidebar dimension, not because this workflow owns
// it. See the module doc.
mod filter_execution;
mod list_execution;
mod membership_execution;
mod persist_execution;
pub mod width_preset;
pub mod workspace_section;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;

use crate::model::workspace::{WorkspaceId, WorkspaceScope, WorkspacesFile};
use crate::services::notifications::NotificationSeverity;
use seams::SidebarFileRowStateSnapshot;

#[cfg(feature = "test-utils")]
pub use test_policy::{
    set_workspace_load_worker_delay_for_test, set_workspace_placeholder_cleanup_delay_for_test,
    set_workspace_rename_worker_delay_for_test,
};

pub use file_tree_item::FileTreeItem;
pub use workspace_section::LushtextWorkspaceSection as WorkspaceSection;

impl LushtextSidebar {
    /// Whether workspace persistence remains dirty, active, failed, or retry-waiting.
    ///
    /// A cheap read for the polled `workspace-persist` readiness blocker, identical by
    /// construction to `WorkspaceTreeEvidence::persistence_pending`.
    #[must_use]
    pub(crate) fn workspace_persistence_pending(&self) -> bool {
        self.imp().persistence.borrow().has_pending_work()
    }

    /// Whether the workspace scope filter fade sequence is currently running.
    ///
    /// A cheap read for the polled `workspace-filter-animation` readiness blocker,
    /// identical by construction to `WorkspaceTreeEvidence::filter_animation_active`
    /// because both read the one cell. Retiring the old `ui/automation.rs` `imp()`
    /// reach-through is what this accessor bought.
    #[must_use]
    pub(crate) fn workspace_filter_animation_active(&self) -> bool {
        self.imp().workspace_filter_animation_active.get()
    }

    /// Whether any section still has watcher lifecycle, mailbox, or refresh work.
    pub(crate) fn workspace_refresh_blocks_readiness(&self) -> bool {
        self.any_section_blocks_refresh_readiness()
    }

    /// Move focus to the first visible workspace file tree.
    pub(crate) fn focus_first_visible_file_tree(&self) -> bool {
        self.with_first_visible_section(WorkspaceSection::focus_file_tree)
    }

    /// Move focus to the first visible workspace header control.
    pub(crate) fn focus_first_visible_header_controls(&self) -> bool {
        self.with_first_visible_section(WorkspaceSection::focus_header_controls)
    }

    /// Open the selected row's context menu in the first visible workspace tree.
    pub(crate) fn show_first_visible_file_tree_context_menu(&self) -> bool {
        self.with_first_visible_section(WorkspaceSection::show_selected_file_context_menu)
    }

    /// Open the first visible workspace header's context menu.
    pub(crate) fn show_first_visible_header_context_menu(&self) -> bool {
        self.with_first_visible_section(WorkspaceSection::show_header_context_menu)
    }
}

glib::wrapper! {
    // Exposes the private sidebar implementation as the public widget mounted
    // by the main window.
    /// Public multi-workspace sidebar mounted by the main window shell.
    ///
    /// The wrapper exposes workspace, file-tree, and callback operations; the
    /// private implementation owns template children, stores, and persistence.
    pub struct LushtextSidebar(ObjectSubclass<imp::LushtextSidebar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSidebar {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Remove a file/directory from the correct workspace section's model.
    pub fn remove_from_model(&self, target_path: &Path) {
        for section in self.imp().sections.borrow().iter() {
            if section.remove_from_model(target_path) {
                return;
            }
        }
    }

    /// Replace the open/active file projection and resync realized section rows.
    pub(crate) fn set_file_row_state_snapshot(&self, snapshot: SidebarFileRowStateSnapshot) {
        let imp = self.imp();
        if imp.file_row_state_snapshot.borrow().as_ref() == &snapshot {
            return;
        }

        let snapshot = Rc::new(snapshot);
        *imp.file_row_state_snapshot.borrow_mut() = Rc::clone(&snapshot);
        for section in self.imp().sections.borrow().iter() {
            section.set_file_row_state_snapshot(Rc::clone(&snapshot));
        }
    }

    pub fn connect_file_activated<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().file_activated_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_local_history_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().local_history_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_document_note_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().document_note_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_renamed<F: Fn(&Path, &Path) + 'static>(&self, f: F) {
        *self.imp().rename_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_deleted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().delete_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_created<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().create_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_message<F: Fn(&str, NotificationSeverity) + 'static>(&self, f: F) {
        *self.imp().message_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_folder_note_requested<F: Fn(WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().folder_note_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_folder_note_for_folder_requested<F: Fn(WorkspaceId, PathBuf) + 'static>(
        &self,
        f: F,
    ) {
        *self.imp().folder_note_for_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_workspace_structure_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().workspace_structure_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store a callback invoked whenever the current workspace scope changes.
    pub fn connect_workspace_scope_changed<F: Fn(WorkspaceScope) + 'static>(&self, f: F) {
        *self.imp().workspace_scope_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Return the current workspace scope mirrored by the sidebar shell.
    #[must_use]
    pub fn current_scope(&self) -> WorkspaceScope {
        self.imp().current_scope.borrow().clone()
    }

    /// Collect all persisted workspace folders regardless of the current scope.
    #[must_use]
    pub fn all_workspace_folder_paths(&self) -> Vec<PathBuf> {
        self.imp()
            .workspaces_file
            .borrow()
            .all_workspace_folder_paths()
    }

    /// Collect the workspace folders covered by one explicit scope.
    #[must_use]
    pub fn folder_paths_for_scope(&self, scope: &WorkspaceScope) -> Vec<PathBuf> {
        self.imp()
            .workspaces_file
            .borrow()
            .folder_paths_for_scope(scope)
    }

    /// Collect the current scope's workspace folders.
    #[must_use]
    pub fn current_scope_folder_paths(&self) -> Vec<PathBuf> {
        let scope = self.current_scope();
        self.folder_paths_for_scope(&scope)
    }

    /// Return a snapshot of the current persisted workspace state.
    #[must_use]
    pub fn workspaces_file(&self) -> WorkspacesFile {
        self.imp().workspaces_file.borrow().clone()
    }
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}
