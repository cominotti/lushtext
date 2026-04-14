// SPDX-License-Identifier: GPL-3.0-or-later

//! Session save/restore flows for the main window.
//!
//! This slice owns tab-state collection, debounced session persistence, and
//! startup restore orchestration. Draft-specific lifecycle work stays in
//! `drafts.rs`, even when restore needs to hand draft state across the split.

use std::collections::HashMap;
use std::time::Duration;

use crate::model::draft::DraftManifest;
use crate::model::session::{SessionData, SessionTab};
use crate::services::{async_task, draft_service, json_store, session_service};
use crate::ui::editor_page::LushtextEditorPage;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

/// Draft + session state loaded together on startup so restore only needs one
/// background round-trip before it can rebuild tabs.
struct LoadedRestoreState {
    manifest: DraftManifest,
    session: SessionData,
    preloaded_drafts: HashMap<String, String>,
}

impl super::LushtextWindow {
    /// Snapshot current tab state into one persisted `SessionData` value object.
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        reason = "AdwTabView page indices are non-negative when a tab exists"
    )]
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
                    pinned: page.is_pinned(),
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
        if self.imp().session.restoring.get() {
            return;
        }
        let generation = self.imp().session.save_generation.get().wrapping_add(1);
        self.imp().session.save_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if window.imp().session.save_generation.get() != generation {
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

    /// Synchronous session save for the close-request path.
    pub fn save_session_sync(&self) {
        let session = self.collect_session();
        let data_dir = json_store::data_dir();
        if let Err(e) = session_service::save(&data_dir, &session) {
            tracing::error!("Failed to save session on close: {e}");
        }
    }

    /// Load the session file plus draft restore state in one background task.
    pub fn load_session_and_drafts(&self) {
        let data_dir = json_store::data_dir();
        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let (manifest, session, preloaded_drafts) =
                    draft_service::load_restore_state(&data_dir);
                LoadedRestoreState {
                    manifest,
                    session,
                    preloaded_drafts,
                }
            },
            |window, loaded| {
                *window.imp().drafts.manifest.borrow_mut() = loaded.manifest;
                *window.imp().drafts.preloaded.borrow_mut() = loaded.preloaded_drafts;
                window.restore_tabs(&loaded.session);
                window.schedule_orphan_cleanup();
            },
        );
    }

    /// Restore tabs from a loaded session. Opens file-backed tabs via
    /// `open_document` and creates untitled tabs with draft recovery.
    fn restore_tabs(&self, session: &SessionData) {
        if session.tabs.is_empty() {
            return;
        }
        let had_tabs_before = self.imp().tab_view.n_pages() > 0;
        self.imp().session.restoring.set(true);

        for tab in &session.tabs {
            if let Some(path) = &tab.path {
                self.open_document(path);
                if let Some(page) = self.imp().tab_view.selected_page() {
                    self.restore_tab_pinned_state(&page, tab.pinned);
                    if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                        editor.set_restore_position(
                            tab.cursor_line,
                            tab.cursor_col,
                            tab.scroll_line,
                        );
                    }
                }
            } else {
                self.new_tab();
                if let Some(page) = self.imp().tab_view.selected_page() {
                    self.restore_tab_pinned_state(&page, tab.pinned);
                    if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                        && let Some(ref draft_id) = tab.draft_id
                    {
                        editor.set_draft_id(draft_id.clone());
                        self.check_draft_by_id(editor, draft_id);
                    }
                }
            }
        }

        if !had_tabs_before && let Some(idx) = session.active_tab_index {
            let tab_view = &self.imp().tab_view;
            #[expect(
                clippy::cast_sign_loss,
                reason = "AdwTabView page counts are non-negative"
            )]
            let idx = idx.min(tab_view.n_pages().saturating_sub(1) as usize);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Persisted tab indices come from the current tab set and stay well below i32::MAX"
            )]
            let page = tab_view.nth_page(idx as i32);
            tab_view.set_selected_page(&page);
        }

        self.imp().session.restoring.set(false);
        self.update_content_stack();
        self.refresh_status_bar();
    }
}
