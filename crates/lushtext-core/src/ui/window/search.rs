// SPDX-License-Identifier: GPL-3.0-or-later

//! Search panel wiring: toggle action, keyboard shortcut, pre-fill,
//! result activation, workspace root forwarding, and focus management.
//!
//! Extracted from `window/mod.rs` to stay under the 1000-line file limit.
//! All methods are `impl LushtextWindow` called from `new()` and `constructed()`.

use crate::config::keys;
use crate::ui::editor_page::LushtextEditorPage;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

use super::LushtextWindow;

/// Set up the search panel action, callbacks, and workspace root forwarding.
pub fn setup_search_panel(window: &LushtextWindow) {
    let imp = window.imp();

    // --- Workspace roots: forward current roots and future changes ---
    let initial_roots = imp.sidebar.workspace_roots();
    imp.search_panel.set_workspace_roots(initial_roots);

    let window_weak = window.downgrade();
    imp.sidebar.connect_workspace_changed(move || {
        if let Some(window) = window_weak.upgrade() {
            let roots = window.imp().sidebar.workspace_roots();
            window.imp().search_panel.set_workspace_roots(roots);
            // Also rebuild the command palette file index — the sidebar uses a
            // single-slot callback, so both operations must be in the same closure.
            window.rebuild_file_index();
        }
    });

    // --- Result activation: open file at line ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_open_file(move |path, line| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        window.open_document(path);

        // Scroll to the matching line.
        let tab_view = &window.imp().tab_view;
        if let Some(page) = tab_view.selected_page()
            && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
        {
            if editor.is_evicted() {
                // Evicted tab: buffer was cleared to free memory. Trigger reload
                // and defer scroll — set_restore_position fires after load completes.
                let line_0 = line.saturating_sub(1);
                editor.set_restore_position(line_0, 0, line_0.saturating_sub(3));
                window.reload_if_evicted();
            } else if editor.buffer().char_count() > 0 {
                // File was already open — buffer has content, scroll immediately.
                scroll_editor_to_line(editor, line);
            } else {
                // Newly opened file — async load in progress. Defer scroll via
                // set_restore_position, which is applied after load completes.
                let line_0 = line.saturating_sub(1);
                editor.set_restore_position(line_0, 0, line_0.saturating_sub(3));
            }
        }
    });

    // --- Close request: hide panel + restore focus ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_close_requested(move || {
        if let Some(window) = window_weak.upgrade() {
            window.close_search_panel();
        }
    });

    // --- Restore panel visibility from GSettings ---
    let panel_visible = imp.settings.boolean(keys::SEARCH_PANEL_VISIBLE);
    if panel_visible {
        imp.search_panel_revealer.set_reveal_child(true);
        // Don't grab focus on startup — let session restore handle focus.
    }
}

impl LushtextWindow {
    /// Toggle the search panel visibility. Handles open, re-invocation, and pre-fill.
    pub fn toggle_search_panel(&self) {
        let imp = self.imp();
        let revealer = &imp.search_panel_revealer;

        if revealer.reveals_child() {
            // Re-invocation: refocus and select all text.
            let entry = imp.search_panel.search_entry();
            entry.grab_focus();
            entry.select_region(0, -1);
            return;
        }

        // Save focus before the panel steals it.
        let weak = glib::WeakRef::new();
        if let Some(focused) = gtk4::prelude::GtkWindowExt::focus(self) {
            weak.set(Some(&focused));
        }
        imp.search_saved_focus.replace(Some(weak));

        // Pre-fill: if active editor has selected text, use it.
        if let Some(editor) = self.active_editor() {
            let buffer = editor.buffer();
            if let Some((start, end)) = buffer.selection_bounds() {
                let selected = buffer.text(&start, &end, false);
                if !selected.is_empty() {
                    imp.search_panel.set_query(&selected);
                }
            }
        }

        revealer.set_reveal_child(true);
        imp.search_panel.open();
        let _ = imp.settings.set_boolean(keys::SEARCH_PANEL_VISIBLE, true);
    }

    /// Close the search panel and restore focus.
    /// No-ops if the command palette is open — Escape should close the topmost
    /// overlay first (command palette), not the search panel underneath it.
    pub fn close_search_panel(&self) {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            return; // Command palette is the topmost overlay — let it handle Escape.
        }
        imp.search_panel.close();
        imp.search_panel_revealer.set_reveal_child(false);
        let _ = imp.settings.set_boolean(keys::SEARCH_PANEL_VISIBLE, false);
        self.restore_search_saved_focus();
    }

    /// Restore focus saved before the search panel was opened.
    fn restore_search_saved_focus(&self) {
        let saved = self.imp().search_saved_focus.take();
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
}

/// Scroll an editor to a specific line number (1-based).
/// If the editor is still loading, this is a best-effort attempt — the content
/// may not be available yet, in which case the cursor ends up at line 0.
fn scroll_editor_to_line(editor: &LushtextEditorPage, line: u32) {
    let buffer = editor.buffer();
    let line_0 = line.saturating_sub(1);
    let line_index = i32::try_from(line_0).unwrap_or(i32::MAX);
    let iter = buffer
        .iter_at_line(line_index)
        .unwrap_or_else(|| buffer.end_iter());
    buffer.place_cursor(&iter);
    let mut scroll_iter = iter;
    editor
        .source_view()
        .scroll_to_iter(&mut scroll_iter, 0.0, true, 0.0, 0.3);
}
