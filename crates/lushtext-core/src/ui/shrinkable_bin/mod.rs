// SPDX-License-Identifier: GPL-3.0-or-later

//! A single-child widget that can yield all of its height to persistent chrome.
//!
//! GTK containers normally include their child's minimum size in the parent's
//! minimum size. That is correct for most controls, but the main editor surface
//! is the flexible region of the window: at extremely short heights it should
//! clip before the status bar is pushed below the allocation.

mod imp;

use gtk4::glib;

glib::wrapper! {
    pub struct LushtextShrinkableBin(ObjectSubclass<imp::LushtextShrinkableBin>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl LushtextShrinkableBin {
    #[must_use]
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

impl Default for LushtextShrinkableBin {
    fn default() -> Self {
        Self::new()
    }
}
