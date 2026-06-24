// SPDX-License-Identifier: GPL-3.0-or-later

//! Preferences dialog adapter for GSettings-backed application options.
//!
//! The public dialog owns template wiring and direct settings bindings, while
//! the Data page workflow is split into `data_page.rs` so format-upgrade UI
//! state stays separate from simple preference rows.

mod data_page;
// gtk-rs custom widgets are split into a public wrapper (`mod.rs`) and private
// implementation (`imp.rs`) because GLib stores instance data separately from
// the Rust-facing API.
mod imp;

use glib::Object;

// `glib::wrapper!` generates the reference-counted public GObject wrapper.
// `@extends` declares the GTK class chain, and `@implements` lists interfaces
// the dialog supports when GTK parents or templates treat it generically.
glib::wrapper! {
    /// Public preferences dialog mounted from the application menu.
    ///
    /// The private implementation owns template children and GSettings bindings;
    /// this wrapper exposes construction for window-level actions.
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
