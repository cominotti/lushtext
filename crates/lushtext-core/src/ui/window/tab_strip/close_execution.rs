// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`) for the tab strip's **close stage order**.
//!
//! Stage-order-qualified because `WFR-TAB-STRIP` owns more than one ordered
//! sequence and only this one needs a coordination module: pin and reorder are
//! single-step operations that the facade completes in one turn, while closing
//! runs
//!
//! 1. **select** the eligible targets from the layout policy,
//! 2. **refuse** outright if any target has a save in flight,
//! 3. **ask once** for the whole modified batch rather than once per tab,
//! 4. **authorize** the batch by depositing one consumable token per page,
//! 5. **request** each close, right to left, and
//! 6. **retire** each page's bookkeeping *after* GTK confirms the detach.
//!
//! # The inversion, and the data-safety rule that produced it
//!
//! Stages 5 and 6 are not connected by a call. `AdwTabView::close_page` is
//! **cancellable**: it routes a modified tab through the save-changes dialog,
//! and the user may say no. Control therefore leaves at stage 5 and resumes in
//! [`LushtextWindow::handle_tab_detached`], which GTK calls only once a page has
//! actually detached.
//!
//! Running the teardown before that terminal is the defect slot 7a fixed and
//! this module must not reintroduce: it would leave a live tab whose load was
//! cancelled and whose file monitor was stopped, and a cancelled in-flight load
//! sets `has_incomplete_load_installation`, which makes autosave **skip that
//! tab's draft**. The teardown exists **once**, in the detach terminal;
//! [`LushtextWindow::close_tab_for_path`] requests the close and returns.
//!
//! The authorization token is what keeps stage 3 honest: without it, the close
//! request handler would show its own per-tab dialog for every modified tab the
//! user had just authorized in one batch.

use std::cmp::Reverse;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use std::path::Path;

use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::policy;
use super::surfaces;

