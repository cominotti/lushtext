// SPDX-License-Identifier: GPL-3.0-or-later

//! GNOME Text Editor-style Open popover.
//!
//! The visual structure follows GNOME Text Editor 50.1 commit
//! `d1bc58f3d4d09f168048a1e079d601076f90225e`, especially
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

/// Test-facing child layout tuple for the GNOME-shaped recent row.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPopoverRowChildLayoutSnapshot {
    /// GtkGrid column occupied by the child.
    pub column: i32,
    /// GtkGrid row occupied by the child.
    pub row: i32,
    /// Number of GtkGrid columns spanned by the child.
    pub column_span: i32,
    /// Number of GtkGrid rows spanned by the child.
    pub row_span: i32,
}

/// Test-facing source-contract snapshot for the GNOME-shaped recent row.
///
/// This exists only behind `test-utils`; it is not a public styling API. Widget
/// tests use it to keep LushText's row skeleton aligned with the GNOME Text
/// Editor source constants without poking through GTK's recycled list rows.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone)]
pub struct OpenPopoverRowLayoutSnapshot {
    /// Top margin on the row grid.
    pub grid_margin_top: i32,
    /// Bottom margin on the row grid.
    pub grid_margin_bottom: i32,
    /// Start margin on the row grid.
    pub grid_margin_start: i32,
    /// End margin on the row grid.
    pub grid_margin_end: i32,
    /// Vertical spacing between title and subtitle rows.
    pub grid_row_spacing: u32,
    /// Horizontal spacing between marker, text, age, and remove columns.
    pub grid_column_spacing: u32,
    /// Height request; GNOME's row does not force a fixed row height.
    pub grid_height_request: i32,
    /// Whether the leading marker/spacer stack is horizontally homogeneous.
    pub marker_hhomogeneous: bool,
    /// Whether the leading marker/spacer stack is vertically homogeneous.
    pub marker_vhomogeneous: bool,
    /// Grid position for the leading marker/spacer stack.
    pub marker_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Title overflow mode.
    pub title_overflow: gtk4::InscriptionOverflow,
    /// Title x alignment.
    pub title_xalign: f32,
    /// Whether title can take remaining horizontal room.
    pub title_hexpand: bool,
    /// Grid position for the title.
    pub title_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Subtitle overflow mode.
    pub subtitle_overflow: gtk4::InscriptionOverflow,
    /// Subtitle minimum character width.
    pub subtitle_min_chars: u32,
    /// Subtitle natural character width.
    pub subtitle_nat_chars: u32,
    /// Subtitle minimum line count.
    pub subtitle_min_lines: u32,
    /// Subtitle natural line count.
    pub subtitle_nat_lines: u32,
    /// Whether subtitle carries GNOME's caption class.
    pub subtitle_has_caption: bool,
    /// Whether subtitle carries GNOME's dim-label class.
    pub subtitle_has_dim_label: bool,
    /// Grid position for the subtitle.
    pub subtitle_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Whether the optional age inscription is visible before binding row data.
    pub age_visible: bool,
    /// Age horizontal alignment.
    pub age_halign: gtk4::Align,
    /// Age vertical alignment.
    pub age_valign: gtk4::Align,
    /// Whether age carries GNOME's caption class.
    pub age_has_caption: bool,
    /// Whether age carries GNOME's dim-label class.
    pub age_has_dim_label: bool,
    /// Grid position for the age inscription.
    pub age_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Icon used by the remove button.
    pub remove_icon_name: Option<String>,
    /// Tooltip used by the remove button.
    pub remove_tooltip: Option<String>,
    /// Remove button horizontal alignment.
    pub remove_halign: gtk4::Align,
    /// Remove button vertical alignment.
    pub remove_valign: gtk4::Align,
    /// Whether remove carries GNOME's flat class.
    pub remove_has_flat: bool,
    /// Whether remove carries GNOME's circular class.
    pub remove_has_circular: bool,
    /// Grid position for the remove button.
    pub remove_layout: OpenPopoverRowChildLayoutSnapshot,
}

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

    /// Return the empty-state title currently projected by filtering.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn empty_title_for_test(&self) -> String {
        self.imp().empty_title.label().into()
    }

    /// Return the list viewport height contract used by geometry tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_max_content_height_for_test(&self) -> i32 {
        self.imp().recent_scroller.max_content_height()
    }

    /// Return the list viewport minimum content width from GNOME's source contract.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_min_content_width_for_test(&self) -> i32 {
        self.imp().recent_scroller.min_content_width()
    }

    /// Return the list viewport maximum content width from GNOME's source contract.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_max_content_width_for_test(&self) -> i32 {
        self.imp().recent_scroller.max_content_width()
    }

    /// Return whether the list scroller follows GNOME's natural-height propagation.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn list_propagates_natural_height_for_test(&self) -> bool {
        self.imp().recent_scroller.propagates_natural_height()
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

    /// Return whether the recent list model is the GNOME-style no-selection wrapper.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn recent_list_uses_no_selection_for_test(&self) -> bool {
        self.imp().recent_list_uses_no_selection_for_test()
    }

    /// Return the manually tracked row focus position for NoSelection keynav.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn keyboard_row_position_for_test(&self) -> Option<u32> {
        self.imp().keyboard_row_position_for_test()
    }

    /// Build a source-compatible row snapshot through the same helper the factory uses.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn row_layout_snapshot_for_test(&self) -> OpenPopoverRowLayoutSnapshot {
        imp::LushtextOpenPopover::row_layout_snapshot_for_test().into()
    }
}

