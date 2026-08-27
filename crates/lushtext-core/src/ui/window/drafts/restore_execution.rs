// SPDX-License-Identifier: GPL-3.0-or-later

//! Installing a recovered draft back into the user's buffer.
//!
//! Stage-order-qualified `restore_execution` because this workflow owns **two**
//! execution-shaped coordination jobs — restoring, and the autosave/close write
//! pipelines — and both are new here, so neither is a stable sibling renamed for
//! symmetry.
//!
//! Every path through this module validates
//! `draft_restore_is_current(ticket, facts)` **again** immediately before it
//! publishes, not only when the worker returns: the bounded buffer replacement
//! that installs the body spans main-loop turns, and a tab that was reopened,
//! renamed, or edited in between must not receive a stale recovery body.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use anyhow::Result;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::draft::{FileDraftRestoreSkip, PreloadedDraftSkip};
use crate::services::draft_service;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::ui::editor_page::{
    BufferReplacementOutcome, BufferReplacementRequest, BufferReplacementTicket,
    BufferReplacementWorkflow, LushtextEditorPage,
};

use super::seams::{
    DraftRestoreTicket, DraftRestoreTracking, GuardedDraftRestoreResolution,
    GuardedPreloadedDraftRestore,
};
use crate::ui::window::LushtextWindow;

impl LushtextWindow {
    /// Load draft content for an untitled tab by draft ID.
    pub fn check_draft_by_id(&self, editor: &LushtextEditorPage, draft_id: &str) {
        let entry = self
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(draft_id)
            .cloned();

        let Some(entry) = entry else {
            return;
        };

        if let Some(preloaded) = self.take_preloaded_draft(draft_id) {
            match preloaded {
                GuardedPreloadedDraftRestore::Content(draft_content) => {
                    self.note_draft_restore_started();
                    self.apply_draft(
                        &DraftRestoreTicket::capture(editor, entry),
                        draft_content,
                        DraftRestoreTracking::Ordinary,
                    );
                }
                GuardedPreloadedDraftRestore::Compact(PreloadedDraftSkip::StaleFile) => {
                    tracing::warn!(
                        "Untitled draft {draft_id} unexpectedly carried a stale file warning"
                    );
                }
                GuardedPreloadedDraftRestore::Compact(PreloadedDraftSkip::Oversized) => {
                    Self::show_oversized_draft_skipped(editor);
                }
                GuardedPreloadedDraftRestore::Compact(PreloadedDraftSkip::LazyAggregateBudget) => {
                    self.queue_lazy_draft_restore(editor, entry);
                }
            }
            return;
        }

        self.queue_lazy_draft_restore(editor, entry);
    }

