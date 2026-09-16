// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** for `WFR-COMMAND-PALETTE` — not a role.
//!
//! The window side of the command palette: opening and closing the overlay,
//! keeping its actions' enabled state honest, projecting the query and mode in
//! for automation, restoring the focus the overlay borrowed, and driving the
//! file-index build from window-owned workspace state. The palette workflow's
//! canonical role home is `ui/command_palette/`, which already owns the
//! `admission`, `execution`, `retirement`, `policy`, and `evidence` roles; this
//! module is window-side target resolution that those roles are reached from,
//! so it deliberately carries no role name of its own.
//!
//! # What this file used to be, and why the name changed
//!
//! It was `focus_indexing.rs`, and the inherited census called it "three
//! stories". Re-derived in slot 7b it held **four**, with four different
//! owners:
//!
//! | Story | Production lines | Destination |
//! | --- | --- | --- |
//! | editor-memory eviction | 407 | `WFR-EDITOR-MEMORY-EVICTION`'s role home |
//! | focus restoration | 125 | dissolved into **three** owners — two geometry restorers to `WFR-SHELL-GEOMETRY`, `restore_saved_focus` here with the overlay it belongs to, and `focus_selected_editor_after_action` to cross-cutting `editor_focus.rs` |
//! | palette overlay control | 108 | here |
//! | palette file-index build | 145 | here |
//!
//! The "focus restoration" story contained **no geometry code** at all; two of
//! its four functions were geometry-*triggered*, which is a caller
//! relationship rather than ownership. Leaving that implicit is the failure the
//! stage trace was run to prevent.

use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;

use crate::model::palette::{
    PaletteFileEntry, PaletteFileIdentity, PaletteFileIdentityFailure, SearchMode,
};
use crate::services::palette::{
    FileIndex, FileIndexBuildMetrics, FileIndexBuildOutcome, FileIndexBuildRequest,
    FileIndexBuildStart,
};
use crate::ui::accessibility::AnnouncementLane;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

enum GuardedFileIndexBuildOutcome {
    /// Boxed so the cancelled variant does not pay for the index guard and
    /// metrics that only a completed build carries.
    Complete(Box<CompletedFileIndexBuild>),
    Cancelled,
}

struct CompletedFileIndexBuild {
    index: crate::ui::plain_disposal::DisposalOwned<FileIndex>,
    metrics: FileIndexBuildMetrics,
}

