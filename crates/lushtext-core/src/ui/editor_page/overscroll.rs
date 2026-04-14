// SPDX-License-Identifier: GPL-3.0-or-later

//! Dynamic end-of-document overscroll for one editor tab.
//!
//! This workflow stays in the editor-page adapter layer because it reacts to
//! GTK allocation and updates the live `GtkSourceView` margin that
//! `GtkSourceMap` mirrors. Keeping it separate from `imp.rs` makes the "extra
//! tail room near EOF" behavior easier to reason about without mixing it into
//! template wiring or minimap marker logic.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextEditorPage;

/// Smallest bottom margin kept even before the view has a useful visible rect.
///
/// The UI template starts with 6px of breathing room, so overscroll updates
/// should never collapse below that baseline while the page is being realized.
const MIN_EDITOR_BOTTOM_MARGIN: i32 = 6;
/// Fraction of the visible editor height reserved as EOF overscroll tail room.
///
/// GNOME Text Editor uses 75% of the current visible rect, which leaves enough
/// blank space for the last lines and minimap slider to keep traveling near EOF
/// without making the document feel detached from the viewport.
const EOF_OVERSCROLL_FACTOR: f64 = 0.75;

fn overscroll_margin_from_visible_height(visible_height: i32) -> i32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "GTK visible rect heights are non-negative i32 values, and the 75% overscroll target intentionally rounds back into the same pixel domain"
    )]
    let overscroll = (f64::from(visible_height) * EOF_OVERSCROLL_FACTOR).round() as i32;
    overscroll.max(MIN_EDITOR_BOTTOM_MARGIN)
}

impl LushtextEditorPage {
    /// Coalesce repeated size allocations into one idle overscroll refresh.
    pub(crate) fn schedule_dynamic_overscroll_update(&self) {
        let generation = self
            .imp()
            .overscroll
            .update_generation
            .get()
            .wrapping_add(1);
        self.imp().overscroll.update_generation.set(generation);

        let editor_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().overscroll.update_generation.get() != generation {
                return;
            }
            editor.refresh_dynamic_overscroll();
        });
    }

    /// Recompute the editor's bottom margin from the current visible rect.
    ///
    /// This mirrors GNOME Text Editor's approach: keep extra blank tail space
    /// after the last line so both the main editor and the bound source map
    /// retain usable travel near EOF.
    pub(crate) fn refresh_dynamic_overscroll(&self) {
        let source_view = self.source_view();
        if !source_view.is_mapped() {
            return;
        }

        let visible_rect = source_view.visible_rect();
        let desired_margin = overscroll_margin_from_visible_height(visible_rect.height());

        if source_view.bottom_margin() == desired_margin {
            return;
        }

        source_view.set_bottom_margin(desired_margin);
        self.schedule_minimap_refresh();
    }
}
