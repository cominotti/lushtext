// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject adapter for workspace search results.
//!
//! Wraps search result data into a GObject suitable for `gio::ListStore`.
//! Two kinds: "file" items (expandable group headers) and "match" items
//! (individual line matches shown as children). Contains no domain logic
//! — pure data carrier.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-SEARCH-REPLACE` — it projects the workflow onto widgets (the row model object the list factory binds) — so under
//! `gtk-adapter-module-boundaries` it is outside the five-name role taxonomy,
//! takes none of those names, and owns no `policy.rs` and no `evidence.rs`. Its
//! behavior obligations are unchanged. Named in that workflow's matrix row.

use glib::prelude::*;
use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};

use crate::model::content_search::SearchMatchId;

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
        /// Path relative to a workspace folder, shown in the UI (file items only).
        pub display_path: RefCell<String>,
        /// 1-based line number (match items only; 0 for file items).
        pub line_number: Cell<u32>,
        /// Display text for the matching line (match items only; empty for file items).
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
        /// Dense identity of the plain search-cache entry (match items only).
        pub match_id: Cell<usize>,
    }

    // ObjectSubclass registers this row type with GLib's runtime type system.
    #[glib::object_subclass]
    impl ObjectSubclass for SearchResultItem {
        const NAME: &str = "LushtextSearchResultItem";
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
    /// `match_start`/`match_end` are clamped to the truncated display content;
    /// preview consumers resolve complete source data through `match_id`.
    #[must_use]
    pub fn new_match(
        file_path: &str,
        line_number: u32,
        line_content: &str,
        match_start: u32,
        match_end: u32,
        match_id: SearchMatchId,
    ) -> Self {
        let obj: Self = glib::Object::builder().build();
        let inner = obj.imp();
        inner.kind.set(imp::SearchResultItem::KIND_MATCH);
        inner.file_path.replace(file_path.to_string());
        inner.line_number.set(line_number);
        inner.line_content.replace(line_content.to_string());
        inner.match_start.set(match_start);
        inner.match_end.set(match_end);
        inner.match_id.set(match_id.index());
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
    pub fn match_id(&self) -> SearchMatchId {
        SearchMatchId::from_index(self.imp().match_id.get())
    }
}
