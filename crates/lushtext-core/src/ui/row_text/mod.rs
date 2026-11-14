// SPDX-License-Identifier: GPL-3.0-or-later

//! `LushtextRowText`: one line of list-row text whose size does not depend on
//! its text (`lighten-notes-browser-rows` D2).
//!
//! **Cross-cutting, no role.** A called presentation surface of two
//! workflows, `WFR-NOTES-BOOKMARKS` (the notes browser's rows and section
//! headers) and `WFR-COMMAND-PALETTE` (the palette's grouped and category
//! rows); it holds no decision logic. Named in both matrix rows.
//!
//! # Why not a `GtkLabel`
//!
//! A `GtkListView` measures every realized row (about 205, ledger A1) for its
//! natural width and again for its height at the allocated width. A
//! `GtkLabel` shapes its text for each measure, so a query that rebinds the
//! realized rows spent 18 ms of its frame in layout (ledger A108). This widget
//! measures without shaping when it can:
//!
//! - **Height.** A line drawn by one font alone is that font's height,
//!   whatever its characters, so it is computed once per primary font and
//!   cached per thread. A text with a character the primary font lacks (a
//!   CJK, Arabic or emoji title draws with a fallback font, which changes the
//!   height) or a line separator is measured exactly, as a `GtkLabel` would
//!   measure it, so every row keeps the height `main` gave it.
//! - **Width.** The minimum is the ellipsis width and, by default, the
//!   natural width equals it, so the container decides the width. Where
//!   siblings share a box's width by their natural widths, `natural-width-from-text`
//!   reports the text's shaped width instead, cached per text and font (the
//!   palette row, whose name and subtitle share one line).
//!
//! A text change queues a redraw only (and a resize when the coverage
//! changes, or in `natural-width-from-text` mode). The Pango layout is created
//! on the first snapshot, so a realized row that is never drawn never lays
//! out its text, unlike `GtkInscription`, whose constructor builds a layout
//! from a style that cannot be shared yet (ledger A109).
//!
//! # Look and accessibility
//!
//! The CSS name is `label`, so Adwaita's label rules apply unchanged
//! (`.title`, `.subtitle`, `.caption`, `.dim-label`, `.heading`, selected and
//! high-contrast row colours). The accessible role is `label` (a notes section
//! header sets `heading` before its accessible context exists), and the
//! accessible label follows the text through `ui::accessibility`. Unlike
//! `GtkLabel`, it does not implement `GtkAccessibleText`; screen readers read
//! the rows by their row names and descriptions, which are unchanged.
//!
//! CSS text attributes (`line-height`, `letter-spacing`, font features)
//! are read once per primary font and class set from an unparented
//! `GtkLabel` probe and applied to every layout: `gtk_widget_create_pango_layout`
//! applies none of them, and Adwaita's `.caption` line height makes a caption
//! line 16 px where its font alone gives 15. The widget tests compare its
//! measure with a `GtkLabel`'s for every class set and script it is used
//! with.

mod imp;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::ui::accessibility;

glib::wrapper! {
    /// One line of list-row text measured without shaping when it can.
    pub struct LushtextRowText(ObjectSubclass<imp::LushtextRowText>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for LushtextRowText {
    fn default() -> Self {
        Self::new()
    }
}

impl LushtextRowText {
    /// An empty, end-ellipsized row text aligned to the start.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The text shown.
    #[must_use]
    pub fn text(&self) -> String {
        self.imp().text.borrow().clone()
    }

    /// Show `text`: a redraw, plus a resize only when the measure can change.
    pub fn set_text(&self, text: &str) {
        let imp = self.imp();
        if *imp.text.borrow() == text {
            return;
        }
        let was_covered = imp.covered.get();
        imp.text.replace(text.to_owned());
        imp.shaped.set(None);
        imp.covered.set(None);
        imp.layout_stale.set(true);
        accessibility::set_label(self, text);
        let measure_follows_text =
            imp.natural_width_from_text.get() || imp.ellipsize_mode() == pango::EllipsizeMode::None;
        // Known coverage of the new text needs the font's metrics, which the
        // first measure resolves; until then a resize is queued anyway.
        let now_covered = imp.metrics.borrow().as_ref().map(|_| imp.is_covered());
        let height_may_change = !matches!((was_covered, now_covered), (Some(true), Some(true)));
        // Empty and non-empty texts have different minimum widths.
        if measure_follows_text || height_may_change || text.is_empty() {
            self.queue_resize();
        } else {
            self.queue_draw();
        }
    }

    /// How a text wider than the allocation is shortened.
    pub fn set_ellipsize(&self, mode: pango::EllipsizeMode) {
        let imp = self.imp();
        if imp.ellipsize_mode() == mode {
            return;
        }
        imp.ellipsize.set(Some(mode));
        imp.shaped.set(None);
        self.queue_resize();
    }

    /// The ellipsize mode.
    #[must_use]
    pub fn ellipsize(&self) -> pango::EllipsizeMode {
        self.imp().ellipsize_mode()
    }

    /// Horizontal alignment of the text inside the allocation, 0 to 1 (start
    /// to end; flipped for right-to-left).
    pub fn set_xalign(&self, xalign: f32) {
        let xalign = xalign.clamp(0.0, 1.0);
        #[expect(
            clippy::float_cmp,
            reason = "an unchanged alignment is the exact value set before"
        )]
        if self.imp().xalign.get() == xalign {
            return;
        }
        self.imp().xalign.set(xalign);
        self.queue_draw();
    }

    /// The horizontal alignment.
    #[must_use]
    pub fn xalign(&self) -> f32 {
        self.imp().xalign.get()
    }

    /// Report the text's shaped width as the natural width (width parity
    /// where siblings share a box by their natural widths).
    pub fn set_natural_width_from_text(&self, from_text: bool) {
        if self.imp().natural_width_from_text.replace(from_text) != from_text {
            self.queue_resize();
        }
    }

    /// Whether the natural width is the text's shaped width.
    #[must_use]
    pub fn natural_width_from_text(&self) -> bool {
        self.imp().natural_width_from_text.get()
    }

    /// Exact measures (text shapings) this widget has taken; evidence for
    /// widget tests that a covered rebind does not shape.
    #[must_use]
    pub fn exact_measures(&self) -> u64 {
        self.imp().exact_measures.get()
    }
}
