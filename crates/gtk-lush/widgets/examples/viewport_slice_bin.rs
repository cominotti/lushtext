// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standalone `ViewportSliceBin` example: a long `GtkListView` inside an outer
//! scroller that still renders every row.
//!
//! `GtkListView` realizes at most `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200, plus two
//! extra) row widgets for one visible range. A list view handed its whole
//! content as viewport, which is what a `GtkScrolledWindow` with
//! `propagate-natural-height` and a never-shown scrollbar does, renders blank
//! space after roughly the two-hundredth row. The slice bin keeps the list's
//! full height visible to the outer scroller while allocating only the visible
//! band, so the 1,000 rows below all render as the page scrolls.

use gtk_lush_widgets::ViewportSliceBin;
use gtk4::prelude::*;

const ROWS: u32 = 1_000;

fn build_list() -> gtk4::ListView {
    let strings: Vec<String> = (0..ROWS).map(|index| format!("row {index:04}")).collect();
    let model = gtk4::StringList::new(&strings.iter().map(String::as_str).collect::<Vec<_>>());
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().expect("list item");
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().expect("list item");
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    gtk4::ListView::new(Some(gtk4::NoSelection::new(Some(model))), Some(factory))
}

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.gtk_lush.ViewportSliceBinExample")
        .build();

    app.connect_activate(|app| {
        let header = gtk4::Label::new(Some("A header above the list scrolls with it"));
        header.add_css_class("title-2");
        header.set_margin_top(12);
        header.set_margin_bottom(12);

        let slice = ViewportSliceBin::with_child(&build_list());

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&slice);

        // The outer scroller owns scrolling; the bin finds it as an ancestor.
        let outer = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&content)
            .build();

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("ViewportSliceBin")
            .default_width(360)
            .default_height(480)
            .child(&outer)
            .build();
        window.present();
    });

    app.run();
}
