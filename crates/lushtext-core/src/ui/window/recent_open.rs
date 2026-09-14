// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** for `WFR-RECENT-DOCUMENTS` — not a role.
//!
//! The workflow's canonical role home is `ui/open_popover/`, which holds its
//! facade, its `policy.rs`, and its `evidence.rs`. This module is the window
//! side of the nested home, and it carries **no** role name deliberately: it
//! projects the workflow onto widgets and resolves window-side targets. It owns
//! no ordered stage of its own, no admission budget, no generation counter, and
//! no durable record — the record belongs to `recent_documents_journal.rs`.
//!
//! What it does is wire the popover's four callbacks to the window workflows
//! that answer them, keep the popover-dependent action's enabled state honest,
//! and rebuild the visible row projection. The rebuild is skipped while nothing
//! can see it, which `policy::should_rebuild_rows` decides; the rows are marked
//! dirty instead and the next show pays for them.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{gio, prelude::*};

use crate::services::recent_documents;
use crate::ui::open_popover::policy;

use super::LushtextWindow;

impl LushtextWindow {
    /// Wire Open popover callbacks to the same production workflows as actions.
    pub(super) fn setup_open_popover_callbacks(&self) {
        let window_weak = self.downgrade();
        self.imp().open_popover.connect_show(move |_| {
            if let Some(window) = window_weak.upgrade() {
                // The header MenuButton can show the popover directly, bypassing
                // the window action that normally rebuilds the hidden row model.
                window.rebuild_open_popover_rows();
                window.set_open_popover_actions_enabled(true);
            }
        });

        let window_weak = self.downgrade();
        self.imp().open_popover.connect_closed(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.set_open_popover_actions_enabled(false);
            }
        });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_open_file_requested(move || {
                if let Some(window) = window_weak.upgrade() {
                    window.show_open_file_dialog();
                }
            });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_recent_activated(move |path| {
                if let Some(window) = window_weak.upgrade() {
                    window.open_document(&path);
                    window.focus_selected_editor_after_action();
                }
            });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_remove_requested(move |path| {
                if let Some(window) = window_weak.upgrade() {
                    window.remove_recent_document(&path);
                }
            });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_dismissed_from_keyboard(move || {
                if let Some(window) = window_weak.upgrade() {
                    window.focus_selected_editor_after_action();
                }
            });
    }

    /// Open the recent-document popover and focus its search entry.
    pub(super) fn open_recent_popover(&self) {
        self.rebuild_open_popover_rows();
        self.imp().open_popover.prepare_to_show();
        self.imp().open_menu_button.popup();
    }

    /// Enable actions that require the visible Open popover.
    pub(super) fn set_open_popover_actions_enabled(&self, enabled: bool) {
        if let Some(action) = self.lookup_action("set-open-popover-query")
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            simple.set_enabled(enabled);
        }
    }

    /// Set the visible Open-popover filter text through the popover search entry.
    pub(super) fn set_open_popover_query(&self, query: &str) {
        if !self.imp().open_popover.is_visible() {
            return;
        }
        self.imp().open_popover.set_search_text(query);
    }

    /// Refresh the popover's visible rows after recents or open-tab state changes.
    pub(super) fn refresh_open_popover_rows(&self) {
        if !self.should_rebuild_open_popover_rows() {
            self.imp().recent_documents.rows_dirty.set(true);
            return;
        }
        self.rebuild_open_popover_rows();
    }

    fn should_rebuild_open_popover_rows(&self) -> bool {
        let seeded = {
            #[cfg(feature = "test-utils")]
            {
                self.imp().recent_documents.test_seeded.get()
            }
            #[cfg(not(feature = "test-utils"))]
            {
                false
            }
        };
        policy::should_rebuild_rows(
            self.imp().open_menu_button.is_active(),
            self.imp().open_popover.is_visible(),
        ) || seeded
    }

    pub(super) fn rebuild_open_popover_rows(&self) {
        let entries = self.imp().recent_documents.entries.borrow();
        let open_identities = self.current_open_document_identities();
        let rows = recent_documents::visible_rows_for_open_set(
            &entries,
            &open_identities,
            recent_documents::now_secs(),
        );
        self.imp().open_popover.set_recent_rows(rows);
        self.imp().recent_documents.rows_dirty.set(false);
    }
}
