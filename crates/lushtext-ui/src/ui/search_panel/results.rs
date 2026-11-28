// SPDX-License-Identifier: GPL-3.0-or-later

//! Result-list rendering and match navigation for the search panel widget.
//!
//! These helpers stay in the UI layer because they manipulate `GtkListView`,
//! `TreeListRow`, and the panel's revealers directly.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-SEARCH-REPLACE` — it projects the workflow onto widgets (result-tree and revealer projection) — so under
//! `gtk-adapter-module-boundaries` it is outside the five-name role taxonomy,
//! takes none of those names, and owns no `policy.rs` and no `evidence.rs`. Its
//! behavior obligations are unchanged. Named in that workflow's matrix row.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use lushtext_model::values::SharedPath;

use super::item::SearchResultItem;
use super::{LushtextSearchPanel, SearchMatchLocation, SearchProgressUpdate};

type ReplaceCallback = Rc<dyn Fn(super::GuardedReplacements)>;
type ReplaceResumedCallback = Rc<dyn Fn(super::replace_admission::ReplaceAdmissionResumption)>;

#[derive(Clone)]
pub(super) enum SearchPanelEvent {
    OpenFile(PathBuf, u32),
    CloseRequested,
    Navigate(PathBuf, u32),
    Progress(SearchProgressUpdate),
    Undo(Arc<super::GuardedReplaceUndoBackup>),
    Message(
        String,
        lushtext_services::coordination::notifications::NotificationSeverity,
    ),
    ReplaceInEditor(PathBuf),
}

#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "LushtextSearchPanelEvent")]
pub(super) struct BoxedSearchPanelEvent(pub(super) SearchPanelEvent);

#[derive(Clone, Copy)]
#[repr(usize)]
pub(super) enum PanelEventSlot {
    OpenFile = 0,
    CloseRequested = 1,
    Navigate = 2,
    Progress = 3,
    Undo = 4,
    Message = 5,
    ReplaceInEditor = 6,
}

/// Typed GObject signal handlers + move-only ownership callbacks wired by the window shell.
#[derive(Default)]
pub struct SearchCallbacks {
    signal_handlers: RefCell<[Option<glib::SignalHandlerId>; 7]>,
    /// Called when "Confirm Replace" is clicked with move-only checked replacements.
    pub replace_callback: RefCell<Option<ReplaceCallback>>,
    /// Called when a Replace All waiting for disposal capacity resumes or ends.
    pub(crate) replace_run_resumed_callback: RefCell<Option<ReplaceResumedCallback>>,
}

impl SearchCallbacks {
    fn has_slot(&self, slot: PanelEventSlot) -> bool {
        self.signal_handlers.borrow()[slot as usize].is_some()
    }

    /// Whether an open-file listener is attached (used by widget wiring tests).
    #[must_use]
    pub fn has_open_file(&self) -> bool {
        self.has_slot(PanelEventSlot::OpenFile)
    }

    /// Whether a navigate listener is attached (used by widget wiring tests).
    #[must_use]
    pub fn has_navigate(&self) -> bool {
        self.has_slot(PanelEventSlot::Navigate)
    }

    /// Whether a search-progress listener is attached (used by widget wiring tests).
    #[must_use]
    pub fn has_progress(&self) -> bool {
        self.has_slot(PanelEventSlot::Progress)
    }

    /// Whether an undo-all listener is attached (used by widget wiring tests).
    #[must_use]
    pub fn has_undo(&self) -> bool {
        self.has_slot(PanelEventSlot::Undo)
    }

    /// Whether a close-requested listener is attached (used by widget wiring tests).
    #[must_use]
    pub fn has_close_requested(&self) -> bool {
        self.has_slot(PanelEventSlot::CloseRequested)
    }

    pub(super) fn dispose(&self, panel: &LushtextSearchPanel) {
        let handlers = self.signal_handlers.take();
        for handler in handlers.into_iter().flatten() {
            panel.disconnect(handler);
        }
        self.replace_callback.take();
        self.replace_run_resumed_callback.take();
    }
}

type PathLineCallback = Box<dyn Fn(&Path, u32) + 'static>;
type MessageCallback = Box<
    dyn Fn(&str, lushtext_services::coordination::notifications::NotificationSeverity) + 'static,
>;

