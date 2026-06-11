// SPDX-License-Identifier: GPL-3.0-or-later

//! Session save/restore flows for the main window.
//!
//! This slice owns tab-state collection, debounced session persistence, and
//! startup restore orchestration. Draft-specific lifecycle work stays in
//! `drafts.rs`, even when restore needs to hand draft state across the split.

use std::time::Duration;

use crate::model::session::{SessionData, SessionTab};
use crate::services::notifications::NotificationSeverity;
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryPreservation, RecoveryProblem,
};
use crate::services::{async_task, draft_service, json_store, session_service};
use crate::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

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
                let path =
                    if editor.load_state() == EditorLoadState::Failed && !editor.is_modified() {
                        None
                    } else {
                        editor.file_path()
                    };
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

        self.imp().session.save_debounce.schedule(
            self,
            Duration::from_millis(500),
            move |window, token| {
                let generation = token.value();
                let session = window.collect_session();
                let data_dir = json_store::data_dir();
                let ordered_generation = u64::from(generation);
                async_task::spawn_blocking_then(
                    window,
                    move || session_service::save_ordered(&data_dir, &session, ordered_generation),
                    move |window, result| match result {
                        Ok(true) => window.clear_session_save_failure(generation),
                        Ok(false) => {}
                        Err(error) => {
                            tracing::error!("Failed to save session: {error}");
                            let detail = error.to_string();
                            window.record_session_save_failure(generation, &detail, true);
                        }
                    },
                );
            },
        );
    }

    /// Synchronous session save for the close-request path.
    pub fn save_session_sync(&self) {
        let generation = self.imp().session.save_debounce.advance().value();
        let session = self.collect_session();
        let data_dir = json_store::data_dir();
        match session_service::save_ordered(&data_dir, &session, u64::from(generation)) {
            Ok(true) => self.clear_session_save_failure(generation),
            Ok(false) => {}
            Err(error) => {
                tracing::error!("Failed to save session on close: {error}");
                let detail = error.to_string();
                self.record_session_save_failure(generation, &detail, true);
            }
        }
    }

    /// Save session for close on a background worker, then continue the close flow.
    pub fn save_session_for_close_async<F: FnOnce() + 'static>(&self, on_done: F) {
        let generation = self.imp().session.save_debounce.advance().value();
        let session = self.collect_session();
        let data_dir = json_store::data_dir();
        async_task::spawn_blocking_then(
            self.clone(),
            move || session_service::save_ordered(&data_dir, &session, u64::from(generation)),
            move |window, result| {
                match result {
                    Ok(true) => window.clear_session_save_failure(generation),
                    Ok(false) => {}
                    Err(error) => {
                        tracing::error!("Failed to save session on close: {error}");
                        let detail = error.to_string();
                        window.record_session_save_failure(generation, &detail, true);
                    }
                }
                on_done();
            },
        );
    }

    /// Load the session file plus draft restore state in one background task.
    pub fn load_session_and_drafts(&self) {
        let data_dir = json_store::data_dir();
        async_task::spawn_blocking_then(
            self.clone(),
            move || draft_service::load_restore_state(&data_dir),
            |window, loaded| {
                *window.imp().drafts.manifest.borrow_mut() = loaded.manifest;
                *window.imp().drafts.preloaded.borrow_mut() = loaded.preloaded_drafts;
                for diagnostic in &loaded.diagnostics {
                    tracing::warn!("{}", diagnostic.summary());
                }
                window.restore_tabs(&loaded.session);
                window.publish_startup_recovery_diagnostics(&loaded.diagnostics);
                window.schedule_orphan_cleanup(loaded.orphan_cleanup_allowed);
            },
        );
    }

    /// Restore tabs from a loaded session. Opens file-backed tabs via
    /// `open_document` and creates untitled tabs with draft recovery.
    fn restore_tabs(&self, session: &SessionData) {
        if session.tabs.is_empty() {
            return;
        }
        let tab_view = &self.imp().tab_view;
        let had_tabs_before = tab_view.n_pages() > 0;
        let selected_before_restore = tab_view.selected_page();
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

        if had_tabs_before {
            // Explicit file activations can create tabs while the startup
            // session is still loading. Restore may add older tabs after that,
            // but it must not steal selection from the user's requested file.
            if let Some(page) = selected_before_restore {
                tab_view.set_selected_page(&page);
            }
        } else if let Some(idx) = session.active_tab_index {
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

    fn publish_startup_recovery_diagnostics(&self, diagnostics: &[RecoveryDiagnostic]) {
        if diagnostics.is_empty() {
            return;
        }
        let message = startup_recovery_status_message(diagnostics);
        self.publish_status_message(&message, NotificationSeverity::Warning);
    }

    fn record_session_save_failure(&self, generation: u32, detail: &str, visible: bool) {
        let session = &self.imp().session;
        session.save_failed.set(true);
        session.failed_generation.set(generation);
        *session.failure_detail.borrow_mut() = Some(detail.to_string());
        if visible {
            self.publish_status_message(
                &format!("Session layout may not restore: {detail}"),
                NotificationSeverity::Warning,
            );
        }
    }

    fn clear_session_save_failure(&self, generation: u32) {
        let session = &self.imp().session;
        // A late successful save must not clear a newer failure banner, so only
        // the same or newer generation may mark the session state healthy again.
        if session.save_failed.get() && generation >= session.failed_generation.get() {
            session.save_failed.set(false);
            session.failed_generation.set(0);
            session.failure_detail.borrow_mut().take();
        }
    }
}

fn startup_recovery_status_message(diagnostics: &[RecoveryDiagnostic]) -> String {
    let damaged = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::Malformed { .. }
                    | RecoveryProblem::UnsupportedFormat { .. }
                    | RecoveryProblem::UnsupportedVersion { .. }
                    | RecoveryProblem::Unreadable { .. }
                    | RecoveryProblem::UnsupportedFileKind { .. }
                    | RecoveryProblem::Oversized { .. }
            )
        })
        .count();
    let repaired = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.problem, RecoveryProblem::Repaired { .. }))
        .count();
    let skipped = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.problem, RecoveryProblem::RepairSkipped { .. }))
        .count();
    let preserved = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.preservation,
                RecoveryPreservation::Quarantined { .. }
                    | RecoveryPreservation::CopiedToQuarantine { .. }
                    | RecoveryPreservation::PreservedInPlace
            )
        })
        .count();

    match (damaged > 0, repaired > 0, skipped > 0, preserved > 0) {
        (true, true, _, true) => format!(
            "Some recovery data was repaired; {damaged} issue(s) were preserved for inspection"
        ),
        (true, false, true, true) => format!(
            "Some recovery data could not be loaded; {damaged} issue(s) were preserved for inspection"
        ),
        (true, _, _, _) => {
            format!("Some recovery data could not be loaded ({damaged} issue(s))")
        }
        (false, true, true, _) => {
            "Some recovery data was partially repaired; other items were preserved".to_string()
        }
        (false, true, false, _) => "Some recovery data was repaired".to_string(),
        (false, false, true, _) => {
            "Some recovery data could not be repaired automatically".to_string()
        }
        (false, false, false, _) => "Recovery data changed during startup".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::services::recovery_metadata::{RecoveryMetadataClass, RecoveryPreservation};

    #[test]
    fn startup_recovery_status_groups_damage_and_repair() {
        let diagnostics = vec![
            RecoveryDiagnostic::with_preservation(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                RecoveryProblem::Malformed {
                    detail: "bad JSON".to_string(),
                },
                RecoveryPreservation::Quarantined {
                    path: PathBuf::from("/tmp/quarantine/manifest.json"),
                },
            ),
            RecoveryDiagnostic::repaired(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                "rebuilt one draft",
            ),
        ];

        let message = startup_recovery_status_message(&diagnostics);

        assert!(message.contains("repaired"));
        assert!(message.contains("preserved"));
    }

    #[test]
    fn startup_recovery_status_mentions_unrepaired_items() {
        let diagnostics = vec![RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::DraftManifest,
            PathBuf::from("/tmp/manifest.json"),
            "ambiguous draft",
        )];

        let message = startup_recovery_status_message(&diagnostics);

        assert!(message.contains("could not be repaired"));
    }
}