    /// Check whether a file-backed editor has restored draft content available.
    pub fn check_draft_on_open(&self, editor: &LushtextEditorPage, path: &Path) {
        if self.apply_preloaded_draft_for_path(editor, path) {
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

        self.queue_lazy_draft_restore(editor, entry);
    }

    /// Apply startup-preloaded draft data for a path, if one was prepared.
    ///
    /// Failed first-open placeholders use this before their path identity is
    /// cleared so crash-recovered edits remain tied to the user-requested file.
    pub(crate) fn apply_preloaded_draft_for_path(
        &self,
        editor: &LushtextEditorPage,
        path: &Path,
    ) -> bool {
        let draft_id = draft_service::draft_id_for_path(path);
        let Some(preloaded) = self.take_preloaded_draft(&draft_id) else {
            return false;
        };
        match preloaded {
            GuardedPreloadedDraftRestore::Content(draft_content) => {
                let Some(entry) = self
                    .imp()
                    .drafts
                    .manifest
                    .borrow()
                    .find_by_id(&draft_id)
                    .cloned()
                else {
                    return false;
                };
                self.note_draft_restore_started();
                self.apply_draft(
                    &DraftRestoreTicket::capture(editor, entry),
                    draft_content,
                    DraftRestoreTracking::Ordinary,
                );
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftSkip::StaleFile) => {
                Self::show_stale_draft_skipped(editor);
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftSkip::Oversized) => {
                Self::show_oversized_draft_skipped(editor);
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftSkip::LazyAggregateBudget) => {
                let Some(entry) = self
                    .imp()
                    .drafts
                    .manifest
                    .borrow()
                    .find_by_id(&draft_id)
                    .cloned()
                else {
                    return false;
                };
                self.queue_lazy_draft_restore(editor, entry);
            }
        }
        true
    }

    /// Apply one worker result only while its complete editor and manifest ticket is current.
    pub(super) fn finish_draft_restore(
        &self,
        ticket: &DraftRestoreTicket,
        result: Result<GuardedDraftRestoreResolution>,
        tracking: DraftRestoreTracking,
    ) {
        let Some(editor) = ticket.current_editor(self) else {
            self.finish_draft_restore_tracking(tracking);
            return;
        };
        let draft_id = ticket.entry.draft_id.clone();
        match result {
            Ok(GuardedDraftRestoreResolution::Restore(content)) => {
                self.apply_draft(ticket, content, tracking);
                return;
            }
            Ok(GuardedDraftRestoreResolution::Compact(FileDraftRestoreSkip::Stale)) => {
                Self::show_stale_draft_skipped(&editor);
                self.delete_draft_by_id(&draft_id);
            }
            Ok(GuardedDraftRestoreResolution::Compact(FileDraftRestoreSkip::Oversized)) => {
                Self::show_oversized_draft_skipped(&editor);
            }
            Ok(GuardedDraftRestoreResolution::Compact(
                FileDraftRestoreSkip::Unavailable | FileDraftRestoreSkip::MissingDraft,
            )) => {}
            Err(error) => {
                tracing::warn!("Failed to restore draft {draft_id}: {error}");
                editor.emit_inline_notification(InlineActionNotification {
                    style: InlineNotificationStyle::Warning,
                    title: "Draft Restore Failed".to_string(),
                    body: "The preserved recovery draft could not be read. The tab remains usable and the recovery files were kept.".to_string(),
                    primary_button: None,
                    secondary_button: None,
                });
            }
        }
        self.finish_draft_restore_tracking(tracking);
    }

    /// Install restored draft content without publishing partial recovery state.
    pub(super) fn apply_draft(
        &self,
        ticket: &DraftRestoreTicket,
        content: crate::ui::plain_disposal::DisposalOwned<String>,
        tracking: DraftRestoreTracking,
    ) {
        let Some(editor) = ticket.current_editor(self) else {
            self.finish_draft_restore_tracking(tracking);
            return;
        };
        let freshness_window = self.downgrade();
        let terminal_window = self.downgrade();
        let freshness_ticket = ticket.clone();
        let terminal_ticket = ticket.clone();
        let accepted_body = Rc::new(RefCell::new(None));
        let accepted_body_for_terminal = Rc::clone(&accepted_body);
        let request = BufferReplacementRequest::new_guarded_returning_body_on_complete(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::DraftRecovery,
                generation: ticket.dirty_generation,
            },
            content,
            move |_| {
                freshness_window
                    .upgrade()
                    .and_then(|window| freshness_ticket.current_editor(&window))
                    .is_some()
            },
            move |outcome| {
                let Some(window) = terminal_window.upgrade() else {
                    return;
                };
                if let BufferReplacementOutcome::Complete {
                    ticket:
                        BufferReplacementTicket {
                            workflow: BufferReplacementWorkflow::DraftRecovery,
                            generation,
                        },
                    ..
                } = outcome
                    && generation == terminal_ticket.dirty_generation
                    && let Some(editor) = terminal_ticket.current_editor(&window)
                    && let Some(body) = accepted_body_for_terminal.borrow_mut().take()
                {
                    Self::finish_applied_draft(&editor, body);
                }
                window.finish_draft_restore_tracking(tracking);
            },
            move |body| {
                accepted_body.borrow_mut().replace(body);
            },
        );
        editor.replace_buffer_bounded(request);
    }

    pub(super) fn finish_applied_draft(
        editor: &LushtextEditorPage,
        content: crate::ui::plain_disposal::DisposalOwned<String>,
    ) {
        let buffer = editor.buffer();
        editor.seed_local_history_from_guarded_restored_draft(content);
        buffer.set_modified(true);
        editor.capture_restored_draft_baseline();
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
    pub(super) fn show_stale_draft_skipped(editor: &LushtextEditorPage) {
        editor.set_draft_restored(false);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Draft Not Restored".to_string(),
            body: "Unsaved changes from a previous session were not restored because the file changed on disk.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Warn that a draft was preserved on disk but skipped because it is too large.
    pub(super) fn show_oversized_draft_skipped(editor: &LushtextEditorPage) {
        editor.set_draft_restored(false);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Draft Not Restored".to_string(),
            body: "Unsaved changes from a previous session were not restored automatically because the draft is very large.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Warn that the current buffer is too large for automatic crash recovery.
    pub(super) fn show_automatic_recovery_limit(editor: &LushtextEditorPage) {
        editor.set_automatic_recovery_limited(true);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Automatic Recovery Paused".to_string(),
            body: "This document is over the 64 MiB automatic recovery limit. Keep editing, or use Save / Save As to protect the latest changes.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Clear the limit warning only after a matching generation is accepted.
    pub(super) fn clear_automatic_recovery_limit(&self, editor: &LushtextEditorPage) {
        if editor.automatic_recovery_limited() {
            editor.set_automatic_recovery_limited(false);
            let warning_is_visible = self
                .imp()
                .notification_bus
                .editor_info_bar_view(editor.notification_owner_id())
                .is_some_and(|notification| notification.title == "Automatic Recovery Paused");
            if warning_is_visible {
                self.resolve_editor_inline_notification(editor);
            }
        }
    }
}
