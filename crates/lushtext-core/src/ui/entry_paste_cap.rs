// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting coordination (no role): a cap on what one insertion can put
//! into a single-line entry (`protected-view-inspector-and-surfaces` review
//! S1b).
//!
//! A single-line `GtkText` lays its whole text out as one Pango line on the
//! GTK thread at every change, so pasting a giant line (a Copy Whole Line of a giant
//! paragraph, into Find) would freeze the window for as long as that layout
//! takes. Every single-line entry LushText builds therefore stops an
//! insertion of more than [`ENTRY_INSERT_CAP_BYTES`] in its `insert-text`
//! emission, before GtkText's default handler stores or lays out anything,
//! inserts the text cut at that many bytes (on a character boundary) instead,
//! and says so: a status-bar warning in the entry's window, announced, or an
//! accessible announcement on the entry when it has no LushText window.
//!
//! Callers name each entry once where it is built: the search bar, the search
//! panel, the command palette, the Open popover, the notes browser, the
//! sidebar's name and rename entries, the Line Inspector's position fields and
//! the excluded-names row in Preferences.

use gtk4::prelude::*;

use crate::ui::accessibility;
use crate::ui::status_bar::MessageKind;

/// The most bytes one insertion may put into a single-line entry: 16 KiB, a
/// power of two (`.agents/rules/rust.md`) far above any search, glob, name or
/// position a person types or pastes. GTK's own entry buffer stops at 65,535
/// characters, so a 64 KiB cap would barely bind; one unwrapped Pango line of
/// 16 Ki characters lays out in a few milliseconds (C5 measured about 7.6 ms
/// for 50,000), so every later keystroke in the field stays within
/// `B_interact`.
pub const ENTRY_INSERT_CAP_BYTES: usize = 16 * 1024;

/// The notice a capped insertion publishes.
pub const ENTRY_INSERT_CAPPED_NOTICE: &str =
    "The pasted text was too long for this field, so only its first 16 KiB was inserted";

/// The longest prefix of `text` of at most `cap` bytes that ends on a
/// character boundary, or `None` when `text` already fits.
#[must_use]
pub fn capped_insertion(text: &str, cap: usize) -> Option<&str> {
    if text.len() <= cap {
        return None;
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(&text[..end])
}

/// Cap insertions into `editable` (an entry, a search entry, a spin button or
/// an entry row; the cap goes on the `GtkText` it delegates to).
pub(crate) fn cap_single_line_inserts(editable: &impl IsA<gtk4::Editable>) {
    let editable = editable.upcast_ref::<gtk4::Editable>();
    let text = editable.delegate().unwrap_or_else(|| editable.clone());
    text.connect_insert_text(|editable, inserted, position| {
        let Some(capped) = capped_insertion(inserted, ENTRY_INSERT_CAP_BYTES) else {
            return;
        };
        editable.stop_signal_emission_by_name("insert-text");
        // The capped text fits, so this nested emission runs normally.
        let capped = capped.to_owned();
        editable.insert_text(&capped, position);
        publish_capped_notice(editable.upcast_ref());
    });
}

fn publish_capped_notice(widget: &gtk4::Widget) {
    if let Some(window) = widget
        .root()
        .and_downcast::<crate::ui::window::LushtextWindow>()
    {
        window.publish_status_message(ENTRY_INSERT_CAPPED_NOTICE, MessageKind::Warning);
    } else {
        accessibility::announce(
            widget,
            ENTRY_INSERT_CAPPED_NOTICE,
            gtk4::AccessibleAnnouncementPriority::High,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insertions_are_cut_on_a_character_boundary_at_the_cap() {
        assert_eq!(capped_insertion("short", 8), None);
        assert_eq!(capped_insertion("12345678", 8), None);
        assert_eq!(capped_insertion("123456789", 8), Some("12345678"));
        // `é` is two bytes: a cut inside it moves back to its start.
        assert_eq!(capped_insertion("1234567é", 8), Some("1234567"));
        assert_eq!(capped_insertion("€€€", 4), Some("€"));
    }
}
