// SPDX-License-Identifier: GPL-3.0-or-later

//! GNOME Text Editor-style Open popover.
//!
//! The visual structure follows GNOME Text Editor commit
//! `f00b4f5c2f5e03e4833cf14ad58cb04d31480f98`, especially
//! `editor-window.ui`, `editor-open-popover.ui`, `editor-open-popover.c`,
//! `editor-sidebar-row.ui`, and `style.css`: a flat Open menu button owns a
//! custom popover with fixed search/chooser controls and a scrolling recent list.

pub mod item;
// gtk-rs custom widgets are split into a public wrapper (`mod.rs`) and private
// implementation (`imp.rs`), mirroring GObject class/instance storage.
mod imp;

use crate::model::recent_document::RecentDocumentRow;
use glib::Object;
#[cfg(feature = "test-utils")]
use glib::object::{Cast, CastNone, ObjectType};
use glib::subclass::prelude::ObjectSubclassIsExt;
#[cfg(feature = "test-utils")]
use gtk4::prelude::{AdjustmentExt, EditableExt, ListModelExt};
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

    /// Replace visible recent rows after the window applies open-tab exclusion.
    pub fn set_recent_rows(&self, rows: Vec<RecentDocumentRow>) {
        self.imp().set_source_rows(rows);
    }

    /// Reset search/scroll state and focus the search entry before popup.
    pub fn prepare_to_show(&self) {
        self.imp().prepare_to_show();
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

    /// Wire the compact file-chooser button.
    pub fn connect_open_file_requested(&self, callback: impl Fn() + 'static) {
        self.imp()
            .open_file_callback
            .replace(Some(Box::new(callback)));
    }

    /// Wire recent-row activation.
    pub fn connect_recent_activated(&self, callback: impl Fn(PathBuf) + 'static) {
        self.imp()
            .open_recent_callback
            .replace(Some(Box::new(callback)));
    }

    /// Wire row-level removal.
    pub fn connect_remove_requested(&self, callback: impl Fn(PathBuf) + 'static) {
        self.imp()
            .remove_recent_callback
            .replace(Some(Box::new(callback)));
    }

    /// Wire keyboard dismissal focus restoration.
    pub fn connect_dismissed_from_keyboard(&self, callback: impl Fn() + 'static) {
        self.imp()
            .dismiss_callback
            .replace(Some(Box::new(callback)));
    }

    /// Number of visible filtered rows in the list model.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn visible_row_count_for_test(&self) -> u32 {
        self.imp().rows_store.n_items()
    }

    /// Titles in the current filtered row model.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn visible_titles_for_test(&self) -> Vec<String> {
        (0..self.imp().rows_store.n_items())
            .filter_map(|position| {
                self.imp()
                    .rows_store
                    .item(position)
                    .and_downcast::<item::OpenPopoverItem>()
                    .map(|item| item.title())
            })
            .collect()
    }

    /// Programmatically set search text for widget tests.
    #[cfg(feature = "test-utils")]
    pub fn set_search_text_for_test(&self, query: &str) {
        self.imp().search_entry.set_text(query);
    }

    /// Return whether the recent-list scroller is the visible stack child.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_visible_for_test(&self) -> bool {
        self.imp()
            .stack
            .visible_child()
            .as_ref()
            .is_some_and(|child| {
                child.as_ptr()
                    == self
                        .imp()
                        .recent_scroller
                        .upcast_ref::<gtk4::Widget>()
                        .as_ptr()
            })
    }

    /// Return the list viewport height contract used by geometry tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_max_content_height_for_test(&self) -> i32 {
        self.imp().recent_scroller.max_content_height()
    }

    /// Return whether the list scroller can expand horizontally from row content.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_propagates_natural_width_for_test(&self) -> bool {
        self.imp().recent_scroller.propagates_natural_width()
    }

    /// Return the horizontal-scroll policy owned by the recent-list scroller.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_hscrollbar_policy_for_test(&self) -> gtk4::PolicyType {
        self.imp().recent_scroller.hscrollbar_policy()
    }

    /// Move the recent-list adjustment so tests can prove open-time reset behavior.
    #[cfg(feature = "test-utils")]
    pub fn set_list_scroll_value_for_test(&self, value: f64) {
        let adjustment = self.imp().recent_scroller.vadjustment();
        adjustment.configure(value, 0.0, value + 200.0, 1.0, 20.0, 80.0);
    }

    /// Return the current recent-list vertical adjustment value.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_scroll_value_for_test(&self) -> f64 {
        self.imp().recent_scroller.vadjustment().value()
    }

    /// Expose the search entry focus target to widget tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn search_entry_for_test(&self) -> gtk4::SearchEntry {
        self.imp().search_entry.clone()
    }

    /// Expose the file chooser button to widget tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn chooser_button_for_test(&self) -> gtk4::Button {
        self.imp().chooser_button.clone()
    }

    /// Expose the recent list view to widget tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_view_for_test(&self) -> gtk4::ListView {
        self.imp().list_view.clone()
    }
}

impl Default for LushtextOpenPopover {
    fn default() -> Self {
        Self::new()
    }
}
