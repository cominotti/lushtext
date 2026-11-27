// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the tab-strip workflow's single observable state.
//!
//! This surface is a **gain from zero**: before it, the workflow had no
//! observation path at all. Its bookkeeping — which page the context menu is
//! aimed at, which pages carry an unconsumed bulk-close authorization, which
//! pages have had their pin-notify handler attached, and how deep the
//! projection-refresh batch is — was reachable only by reaching into
//! `window.imp().tab_management` from a test, which is the ungated shadow
//! introspection the convention exists to remove.
//!
//! * **Reading must not mutate.** Every field is a widget-property read, a set
//!   length, or a pure derivation from the live page order. No field advances a
//!   counter, consumes an authorization token, or refreshes the menu.
//! * **No field may be read from inside a mutable borrow.** Every `Ref` taken
//!   here is dropped before the returned struct literal is built. In particular
//!   the accessor is callable from inside a close callback, which will later
//!   take `preconfirmed_close_pages.borrow_mut()`.
//! * **A disposed widget is a stage.** `tab_view` is a `TemplateChild`; it is
//!   reached through `try_get()` so a teardown observation cannot become a
//!   crash.
//! * **Bounded child aggregation.** `pinned_layout` and `titles` are bounded by
//!   the live page count, answer honestly when there are no pages, and skip a
//!   page whose child has already gone rather than panicking on it.
//!
//! **No materialization.** `AdwTabView` holds its pages eagerly and registers no
//! lazily created store, so walking it brings nothing into being — unlike the
//! `GtkTreeListModel` hazard the rule was written for.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::super::LushtextWindow;
use super::policy::TabLayoutEntry;
use super::surfaces;

/// Everything a test or probe may observe about the tab strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStripEvidence {
    /// Number of pages currently in the strip.
    pub page_count: i32,
    /// The pinned-segment layout the policy rules operate over, in visual order.
    pub pinned_layout: Vec<TabLayoutEntry>,
    /// Display titles in visual order, as the status messages would name them.
    pub titles: Vec<String>,
    /// Visual index of the page the context menu is currently aimed at.
    ///
    /// `None` both when no menu target is set and when the stored weak handle
    /// no longer upgrades — the two are indistinguishable to the workflow and
    /// are treated identically by every consumer.
    pub menu_target_index: Option<usize>,
    /// Enabled state of the five menu-backed actions, in a fixed order matching
    /// `surfaces::TAB_CONTEXT_ACTIONS`.
    pub menu_action_enabled: [bool; 5],
    /// Pages holding an unconsumed bulk-close authorization token.
    ///
    /// A non-zero count outside a bulk close means a token was deposited and
    /// never spent, which would let a later close skip its confirmation dialog.
    pub preconfirmed_close_count: usize,
    /// Pages whose pin-notify bookkeeping has been attached.
    pub configured_page_count: usize,
    /// Depth of the projection-refresh deferral batch. Non-zero outside a bulk
    /// operation means a batch was begun and never released.
    pub projection_refresh_defer_depth: u32,
    /// Whether any page in the strip owns an editor with a save in flight,
    /// which is the condition that refuses a bulk close outright.
    pub any_page_saving: bool,
}

/// Read the whole tab-strip surface.
#[must_use]
pub fn tab_strip_evidence(window: &LushtextWindow) -> TabStripEvidence {
    let imp = window.imp();

    // A disposed window has no tab view; answer honestly rather than panicking.
    let Some(tab_view) = imp.tab_view.try_get() else {
        return TabStripEvidence {
            page_count: 0,
            pinned_layout: Vec::new(),
            titles: Vec::new(),
            menu_target_index: None,
            menu_action_enabled: [false; 5],
            preconfirmed_close_count: imp.tab_management.preconfirmed_close_pages.borrow().len(),
            configured_page_count: imp.tab_management.configured_pages.borrow().len(),
            projection_refresh_defer_depth: imp.tab_projection_refresh_defer_depth.get(),
            any_page_saving: false,
        };
    };

    let pages: Vec<libadwaita::TabPage> = (0..tab_view.n_pages())
        .map(|index| tab_view.nth_page(index))
        .collect();
    let pinned_layout = pages
        .iter()
        .map(|page| TabLayoutEntry {
            pinned: page.is_pinned(),
        })
        .collect();
    let titles = pages.iter().map(surfaces::tab_display_title).collect();
    let any_page_saving = pages.iter().any(surfaces::page_has_saving_editor);

    // Each borrow is scoped and released before the struct literal below.
    let menu_target_index = {
        let target_slot = imp.tab_management.target_page.borrow();
        target_slot
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            .and_then(|target| {
                let key = surfaces::tab_page_key(&target);
                pages
                    .iter()
                    .position(|page| surfaces::tab_page_key(page) == key)
            })
    };
    let preconfirmed_close_count = imp.tab_management.preconfirmed_close_pages.borrow().len();
    let configured_page_count = imp.tab_management.configured_pages.borrow().len();

    let mut menu_action_enabled = [false; 5];
    for (slot, name) in menu_action_enabled
        .iter_mut()
        .zip(surfaces::TAB_CONTEXT_ACTIONS)
    {
        *slot = window
            .lookup_action(name)
            .is_some_and(|action| action.is_enabled());
    }

    TabStripEvidence {
        page_count: tab_view.n_pages(),
        pinned_layout,
        titles,
        menu_target_index,
        menu_action_enabled,
        preconfirmed_close_count,
        configured_page_count,
        projection_refresh_defer_depth: imp.tab_projection_refresh_defer_depth.get(),
        any_page_saving,
    }
}
