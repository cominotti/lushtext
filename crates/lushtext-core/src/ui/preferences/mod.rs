// SPDX-License-Identifier: GPL-3.0-or-later

//! Preferences dialog.

mod imp;

use glib::Object;

glib::wrapper! {
    pub struct LushtextPreferences(ObjectSubclass<imp::LushtextPreferences>)
        @extends libadwaita::PreferencesDialog, libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl LushtextPreferences {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }
}

impl Default for LushtextPreferences {
    fn default() -> Self {
        Self::new()
    }
}