impl LushtextSearchPanel {
    pub(super) fn connect_panel_event_boxed(
        &self,
        slot: PanelEventSlot,
        handler: Box<dyn Fn(&SearchPanelEvent) + 'static>,
    ) {
        let index = slot as usize;
        let prev = self.imp().callbacks.signal_handlers.borrow_mut()[index].take();
        if let Some(prev) = prev {
            self.disconnect(prev);
        }
        let id = crate::ui::signal_connect::connect_void(self, "panel-event", false, move |vals| {
            if let Some(Ok(boxed)) = vals.get(1).map(glib::Value::get::<&BoxedSearchPanelEvent>) {
                handler(&boxed.0);
            }
        });
        self.imp().callbacks.signal_handlers.borrow_mut()[index] = Some(id);
    }

    pub(super) fn emit_panel_event(&self, event: SearchPanelEvent) {
        crate::ui::signal_connect::emit_void(self, "panel-event", &[&BoxedSearchPanelEvent(event)]);
    }

    pub(super) fn emit_close_requested(&self) {
        self.emit_panel_event(SearchPanelEvent::CloseRequested);
    }

    pub(super) fn emit_open_file(&self, path: &Path, line: u32) {
        self.emit_panel_event(SearchPanelEvent::OpenFile(path.to_path_buf(), line));
    }

    pub(super) fn emit_navigate(&self, path: &Path, line: u32) {
        self.emit_panel_event(SearchPanelEvent::Navigate(path.to_path_buf(), line));
    }

    pub(super) fn emit_progress(&self, update: SearchProgressUpdate) {
        self.emit_panel_event(SearchPanelEvent::Progress(update));
    }

    pub(super) fn emit_undo(&self, backup: Arc<super::GuardedReplaceUndoBackup>) {
        self.emit_panel_event(SearchPanelEvent::Undo(backup));
    }

    pub(super) fn emit_message(
        &self,
        text: &str,
        severity: lushtext_services::coordination::notifications::NotificationSeverity,
    ) {
        self.emit_panel_event(SearchPanelEvent::Message(text.to_string(), severity));
    }

    pub(super) fn emit_replace_in_editor(&self, path: &Path) {
        self.emit_panel_event(SearchPanelEvent::ReplaceInEditor(path.to_path_buf()));
    }

    pub(super) fn connect_open_file_boxed(&self, f: PathLineCallback) {
        self.connect_panel_event_boxed(
            PanelEventSlot::OpenFile,
            Box::new(move |ev| {
                if let SearchPanelEvent::OpenFile(path, line) = ev {
                    f(path, *line);
                }
            }),
        );
    }

    pub(super) fn connect_close_requested_boxed(&self, f: Box<dyn Fn() + 'static>) {
        self.connect_panel_event_boxed(
            PanelEventSlot::CloseRequested,
            Box::new(move |ev| {
                if matches!(ev, SearchPanelEvent::CloseRequested) {
                    f();
                }
            }),
        );
    }

    pub(super) fn connect_navigate_to_match_boxed(&self, f: PathLineCallback) {
        self.connect_panel_event_boxed(
            PanelEventSlot::Navigate,
            Box::new(move |ev| {
                if let SearchPanelEvent::Navigate(path, line) = ev {
                    f(path, *line);
                }
            }),
        );
    }

    pub(super) fn connect_search_progress_boxed(
        &self,
        f: Box<dyn Fn(SearchProgressUpdate) + 'static>,
    ) {
        self.connect_panel_event_boxed(
            PanelEventSlot::Progress,
            Box::new(move |ev| {
                if let SearchPanelEvent::Progress(update) = ev {
                    f(*update);
                }
            }),
        );
    }

    pub(super) fn connect_guarded_undo_all_boxed(
        &self,
        f: Box<dyn Fn(Arc<super::GuardedReplaceUndoBackup>) + 'static>,
    ) {
        self.connect_panel_event_boxed(
            PanelEventSlot::Undo,
            Box::new(move |ev| {
                if let SearchPanelEvent::Undo(backup) = ev {
                    f(Arc::clone(backup));
                }
            }),
        );
    }

    pub(super) fn connect_message_boxed(&self, f: MessageCallback) {
        self.connect_panel_event_boxed(
            PanelEventSlot::Message,
            Box::new(move |ev| {
                if let SearchPanelEvent::Message(text, severity) = ev {
                    f(text, *severity);
                }
            }),
        );
    }
}

