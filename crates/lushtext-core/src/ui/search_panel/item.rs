// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject adapter for workspace search results.
//!
//! Wraps search result data into a GObject suitable for `gio::ListStore`.
//! Two kinds: "file" items (expandable group headers) and "match" items
//! (individual line matches shown as children). Contains no domain logic
//! — pure data carrier.

use glib::prelude::*;
use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};

mod imp {
    use super::{
        Cell, DerivedObjectProperties, ObjectExt, ObjectImpl, ObjectSubclass, RefCell, glib,
    };

    /// Discriminant for the item type. Avoids string comparison in hot paths.
    const KIND_FILE: u8 = 0;
    const KIND_MATCH: u8 = 1;

    // GTK list models hold row items as shared GObjects, so methods receive
    // &self. Cell handles Copy fields; RefCell handles strings with runtime
    // borrow checks.
    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::SearchResultItem)]
    pub struct SearchResultItem {
        /// Discriminant: `KIND_FILE` (0) or `KIND_MATCH` (1).
        pub kind: Cell<u8>,
        /// Absolute file path (set on both file and match items).
        pub file_path: RefCell<String>,
        /// Path relative to workspace root, shown in the UI (file items only).
        pub display_path: RefCell<String>,
        /// 1-based line number (match items only; 0 for file items).
        pub line_number: Cell<u32>,
        /// Full text of the matching line (match items only; empty for file items).
        pub line_content: RefCell<String>,
        /// Number of matches found in this file (file items only; updated as
        /// results stream in). Registered as a GObject property so
        /// `bind_property` in the factory keeps the badge label in sync.
        #[property(get, set)]
        pub match_count: Cell<u32>,
        /// Byte offset where the match starts within `line_content` (match items only).
        /// Clamped to the truncated display content for highlight rendering.
        pub match_start: Cell<u32>,
        /// Byte offset where the match ends within `line_content` (match items only).
        /// Clamped to the truncated display content for highlight rendering.
        pub match_end: Cell<u32>,
        /// Original full line content before truncation (match items only).
        /// Used by Replace All to generate correct replacements on long lines.
        pub original_line_content: RefCell<String>,
        /// Unclamped match start from the search engine (match items only).
        pub original_match_start: Cell<u32>,
        /// Unclamped match end from the search engine (match items only).
        pub original_match_end: Cell<u32>,
    }

    // ObjectSubclass registers this row type with GLib's runtime type system.
    #[glib::object_subclass]
    impl ObjectSubclass for SearchResultItem {
        const NAME: &'static str = "LushtextSearchResultItem";
        type Type = super::SearchResultItem;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for SearchResultItem {}

    impl SearchResultItem {
        pub const KIND_FILE: u8 = KIND_FILE;
        pub const KIND_MATCH: u8 = KIND_MATCH;
    }
}

glib::wrapper! {
    /// Public GObject row model used by workspace search result lists.
    ///
    /// File rows and match rows share this data-only wrapper so `GtkTreeListModel`
    /// can build expandable result groups without pulling in search-domain logic.
    pub struct SearchResultItem(ObjectSubclass<imp::SearchResultItem>);
}

impl SearchResultItem {
    /// Create a file header item (expandable group).
    #[must_use]
    pub fn new_file(file_path: &str, display_path: &str, match_count: u32) -> Self {
        let obj: Self = glib::Object::builder().build();
        let inner = obj.imp();
        inner.kind.set(imp::SearchResultItem::KIND_FILE);
        inner.file_path.replace(file_path.to_string());
        inner.display_path.replace(display_path.to_string());
        inner.match_count.set(match_count);
        obj
    }

    /// Create a match item (child of a file group).
    ///
    /// `match_start`/`match_end` are clamped to the truncated display content.
    /// `original_line_content` and `original_match_start`/`original_match_end`
    /// store the unclamped values for Replace All correctness on long lines.
    #[expect(
        clippy::too_many_arguments,
        reason = "Search result rows need both display-clamped and original match coordinates to keep Replace All correct on truncated lines"
    )]
    #[must_use]
    pub fn new_match(
        file_path: &str,
        line_number: u32,
        line_content: &str,
        match_start: u32,
        match_end: u32,
        original_line_content: &str,
        original_match_start: u32,
        original_match_end: u32,
    ) -> Self {
        let obj: Self = glib::Object::builder().build();
        let inner = obj.imp();
        inner.kind.set(imp::SearchResultItem::KIND_MATCH);
        inner.file_path.replace(file_path.to_string());
        inner.line_number.set(line_number);
        inner.line_content.replace(line_content.to_string());
        inner.match_start.set(match_start);
        inner.match_end.set(match_end);
        inner
            .original_line_content
            .replace(original_line_content.to_string());
        inner.original_match_start.set(original_match_start);
        inner.original_match_end.set(original_match_end);
        obj
    }

    #[must_use]
    pub fn is_file_item(&self) -> bool {
        self.imp().kind.get() == imp::SearchResultItem::KIND_FILE
    }

    #[must_use]
    pub fn is_match_item(&self) -> bool {
        self.imp().kind.get() == imp::SearchResultItem::KIND_MATCH
    }

    #[must_use]
    pub fn file_path(&self) -> String {
        self.imp().file_path.borrow().clone()
    }

    #[must_use]
    pub fn display_path(&self) -> String {
        self.imp().display_path.borrow().clone()
    }

    #[must_use]
    pub fn line_number(&self) -> u32 {
        self.imp().line_number.get()
    }

    #[must_use]
    pub fn line_content(&self) -> String {
        self.imp().line_content.borrow().clone()
    }

    #[must_use]
    pub fn match_start(&self) -> u32 {
        self.imp().match_start.get()
    }

    #[must_use]
    pub fn match_end(&self) -> u32 {
        self.imp().match_end.get()
    }

    #[must_use]
    pub fn original_line_content(&self) -> String {
        self.imp().original_line_content.borrow().clone()
    }

    #[must_use]
    pub fn original_match_start(&self) -> u32 {
        self.imp().original_match_start.get()
    }

    #[must_use]
    pub fn original_match_end(&self) -> u32 {
        self.imp().original_match_end.get()
    }
}
