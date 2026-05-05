// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence, recovery, and autosave flows for the main window.
//!
//! This slice owns the data-safety-sensitive draft lifecycle: close-time flush,
//! crash recovery, autosave, and manifest maintenance. Session-only tab-state
//! capture lives separately in `session_persistence.rs`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::draft::{DraftEntry, FileDraftRestoreResolution, PreloadedDraftRestore};
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{async_task, draft_service, editor_io, json_store};
use crate::ui::editor_page::LushtextEditorPage;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

/// Snapshot of one dirty editor at the moment an autosave tick starts.
///
/// The background thread receives owned text and path data so it never has to
/// touch GTK objects off the main thread.
struct DirtyDraftSnapshot {
    draft_id: String,
    text: String,
    original_path: Option<PathBuf>,
}

/// Result of one autosave batch after background I/O finishes.
struct DraftAutosaveResult {
    manifest: Option<crate::model::draft::DraftManifest>,
    failed_ids: Vec<String>,
}

impl super::LushtextWindow {
    /// Write all dirty drafts synchronously during window close.
    pub fn flush_dirty_drafts(&self) {
        let tab_view = &self.imp().tab_view;
        let data_dir = json_store::data_dir();
        let now = editor_io::now_epoch_secs();
        let mut manifest_updates = Vec::new();
        let discarded_draft_ids = self.imp().drafts.close_discard_ids.borrow().clone();

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if !editor.is_modified() || editor.is_evicted() {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            if discarded_draft_ids.contains(&draft_id) {
                continue;
            }
            let buffer = editor.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();
            if let Err(e) = draft_service::write_draft(&data_dir, &draft_id, &text) {
                tracing::error!("Failed to write draft on close: {e}");
                continue;
            }
            let original_path = editor.file_path();
            let mtime = original_path
                .as_ref()
                .and_then(|path| editor_io::mtime_secs(path));
            manifest_updates.push(DraftEntry {
                draft_id,
                original_path,
                original_mtime_secs: mtime,
                saved_at_secs: now,
            });
        }
        if manifest_updates.is_empty() {
            self.clear_close_discard_drafts();
            return;
        }
        if let Err(e) = draft_service::update_manifest(&data_dir, |manifest| {
            for entry in manifest_updates {
                manifest.upsert(entry);
            }
        }) {
            tracing::error!("Failed to save draft manifest on close: {e}");
        }
        self.clear_close_discard_drafts();
    }

