// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject adapter for palette search results.
//!
//! Wraps palette row data into a GObject suitable for `gio::ListStore`.
//! Contains no domain logic — pure data carrier for the GTK adapter.

use crate::model::palette::{CommandDef, IndexedFile, PaletteNoteTarget};
use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

// The imp module owns private GObject storage; the public wrapper below is the
// ListStore-facing API.
mod imp {
    use super::{Cell, ObjectImpl, ObjectSubclass, PaletteNoteTarget, PathBuf, RefCell, glib};

    // GObject methods take &self; RefCell/Cell provide interior mutability.
    #[derive(Default)]
    pub struct PaletteItem {
        /// Human-readable name shown in the results list.
        pub display_name: RefCell<String>,
        /// Secondary text (relative path for files, category for commands).
        pub subtitle: RefCell<String>,
        /// For commands: the action identifier (e.g., "win.save"). Empty for files.
        pub action_id: RefCell<String>,
        /// For files: the absolute path to open. `None` for commands.
        pub file_path: RefCell<Option<PathBuf>>,
        /// For notes: the activation target. `None` for files and commands.
        pub note_target: RefCell<Option<PaletteNoteTarget>>,
        /// Discriminant: header, file, command, or note.
        pub kind: Cell<u8>,
    }

    // ObjectSubclass registers this struct with GLib's runtime type system.
    #[glib::object_subclass]
    impl ObjectSubclass for PaletteItem {
        const NAME: &'static str = "LushtextPaletteItem";
        type Type = super::PaletteItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for PaletteItem {}
}

// glib::wrapper! generates the public wrapper type. Since PaletteItem is a
// pure data GObject (not a widget), the @extends chain is empty.
glib::wrapper! {
    /// Public GObject row model used by GTK list stores for palette results.
    ///
    /// This is a data-only wrapper; domain search logic stays in `model::palette`.
    pub struct PaletteItem(ObjectSubclass<imp::PaletteItem>);
}

/// Discriminant value for file search results.
const KIND_FILE: u8 = 0;
/// Discriminant value for command search results.
const KIND_COMMAND: u8 = 1;
/// Discriminant value for non-activatable source headers.
const KIND_HEADER: u8 = 2;
/// Discriminant value for note search results.
const KIND_NOTE: u8 = 3;

impl PaletteItem {
    /// Create a non-activatable group header row.
    #[must_use]
    pub fn new_header_raw(label: impl Into<String>) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(label.into());
        imp.kind.set(KIND_HEADER);
        obj
    }

    /// Create a palette item for a file search result.
    #[must_use]
    pub fn new_file_raw(display_name: String, subtitle: String, file_path: PathBuf) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(display_name);
        imp.subtitle.replace(subtitle);
        imp.file_path.replace(Some(file_path));
        imp.kind.set(KIND_FILE);
        obj
    }

    /// Create a palette item for a command search result.
    pub fn new_command_raw(
        display_name: impl Into<String>,
        subtitle: impl Into<String>,
        action_id: impl Into<String>,
    ) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(display_name.into());
        imp.subtitle.replace(subtitle.into());
        imp.action_id.replace(action_id.into());
        imp.kind.set(KIND_COMMAND);
        obj
    }

    /// Create a palette item for a note search result.
    pub fn new_note_raw(
        display_name: impl Into<String>,
        subtitle: impl Into<String>,
        target: PaletteNoteTarget,
    ) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.display_name.replace(display_name.into());
        imp.subtitle.replace(subtitle.into());
        imp.note_target.replace(Some(target));
        imp.kind.set(KIND_NOTE);
        obj
    }

    /// Create a palette item from an indexed file.
    #[must_use]
    pub fn from_indexed_file(file: &IndexedFile) -> Self {
        Self::new_file_raw(
            file.name.clone(),
            file.relative_display(),
            file.path.clone(),
        )
    }

    /// Create a palette item from a command definition.
    #[must_use]
    pub fn from_command_def(cmd: &CommandDef) -> Self {
        Self::new_command_raw(cmd.label, cmd.display_subtitle(), cmd.id)
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.imp().display_name.borrow().clone()
    }

    #[must_use]
    pub fn subtitle(&self) -> String {
        self.imp().subtitle.borrow().clone()
    }

    #[must_use]
    pub fn action_id(&self) -> String {
        self.imp().action_id.borrow().clone()
    }

    #[must_use]
    pub fn file_path(&self) -> Option<PathBuf> {
        self.imp().file_path.borrow().clone()
    }

    #[must_use]
    pub fn note_target(&self) -> Option<PaletteNoteTarget> {
        self.imp().note_target.borrow().clone()
    }

    #[must_use]
    pub fn is_file(&self) -> bool {
        self.imp().kind.get() == KIND_FILE
    }

    #[must_use]
    pub fn is_command(&self) -> bool {
        self.imp().kind.get() == KIND_COMMAND
    }

    #[must_use]
    pub fn is_note(&self) -> bool {
        self.imp().kind.get() == KIND_NOTE
    }

    /// Return whether this row is a presentation-only source header.
    #[must_use]
    pub fn is_header(&self) -> bool {
        self.imp().kind.get() == KIND_HEADER
    }

    /// Return whether this row can activate a file or command.
    #[must_use]
    pub fn is_activatable(&self) -> bool {
        self.is_file() || self.is_command() || self.is_note()
    }
}