impl LushtextWindow {
    /// Stage 6 gate: handle `AdwTabView`'s close request for one tab page.
    ///
    /// Returns `Propagation::Stop` in every arm because this handler always
    /// resolves the request itself, either immediately or from the dialog
    /// callback.
    pub(in crate::ui::window) fn handle_tab_close_request(
        window: Option<&Self>,
        tab_view: &libadwaita::TabView,
        page: &libadwaita::TabPage,
    ) -> glib::Propagation {
        if let Some(window) = window
            && window.consume_preconfirmed_tab_close(page)
        {
            tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        }

        let child = page.child();
        let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
            tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        };
        if !editor.is_modified() {
            tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        }
        let Some(window) = window else {
            tab_view.close_page_finish(page, false);
            return glib::Propagation::Stop;
        };
        let tab_view = tab_view.clone();
        let page = page.clone();
        let page_for_finish = page.clone();
        window.confirm_close_tab(&page, editor, move |confirmed| {
            tab_view.close_page_finish(&page_for_finish, confirmed);
        });
        glib::Propagation::Stop
    }

    /// The close stage order's terminal: clean up all window bookkeeping after
    /// `AdwTabView` has detached a page.
    ///
    /// This is the **only** place a tab's teardown runs. See the module doc.
    pub(in crate::ui::window) fn handle_tab_detached(&self, page: &libadwaita::TabPage) {
        self.forget_tab_page(page);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
            // Bookmarks live in a sidecar the debounce holds only weakly, so a
            // bookmark added inside the quiet window before this tab detaches
            // would be dropped. Flush before the rest of the teardown, while the
            // editor's live projection is still readable.
            self.flush_bookmarks_for_editor(editor);
            if let Some(ref path) = editor.file_path() {
                let mut paths = self.imp().open_paths.borrow_mut();
                paths.remove(path.as_path());
                paths.remove(&crate::ui::window::documents::open_path_key(path));
                if let Some(canonical_path) = editor.canonical_file_path() {
                    paths.remove(&canonical_path);
                }
            }
            self.dismiss_editor_notifications(editor);
            editor.notify_memory_policy_changed();
            self.untrack_editor_memory(editor);
            editor.cancel_load();
            editor.stop_file_monitor();
        }
        if !self.tab_projection_refresh_deferred() {
            self.refresh_tab_model_projections();
        }
        self.save_session_debounced();
    }

    /// Consume one bulk-close authorization token for `page`, if present.
    pub(in crate::ui::window) fn consume_preconfirmed_tab_close(
        &self,
        page: &libadwaita::TabPage,
    ) -> bool {
        self.imp()
            .tab_management
            .preconfirmed_close_pages
            .borrow_mut()
            .remove(&surfaces::tab_page_key(page))
    }

    /// Drop any stored menu or close state that still points at `page`.
    pub(in crate::ui::window) fn forget_tab_page(&self, page: &libadwaita::TabPage) {
        let page_key = surfaces::tab_page_key(page);
        self.imp()
            .tab_management
            .configured_pages
            .borrow_mut()
            .remove(&page_key);
        self.imp()
            .tab_management
            .preconfirmed_close_pages
            .borrow_mut()
            .remove(&page_key);

        let target_matches = self
            .imp()
            .tab_management
            .target_page
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            .is_some_and(|target| surfaces::tab_page_key(&target) == page_key);
        if target_matches {
            surfaces::refresh_tab_context_menu(self, None);
        }
    }

    /// Stages 2–4: refuse, ask once for any modified targets, then close the
    /// authorized batch.
    pub(super) fn confirm_and_close_tab_pages(&self, mut targets: Vec<libadwaita::TabPage>) {
        if targets.is_empty() {
            surfaces::refresh_tab_context_menu(self, None);
            return;
        }

        // Close from right to left so page indices and selection adjustments
        // stay stable while `page_detached` updates shell bookkeeping.
        targets.sort_by_key(|page| Reverse(self.imp().tab_view.page_position(page)));
        if targets.iter().any(surfaces::page_has_saving_editor) {
            self.publish_save_in_progress_warning();
            surfaces::refresh_tab_context_menu(self, None);
            return;
        }
        let modified_targets = surfaces::collect_modified_close_targets(&targets);
        let close_count = targets.len();
        let window = self.clone();
        if modified_targets.is_empty() {
            self.authorize_and_close_tab_pages(&targets, close_count);
            return;
        }

        self.show_save_changes_dialog(&modified_targets, move |confirmed| {
            if confirmed {
                window.authorize_and_close_tab_pages(&targets, close_count);
            } else {
                surfaces::refresh_tab_context_menu(&window, None);
            }
        });
    }

    /// Stages 4–5: mark the batch as already confirmed, then drive the normal
    /// close path so the cancellable request still runs per page.
    fn authorize_and_close_tab_pages(&self, targets: &[libadwaita::TabPage], close_count: usize) {
        {
            let mut preconfirmed = self
                .imp()
                .tab_management
                .preconfirmed_close_pages
                .borrow_mut();
            preconfirmed.extend(targets.iter().map(surfaces::tab_page_key));
        }

        self.begin_tab_projection_refresh_batch();
        for page in targets {
            self.imp().tab_view.close_page(page);
        }
        self.end_tab_projection_refresh_batch();

        self.publish_status_message(
            &policy::bulk_close_message(close_count),
            crate::ui::status_bar::MessageKind::Info,
        );
        surfaces::refresh_tab_context_menu(self, None);
    }

    /// Close any tab whose file path matches `path` or is inside it (for
    /// directories), reached from sidebar delete and directory removal.
    ///
    /// It **requests** the closes and returns. The teardown belongs to the
    /// detach terminal; see the module doc for why running it here would strand
    /// a cancelled tab's draft.
    pub fn close_tab_for_path(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        self.begin_tab_projection_refresh_batch();
        // Closing pages from the end preserves earlier page indexes while
        // directory deletes may remove many matching tabs in one pass.
        for i in (0..tab_view.n_pages()).rev() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let Some(ep) = editor.file_path() else {
                    continue;
                };
                if ep.as_path() == path || ep.starts_with(path) {
                    tab_view.close_page(&page);
                }
            }
        }
        self.end_tab_projection_refresh_batch();
    }
}
