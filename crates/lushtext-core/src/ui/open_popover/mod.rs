// SPDX-License-Identifier: GPL-3.0-or-later

//! The recent-documents workflow (`WFR-RECENT-DOCUMENTS`) — narrative facade.
//!
//! One user-initiated operation with two ordered stage orders: *reopen something
//! I had open before*, and *forget something I had open before*. Both start at
//! the header Open menu button, both run through this popover, and both leave
//! through the same four callbacks into the window.
//!
//! # Role home
//!
//! **Nested.** The canonical role home is this directory, holding this facade,
//! the single [`policy`], and the single `evidence` surface. The window side of
//! the workflow is nested in `ui/window/`:
//!
//! | Module | Role |
//! | --- | --- |
//! | `ui/open_popover/mod.rs` (this file) | narrative facade |
//! | `ui/open_popover/policy.rs` | pure policy — empty-state copy, the announced list value, and whether a rebuild is worth doing |
//! | `ui/open_popover/evidence.rs` | evidence surface (`test-utils`-gated; production reads live state directly) |
//! | `ui/open_popover/imp.rs` | **called presentation surface** — template binding, the row factory, filtering, keynav. Carries no role and owns no policy or evidence |
//! | `ui/open_popover/item.rs` | **called presentation surface** — the row model object the factory binds |
//! | `ui/window/recent_documents_journal.rs` | coordination (`journal`) — the durable `recents.json` record: startup recovery with pruning, generation-guarded merge, debounced worker writes |
//! | `ui/window/recent_open.rs` | **called presentation surface** — window-side wiring, action enablement, and the row projection that reaches this popover |
//!
//! # The stages, in order
//!
//! 1. **Recover.** At startup the journal loads `recents.json` on a worker,
//!    prunes entries whose files are gone, and subtracts anything the user
//!    removed while the load was in flight.
//! 2. **Project.** Before the popover can be seen, the window excludes documents
//!    that are already open and hands the remainder in through
//!    [`set_recent_rows`](LushtextOpenPopover::set_recent_rows). While nothing
//!    can see the result the projection is skipped and the rows are marked
//!    dirty instead — `policy::should_rebuild_rows` owns that decision.
//! 3. **Present.** [`prepare_to_show`](LushtextOpenPopover::prepare_to_show)
//!    resets the query and the scroll offset and focuses the search entry, so a
//!    second popup never opens onto the first popup's filter.
//! 4. **Filter.** Typing re-runs the search and swaps the list page for one of
//!    **two** empty states. Which one is a policy decision, not a widget
//!    detail: an empty history and an over-filtered list look alike on screen
//!    and say different things.
//! 5. **Act.** A row activation, a row removal, the chooser button, or a
//!    keyboard dismissal leaves through the four `connect_*` callbacks.
//!
//! # The inversion
//!
//! Stages 1, 2, and 5 do not run here. This widget owns no window, no file
//! opening, and no persistence; the ordered sequence **leaves** this module at
//! stage 5 through a callback and **resumes** at stage 2 when the window calls
//! `set_recent_rows` again — which is how removing a row updates the visible
//! list without this module knowing what a recent document is. The journal's
//! own stage 1 completion resumes at the same point. A reader tracing "why did
//! the list change?" should look at `ui/window/recent_open.rs` for the
//! resumption and `ui/window/recent_documents_journal.rs` for the record.
//!
//! # Absences, recorded as conclusions
//!
//! * **No coordination role at this home.** The workflow's one coordination job
//!   is the journal, and it is window-side by necessity — it owns the
//!   application's data directory, not a widget.
//! * **No seam value object.** The workflow's cross-boundary values are a
//!   `Vec<RecentDocumentRow>` in one direction and a `PathBuf` in the other.
//!   Neither is a bundle, so reifying either would be the parallel-shape
//!   invention the rule forbids.
//! * **No `test_policy.rs`.** The workflow has no test-only timing or limit
//!   override; its one timing constant
//!   (`RECENT_DOCUMENTS_SAVE_DEBOUNCE_MS`) is production policy that no test
//!   overrides.
//!
//! The visual structure follows GNOME Text Editor 50.1 commit
//! `d1bc58f3d4d09f168048a1e079d601076f90225e`, especially
//! `editor-window.ui`, `editor-open-popover.ui`, `editor-open-popover.c`,
//! `editor-sidebar-row.ui`, and `style.css`: a flat Open menu button owns a
//! custom popover with fixed search/chooser controls and a scrolling recent list.

pub mod item;
pub mod policy;
// gtk-rs custom widgets are split into a public wrapper (`mod.rs`) and private
// implementation (`imp.rs`), mirroring GObject class/instance storage.
mod imp;

#[cfg(feature = "test-utils")]
pub mod evidence;

use crate::model::recent_document::RecentDocumentRow;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
#[cfg(feature = "test-utils")]
use gtk4::prelude::{AdjustmentExt, EditableExt};
use gtk4::{gio, glib};
use std::path::PathBuf;

