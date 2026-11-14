// SPDX-License-Identifier: GPL-3.0-or-later

//! `LushtextRowText` against `GtkLabel` (`lighten-notes-browser-rows` D2,
//! task 2.4): the same height for every class and script the notes browser
//! and palette rows use, a resize when a retarget changes the script, and the
//! palette row's name/subtitle split at the widths a `GtkLabel` row gets.

use crate::common::{ensure_gtk_init, flush_after_delay, present_window};
use gtk4::prelude::*;
use lushtext_core::ui::row_text::LushtextRowText;
use std::time::Duration;

/// Latin, CJK, Arabic, Devanagari and emoji titles: the first takes the
/// cached line height, the others need a fallback font and are measured
/// exactly (on the reference host Latin is 19 px, CJK 21, Devanagari 20,
/// Arabic 29 with Adwaita Sans 11).
const TITLES: [&str; 5] = [
    "Release notes for the summer build",
    "会議の議事録とメモ",
    "ملاحظات الاجتماع الأسبوعي",
    "बैठक के नोट्स",
    "Notes 📝 for the trip",
];

/// The CSS class sets the rows use.
const CLASS_SETS: [&[&str]; 4] = [
    &["title"],
    &["subtitle", "caption", "dim-label"],
    &["heading"],
    &["dim-label", "caption"],
];

fn styled_pair(classes: &[&str], text: &str) -> (gtk4::Label, LushtextRowText) {
    let label = gtk4::Label::new(Some(text));
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    let row_text = LushtextRowText::new();
    row_text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    row_text.set_text(text);
    for class in classes {
        label.add_css_class(class);
        row_text.add_css_class(class);
    }
    (label, row_text)
}

/// A window holding one `navigation-sidebar` list-like column of rows, so
/// the rows' CSS matches the browser's.
fn host() -> (gtk4::Window, gtk4::Box) {
    let window = gtk4::Window::new();
    window.set_default_size(800, 600);
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.add_css_class("navigation-sidebar");
    window.set_child(Some(&column));
    (window, column)
}

fn vertical(widget: &impl IsA<gtk4::Widget>) -> (i32, i32, i32) {
    let (minimum, natural, baseline, _) = widget.measure(gtk4::Orientation::Vertical, -1);
    (minimum, natural, baseline)
}

#[test]
fn test_row_text_matches_label_height_for_every_class_and_script() {
    ensure_gtk_init();
    let (window, column) = host();
    let mut pairs = Vec::new();
    for classes in CLASS_SETS {
        for title in TITLES {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let (label, row_text) = styled_pair(classes, title);
            label.set_hexpand(true);
            row_text.set_hexpand(true);
            row.append(&label);
            row.append(&row_text);
            column.append(&row);
            pairs.push((classes, title, label, row_text));
        }
    }
    present_window(&window);
    flush_after_delay(Duration::from_millis(100));
    for (classes, title, label, row_text) in &pairs {
        assert_eq!(
            vertical(row_text),
            vertical(label),
            "{classes:?} {title:?}: measured height and baseline"
        );
        assert_eq!(
            row_text.height(),
            label.height(),
            "{classes:?} {title:?}: allocated height"
        );
    }
    // Only the texts the primary font covers skip shaping.
    let latin = &pairs[0].3;
    assert_eq!(latin.exact_measures(), 0, "a covered title is not shaped");
    window.close();
}

#[test]
fn test_a_retarget_to_another_script_resizes_the_row() {
    ensure_gtk_init();
    let (window, column) = host();
    let (label, row_text) = styled_pair(&["title"], TITLES[0]);
    column.append(&label);
    column.append(&row_text);
    present_window(&window);
    flush_after_delay(Duration::from_millis(100));
    assert_eq!(row_text.height(), label.height());
    for title in [TITLES[2], TITLES[1], TITLES[0], TITLES[4]] {
        label.set_label(title);
        row_text.set_text(title);
        flush_after_delay(Duration::from_millis(60));
        assert_eq!(row_text.height(), label.height(), "{title:?}");
        assert_eq!(vertical(&row_text), vertical(&label), "{title:?}");
    }
    // A covered rebind redraws without shaping its text.
    label.set_label("Another Latin title");
    let shaped = row_text.exact_measures();
    row_text.set_text(TITLES[0]);
    row_text.set_text("Another Latin title");
    flush_after_delay(Duration::from_millis(60));
    assert_eq!(row_text.height(), label.height());
    assert_eq!(
        row_text.exact_measures(),
        shaped,
        "no shaping for a covered text"
    );
    window.close();
}

