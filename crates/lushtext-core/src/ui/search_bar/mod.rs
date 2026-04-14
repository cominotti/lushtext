// SPDX-License-Identifier: GPL-3.0-or-later

//! Search and replace bar widget.
//!
//! Wraps GtkSourceView's SearchContext/SearchSettings to provide find, replace,
//! match highlighting, and navigation. Attaches to an EditorPage's buffer/view
//! for each search session and detaches on close.

mod imp;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use sourceview5::prelude::*;

glib::wrapper! {
    pub struct LushtextSearchBar(ObjectSubclass<imp::LushtextSearchBar>)
        @extends gtk4::Grid, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSearchBar {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    #[must_use]
    pub fn search_entry(&self) -> &gtk4::SearchEntry {
        &self.imp().search_entry
    }

    #[must_use]
    pub fn replace_entry(&self) -> &gtk4::Entry {
        &self.imp().replace_entry
    }

    #[must_use]
    pub fn close_button(&self) -> &gtk4::Button {
        &self.imp().close_button
    }

    #[must_use]
    pub fn replace_mode_button(&self) -> &gtk4::ToggleButton {
        &self.imp().replace_mode_button
    }

    /// Set the match count display. Blank when total is 0 or negative
    /// (SearchContext returns -1 while scanning). "X of Y" otherwise.
    pub fn set_match_count(&self, current: i32, total: i32) {
        let label = &self.imp().match_label;
        if total <= 0 || current <= 0 {
            label.set_label("");
        } else {
            label.set_label(&format!("{current} of {total}"));
        }
    }

    /// Connect a handler for when the search bar should close
    /// (close button clicked or Escape pressed in the search entry).
    pub fn connect_close<F: Fn() + Clone + 'static>(&self, f: F) {
        // Store for use by keyboard handlers (e.g., Escape in replace entry).
        *self.imp().close_callback.borrow_mut() = Some(Box::new(f.clone()));
        // Wire close button and stop-search signal directly so they work
        // even before attach() is called (important for tests and the
        // initial EditorPage construction).
        let f2 = f.clone();
        self.imp().close_button.connect_clicked(move |_| f2());
        self.imp().search_entry.connect_stop_search(move |_| f());
    }

    /// Whether the user has navigated to a match (next/prev) during this
    /// search session. Controls whether Escape restores the pre-search cursor.
    #[must_use]
    pub fn has_navigated(&self) -> bool {
        self.imp().navigated.get()
    }

    /// Open in replace mode (show replace row and activate toggle).
    pub fn set_replace_mode(&self, active: bool) {
        self.imp().replace_mode_button.set_active(active);
    }

    /// Whether the replace row is revealed (target state, not animation state).
    #[must_use]
    pub fn is_replace_revealed(&self) -> bool {
        self.imp().replace_entry_revealer.reveals_child()
    }

    /// Register a callback fired when the active search state changes.
    ///
    /// This is used by the editor minimap so it can follow query text,
    /// search-option toggles, and attach or detach transitions without
    /// reaching through unrelated widget internals.
    pub fn connect_search_state_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        *self.imp().search_state_changed_callback.borrow_mut() = Some(Box::new(f.clone()));
        let f2 = f.clone();
        self.search_entry().connect_stop_search(move |_| f2());
    }

    /// Return the active `SearchContext`, if the search bar is currently attached.
    #[must_use]
    pub fn search_context(&self) -> Option<sourceview5::SearchContext> {
        self.imp().search_context.borrow().clone()
    }

    // ─── Attach / Detach ──────────────────────────────────────────────

    /// Attach this search bar to a buffer and view, creating a fresh
    /// SearchContext for match highlighting and navigation.
    ///
    /// Button signal handlers and keyboard controllers are wired once in
    /// `constructed()` (imp.rs) and check for an active context, so only
    /// SearchContext creation and the occurrences-count signal live here.
    pub fn attach(&self, buffer: &sourceview5::Buffer, view: &sourceview5::View) {
        // Clean up any previous session.
        self.detach();

        let settings = sourceview5::SearchSettings::builder()
            .wrap_around(true)
            .build();
        let context = sourceview5::SearchContext::new(buffer, Some(&settings));
        context.set_highlight(true);

        // Sync option actions → SearchSettings.
        self.sync_options_to_settings(&settings);

        // If the search entry already has text (retained from a previous session),
        // apply it to the new settings so highlights appear immediately.
        let text = self.search_entry().text();
        if !text.is_empty() {
            settings.set_search_text(Some(text.as_str()));
        }

        // React to occurrence count changes (async scanning completes).
        let bar_weak = self.downgrade();
        let handler_id = context.connect_occurrences_count_notify(move |_ctx| {
            if let Some(bar) = bar_weak.upgrade() {
                bar.update_match_info();
            }
        });

        // Store the view as a weak ref for scroll_mark_onscreen.
        let weak_view = glib::WeakRef::new();
        weak_view.set(Some(view));

        let imp = self.imp();
        imp.search_context.replace(Some(context));
        imp.search_settings.replace(Some(settings));
        imp.view_ref.replace(Some(weak_view));
        imp.occurrences_handler_id.replace(Some(handler_id));
        imp.navigated.set(false);
        self.emit_search_state_changed();
    }

    /// Detach from the current buffer, disabling highlighting and clearing state.
    pub fn detach(&self) {
        let imp = self.imp();

        // Disconnect the occurrences-count handler to break the ref cycle.
        if let (Some(handler_id), Some(context)) = (
            imp.occurrences_handler_id.take(),
            imp.search_context.borrow().as_ref().cloned(),
        ) {
            context.disconnect(handler_id);
            context.set_highlight(false);
        }

        imp.search_context.replace(None);
        imp.search_settings.replace(None);
        imp.view_ref.replace(None);
        imp.navigated.set(false);

        // Clear UI state.
        self.set_match_count(0, 0);
        self.search_entry().remove_css_class("error");
        self.emit_search_state_changed();
    }

    // ─── Navigation ───────────────────────────────────────────────────

    /// Move to the next match in the buffer.
    pub fn move_next(&self) {
        let imp = self.imp();
        let Some(context) = imp.search_context.borrow().clone() else {
            return;
        };
        let buffer = context.buffer();
        // Start searching from one character after the current insert position
        // so we advance past the current match rather than re-finding it.
        let mut iter = buffer.iter_at_mark(&buffer.get_insert());
        iter.forward_char();

        if let Some((match_start, match_end, _wrapped)) = context.forward(&iter) {
            buffer.select_range(&match_start, &match_end);
            self.scroll_to_insert();
            imp.navigated.set(true);
        }
        self.update_match_info();
    }

    /// Move to the previous match in the buffer.
    pub fn move_prev(&self) {
        let imp = self.imp();
        let Some(context) = imp.search_context.borrow().clone() else {
            return;
        };
        let buffer = context.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());

        if let Some((match_start, match_end, _wrapped)) = context.backward(&iter) {
            buffer.select_range(&match_start, &match_end);
            self.scroll_to_insert();
            imp.navigated.set(true);
        }
        self.update_match_info();
    }

    // ─── Replace ──────────────────────────────────────────────────────

    /// Replace the current match and advance to the next one.
    pub fn replace_current(&self) {
        let imp = self.imp();
        let Some(context) = imp.search_context.borrow().clone() else {
            return;
        };
        let buffer = context.buffer();
        let replace_text = imp.replace_entry.text();

        // Get the current selection — it must match a search result.
        let (sel_start, sel_end) = buffer.selection_bounds().unwrap_or_else(|| {
            let iter = buffer.iter_at_mark(&buffer.get_insert());
            (iter, iter)
        });
        let mut match_start = sel_start;
        let mut match_end = sel_end;

        if context
            .replace(&mut match_start, &mut match_end, replace_text.as_str())
            .is_ok()
        {
            // Advance to the next match after replacement.
            self.move_next();
        }
    }

    /// Replace all matches in the buffer.
    pub fn replace_all(&self) {
        let imp = self.imp();
        let Some(context) = imp.search_context.borrow().clone() else {
            return;
        };
        let replace_text = imp.replace_entry.text();
        if let Err(e) = context.replace_all(replace_text.as_str()) {
            tracing::error!("Replace all failed: {e}");
        }
        self.update_match_info();
    }

    // ─── Internal helpers ─────────────────────────────────────────────

    /// Update the match count label and error styling based on current state.
    /// Called from imp.rs signal handlers (search-changed, occurrences-count)
    /// and from navigation methods.
    pub(crate) fn update_match_info(&self) {
        let imp = self.imp();
        let Some(context) = imp.search_context.borrow().clone() else {
            return;
        };
        let total = context.occurrences_count();
        let search_text = self.search_entry().text();

        // Occurrence position for the current selection.
        let current = if total > 0 {
            let buffer = context.buffer();
            let (sel_start, sel_end) = buffer.selection_bounds().unwrap_or_else(|| {
                let iter = buffer.iter_at_mark(&buffer.get_insert());
                (iter, iter)
            });
            let pos = context.occurrence_position(&sel_start, &sel_end);
            pos.max(0)
        } else {
            0
        };

        self.set_match_count(current, total);

        // Error styling: red tint when text is entered but no matches found.
        // total == -1 means scanning is still in progress — don't show error yet.
        let entry = self.search_entry();
        if !search_text.is_empty() && total == 0 {
            entry.add_css_class("error");
        } else {
            entry.remove_css_class("error");
        }
    }

    pub(crate) fn emit_search_state_changed(&self) {
        if let Some(callback) = self.imp().search_state_changed_callback.borrow().as_ref() {
            callback();
        }
    }

    /// Scroll the view so the insert mark (current match) is visible.
    fn scroll_to_insert(&self) {
        let imp = self.imp();
        if let Some(ref weak_view) = *imp.view_ref.borrow()
            && let Some(view) = weak_view.upgrade()
        {
            let buffer = view.buffer();
            view.scroll_mark_onscreen(&buffer.get_insert());
        }
    }

    /// Wire the search-options action group state changes to SearchSettings.
    /// Called during attach() so each session's settings stay in sync.
    fn sync_options_to_settings(&self, settings: &sourceview5::SearchSettings) {
        let Some(group) = self.imp().options_group.borrow().clone() else {
            return;
        };

        // Helper: look up a boolean action and wire its state → SearchSettings.
        let wire = |name: &str, setter: Box<dyn Fn(bool)>| {
            if let Some(action) = group.lookup_action(name) {
                let simple = action
                    .downcast::<gio::SimpleAction>()
                    .expect("option action is SimpleAction");
                // Apply current state immediately.
                let current: bool = simple.state().and_then(|v| v.get()).unwrap_or(false);
                setter(current);
                // React to future toggles.
                simple.connect_notify_local(Some("state"), move |action: &gio::SimpleAction, _| {
                    let on: bool = action.state().and_then(|v| v.get()).unwrap_or(false);
                    setter(on);
                });
            }
        };

        let s = settings.clone();
        wire("regex", Box::new(move |on| s.set_regex_enabled(on)));
        let s = settings.clone();
        wire(
            "case-sensitive",
            Box::new(move |on| s.set_case_sensitive(on)),
        );
        let s = settings.clone();
        wire(
            "whole-word",
            Box::new(move |on| s.set_at_word_boundaries(on)),
        );
    }
}

impl Default for LushtextSearchBar {
    fn default() -> Self {
        Self::new()
    }
}
