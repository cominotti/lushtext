// SPDX-License-Identifier: GPL-3.0-or-later

//! Search and replace bar widget.

mod imp;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::prelude::*;

glib::wrapper! {
    pub struct LushtextSearchBar(ObjectSubclass<imp::LushtextSearchBar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSearchBar {
    pub fn new() -> Self {
        Object::builder().build()
    }

    pub fn search_entry(&self) -> &gtk4::SearchEntry {
        &self.imp().search_entry
    }

    pub fn replace_entry(&self) -> &gtk4::Entry {
        &self.imp().replace_entry
    }

    pub fn close_button(&self) -> &gtk4::Button {
        &self.imp().close_button
    }

    pub fn set_match_count(&self, current: u32, total: u32) {
        if total == 0 {
            self.imp().match_label.set_label("No results");
        } else {
            self.imp()
                .match_label
                .set_label(&format!("{}/{}", current, total));
        }
    }

    /// Connect a handler for when the search bar should close
    /// (close button clicked or Escape pressed in the search entry).
    pub fn connect_close<F: Fn() + Clone + 'static>(&self, f: F) {
        let f2 = f.clone();
        self.imp().close_button.connect_clicked(move |_| f2());
        self.imp().search_entry.connect_stop_search(move |_| f());
    }
}

impl Default for LushtextSearchBar {
    fn default() -> Self {
        Self::new()
    }
}
