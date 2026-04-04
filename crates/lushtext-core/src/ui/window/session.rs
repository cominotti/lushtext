// SPDX-License-Identifier: GPL-3.0-or-later

//! Session persistence and draft management for the main window.
//!
//! Extracted from `mod.rs` to keep per-file line counts under the 1000-line
//! production code limit. All methods are `impl super::LushtextWindow`.

use crate::model::draft::DraftEntry;
use crate::model::session::{SessionData, SessionTab};
use crate::services::{async_task, draft_service, editor_io, json_store, session_service};
use crate::ui::editor_page::LushtextEditorPage;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use std::path::Path;
use std::time::Duration;

impl super::LushtextWindow {
    /// Snapshot current tab state into a `SessionData`.
    pub fn collect_session(&self) -> SessionData {
        let tab_view = &self.imp().tab_view;
        let mut tabs = Vec::with_capacity(tab_view.n_pages() as usize);

        let selected = tab_view.selected_page();
        let mut active_tab_index = None;

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let (cursor_line, cursor_col) = editor.cursor_position();
                let path = editor.file_path();
                let draft_id = if path.is_none() {
                    editor.draft_id()
                } else {
                    None
                };
                tabs.push(SessionTab {
                    path,
                    draft_id,
                    cursor_line,
                    cursor_col,
                    scroll_line: editor.visible_top_line(),
                });
                if selected.as_ref() == Some(&page) {
                    active_tab_index = Some(i as usize);
                }
            }
        }

        SessionData {
            tabs,
            active_tab_index,
        }
    }

    /// Save session with a 500ms debounce. No-op during session restore.
    pub fn save_session_debounced(&self) {
        if self.imp().restoring_session.get() {
            return;
        }
        let generation = self.imp().session_save_generation.get().wrapping_add(1);
        self.imp().session_save_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if window.imp().session_save_generation.get() != generation {
                return;
            }
            let session = window.collect_session();
            let data_dir = json_store::data_dir();
            async_task::spawn_blocking_then(
                (),
                move || {
                    if let Err(e) = session_service::save(&data_dir, &session) {
                        tracing::error!("Failed to save session: {e}");
                    }
                },
                |(), ()| {},
            );
        });
    }

    /// Synchronous session save for the close_request path.
    /// Session JSON is tiny so this completes in <1ms.
    pub fn save_session_sync(&self) {
        let session = self.collect_session();
        let data_dir = json_store::data_dir();
        if let Err(e) = session_service::save(&data_dir, &session) {
            tracing::error!("Failed to save session on close: {e}");
        }
    }

    /// Write all dirty drafts synchronously. Called on window close so that
    /// unsaved buffer content survives even if the autosave timer hadn't
    /// fired yet (its 30-second interval can miss short editing sessions).
    pub fn flush_dirty_drafts(&self) {
        let tab_view = &self.imp().tab_view;
        let data_dir = json_store::data_dir();
        let mut manifest = self.imp().draft_manifest.borrow().clone();
        let now = editor_io::now_epoch_secs();

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
            let buffer = editor.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();
            if text.is_empty() {
                continue;
            }
            if let Err(e) = draft_service::write_draft(&data_dir, &draft_id, &text) {
                tracing::warn!("Failed to write draft on close: {e}");
                continue;
            }
            let original_path = editor.file_path();
            let mtime = original_path
                .as_ref()
                .and_then(|p| editor_io::mtime_secs(p));
            manifest.upsert(DraftEntry {
                draft_id,
                original_path,
                original_mtime_secs: mtime,
                saved_at_secs: now,
            });
        }
        let _ = draft_service::save_manifest(&data_dir, &manifest);
    }

    // --- Session restore + draft persistence ---

    /// Load draft manifest and session in one background task, then restore
    /// tabs. Combined so that the manifest is ready before `open_document`
    /// calls `check_draft_on_open` for each restored tab.
    pub fn load_session_and_drafts(&self) {
        let data_dir = json_store::data_dir();
        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let mut manifest = draft_service::load_manifest(&data_dir).unwrap_or_default();
                let _ = draft_service::cleanup_orphans(&data_dir, &mut manifest);
                let _ = draft_service::save_manifest(&data_dir, &manifest);

                let mut session = session_service::load(&data_dir).unwrap_or_default();
                session_service::filter_existing_tabs(&mut session);

                (manifest, session)
            },
            |window, (manifest, session)| {
                *window.imp().draft_manifest.borrow_mut() = manifest;
                window.restore_tabs(session);
            },
        );
    }

    /// Restore tabs from a loaded session. Opens file-backed tabs via
    /// `open_document` and creates untitled tabs with draft recovery.
    fn restore_tabs(&self, session: SessionData) {
        if session.tabs.is_empty() {
            return;
        }
        let had_tabs_before = self.imp().tab_view.n_pages() > 0;
        self.imp().restoring_session.set(true);

        for tab in &session.tabs {
            match &tab.path {
                Some(path) => {
                    self.open_document(path);
                    // Find the just-opened editor and set restore position.
                    if let Some(editor) = self.active_editor() {
                        editor.set_restore_position(
                            tab.cursor_line,
                            tab.cursor_col,
                            tab.scroll_line,
                        );
                    }
                }
                None => {
                    self.new_tab();
                    // For untitled tabs, override the draft ID and trigger
                    // draft content recovery.
                    if let Some(editor) = self.active_editor()
                        && let Some(ref draft_id) = tab.draft_id
                    {
                        editor.set_draft_id(draft_id.clone());
                        self.check_draft_by_id(&editor, draft_id);
                    }
                }
            }
        }

        // Restore the active tab selection — but only if no CLI files
        // were opened before this restore (they take priority).
        if !had_tabs_before && let Some(idx) = session.active_tab_index {
            let tab_view = &self.imp().tab_view;
            let idx = idx.min(tab_view.n_pages().saturating_sub(1) as usize);
            let page = tab_view.nth_page(idx as i32);
            tab_view.set_selected_page(&page);
        }

        self.imp().restoring_session.set(false);
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Load draft content for an untitled tab by draft ID.
    pub fn check_draft_by_id(&self, editor: &LushtextEditorPage, draft_id: &str) {
        let entry = self
            .imp()
            .draft_manifest
            .borrow()
            .find_by_id(draft_id)
            .cloned();

        let Some(_entry) = entry else {
            return;
        };

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
                if let Ok(Some(draft_content)) = result {
                    let buffer = editor.buffer();
                    buffer.begin_irreversible_action();
                    buffer.set_text(&draft_content);
                    buffer.end_irreversible_action();
                    buffer.set_modified(true);
                    editor.set_draft_restored(true);
                    editor.info_bar().show_draft_restored(false);
                }
            },
        );
    }

    /// Start the global 30-second autosave timer. Iterates all tabs on each
    /// tick and writes drafts for modified buffers that changed since the
    /// last draft write.
    pub fn start_autosave_timer(&self) {
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(30), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            window.autosave_tick();
            glib::ControlFlow::Continue
        });
        *self.imp().autosave_source_id.borrow_mut() = Some(source_id);
    }

    /// Single autosave tick: collect dirty tabs and write drafts.
    fn autosave_tick(&self) {
        let tab_view = &self.imp().tab_view;
        // Collect draft_id, text, and optional path (for mtime read on background thread).
        let mut dirty_tabs: Vec<(String, String, Option<std::path::PathBuf>)> = Vec::new();

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
            if text.is_empty() {
                continue;
            }
            dirty_tabs.push((draft_id, text, editor.file_path()));
            editor.set_draft_dirty(false);
        }

        if dirty_tabs.is_empty() {
            return;
        }

        let manifest = self.imp().draft_manifest.borrow().clone();
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();

        async_task::spawn_blocking_then(
            (),
            move || {
                let mut manifest = manifest;
                let now = editor_io::now_epoch_secs();

                for (draft_id, text, path) in &dirty_tabs {
                    if let Err(e) = draft_service::write_draft(&data_dir, draft_id, text) {
                        tracing::warn!("Failed to write draft {draft_id}: {e}");
                        continue;
                    }
                    // Read mtime on background thread to avoid blocking GTK main thread
                    // (stat syscall can be slow on NFS/FUSE mounts).
                    let mtime = path.as_deref().and_then(editor_io::mtime_secs);
                    let original_path = manifest
                        .find_by_id(draft_id)
                        .and_then(|e| e.original_path.clone());
                    manifest.upsert(DraftEntry {
                        draft_id: draft_id.clone(),
                        original_path,
                        original_mtime_secs: mtime,
                        saved_at_secs: now,
                    });
                }
                let _ = draft_service::save_manifest(&data_dir, &manifest);
                manifest
            },
            move |(), manifest| {
                if let Some(window) = window_weak.upgrade() {
                    *window.imp().draft_manifest.borrow_mut() = manifest;
                }
            },
        );
    }

    /// Check if a draft exists for the given path. If found, load it on a
    /// background thread and apply to the editor buffer.
    pub fn check_draft_on_open(&self, editor: &LushtextEditorPage, path: &Path) {
        let draft_entry = self
            .imp()
            .draft_manifest
            .borrow()
            .find_by_path(path)
            .cloned();

        let Some(_entry) = draft_entry else {
            return;
        };

        let data_dir = json_store::data_dir();
        let draft_id = _entry.draft_id.clone();
        let editor_weak = editor.downgrade();

        async_task::spawn_blocking_then(
            (),
            move || draft_service::read_draft(&data_dir, &draft_id),
            move |(), result| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if let Ok(Some(draft_content)) = result {
                    let buffer = editor.buffer();
                    buffer.begin_irreversible_action();
                    buffer.set_text(&draft_content);
                    buffer.end_irreversible_action();
                    buffer.set_modified(true);
                    let has_backing_file = editor.file_path().is_some();
                    editor.set_draft_restored(true);
                    editor.info_bar().show_draft_restored(has_backing_file);
                }
            },
        );
    }

    /// Delete the draft for a given file path. Removes the in-memory manifest
    /// entry immediately; background file deletion is batched by `flush_draft_deletions`.
    pub fn delete_draft_for_path(&self, path: &Path) {
        let draft_id = {
            let manifest = self.imp().draft_manifest.borrow();
            manifest.find_by_path(path).map(|e| e.draft_id.clone())
        };
        if let Some(draft_id) = draft_id {
            self.delete_draft_by_id(&draft_id);
        }
    }

    /// Delete a draft by its ID. Removes the in-memory manifest entry
    /// immediately and schedules background file deletion.
    pub fn delete_draft_by_id(&self, draft_id: &str) {
        self.imp()
            .draft_manifest
            .borrow_mut()
            .remove_by_id(draft_id);

        let data_dir = json_store::data_dir();
        let draft_id = draft_id.to_string();
        let manifest = self.imp().draft_manifest.borrow().clone();

        std::thread::spawn(move || {
            let _ = draft_service::delete_draft_file(&data_dir, &draft_id);
            let _ = draft_service::save_manifest(&data_dir, &manifest);
        });
    }

    /// Allocate a draft ID for a new editor page. For path-backed files,
    /// uses a deterministic hash. For untitled tabs, uses a monotonic counter.
    pub fn assign_draft_id(&self, editor: &LushtextEditorPage) {
        let id = if let Some(ref path) = editor.file_path() {
            draft_service::draft_id_for_path(path)
        } else {
            let counter = self.imp().next_tab_id.get();
            self.imp().next_tab_id.set(counter.wrapping_add(1));
            draft_service::draft_id_for_untitled(counter)
        };
        editor.set_draft_id(id);
    }
}
