// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fixture shared by A93–A95: a lazy `GListModel` + `GtkSectionModel` that
//! creates one `StringObject` per `get_item` call and counts the calls, shown
//! by a presented `GtkListView` with a `header-factory` over a
//! `GtkSingleSelection` — the shape of LushText's notes browser
//! (`bound-notes-browser-by-budget` D2).

use std::cell::Cell;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

mod imp {
    use super::{Cell, glib};
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct CountingSectionModel {
        pub(super) len: Cell<u32>,
        pub(super) section_len: Cell<u32>,
        pub(super) item_calls: Cell<u64>,
        pub(super) section_calls: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CountingSectionModel {
        const NAME: &'static str = "GtkLushAxiomsCountingSectionModel";
        type Type = super::CountingSectionModel;
        type Interfaces = (gtk4::gio::ListModel, gtk4::SectionModel);
    }

    impl ObjectImpl for CountingSectionModel {}

    impl ListModelImpl for CountingSectionModel {
        fn item_type(&self) -> glib::Type {
            gtk4::StringObject::static_type()
        }

        fn n_items(&self) -> u32 {
            self.len.get()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.item_calls.set(self.item_calls.get() + 1);

            (position < self.len.get())
                .then(|| gtk4::StringObject::new(&format!("row {position}")).upcast())
        }
    }

    impl SectionModelImpl for CountingSectionModel {
        fn section(&self, position: u32) -> (u32, u32) {
            self.section_calls.set(self.section_calls.get() + 1);
            let len = self.len.get();
            if position >= len {
                return (len, u32::MAX);
            }
            let size = self.section_len.get().max(1);
            let start = position / size * size;
            (start, (start + size).min(len))
        }
    }
}

glib::wrapper! {
    /// A lazy, sectioned list model that counts `get_item`/`get_section`.
    pub struct CountingSectionModel(ObjectSubclass<imp::CountingSectionModel>)
        @implements gtk4::gio::ListModel, gtk4::SectionModel;
}

impl CountingSectionModel {
    /// A model of `len` rows in sections of `section_len`.
    #[must_use]
    pub fn new(len: u32, section_len: u32) -> Self {
        let model: Self = glib::Object::new();
        model.imp().len.set(len);
        model.imp().section_len.set(section_len);
        model
    }

    /// Replace every row with `len` new ones in one `items-changed`.
    pub fn publish(&self, len: u32) {
        let old = self.imp().len.replace(len);
        self.items_changed(0, old, len);
    }

    /// Replace every row in two emissions: remove all, then add `len`.
    pub fn publish_two_step(&self, len: u32) {
        let old = self.imp().len.replace(0);
        self.items_changed(0, old, 0);
        self.imp().len.set(len);
        self.items_changed(0, 0, len);
    }

    /// `get_item` and `get_section` calls since the last reset.
    #[must_use]
    pub fn calls(&self) -> (u64, u64) {
        (self.imp().item_calls.get(), self.imp().section_calls.get())
    }

    /// Reset the call counters.
    pub fn reset_calls(&self) {
        self.imp().item_calls.set(0);
        self.imp().section_calls.set(0);
    }
}

/// A list view with a header factory over a single selection over `model`,
/// in a scrolled window.
#[must_use]
pub fn sectioned_list(
    model: &CountingSectionModel,
) -> (gtk4::ScrolledWindow, gtk4::ListView, gtk4::SingleSelection) {
    let selection = gtk4::SingleSelection::new(Some(model.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(false);
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk4::ListItem>() {
            item.set_child(Some(&gtk4::Label::new(None)));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    let headers = gtk4::SignalListItemFactory::new();
    headers.connect_setup(|_, header| {
        if let Some(header) = header.downcast_ref::<gtk4::ListHeader>() {
            let label = gtk4::Label::new(Some("section"));
            label.add_css_class("heading");
            header.set_child(Some(&label));
        }
    });
    let list = gtk4::ListView::new(Some(selection.clone()), Some(factory));
    list.set_header_factory(Some(&headers));
    list.add_css_class("navigation-sidebar");
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    (scroller, list, selection)
}

/// The list's direct children whose CSS name is `name` (`row`, `header`).
#[must_use]
pub fn children_named(list: &gtk4::ListView, name: &str) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        if widget.css_name() == name {
            found.push(widget.clone());
        }
        child = widget.next_sibling();
    }
    found
}