    /// Load draft content for an untitled tab by draft ID.
    pub fn check_draft_by_id(&self, editor: &LushtextEditorPage, draft_id: &str) {
        let entry = self
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(draft_id)
            .cloned();

        let Some(_entry) = entry else {
            return;
        };

        if let Some(preloaded) = self.imp().drafts.preloaded.borrow_mut().remove(draft_id) {
            match preloaded {
                PreloadedDraftRestore::Content(draft_content) => {
                    Self::apply_draft(editor, &draft_content);
                }
                PreloadedDraftRestore::SkipStaleFile => {
                    tracing::warn!(
                        "Untitled draft {draft_id} unexpectedly carried a stale file warning"
                    );
                }
            }
            return;
        }

        let data_dir = json_store::data_dir();
        let draft_id = draft_id.to_string();
        let editor_weak = editor.downgrade();

        async_task::spawn_blocking_then(
            (),
            move || draft_service::read_draft(&data_dir, &draft_id),
            move |(), result| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(Some(draft_content)) => {
                        Self::apply_draft(&editor, &draft_content);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::error!("Failed to read draft from disk: {e}");
                    }
                }
            },
        );
    }

    /// Apply restored draft content to the editor buffer and show the infobar action.
    fn apply_draft(editor: &LushtextEditorPage, content: &str) {
        let buffer = editor.buffer();
        // Seed local history before mutating the buffer because `set_text()`
        // can already flip the modified state and trigger the baseline path.
        // Restored drafts should baseline the restored work, not the stale file.
        editor.seed_local_history_from_restored_draft(content);
        editor.set_minimap_tracking_suspended(true);
        buffer.begin_irreversible_action();
        buffer.set_text(content);
        buffer.end_irreversible_action();
        editor.set_minimap_tracking_suspended(false);
        buffer.set_modified(true);
        if editor.file_path().is_some() {
            editor.mark_entire_buffer_modified();
        } else {
            editor.schedule_minimap_refresh();
        }
        let has_backing_file = editor.file_path().is_some();
        editor.set_draft_restored(true);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: if has_backing_file {
                "Draft Changes Restored".to_string()
            } else {
                "Document Restored".to_string()
            },
            body: if has_backing_file {
                "Unsaved changes from a previous session have been restored.".to_string()
            } else {
                "Unsaved document has been restored.".to_string()
            },
            primary_button: Some("_Discard…".to_string()),
            secondary_button: Some(if has_backing_file {
                "_Save…".to_string()
            } else {
                "Save _As…".to_string()
            }),
        });
    }

    /// Warn that a file-backed draft was skipped because the file changed on disk.
    fn show_stale_draft_skipped(editor: &LushtextEditorPage) {
        editor.set_draft_restored(false);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Draft Not Restored".to_string(),
            body: "Unsaved changes from a previous session were not restored because the file changed on disk.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Deferred orphan cleanup — runs after restore so startup stays responsive.
    pub(super) fn schedule_orphan_cleanup(&self) {
        let window_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_secs(2), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            window.imp().drafts.preloaded.borrow_mut().clear();

            let data_dir = json_store::data_dir();
            let manifest = window.imp().drafts.manifest.borrow().clone();
            let ids_before: Vec<String> = manifest
                .drafts
                .iter()
                .map(|entry| entry.draft_id.clone())
                .collect();

            async_task::spawn_blocking_then(
                window,
                move || {
                    let mut manifest = manifest;
                    let _ = draft_service::cleanup_orphans(&data_dir, &mut manifest);
                    let ids_after: HashSet<&str> = manifest
                        .drafts
                        .iter()
                        .map(|entry| entry.draft_id.as_str())
                        .collect();
                    ids_before
                        .into_iter()
                        .filter(|id| !ids_after.contains(id.as_str()))
                        .collect::<Vec<_>>()
                },
                |window, removed_ids| {
                    if !removed_ids.is_empty() {
                        window
                            .imp()
                            .drafts
                            .manifest
                            .borrow_mut()
                            .drafts
                            .retain(|entry| !removed_ids.contains(&entry.draft_id));
                    }
                },
            );
        });
    }

    /// Start the global 5-second autosave timer.
    pub fn start_autosave_timer(&self) {
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(5), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            window.autosave_tick();
            glib::ControlFlow::Continue
        });
        *self.imp().drafts.autosave_source_id.borrow_mut() = Some(source_id);
    }

    /// Single autosave tick: collect dirty tabs and write drafts.
    fn autosave_tick(&self) {
        if self.imp().drafts.autosave_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
            return;
        }

        let tab_view = &self.imp().tab_view;
        let mut dirty_tabs = Vec::new();

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if !editor.is_modified() || !editor.draft_dirty() || editor.is_evicted() {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            let buffer = editor.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();
            dirty_tabs.push(DirtyDraftSnapshot {
                draft_id,
                text,
                original_path: editor.file_path(),
            });
            editor.set_draft_dirty(false);
        }

        if dirty_tabs.is_empty() {
            return;
        }

        let manifest = self.imp().drafts.manifest.borrow().clone();
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();
        self.imp().drafts.autosave_inflight.set(true);

        async_task::spawn_blocking_then(
            (),
            move || {
                let now = editor_io::now_epoch_secs();
                let mut manifest_updates = Vec::new();
                let mut failed_ids = Vec::new();

                for draft in &dirty_tabs {
                    if let Err(e) =
                        draft_service::write_draft(&data_dir, &draft.draft_id, &draft.text)
                    {
                        tracing::warn!("Failed to write draft {}: {e}", draft.draft_id);
                        failed_ids.push(draft.draft_id.clone());
                        continue;
                    }
                    let mtime = draft
                        .original_path
                        .as_deref()
                        .and_then(editor_io::mtime_secs);
                    manifest_updates.push(DraftEntry {
                        draft_id: draft.draft_id.clone(),
                        original_path: draft.original_path.clone(),
                        original_mtime_secs: mtime,
                        saved_at_secs: now,
                    });
                }

                let manifest = if manifest_updates.is_empty() {
                    Some(manifest)
                } else {
                    match draft_service::update_manifest(&data_dir, |manifest| {
                        for entry in manifest_updates {
                            manifest.upsert(entry);
                        }
                    }) {
                        Ok(manifest) => Some(manifest),
                        Err(e) => {
                            tracing::warn!("Failed to save draft manifest: {e}");
                            failed_ids
                                .extend(dirty_tabs.iter().map(|draft| draft.draft_id.clone()));
                            None
                        }
                    }
                };

                DraftAutosaveResult {
                    manifest,
                    failed_ids,
                }
            },
            move |(), result| {
                if let Some(window) = window_weak.upgrade() {
                    window.imp().drafts.autosave_inflight.set(false);
                    if let Some(manifest) = result.manifest {
                        *window.imp().drafts.manifest.borrow_mut() = manifest;
                    }
                    if !result.failed_ids.is_empty() {
                        let tab_view = &window.imp().tab_view;
                        for failed_id in result.failed_ids {
                            for i in 0..tab_view.n_pages() {
                                let page = tab_view.nth_page(i);
                                let child = page.child();
                                if let Some(editor) = child.downcast_ref::<LushtextEditorPage>()
                                    && editor.draft_id().as_deref() == Some(failed_id.as_str())
                                {
                                    editor.set_draft_dirty(true);
                                }
                            }
                        }
                    }
                    let rerun = window.imp().drafts.autosave_pending.get();
                    window.imp().drafts.autosave_pending.set(false);
                    if rerun {
                        window.autosave_tick();
                    }
                }
            },
        );
    }

    /// Remember that a fresh autosave pass is needed after the active batch.
    pub(crate) fn mark_draft_autosave_pending_if_inflight(&self) {
        if self.imp().drafts.autosave_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
        }
    }

    /// Check whether a file-backed editor has restored draft content available.
    pub fn check_draft_on_open(&self, editor: &LushtextEditorPage, path: &Path) {
        let draft_id = draft_service::draft_id_for_path(path);
        if let Some(preloaded) = self.imp().drafts.preloaded.borrow_mut().remove(&draft_id) {
            match preloaded {
                PreloadedDraftRestore::Content(draft_content) => {
                    Self::apply_draft(editor, &draft_content);
                }
                PreloadedDraftRestore::SkipStaleFile => {
                    Self::show_stale_draft_skipped(editor);
                }
            }
            return;
        }

        let draft_entry = self
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_path(path)
            .cloned();

        let Some(entry) = draft_entry else {
            return;
        };

        let data_dir = json_store::data_dir();
        let draft_id = entry.draft_id.clone();
        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();

        async_task::spawn_blocking_then(
            (),
            move || draft_service::resolve_file_draft_restore(&data_dir, &entry),
            move |(), result| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(FileDraftRestoreResolution::Restore { content }) => {
                        Self::apply_draft(&editor, &content);
                    }
                    Ok(FileDraftRestoreResolution::SkipStale) => {
                        Self::show_stale_draft_skipped(&editor);
                        if let Some(window) = window_weak.upgrade() {
                            window.delete_draft_by_id(&draft_id);
                        }
                    }
                    Ok(
                        FileDraftRestoreResolution::SkipUnavailable
                        | FileDraftRestoreResolution::MissingDraft,
                    ) => {}
                    Err(e) => {
                        tracing::error!("Failed to resolve draft for open file: {e}");
                    }
                }
            },
        );
    }

    /// Delete the draft for a given file path.
    pub fn delete_draft_for_path(&self, path: &Path) {
        let draft_id = {
            let manifest = self.imp().drafts.manifest.borrow();
            manifest
                .find_by_path(path)
                .map(|entry| entry.draft_id.clone())
        };
        if let Some(draft_id) = draft_id {
            self.delete_draft_by_id(&draft_id);
        }
    }

    /// Delete a draft by its ID and persist the manifest update.
    pub fn delete_draft_by_id(&self, draft_id: &str) {
        self.imp()
            .drafts
            .manifest
            .borrow_mut()
            .remove_by_id(draft_id);

        let data_dir = json_store::data_dir();
        let draft_id = draft_id.to_string();
        let window_weak = self.downgrade();
        async_task::spawn_blocking_then(
            (),
            move || {
                if let Err(e) = draft_service::delete_draft_file(&data_dir, &draft_id) {
                    tracing::warn!("Failed to delete draft file {draft_id}: {e}");
                }
                draft_service::update_manifest(&data_dir, |manifest| {
                    manifest.remove_by_id(&draft_id);
                })
            },
            move |(), result| {
                if let Some(window) = window_weak.upgrade() {
                    match result {
                        Ok(manifest) => {
                            *window.imp().drafts.manifest.borrow_mut() = manifest;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to save manifest after draft deletion: {e}");
                        }
                    }
                }
            },
        );
    }

    /// Allocate a draft ID for a new editor page.
    pub fn assign_draft_id(&self, editor: &LushtextEditorPage) {
        let id = if let Some(ref path) = editor.file_path() {
            draft_service::draft_id_for_path(path)
        } else {
            let counter = self.imp().drafts.next_tab_id.get();
            self.imp().drafts.next_tab_id.set(counter.wrapping_add(1));
            draft_service::draft_id_for_untitled(counter)
        };
        editor.set_draft_id(id);
    }
}
