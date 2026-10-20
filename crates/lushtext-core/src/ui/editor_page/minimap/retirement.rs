// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **retirement**.
//!
//! Destroying payloads the workflow is finished with: an in-flight analysis
//! session and its buffer cursor mark, the sole idle continuation source, the
//! accepted content cache, the retained long-line marker identities, and the
//! modified-since-save source marks.
//!
//! Cancellation is the one place the analysis generation advances outside
//! admission, and it must: a cancelled generation can never publish, so every
//! in-flight slice observing the new value stops on its next resumption. The
//! editor *lifetime* advances only in `retire_minimap_analysis`, which is the
//! stronger statement that no slice from this editor may ever run again.
//!
//! `apply_minimap_tracking_suspension` lives here because suspension is what stops
//! a programmatic buffer replacement from being recorded as a user edit; the
//! guard suspends and exactly restores `imp().minimap.tracking_suspended`, and
//! splitting the suspend from the restore across modules is how that exactness
//! would be lost.

use glib::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::projection_execution::MINIMAP_MODIFIED_MARK_CATEGORY;
use crate::config::keys;

impl LushtextEditorPage {
    pub(super) fn cancel_minimap_analysis(&self, clear_cache: bool, release_markers: bool) {
        let imp = self.imp();
        let had_session = imp.minimap.analysis_session.borrow().is_some();
        let source_id = imp.minimap.analysis_source_id.take();
        let had_source = source_id.is_some();
        if had_session || had_source || clear_cache {
            imp.minimap
                .analysis_generation
                .set(imp.minimap.analysis_generation.get().wrapping_add(1));
        }
        if let Some(source_id) = source_id {
            source_id.remove();
        }
        if let Some(session) = imp.minimap.analysis_session.take() {
            session.buffer.delete_mark(&session.cursor_mark);
        }
        if clear_cache {
            imp.minimap.analysis_cache.take();
        } else if release_markers
            && let Some(cache) = imp.minimap.analysis_cache.borrow_mut().as_mut()
        {
            cache.result.long_line_lines.clear();
            cache.markers_collected = false;
        }
        #[cfg(feature = "test-utils")]
        if had_session || had_source {
            imp.minimap
                .analysis_cancellations
                .set(imp.minimap.analysis_cancellations.get().saturating_add(1));
        }
    }

    pub(super) fn discard_minimap_analysis_content(&self) {
        self.cancel_minimap_analysis(true, false);
    }

    pub(super) fn discard_minimap_analysis_request(&self, marker_preference_changed: bool) {
        let release_markers = marker_preference_changed
            && !self
                .imp()
                .settings
                .boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE);
        self.cancel_minimap_analysis(false, release_markers);
    }

    pub(super) fn retire_minimap_analysis(&self) {
        let lifetime = &self.imp().minimap.analysis_lifetime;
        lifetime.set(lifetime.get().wrapping_add(1));
        self.cancel_minimap_analysis(true, false);
    }

    /// Temporarily suspend edit tracking while programmatic buffer mutations run.
    pub(super) fn apply_minimap_tracking_suspension(&self, suspended: bool) {
        if suspended && !self.imp().minimap.tracking_suspended.get() {
            self.discard_minimap_analysis_content();
        }
        self.imp().minimap.tracking_suspended.set(suspended);
    }

    /// Retire the native source map while installation or the scale policy
    /// keeps it away from the buffer.
    ///
    /// `GtkSourceMap` is a second text view, so a hidden but bound map still
    /// lays out every paragraph. Clearing its `view` while its update tick may
    /// be armed makes the tick run against no view (ledger A32: a
    /// `Gtk-CRITICAL` and a `GLib-GObject-CRITICAL`), and LushText cannot read
    /// that tick. So the map is **retired**: the overlay is hidden, the map is
    /// unparented, which unrealizes it and detaches it from the frame clock so
    /// an armed tick never runs (ledger A45), and LushText drops its
    /// reference. `attach_native_source_map` builds a fresh one when the
    /// minimap is shown again.
    pub(super) fn detach_minimap_projection(&self) {
        self.discard_minimap_analysis_content();
        self.retire_native_source_map();
    }

    /// Unparent and drop the current map, if any.
    pub(super) fn retire_native_source_map(&self) {
        let imp = self.imp();
        imp.minimap_overlay.set_visible(false);
        let Some(source_map) = imp.minimap.source_map.borrow_mut().take() else {
            return;
        };
        imp.minimap.map_signals.clear();
        if let Some(render_hold) = imp.minimap.render_hold.borrow().as_ref() {
            render_hold.clear();
        }
        if source_map.parent().is_some() {
            imp.minimap_overlay.set_child(None::<&gtk4::Widget>);
        }
        drop(source_map);
    }

    /// Clear all modified-since-save markers for this editor.
    pub(super) fn release_modified_line_marks(&self) {
        let buffer = self.buffer();
        buffer.remove_source_marks(
            &buffer.start_iter(),
            &buffer.end_iter(),
            Some(MINIMAP_MODIFIED_MARK_CATEGORY),
        );
        self.imp().minimap.modified_marks.borrow_mut().clear();
        self.imp().minimap.modified_lines_cache.borrow_mut().clear();
    }
}
