// SPDX-License-Identifier: GPL-3.0-or-later

//! Small UI helpers shared by the adoption-lab workflow pages.

use gtk4::prelude::*;

/// Create the vertical page shell shared by each GTK Lush workflow demo.
pub(crate) fn workflow_box(title: &str) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_hexpand(true);
    content.set_vexpand(true);

    let label = gtk4::Label::new(Some(title));
    label.add_css_class("title-2");
    label.set_xalign(0.0);
    content.append(&label);
    content
}

/// Append wrapped explanatory text under a workflow heading.
pub(crate) fn append_body(container: &gtk4::Box, text: &str) {
    let label = gtk4::Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    container.append(&label);
}

/// Append a labeled control plus live status label to a workflow page.
pub(crate) fn append_control_row(
    container: &gtk4::Box,
    title: &str,
    control: &impl IsA<gtk4::Widget>,
    status: &gtk4::Label,
) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.set_hexpand(true);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_width_chars(20);
    title_label.set_xalign(0.0);
    row.append(&title_label);
    row.append(control);
    row.append(status);
    container.append(&row);
}

/// Append a static key/value diagnostic row to a workflow page.
pub(crate) fn append_fact(container: &gtk4::Box, key: &str, value: &str) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let key_label = gtk4::Label::new(Some(key));
    key_label.set_width_chars(20);
    key_label.set_xalign(0.0);
    let value_label = status_label(value);
    row.append(&key_label);
    row.append(&value_label);
    container.append(&row);
}

/// Build the wrapping status label used by interactive rows.
pub(crate) fn status_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_hexpand(true);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

/// Wrap one page in the scroll policy expected by the adoption lab shell.
pub(crate) fn scroll_page(content: &gtk4::Box) -> gtk4::Widget {
    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_width(false)
        .child(content)
        .build()
        .upcast()
}

/// Generate long rows used to exercise viewport and clipping behavior.
pub(crate) fn awkward_rows(prefix: &str, count: usize) -> String {
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        rows.push(format!(
            "{prefix} {index:02} -- alpha beta gamma delta epsilon zeta eta theta iota kappa"
        ));
    }
    rows.join("\n")
}
