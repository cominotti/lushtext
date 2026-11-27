// SPDX-License-Identifier: GPL-3.0-or-later

//! Mounting restored pages, and settling which one the user ends up on.
//!
//! Everything here is widget work the admission half must not do: creating a tab
//! for one admitted descriptor, handing its planning terminal to the load
//! workflow, and choosing the selected page at the end without overriding an
//! intent the user expressed while the restore was still running.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;

use super::admission::SessionRestoreRuntime;
use super::policy::{SessionRestoreAdmission, SessionRestorePlanPermit};
use crate::ui::window::LushtextWindow;

impl LushtextWindow {
    /// Mount one restored page for an admitted descriptor.
    ///
    /// Returning a permit means **no background planning was started** — the path
    /// already had a live page, or no page could be created at all — so the
    /// caller must release it after the current bounded turn instead of waiting
    /// for a terminal that will never arrive.
    pub(super) fn mount_restored_page(
        &self,
        generation: u64,
        admission: SessionRestoreAdmission,
    ) -> Option<SessionRestorePlanPermit> {
        let SessionRestoreAdmission {
            ordinal,
            tab,
            permit,
        } = admission;
        let mut inline_release = None;
        self.imp().session.applying_restore_selection.set(true);
        let page = if let Some(path) = tab.path.as_deref() {
            let permit = permit.expect("file-backed restore admission owns a planning permit");
            let window_weak = self.downgrade();
            // `load_file_async_with_planning_terminal` is the load workflow's own
            // entry point for exactly this: it guarantees the terminal fires once
            // on every path, including the ones that park or discard the request.
            let opened = self.open_document_from_session_restore(path, move || {
                if let Some(window) = window_weak.upgrade() {
                    window.release_session_restore_plan_permit(permit);
                }
            });
            match opened {
                Some((page, true)) => Some(page),
                Some((page, false)) => {
                    inline_release = Some(permit);
                    Some(page)
                }
                None => {
                    inline_release = Some(permit);
                    None
                }
            }
        } else {
            debug_assert!(permit.is_none());
            self.new_tab();
            self.imp().tab_view.selected_page()
        };
        self.imp().session.applying_restore_selection.set(false);

        if let Some(page) = page {
            self.restore_tab_pinned_state(&page, tab.pinned);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                editor.set_restore_position(tab.cursor_line, tab.cursor_col, tab.scroll_line);
                if tab.path.is_none()
                    && let Some(draft_id) = tab.draft_id.as_deref()
                {
                    editor.set_draft_id(draft_id.to_string());
                    self.check_draft_by_id(editor, draft_id);
                }
            }
            let mut runtime = self.imp().session.restore_runtime.borrow_mut();
            if let Some(runtime) = runtime.as_mut()
                && runtime.policy.generation() == generation
                && runtime.policy.requested_active_ordinal() == Some(ordinal)
            {
                runtime.requested_page = Some(page.downgrade());
            }
        }

        inline_release
    }
}

/// Select one page without letting restore look like user selection intent.
///
/// The `applying_restore_selection` flag is what keeps the selection generation
/// from advancing, so a restore-owned transient selection cannot be mistaken for
/// the user choosing a tab.
pub(super) fn apply_restore_selection(window: &LushtextWindow, page: &libadwaita::TabPage) {
    window.imp().session.applying_restore_selection.set(true);
    window.imp().tab_view.set_selected_page(page);
    window.imp().session.applying_restore_selection.set(false);
}

/// Choose the final selected page as the generation reaches its terminal.
///
/// The precedence is deliberate and user-first: if the user selected a tab while
/// the restore was running, the selection generation has moved and **nothing
/// here overrides it**. Otherwise a pre-existing selection wins over the
/// persisted one, because the window already had a document the user was
/// looking at.
pub(super) fn settle_restore_selection(
    window: &LushtextWindow,
    runtime: &mut SessionRestoreRuntime,
) {
    let selection_intent_is_current =
        runtime.selection_generation == window.imp().session.selection_generation.get();
    if !selection_intent_is_current {
        return;
    }

    if runtime.preserve_existing_selection {
        if let Some(page) = runtime
            .selected_before
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
        {
            apply_restore_selection(window, &page);
        }
        return;
    }

    let Some(ordinal) = runtime.policy.requested_active_ordinal() else {
        return;
    };
    let requested = runtime
        .requested_page
        .as_ref()
        .and_then(glib::WeakRef::upgrade);
    if let Some(page) = requested {
        apply_restore_selection(window, &page);
    } else if window.imp().tab_view.n_pages() > 0 {
        // The requested ordinal can exceed what actually mounted — a descriptor
        // may have failed to open — so it is clamped rather than trusted.
        #[expect(
            clippy::cast_sign_loss,
            reason = "AdwTabView page counts are non-negative"
        )]
        let ordinal = ordinal.min(window.imp().tab_view.n_pages() as usize - 1);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Persisted tab counts remain well below i32::MAX"
        )]
        let page = window.imp().tab_view.nth_page(ordinal as i32);
        apply_restore_selection(window, &page);
    }
}
