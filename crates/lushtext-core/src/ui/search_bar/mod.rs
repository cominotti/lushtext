// SPDX-License-Identifier: GPL-3.0-or-later

//! Find and replace inside the active tab.
//!
//! One user-initiated operation with one ordered stage sequence, entered from
//! `Ctrl+F`, `Ctrl+H`, `Ctrl+G`, `Ctrl+Shift+G`, or `Escape`. Every entry point
//! drives the same session: a `sourceview5::SearchContext` is created against
//! the active editor's buffer, lives for as long as the bar is revealed, and is
//! torn down on close. This is the workspace-wide search row's in-tab
//! counterpart and shares nothing with it but the `services/content_search`
//! engine, which neither row owns.
//!
//! ## Stages
//!
//! 1. **Open.** The editor page reveals the bar and calls `attach`, which begins
//!    a session: fresh settings and context, option actions applied, a query
//!    retained from the previous session re-applied so highlights appear at
//!    once, and the scan notification wired.
//! 2. **Query.** Typing updates the search settings. GtkSourceView then scans
//!    **asynchronously**.
//! 3. *(resume)* **Report.** The scan completes and control resumes in
//!    `execution::report_match_state`, driven by the scanner rather than by the
//!    keystroke that started it. This is the workflow's **only inversion**, and
//!    the census recorded the row as "fully synchronous ... no worker completion
//!    seam" — that is wrong. The counter, the invalid-query styling, and the
//!    throttled screen-reader announcement are all projected here.
//! 4. **Navigate.** Next and previous select a match, scroll it on screen, and
//!    latch that the user navigated, then re-report. The latch is what decides
//!    whether closing restores the pre-search cursor.
//! 5. **Replace.** Replace-one replaces the selection and advances; replace-all
//!    hands the whole buffer to the context. Both re-report.
//! 6. **Close.** `detach` ends the session and clears every session-scoped slot,
//!    including the occurrences handler, whose closure holds this widget and
//!    would otherwise leak as a reference cycle.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — counter text, announcement wording, the invalid-query predicate, and the option vocabulary |
//! | `execution` | coordination — owns one search session and its single inversion |
//! | `imp` | **called presentation surface**, not a role: template children, button and key wiring, revealer state, accessible labels |
//!
//! `ui/editor_page/search.rs` is this workflow's other **called presentation
//! surface**: it owns the editor-side reveal, focus, and cursor-restore
//! choreography and calls `attach` / `detach`. It is recorded in the matrix row
//! rather than named as a role here, because it belongs to the editor page's
//! widget tree, not to this directory.
//!
//! ## What a test reads
//!
//! Nothing test-only. This row has **zero** gated declarations and **zero** gate
//! sites — measured, not inherited — so it owns **no evidence surface** and has
//! nothing to consolidate. Its observable state is already production API:
//! `search_context`, `has_navigated`, `is_replace_revealed`, and the template
//! children the presentation surface exposes. Adding a surface here would widen
//! the API for no reader, which is the manufacture-a-role move the convention
//! exists to stop.

// gtk-rs splits each custom widget into a private implementation module for
// fields and trait impls, plus a public wrapper API in this file.
mod execution;
mod imp;
pub mod policy;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

