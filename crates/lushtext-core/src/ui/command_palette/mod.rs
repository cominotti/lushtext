// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette widget — floating search overlay for files and commands.

mod imp;
pub mod item;

use crate::model::palette::SearchMode;
use crate::services::palette::FileIndex;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::prelude::*;
use item::PaletteItem;

glib::wrapper! {
    pub struct LushtextCommandPalette(ObjectSubclass<imp::LushtextCommandPalette>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextCommandPalette {
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Replace the file index. Called when workspace roots change.
    pub fn set_file_index(&self, index: FileIndex) {
        *self.imp().file_index.borrow_mut() = index;
        // Re-run search if the palette is currently showing results
        let query = self.imp().search_entry.text();
        self.imp().rebuild_results(&query);
    }

    /// Open the palette: focus the search entry and show initial results.
    pub fn open(&self) {
        let imp = self.imp();
        imp.mode.set(SearchMode::All);
        imp.mode_label.set_label(SearchMode::All.label());
        imp.search_entry.set_text("");
        imp.rebuild_results("");
        imp.search_entry.grab_focus();
    }

    /// Close the palette: clear the search entry.
    pub fn close(&self) {
        let imp = self.imp();
        imp.search_entry.set_text("");
        imp.results_store.remove_all();
        imp.no_results_label.set_visible(false);
    }

    /// Register a callback for when an item is activated (Enter or click).
    pub fn connect_item_activated<F: Fn(&PaletteItem) + 'static>(&self, f: F) {
        *self.imp().activate_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback for when the palette should close (Escape).
    pub fn connect_close_requested<F: Fn() + 'static>(&self, f: F) {
        *self.imp().close_callback.borrow_mut() = Some(Box::new(f));
    }

    /// The current search mode.
    pub fn mode(&self) -> SearchMode {
        self.imp().mode.get()
    }
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