/// Match-navigation state used by F4 / Shift+F4.
#[derive(Default)]
pub struct SearchNavigationState {
    /// Flat navigation index in match arrival order.
    pub match_positions: RefCell<Vec<SearchMatchLocation>>,
    /// Visible tree rows for each navigation target, captured when rows are appended.
    pub match_rows: RefCell<Vec<Option<gtk4::TreeListRow>>>,
    /// File-header rows keyed by absolute file path for direct child-row lookup.
    pub file_rows: RefCell<BTreeMap<SharedPath, gtk4::TreeListRow>>,
    /// Current position in `match_positions` for wraparound navigation.
    pub current_match_index: Cell<Option<usize>>,
}

/// Compute a display-friendly relative path for a result file.
/// Strips the workspace folder prefix for readability.
pub fn make_display_path<P: AsRef<Path>>(path: &Path, workspace_folders: &[P]) -> String {
    for folder in workspace_folders {
        if let Ok(relative) = path.strip_prefix(folder.as_ref()) {
            return relative.display().to_string();
        }
    }
    path.display().to_string()
}

/// Read the scrolled window's max-content-height limit when one is configured.
fn max_content_height_limit(scroll: &gtk4::ScrolledWindow) -> Option<i32> {
    let height = scroll.max_content_height();
    (height != -1).then_some(height) // ffi-sentinel: GTK returns -1 when max-content-height is unset
}

impl LushtextSearchPanel {
    /// Whether the panel has any search results.
    #[must_use]
    pub fn has_results(&self) -> bool {
        self.imp().runtime.total_matches.get() > 0
    }

    /// Navigate to the next match (F4). Wraps around at the end.
    pub fn navigate_next_match(&self) {
        let imp = self.imp();
        let positions = imp.navigation.match_positions.borrow();
        let len = positions.len();
        if len == 0 {
            return;
        }

        let next = imp
            .navigation
            .current_match_index
            .get()
            .map_or(0, |i| (i + 1) % len);
        imp.navigation.current_match_index.set(Some(next));

        let SearchMatchLocation { path, line_number } = positions[next].clone();
        drop(positions);

        self.select_match_in_results(next);
        self.emit_navigate(&path, line_number);
    }

    /// Navigate to the previous match (Shift+F4). Wraps around at the beginning.
    pub fn navigate_prev_match(&self) {
        let imp = self.imp();
        let positions = imp.navigation.match_positions.borrow();
        let len = positions.len();
        if len == 0 {
            return;
        }

        let prev = imp
            .navigation
            .current_match_index
            .get()
            .map_or(len - 1, |i| if i == 0 { len - 1 } else { i - 1 });
        imp.navigation.current_match_index.set(Some(prev));

        let SearchMatchLocation { path, line_number } = positions[prev].clone();
        drop(positions);

        self.select_match_in_results(prev);
        self.emit_navigate(&path, line_number);
    }

    /// Get the search entry widget (for re-invocation focus/selection).
    #[must_use]
    pub fn search_entry(&self) -> &gtk4::SearchEntry {
        &self.imp().search_entry
    }

    /// Clamp the results scroll area to at most `max_height` pixels.
    ///
    /// The search panel is optional chrome, so it must be allowed to collapse
    /// in very short windows instead of forcing the persistent status bar below
    /// the allocation.
    pub fn clamp_results_height(&self, max_height: i32) {
        let imp = self.imp();
        let clamped = max_height.max(0);

        // GTK validates min <= max at each setter call, so update the bound
        // that is moving outward first and the bound that is moving inward
        // second. Otherwise a resize from 60px to 100px can briefly set
        // min-content-height above the old max-content-height and warn.
        if let Some(current_max) = max_content_height_limit(&imp.results_scroll)
            && clamped > current_max
        {
            imp.results_scroll.set_max_content_height(clamped);
        }
        if imp.results_scroll.min_content_height() != clamped {
            imp.results_scroll.set_min_content_height(clamped);
        }
        if imp.results_scroll.max_content_height() != clamped {
            imp.results_scroll.set_max_content_height(clamped);
        }
        if imp.results_scroll.height_request() != clamped {
            imp.results_scroll.set_height_request(clamped);
        }
    }

    pub(super) fn reveal_results_feedback(&self) {
        let imp = self.imp();
        imp.results_body_revealer.set_reveal_child(false);
        imp.results_feedback_revealer.set_reveal_child(true);
    }