// glib::wrapper! generates the public wrapper for the custom GtkPopover.
glib::wrapper! {
    /// Searchable recent-document popover used by the header Open menu button.
    pub struct LushtextOpenPopover(ObjectSubclass<imp::LushtextOpenPopover>)
        @extends gtk4::Popover, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::ShortcutManager;
}

impl LushtextOpenPopover {
    /// Create an unwired Open popover.
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Stage 2 resumption point: replace visible recent rows after the window
    /// applies open-tab exclusion.
    pub fn set_recent_rows(&self, rows: Vec<RecentDocumentRow>) {
        self.imp().set_source_rows(rows);
    }

    /// Stage 3: reset search/scroll state and focus the search entry before popup.
    pub fn prepare_to_show(&self) {
        self.imp().prepare_to_show();
    }

    /// Stage 4 entry point for automation and window-level actions.
    pub fn set_search_text(&self, query: &str) {
        self.imp().set_search_text(query);
    }

    /// Search entry surface used by automation visual geometry.
    pub(crate) fn search_entry_widget(&self) -> gtk4::SearchEntry {
        self.imp().search_entry.clone()
    }

    /// File chooser button surface used by automation visual geometry.
    pub(crate) fn chooser_button_widget(&self) -> gtk4::Button {
        self.imp().chooser_button.clone()
    }

    /// Recent-list viewport surface used by automation visual geometry.
    pub(crate) fn recent_scroller_widget(&self) -> gtk4::ScrolledWindow {
        self.imp().recent_scroller.clone()
    }

    /// Empty-state surface used by automation visual geometry.
    pub(crate) fn empty_state_widget(&self) -> gtk4::Box {
        self.imp().empty_state.clone()
    }

    /// Stage 5 exit: the compact file-chooser button.
    pub fn connect_open_file_requested(&self, callback: impl Fn() + 'static) {
        self.imp()
            .open_file_callback
            .replace(Some(Box::new(callback)));
    }

    /// Stage 5 exit: recent-row activation.
    pub fn connect_recent_activated(&self, callback: impl Fn(PathBuf) + 'static) {
        self.imp()
            .open_recent_callback
            .replace(Some(Box::new(callback)));
    }

    /// Stage 5 exit: row-level removal.
    pub fn connect_remove_requested(&self, callback: impl Fn(PathBuf) + 'static) {
        self.imp()
            .remove_recent_callback
            .replace(Some(Box::new(callback)));
    }

    /// Stage 5 exit: keyboard dismissal focus restoration.
    pub fn connect_dismissed_from_keyboard(&self, callback: impl Fn() + 'static) {
        self.imp()
            .dismiss_callback
            .replace(Some(Box::new(callback)));
    }

    // --- Actuation seams -----------------------------------------------------
    //
    // These drive the workflow; they do not observe it. Every observation went
    // into `evidence::open_popover_evidence`, and a setter folded into that
    // surface would make reading change what it reports.

    /// Type into the same search entry users type into.
    #[cfg(feature = "test-utils")]
    pub fn set_search_text_for_test(&self, query: &str) {
        self.imp().search_entry.set_text(query);
    }

    /// Move the recent-list adjustment so tests can prove open-time reset behavior.
    #[cfg(feature = "test-utils")]
    pub fn set_list_scroll_value_for_test(&self, value: f64) {
        let adjustment = self.imp().recent_scroller.vadjustment();
        adjustment.configure(value, 0.0, value + 200.0, 1.0, 20.0, 80.0);
    }

    // --- Widget handles ------------------------------------------------------
    //
    // These hand a test the widget itself, for event emission, focus-chain
    // checks, and the AT-SPI metadata audit. A widget handle is not a workflow
    // fact, so these are not evidence and are not duplicated in the surface.

    /// Search entry widget, for focus and event-driving tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn search_entry_for_test(&self) -> gtk4::SearchEntry {
        self.imp().search_entry.clone()
    }

    /// File chooser button widget, for click emission and metadata audits.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn chooser_button_for_test(&self) -> gtk4::Button {
        self.imp().chooser_button.clone()
    }

    /// Recent list view widget, for keynav and metadata audits.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_view_for_test(&self) -> gtk4::ListView {
        self.imp().list_view.clone()
    }

    /// Empty-state widget, for metadata audits.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn empty_state_for_test(&self) -> gtk4::Box {
        self.imp().empty_state.clone()
    }

    /// Recent scroller widget, for metadata audits.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn recent_scroller_for_test(&self) -> gtk4::ScrolledWindow {
        self.imp().recent_scroller.clone()
    }

    /// The row template's geometry, as a **class constant**.
    ///
    /// Deliberately not a field of the evidence surface: it builds a throwaway
    /// row, which would make every read of that surface construct eight GTK
    /// widgets for a value identical across every popover and every read. It is
    /// an associated function rather than a method for the same reason — it
    /// describes the class, not an instance.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn row_layout_snapshot_for_test() -> evidence::OpenPopoverRowLayoutSnapshot {
        imp::LushtextOpenPopover::row_layout_snapshot().into()
    }
}

impl Default for LushtextOpenPopover {
    fn default() -> Self {
        Self::new()
    }
}