impl LushtextWindow {
    pub(super) fn toggle_command_palette(&self) {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            self.close_command_palette();
        } else {
            let weak = glib::WeakRef::new();
            if let Some(focused) = gtk4::prelude::GtkWindowExt::focus(self) {
                weak.set(Some(&focused));
            }
            imp.saved_focus.replace(Some(weak));

            // Covers the case window activation cannot: a build or checkout
            // that completed while the user never left the application. The
            // palette opens against the installed index immediately; results
            // update in place if the rebuild finds anything.
            //
            // `IndexOnly`: the palette does not read the sidebar tree, and a
            // full materialized-tree rescan per `Ctrl+Shift+P` is real
            // filesystem work competing for the same worker slots as the
            // rebuild the palette actually needs.
            let _admission =
                self.refresh_workspace_surfaces_on_attention(super::AttentionSurfaces::IndexOnly);
            self.refresh_command_palette_sources();
            imp.palette_revealer.set_reveal_child(true);
            imp.command_palette.open();
            self.set_command_palette_actions_enabled(true);
            self.refresh_command_palette_note_source();
        }
    }

    pub(super) fn close_command_palette(&self) {
        let imp = self.imp();
        imp.command_palette.close();
        imp.palette_revealer.set_reveal_child(false);
        self.set_command_palette_actions_enabled(false);
        self.restore_saved_focus();
    }

    /// Enable actions that require the visible command-palette overlay.
    pub(super) fn set_command_palette_actions_enabled(&self, enabled: bool) {
        for name in ["set-command-palette-query", "set-command-palette-mode"] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Set command-palette text through the visible search entry.
    pub(super) fn set_command_palette_query(&self, query: &str) {
        if !self.imp().palette_revealer.reveals_child() {
            return;
        }
        self.imp().command_palette.set_query(query);
    }

    /// Set the command-palette mode using the same stable names as snapshots.
    pub(super) fn set_command_palette_mode(&self, mode_name: &str) {
        if !self.imp().palette_revealer.reveals_child() {
            return;
        }
        let Some(mode) = SearchMode::from_stable_name(mode_name) else {
            tracing::error!(
                "set-command-palette-mode: expected one of all, files, notes, commands"
            );
            return;
        };
        self.imp().command_palette.set_search_mode(mode);
    }

    /// Restore focus to the widget saved before an overlay was opened.
    ///
    /// The read half of `saved_focus`, whose write half is
    /// `toggle_command_palette` directly above. Its **only** caller is
    /// `close_command_palette`, which is why it belongs with the overlay rather
    /// than with the focus-restoration block it was extracted from.
    fn restore_saved_focus(&self) {
        let saved = self.imp().saved_focus.take();
        let target = saved.as_ref().and_then(glib::WeakRef::upgrade).or_else(|| {
            self.active_editor()
                .map(|e| e.source_view().clone().upcast::<gtk4::Widget>())
        });

        match target {
            Some(widget) => {
                widget.grab_focus();
            }
            None => {
                gtk4::prelude::GtkWindowExt::set_focus(self, gtk4::Widget::NONE);
            }
        }
    }

    /// Build the file index from all workspace folders on a background thread.
    pub fn rebuild_file_index(&self) {
        self.imp().index_rebuild_debounce.schedule(
            self,
            std::time::Duration::from_millis(300),
            move |window, _| {
                let request = FileIndexBuildRequest {
                    workspace_folders: Arc::from(window.current_workspace_folder_paths()),
                    capacity_hint: window.imp().command_palette.file_index_len().max(64),
                    visibility: crate::ui::workspace_visibility::workspace_entry_visibility(
                        &window.imp().settings,
                    ),
                };
                let start = window.imp().file_index_builds.borrow_mut().submit(request);
                if let Some(start) = start {
                    window.start_file_index_build(start);
                } else {
                    window.finish_cancelled_file_index_admission();
                }
            },
        );
    }

    fn start_file_index_build(&self, start: FileIndexBuildStart) {
        if start.cancellation.is_cancelled() {
            self.finish_file_index_build(start.generation, GuardedFileIndexBuildOutcome::Cancelled);
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let weight = crate::services::palette::MAX_FILE_INDEX_RETAINED_BYTES;
        let reservation = self
            .imp()
            .command_palette
            .file_index_reservation_weight()
            .map_or_else(
                || crate::ui::plain_disposal::try_reserve_for_gtk(weight),
                |current_weight| {
                    crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                        weight,
                        current_weight,
                    )
                },
            );
        let Some(reservation) = reservation else {
            debug_assert!(self.imp().file_index_admission.borrow().is_none());
            self.imp().file_index_admission.replace(Some(start));
            let window_weak = self.downgrade();
            self.imp()
                .file_index_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.retry_file_index_admission();
                    }
                });
            self.publish_status_message(
                "Workspace file index update deferred by memory pressure",
                MessageKind::Warning,
            );
            return;
        };

        let FileIndexBuildStart {
            generation,
            request,
            cancellation,
        } = start;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let outcome = FileIndex::rebuild_cancellable_with_hint(
                    &request.workspace_folders,
                    request.capacity_hint,
                    &cancellation,
                    &request.visibility,
                );
                match outcome {
                    FileIndexBuildOutcome::Complete { index, metrics } => {
                        let retained_bytes = index.retained_byte_weight();
                        debug_assert!(
                            retained_bytes
                                <= crate::services::palette::MAX_FILE_INDEX_RETAINED_BYTES
                        );
                        GuardedFileIndexBuildOutcome::Complete(Box::new(CompletedFileIndexBuild {
                            index: reservation.shrink_to_and_own(retained_bytes, index),
                            metrics,
                        }))
                    }
                    FileIndexBuildOutcome::Cancelled { .. } => {
                        GuardedFileIndexBuildOutcome::Cancelled
                    }
                }
            },
            move |(), outcome| {
                let Some(window) = window_weak.upgrade() else {
                    retire_file_index_outcome(outcome);
                    return;
                };
                window.finish_file_index_build(generation, outcome);
            },
        );
    }

    fn retry_file_index_admission(&self) {
        let Some(start) = self.imp().file_index_admission.borrow_mut().take() else {
            return;
        };
        self.start_file_index_build(start);
    }

    fn finish_cancelled_file_index_admission(&self) {
        let cancelled = self
            .imp()
            .file_index_admission
            .borrow()
            .as_ref()
            .is_some_and(|start| start.cancellation.is_cancelled());
        if !cancelled {
            return;
        }
        self.imp().file_index_capacity_wakeup.cancel();
        let start = self.imp().file_index_admission.borrow_mut().take();
        if let Some(start) = start {
            self.finish_file_index_build(start.generation, GuardedFileIndexBuildOutcome::Cancelled);
        }
    }

    fn finish_file_index_build(&self, generation: u64, outcome: GuardedFileIndexBuildOutcome) {
        // Timed here rather than at the caller because this is the one terminal
        // every build reaches, accepted or superseded; the adaptive interval
        // must learn from a slow pass even when its result was discarded.
        super::attention_refresh::note_attention_refresh_settled();
        let (accepted, next) = {
            let mut builds = self.imp().file_index_builds.borrow_mut();
            let accepted = builds.is_current(generation);
            let next = builds.finish(generation);
            (accepted, next)
        };

        if accepted {
            match outcome {
                GuardedFileIndexBuildOutcome::Complete(completed) => {
                    let CompletedFileIndexBuild { index, metrics } = *completed;
                    let indexed_files = index.len();
                    self.imp().command_palette.set_guarded_file_index(index);
                    self.announce_workflow_update(
                        AnnouncementLane::ProgressMilestone,
                        "workspace-file-index-updated",
                        &format!("Workspace file index updated with {indexed_files} files"),
                    );
                    if metrics.truncation.is_some() {
                        self.publish_status_message(
                            &format!("Workspace file index limited to {indexed_files} entries"),
                            MessageKind::Warning,
                        );
                    }
                }
                GuardedFileIndexBuildOutcome::Cancelled => {}
            }
        } else {
            retire_file_index_outcome(outcome);
        }

        if let Some(next) = next {
            self.start_file_index_build(next);
        }
    }

    /// Refresh command-palette source metadata owned by the window shell.
    pub(super) fn refresh_command_palette_sources(&self) {
        let open_tabs = self.open_file_palette_entries();
        let workspace_group_label = self.command_palette_workspace_group_label();
        self.imp()
            .command_palette
            .set_sources(open_tabs, workspace_group_label);
        self.refresh_command_palette_note_source();
    }

    /// Snapshot file-backed tabs so the palette can search active documents.
    fn open_file_palette_entries(&self) -> Vec<PaletteFileEntry> {
        let tab_view = &self.imp().tab_view;
        let mut entries =
            Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or_default());

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                && let Some(path) = editor.file_path()
            {
                entries.push(PaletteFileEntry::new(
                    editor.title(),
                    path.display().to_string(),
                    path,
                    editor.canonical_file_path().map_or(
                        PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotResolved),
                        PaletteFileIdentity::canonical,
                    ),
                ));
            }
        }

        entries
    }

    /// Name the workspace file group according to the sidebar's current scope.
    fn command_palette_workspace_group_label(&self) -> &'static str {
        if self.current_workspace_scope().is_all() {
            "All Workspaces"
        } else {
            "Selected Workspace"
        }
    }
}

fn retire_file_index_outcome(outcome: GuardedFileIndexBuildOutcome) {
    if let GuardedFileIndexBuildOutcome::Complete(completed) = outcome {
        drop(completed.index);
    }
}