    pub(super) fn reveal_results_body(&self) {
        let imp = self.imp();
        imp.results_feedback_revealer.set_reveal_child(true);
        imp.results_body_revealer.set_reveal_child(true);
    }

    pub(super) fn hide_results_feedback(&self) {
        let imp = self.imp();
        imp.results_body_revealer.set_reveal_child(false);
        imp.results_feedback_revealer.set_reveal_child(false);
    }

    /// Snapshot service-layer match data for preview generation.
    ///
    /// The search worker streams each match into this plain Rust cache as it
    /// arrives, so entering Replace preview does not have to walk GTK tree
    /// models or reconstruct original ranges from row objects.
    pub(super) fn accepted_search_matches(
        &self,
    ) -> Option<std::sync::Arc<Vec<lushtext_model::workspace::content_search::SearchMatch>>> {
        let imp = self.imp();
        let accepted = imp.runtime.accepted_matches_snapshot();
        if accepted.is_some() {
            imp.runtime
                .shared_snapshot_handoffs
                .set(imp.runtime.shared_snapshot_handoffs.get().wrapping_add(1));
        }
        accepted
    }

    /// Trigger a visual refresh of the results list by invalidating the factory.
    pub(super) fn refresh_results_display(&self) {
        super::list_factory::rebind_bound_result_rows(self);
    }

    /// Restore the search summary label and Replace All button description
    /// after leaving or cancelling preview mode.
    pub(super) fn restore_search_summary(&self) {
        let imp = self.imp();
        let total = imp.runtime.total_matches.get();
        let files = imp.runtime.total_files.get();
        if total == 0 {
            imp.count_label.set_text("No results found");
        } else if imp.runtime.is_result_capped() {
            imp.count_label
                .set_text(&super::policy::limited_count_label(
                    total as usize,
                    files as usize,
                    imp.runtime.matches_found.get(),
                ));
            imp.count_label.add_css_class("warning");
        } else {
            imp.count_label.remove_css_class("warning");
            imp.count_label
                .set_text(&super::policy::count_label(total as usize, files as usize));
        }
        // A search that completes while a confirmed run waits for memory keeps
        // the wait's description on the button (design D6).
        let description = if self.replace_admission_deferred() {
            super::accessibility::REPLACE_WAITING_DESCRIPTION
        } else {
            super::accessibility::REPLACE_BUTTON_DESCRIPTION
        };
        crate::ui::accessibility::set_labelled_description(
            &*imp.replace_all_button,
            "Replace all matches",
            description,
        );
    }

    /// Visually select the match row corresponding to `match_positions[match_index]`
    /// in the `SingleSelection` model, and scroll to make it visible.
    fn select_match_in_results(&self, match_index: usize) {
        let imp = self.imp();
        let positions = imp.navigation.match_positions.borrow();
        let Some(target) = positions.get(match_index).cloned() else {
            return;
        };
        let target_path_str = target.path.display().to_string();
        let target_line = target.line_number;
        drop(positions);

        let rows = imp.navigation.match_rows.borrow();
        let Some(Some(row)) = rows.get(match_index).cloned() else {
            return;
        };
        drop(rows);

        let Some(model) = imp.results_list.model() else {
            return;
        };

        if let Some(parent) = row.parent() {
            parent.set_expanded(true);
        }

        let position = row.position();
        if let Some(obj) = model.item(position)
            && let Some(visible_row) = obj.downcast_ref::<gtk4::TreeListRow>()
            && let Some(item) = visible_row.item().and_downcast::<SearchResultItem>()
            && item.is_match_item()
            && item.line_number() == target_line
            && item.file_path() == target_path_str
        {
            if let Some(selection) = model.downcast_ref::<gtk4::SingleSelection>() {
                selection.set_selected(position);
            }
            self.imp()
                .results_list
                .scroll_to(position, gtk4::ListScrollFlags::FOCUS, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_uses_first_covering_workspace_folder() {
        let workspace_folder = PathBuf::from("/repo");
        let nested_folder = PathBuf::from("/repo/src");
        let path = Path::new("/repo/src/main.rs");

        assert_eq!(
            make_display_path(path, &[workspace_folder, nested_folder.clone()]),
            Path::new("src/main.rs").display().to_string()
        );
        assert_eq!(
            make_display_path(path, &[nested_folder]),
            Path::new("main.rs").display().to_string()
        );
    }
}