impl Default for LushtextOpenPopover {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "test-utils")]
impl From<imp::RecentRowChildLayout> for OpenPopoverRowChildLayoutSnapshot {
    fn from(value: imp::RecentRowChildLayout) -> Self {
        Self {
            column: value.column,
            row: value.row,
            column_span: value.column_span,
            row_span: value.row_span,
        }
    }
}

#[cfg(feature = "test-utils")]
impl From<imp::RecentRowLayoutSnapshot> for OpenPopoverRowLayoutSnapshot {
    fn from(value: imp::RecentRowLayoutSnapshot) -> Self {
        Self {
            grid_margin_top: value.grid_margin_top,
            grid_margin_bottom: value.grid_margin_bottom,
            grid_margin_start: value.grid_margin_start,
            grid_margin_end: value.grid_margin_end,
            grid_row_spacing: value.grid_row_spacing,
            grid_column_spacing: value.grid_column_spacing,
            grid_height_request: value.grid_height_request,
            marker_hhomogeneous: value.marker_hhomogeneous,
            marker_vhomogeneous: value.marker_vhomogeneous,
            marker_layout: value.marker_layout.into(),
            title_overflow: value.title_overflow,
            title_xalign: value.title_xalign,
            title_hexpand: value.title_hexpand,
            title_layout: value.title_layout.into(),
            subtitle_overflow: value.subtitle_overflow,
            subtitle_min_chars: value.subtitle_min_chars,
            subtitle_nat_chars: value.subtitle_nat_chars,
            subtitle_min_lines: value.subtitle_min_lines,
            subtitle_nat_lines: value.subtitle_nat_lines,
            subtitle_has_caption: value.subtitle_has_caption,
            subtitle_has_dim_label: value.subtitle_has_dim_label,
            subtitle_layout: value.subtitle_layout.into(),
            age_visible: value.age_visible,
            age_halign: value.age_halign,
            age_valign: value.age_valign,
            age_has_caption: value.age_has_caption,
            age_has_dim_label: value.age_has_dim_label,
            age_layout: value.age_layout.into(),
            remove_icon_name: value.remove_icon_name,
            remove_tooltip: value.remove_tooltip,
            remove_halign: value.remove_halign,
            remove_valign: value.remove_valign,
            remove_has_flat: value.remove_has_flat,
            remove_has_circular: value.remove_has_circular,
            remove_layout: value.remove_layout.into(),
        }
    }
}
