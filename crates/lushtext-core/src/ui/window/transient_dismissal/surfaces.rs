// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** — not a role.
//!
//! This module is the workflow's only contact with widgets: it reads the live
//! visibility of the shell's dismissible surfaces, answers whether a window
//! point lies inside the command palette, and applies one dismissal verdict to
//! the surface it names. It owns no ordered stage, no timer, no generation
//! counter, and no coordination job, so it takes **none** of the bounded role
//! names (`admission`, `execution`, `retirement`, `watch`, `journal`) and holds
//! no `policy.rs` or `evidence.rs` of its own. The facade narrates; this module
//! projects.
//!
//! Keeping it separate buys one specific guarantee: the ladder's *order* lives
//! in `policy` and its *effects* live here, in one `match` over
//! `TransientDismissal`. Neither can gain a surface the other does not know
//! about, because the enum is total and the compiler checks it.
//!
//! **Every template child is reached through `try_get()`.** GTK4 clears template
//! children in `dispose()` before Rust's `Drop`, and these functions run from
//! event handlers that can fire while the window is being torn down, so a
//! missing child reads as "not visible" rather than panicking. The convenience
//! accessors are the trap: `LushtextWindow::active_editor` derefs
//! `imp().tab_view`, and `LushtextEditorPage::is_search_visible` derefs the
//! editor's own `search_revealer` — both read as ordinary calls at the call site
//! and both panic on a disposed widget, which is how an observation becomes a
//! crash.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::policy::{TransientDismissal, TransientSurfaceState};

/// Observe every dismissible surface's live visibility.
///
/// Written once and always compiled. The evidence surface calls this rather than
/// repeating it, so a probe and the ladder cannot disagree about what is visible;
/// a second observer would compile in both configurations and drift in one.
pub(super) fn observed_surface_state(window: &LushtextWindow) -> TransientSurfaceState {
    let imp = window.imp();
    TransientSurfaceState {
        command_palette_revealed: imp
            .palette_revealer
            .try_get()
            .is_some_and(|revealer| revealer.reveals_child()),
        editor_search_visible: selected_editor_page(window).is_some_and(|editor| {
            editor
                .imp()
                .search_revealer
                .try_get()
                .is_some_and(|revealer| revealer.reveals_child())
        }),
        search_panel_revealed: imp
            .search_panel_revealer
            .try_get()
            .is_some_and(|revealer| revealer.reveals_child()),
        primary_menu_active: imp
            .primary_menu_button
            .try_get()
            .is_some_and(|button| button.is_active()),
        notes_menu_active: imp
            .notes_menu_button
            .try_get()
            .is_some_and(|button| button.is_active()),
    }
}

/// Whether the palette revealer currently reveals its child.
///
/// The pointer path asks this before paying for a containment probe, so an
/// ordinary click never walks the widget tree.
pub(super) fn command_palette_revealed(window: &LushtextWindow) -> bool {
    window
        .imp()
        .palette_revealer
        .try_get()
        .is_some_and(|revealer| revealer.reveals_child())
}

/// Whether a window-relative point belongs to the palette surface.
///
/// `compute_bounds()` covers the palette allocation in window coordinates, while
/// `pick()` catches descendant widgets GTK may target directly — result rows,
/// controls, scrollbars, and child popup roots — which the bounds rectangle can
/// miss when a popup is drawn outside its parent's allocation.
pub(super) fn command_palette_contains_window_point(
    window: &LushtextWindow,
    x: f64,
    y: f64,
) -> bool {
    let window_widget = window.upcast_ref::<gtk4::Widget>();
    let Some(palette) = window.imp().command_palette.try_get() else {
        return false;
    };
    let palette_widget = palette.upcast_ref::<gtk4::Widget>();
    let Some(bounds) = palette_widget.compute_bounds(window_widget) else {
        return false;
    };
    if bounds.contains_point(&graphene_point_from_window_coordinates(x, y)) {
        return true;
    }

    let Some(picked) = window_widget.pick(x, y, gtk4::PickFlags::DEFAULT) else {
        return false;
    };
    picked.is_ancestor(palette_widget)
}

/// Apply one dismissal verdict to the surface it names.
///
/// The single place a `TransientDismissal` becomes a widget call, so the ladder's
/// order and the ladder's effects cannot drift apart. The palette goes through
/// `close_command_palette()` rather than its revealer, because that path is what
/// clears palette state and restores the focus the palette saved when it opened.
pub(super) fn dismiss(window: &LushtextWindow, surface: TransientDismissal) {
    let imp = window.imp();
    match surface {
        TransientDismissal::CommandPalette => window.close_command_palette(),
        TransientDismissal::EditorSearch => {
            if let Some(editor) = selected_editor_page(window) {
                editor.hide_search();
            }
        }
        TransientDismissal::SearchPanel => window.close_search_panel(),
        TransientDismissal::PrimaryMenu => {
            if let Some(button) = imp.primary_menu_button.try_get() {
                button.set_active(false);
            }
        }
        TransientDismissal::NotesMenu => {
            if let Some(button) = imp.notes_menu_button.try_get() {
                button.set_active(false);
            }
        }
    }
}

/// The selected editor page, reached without the panicking template accessor.
fn selected_editor_page(window: &LushtextWindow) -> Option<LushtextEditorPage> {
    window
        .imp()
        .tab_view
        .try_get()
        .and_then(|tab_view| tab_view.selected_page())
        .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Graphene points use f32 coordinates while GTK pointer events report f64 coordinates."
)]
fn graphene_point_from_window_coordinates(x: f64, y: f64) -> gtk4::graphene::Point {
    gtk4::graphene::Point::new(x as f32, y as f32)
}