glib::wrapper! {
    /// Public GTK widget wrapper for the editor find/replace bar.
    ///
    /// The private implementation owns template children and signal wiring; this
    /// wrapper exposes the small API used by `EditorPage`.
    pub struct LushtextSearchBar(ObjectSubclass<imp::LushtextSearchBar>)
        @extends gtk4::Grid, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSearchBar {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    // ─── Presentation surface accessors ───────────────────────────────

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

    /// Set the match count display.
    ///
    /// The text is `policy`'s decision and the widget write is `execution`'s; the
    /// facade only names the stage.
    pub fn set_match_count(&self, current: i32, total: i32) {
        execution::project_match_count(self, current, total);
    }

    /// Whether the replace row is revealed (target state, not animation state).
    #[must_use]
    pub fn is_replace_revealed(&self) -> bool {
        self.imp().replace_entry_revealer.reveals_child()
    }

    /// Open in replace mode (show replace row and activate toggle).
    pub fn set_replace_mode(&self, active: bool) {
        self.imp().replace_mode_button.set_active(active);
    }

    // ─── Session state ────────────────────────────────────────────────

    /// Return the active `SearchContext`, if a session is attached.
    #[must_use]
    pub fn search_context(&self) -> Option<sourceview5::SearchContext> {
        self.imp().search_context.borrow().clone()
    }

    /// Whether the user navigated to a match during this session.
    ///
    /// Read by the editor page's close path to decide whether `Escape` restores
    /// the pre-search cursor.
    #[must_use]
    pub fn has_navigated(&self) -> bool {
        self.imp().navigated.get()
    }

    // ─── Stage 1 and stage 6 ──────────────────────────────────────────

    /// Stage 1 — begin a session against `buffer` and `view`.
    pub fn attach(&self, buffer: &sourceview5::Buffer, view: &sourceview5::View) {
        execution::begin_session(self, buffer, view);
    }

    /// Stage 6 — end the session and clear every session-scoped slot.
    pub fn detach(&self) {
        execution::end_session(self);
    }

    // ─── Stages 4 and 5 ──────────────────────────────────────────────

    /// Stage 4 — move to the next match in the buffer.
    pub fn move_next(&self) {
        execution::select_next_match(self);
    }

    /// Stage 4 — move to the previous match in the buffer.
    pub fn move_prev(&self) {
        execution::select_previous_match(self);
    }

    /// Stage 5 — replace the current match and advance to the next one.
    pub fn replace_current(&self) {
        execution::replace_selected_match(self);
    }

    /// Stage 5 — replace all matches in the buffer.
    pub fn replace_all(&self) {
        execution::replace_all_matches(self);
    }

    // ─── Called by the presentation surface ───────────────────────────

    /// Stage 3 — re-project the live scan state.
    ///
    /// Called from `imp`'s search-changed handler and from the scan resumption.
    pub(crate) fn update_match_info(&self) {
        execution::report_match_state(self);
    }

    /// Apply one option toggle to the live session.
    pub(crate) fn apply_option_state(&self, name: &str, enabled: bool) {
        execution::apply_option(self, name, enabled);
    }

    // ─── Callback registration ───────────────────────────────────────

    /// Connect a handler for when the search bar should close
    /// (close button clicked or Escape pressed in the search entry).
    pub fn connect_close<F: Fn() + Clone + 'static>(&self, f: F) {
        // Stored for keyboard handlers such as Escape in the replace entry.
        *self.imp().close_callback.borrow_mut() = Some(Box::new(f.clone()));
        // Wired directly so both work before `attach` is ever called, which
        // matters for tests and for the initial editor-page construction.
        let f2 = f.clone();
        self.imp().close_button.connect_clicked(move |_| f2());
        self.imp().search_entry.connect_stop_search(move |_| f());
    }

    /// Register a callback fired when the active search state changes.
    ///
    /// Used by the editor minimap so it can follow query text, option toggles,
    /// and attach or detach transitions without reaching through unrelated
    /// widget internals.
    pub fn connect_search_state_changed<F: Fn() + Clone + 'static>(&self, f: F) {
        *self.imp().search_state_changed_callback.borrow_mut() = Some(Box::new(f.clone()));
        let f2 = f;
        self.search_entry().connect_stop_search(move |_| f2());
    }

    pub(crate) fn emit_search_state_changed(&self) {
        if let Some(callback) = self.imp().search_state_changed_callback.borrow().as_ref() {
            callback();
        }
    }
}

impl Default for LushtextSearchBar {
    fn default() -> Self {
        Self::new()
    }
}
