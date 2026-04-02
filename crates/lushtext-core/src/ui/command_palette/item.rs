// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject adapter for palette search results.
//!
//! Wraps domain types (`IndexedFile`, `CommandDef`) into a GObject suitable
//! for `gio::ListStore`. Contains no domain logic — pure data carrier.

use crate::model::palette::{CommandDef, IndexedFile};
use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PaletteItem {
        pub display_name: RefCell<String>,
        pub subtitle: RefCell<String>,
        pub action_id: RefCell<String>,
        pub file_path: RefCell<Option<PathBuf>>,
        /// 0 = file, 1 = command
        pub kind: Cell<u8>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PaletteItem {
        const NAME: &'static str = "LushtextPaletteItem";
        type Type = super::PaletteItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for PaletteItem {}
}

glib::wrapper! {
    pub struct PaletteItem(ObjectSubclass<imp::PaletteItem>);
}

const KIND_FILE: u8 = 0;
const KIND_COMMAND: u8 = 1;

impl PaletteItem {
    /// Create a palette item from pre-computed display data.
    /// Used by the background search thread which cannot create GObjects.
    pub fn new_raw(
        display_name: String,
        subtitle: String,
        file_path: Option<PathBuf>,
        action_id: String,
        is_file: bool,
    ) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(display_name);
        imp.subtitle.replace(subtitle);
        imp.file_path.replace(file_path);
        imp.action_id.replace(action_id);
        imp.kind.set(if is_file { KIND_FILE } else { KIND_COMMAND });
        obj
    }

    /// Create a palette item from an indexed file.
    pub fn from_indexed_file(file: &IndexedFile) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(file.name.clone());
        imp.subtitle.replace(file.relative_display());
        imp.file_path.replace(Some(file.path.clone()));
        imp.kind.set(KIND_FILE);
        obj
    }

    /// Create a palette item from a command definition.
    pub fn from_command_def(cmd: &CommandDef) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(cmd.label.to_string());
        imp.subtitle.replace(cmd.display_subtitle());
        imp.action_id.replace(cmd.id.to_string());
        imp.kind.set(KIND_COMMAND);
        obj
    }

    pub fn display_name(&self) -> String {
        self.imp().display_name.borrow().clone()
    }

    pub fn subtitle(&self) -> String {
        self.imp().subtitle.borrow().clone()
    }

    pub fn action_id(&self) -> String {
        self.imp().action_id.borrow().clone()
    }

    pub fn file_path(&self) -> Option<PathBuf> {
        self.imp().file_path.borrow().clone()
    }

    pub fn is_file(&self) -> bool {
        self.imp().kind.get() == KIND_FILE
    }

    pub fn is_command(&self) -> bool {
        self.imp().kind.get() == KIND_COMMAND
    }
}
