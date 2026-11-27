// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface — **not a role**.
//!
//! Four read-only accessors that project this workflow's widgets for a caller
//! in another workflow: `ui/automation.rs` builds the Automation1 visual surface
//! and needs the minimap shell, the frozen-pixel cover, the native source map,
//! and the marker strip. A module that only projects the workflow onto widgets
//! is outside the five-name coordination taxonomy, so this file takes none of
//! the bounded role names, owns no `policy.rs` and no `evidence.rs`, and is
//! recorded as a called presentation surface here and in the workflow's matrix
//! row.
//!
//! Its whole reason to exist is that the alternative is an `imp()`
//! reach-through: before this migration `ui/automation.rs` read
//! `editor.imp().minimap_overlay`, `editor.imp().minimap.render_hold`,
//! `editor.imp().minimap.source_map`, and `editor.imp().minimap.marker_strip`
//! directly across a workflow boundary, which shapes this row's state into
//! another workflow's signature without appearing in any seam census.
//!
//! Every accessor answers honestly rather than panicking when the widget is
//! gone: `minimap_overlay` is a `TemplateChild` that GTK4 clears in `dispose()`
//! before Rust's `Drop`, and the other three live in `RefCell<Option<..>>`
//! slots that `dispose()` takes.

use glib::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;

impl LushtextEditorPage {
    /// The minimap shell overlay, or `None` once the template child is gone.
    ///
    /// Read by the Automation1 visual surface, which is a different workflow;
    /// a named operation here is what keeps that from being an `imp()`
    /// reach-through into this row's presentation state.
    pub(crate) fn minimap_shell_widget(&self) -> Option<gtk4::Overlay> {
        self.imp().minimap_overlay.try_get()
    }

    /// The frozen-pixel cover widget, or `None` while no render hold exists.
    pub(crate) fn minimap_reflow_freeze_cover(&self) -> Option<gtk4::Picture> {
        self.imp()
            .minimap
            .render_hold
            .borrow()
            .as_ref()
            .map(|hold| hold.cover().clone())
    }

    /// The native `GtkSourceMap`, or `None` before installation or after dispose.
    ///
    /// `pub` under `test-utils` because the external widget harness is a separate
    /// crate and had grown **four** copy-pasted `page.imp().minimap.source_map`
    /// / `marker_strip` helpers to reach it. An ungated `imp()` read from a test
    /// shapes a production signature without appearing in any seam census, so
    /// the accessor is widened for the harness rather than left duplicated.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn minimap_source_map_widget(&self) -> Option<sourceview5::Map> {
        self.imp().minimap.source_map.borrow().as_ref().cloned()
    }

    /// The native `GtkSourceMap`, or `None` before installation or after dispose.
    #[cfg(not(feature = "test-utils"))]
    pub(crate) fn minimap_source_map_widget(&self) -> Option<sourceview5::Map> {
        self.imp().minimap.source_map.borrow().as_ref().cloned()
    }

    /// The semantic marker strip, or `None` before installation or after dispose.
    ///
    /// `pub` under `test-utils` for the same reason as the source-map accessor
    /// above; see its doc comment.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn minimap_marker_strip_widget(&self) -> Option<gtk4::DrawingArea> {
        self.imp().minimap.marker_strip.borrow().as_ref().cloned()
    }

    /// The semantic marker strip, or `None` before installation or after dispose.
    #[cfg(not(feature = "test-utils"))]
    pub(crate) fn minimap_marker_strip_widget(&self) -> Option<gtk4::DrawingArea> {
        self.imp().minimap.marker_strip.borrow().as_ref().cloned()
    }
}
