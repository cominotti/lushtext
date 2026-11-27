// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** for `WFR-SEARCH-REPLACE` — not a role.
//!
//! Open and Replace in Editor (`win.replace-in-editor`, stream-replace-all
//! design D7), the window side of a Skipped Files row: resolve the row's file
//! and the run's request through the panel, open the file at its first
//! skipped line, and open the in-tab bar in replace mode prefilled from that
//! request. While the tab is loading (or evicted) the prefill waits in the
//! window's one `pending_editor_replace` slot and resumes from the tab's
//! content republish (`ui/window/documents.rs`). Its decisions are the panel's
//! pure policy (`replace_in_editor_availability`, `editor_replace_prefill`);
//! it owns no ordered stage, timer or generation.

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::StagedSurface;
use crate::ui::window::search::open_file_at_line;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

use super::LushtextWindow;

impl LushtextWindow {
    /// Open and Replace in Editor (`win.replace-in-editor`, stream-replace-all
    /// design D7): open a file the last Replace All skipped at its first
    /// skipped line (or select its tab), open the in-tab bar in replace mode
    /// and prefill it from the run's request. Nothing is replaced: the user
    /// reviews and applies in the bar, and nothing is written until a save.
    pub(super) fn replace_in_editor(&self, path: &std::path::Path) {
        use crate::ui::search_panel::policy::{
            ReplaceInEditorAvailability, editor_replace_prefill, replace_in_editor_availability,
        };
        let Some((file, intent)) = self.imp().search_panel.replace_in_editor_target(path) else {
            self.publish_status_message(
                "Replace in Editor needs a file the last Replace All skipped",
                MessageKind::Warning,
            );
            return;
        };
        if replace_in_editor_availability(file.reason, None)
            == ReplaceInEditorAvailability::NotOpenable
        {
            self.publish_status_message(
                "This skipped file cannot be opened in the editor",
                MessageKind::Warning,
            );
            return;
        }
        let prefill = editor_replace_prefill(&intent);
        let line = file
            .first_line
            .map_or(1, |line| u32::try_from(line).unwrap_or(u32::MAX));
        open_file_at_line(self, path, line);
        let Some(editor) = self
            .active_editor()
            .filter(|editor| editor.file_path().as_deref() == Some(path))
        else {
            return;
        };
        if editor.load_state() == crate::ui::editor_page::EditorLoadState::Loading
            || editor.is_evicted()
        {
            // The bar opens on the loaded document: the tab's content
            // republish resumes here (`resume_pending_editor_replace`).
            self.imp()
                .search_progress
                .pending_editor_replace
                .replace(Some((editor.downgrade(), prefill)));
            return;
        }
        self.imp()
            .search_progress
            .pending_editor_replace
            .replace(None);
        self.show_prefilled_replace(&editor, prefill);
    }

    /// Resume a pending Open and Replace in Editor once `editor` has loaded.
    pub(super) fn resume_pending_editor_replace(&self, editor: &LushtextEditorPage) {
        let pending = self.imp().search_progress.pending_editor_replace.take();
        let Some((target, prefill)) = pending else {
            return;
        };
        if target.upgrade().as_ref() != Some(editor) {
            self.imp()
                .search_progress
                .pending_editor_replace
                .replace(Some((target, prefill)));
            return;
        }
        match editor.load_state() {
            crate::ui::editor_page::EditorLoadState::Loaded
                if self.active_editor().as_ref() == Some(editor) =>
            {
                self.show_prefilled_replace(editor, prefill);
            }
            crate::ui::editor_page::EditorLoadState::Loading => {
                self.imp()
                    .search_progress
                    .pending_editor_replace
                    .replace(Some((target, prefill)));
            }
            // A failed load shows its own alert; a tab the user left keeps
            // its state.
            _ => {}
        }
    }

    /// `win.begin-replace` plus the prefill: closes the workspace panel first,
    /// as Ctrl+H does, then opens and fills the bar. Protected View keeps its
    /// replace row hidden and says why (its existing notice).
    fn show_prefilled_replace(
        &self,
        editor: &LushtextEditorPage,
        prefill: crate::ui::search_bar::policy::EditorReplacePrefill,
    ) {
        let show = move |editor: &LushtextEditorPage| {
            editor.show_replace();
            editor.search_bar().prefill_replace(&prefill);
            if editor.is_protected_view() {
                editor.publish_protected_notice(
                    crate::ui::editor_page::ProtectedNotice::ReplaceUnavailable,
                );
            }
        };
        self.ensure_composed(StagedSurface::Content);
        if self.imp().search_panel_revealer.reveals_child() {
            let editor = editor.downgrade();
            self.close_search_panel();
            self.after_search_panel_transition(move |_| {
                if let Some(editor) = editor.upgrade() {
                    show(&editor);
                }
            });
        } else {
            show(editor);
        }
    }
}