/// The palette row: name (start, `hexpand`, middle-ellipsized) and subtitle
/// (end, `caption dim-label`, middle-ellipsized), both `natural-width-from-text`.
fn palette_row(
    name: &str,
    subtitle: &str,
    row_text: bool,
) -> (gtk4::Box, gtk4::Widget, gtk4::Widget) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.set_margin_start(8);
    row.set_margin_end(8);
    let (name_widget, subtitle_widget): (gtk4::Widget, gtk4::Widget) = if row_text {
        let name_text = LushtextRowText::new();
        name_text.set_natural_width_from_text(true);
        name_text.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        name_text.set_text(name);
        let subtitle_text = LushtextRowText::new();
        subtitle_text.set_natural_width_from_text(true);
        subtitle_text.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        subtitle_text.set_text(subtitle);
        (name_text.upcast(), subtitle_text.upcast())
    } else {
        let name_label = gtk4::Label::new(Some(name));
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        let subtitle_label = gtk4::Label::new(Some(subtitle));
        subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        (name_label.upcast(), subtitle_label.upcast())
    };
    name_widget.set_halign(gtk4::Align::Start);
    name_widget.set_hexpand(true);
    subtitle_widget.set_halign(gtk4::Align::End);
    subtitle_widget.add_css_class("dim-label");
    subtitle_widget.add_css_class("caption");
    let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    separator.set_hexpand(true);
    separator.set_visible(false);
    row.append(&name_widget);
    row.append(&separator);
    row.append(&subtitle_widget);
    (row, name_widget, subtitle_widget)
}

#[test]
fn test_palette_row_text_splits_the_row_like_labels() {
    ensure_gtk_init();
    let (window, column) = host();
    let cases = [
        ("main.rs", "src/ui/window"),
        (
            "a-very-long-file-name-that-will-not-fit-the-row-at-all.rs",
            "workspace/deeply/nested/directory/structure/for/the/subtitle",
        ),
        (
            "short",
            "workspace/deeply/nested/directory/structure/for/the/subtitle",
        ),
        (
            "a-very-long-file-name-that-will-not-fit-the-row-at-all.rs",
            "src",
        ),
        ("会議の議事録とメモ.md", "ملاحظات/الاجتماع"),
    ];
    let mut rows = Vec::new();
    for (name, subtitle) in cases {
        let labels = palette_row(name, subtitle, false);
        let texts = palette_row(name, subtitle, true);
        column.append(&labels.0);
        column.append(&texts.0);
        rows.push((name, labels, texts));
    }
    present_window(&window);
    flush_after_delay(Duration::from_millis(100));
    for width in [180, 260, 360, 480, 640] {
        for (name, labels, texts) in &rows {
            for row in [&labels.0, &texts.0] {
                let (_, height, _, _) = row.measure(gtk4::Orientation::Vertical, width);
                row.allocate(width, height, -1, None);
            }
            assert_eq!(
                (labels.1.width(), labels.2.width()),
                (texts.1.width(), texts.2.width()),
                "{name:?} at {width} px: name and subtitle widths"
            );
            assert_eq!(
                labels.2.compute_bounds(&labels.0).map(|b| b.x()),
                texts.2.compute_bounds(&texts.0).map(|b| b.x()),
                "{name:?} at {width} px: subtitle origin"
            );
        }
    }
    window.close();
}

#[test]
fn test_a_header_without_ellipsis_measures_its_whole_text() {
    ensure_gtk_init();
    let (window, column) = host();
    let label = gtk4::Label::new(Some("Workspace Files"));
    label.add_css_class("heading");
    let row_text = LushtextRowText::new();
    row_text.set_ellipsize(gtk4::pango::EllipsizeMode::None);
    row_text.set_text("Workspace Files");
    row_text.add_css_class("heading");
    column.append(&label);
    column.append(&row_text);
    present_window(&window);
    flush_after_delay(Duration::from_millis(60));
    let horizontal = |widget: &gtk4::Widget| {
        let (minimum, natural, _, _) = widget.measure(gtk4::Orientation::Horizontal, -1);
        (minimum, natural)
    };
    assert_eq!(
        horizontal(row_text.upcast_ref()),
        horizontal(label.upcast_ref())
    );
    window.close();
}
