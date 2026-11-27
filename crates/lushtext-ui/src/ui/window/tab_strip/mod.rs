// SPDX-License-Identifier: GPL-3.0-or-later

//! The tab-strip workflow (`WFR-TAB-STRIP`) — narrative facade.
//!
//! A family of operations on the `AdwTabView` strip that share one ordered
//! stage sequence: **resolve the menu target -> read the strip's layout ->
//! decide what is eligible -> apply -> republish the menu's enabled state**.
//! Pin, the two reorders, and the two bulk closes are five entry points into
//! that one sequence, which is why they are one row rather than five. They also
//! share one target slot, one layout snapshot type, and one menu refresh; a
//! change to the pinned-segment rule moves all five together.
//!
//! Closing is the only entry point whose sequence continues past `apply`, and
//! that continuation is the coordination module — see the role table.
//!
//! # Role home
//!
//! **Per-workflow subdirectory** `ui/window/tab_strip/`. `ui/window/` hosts
//! many workflows and the flat `policy.rs` / `evidence.rs` names are not
//! available there, so the roles live here.
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy.rs` | pure policy — the pinned-segment invariant, target eligibility, move bounds, menu label, status messages |
//! | `close_execution.rs` | coordination (`execution`), stage-order-qualified — the close stage order and its cancellable-detach inversion |
//! | `evidence.rs` | evidence surface (`test-utils`-gated; production reads live state directly) |
//! | `surfaces.rs` | **called presentation surface** — page-order snapshots, the menu model, action enablement, the pin indicator. Carries no role |
//!
//! # The pinned-segment invariant
//!
//! `AdwTabView` keeps pinned pages in a contiguous leading segment. Every rule
//! this workflow has is a consequence: a bulk close never takes a pinned tab, an
//! unpinned tab cannot move into the pinned run, and a pinned tab cannot move
//! out of it. Those live in `policy.rs` over a `[TabLayoutEntry]` rather than
//! over live pages, so each is one testable function instead of an `if` inside a
//! widget call.
//!
//! # Absences, recorded as conclusions
//!
//! * **No `test_policy.rs`.** The workflow has no test-only timing or limit
//!   override.
//! * **No seam value object beyond `TabLayoutEntry`.** The target context
//!   crosses exactly one boundary — `surfaces::current_target_context` to its
//!   caller — and is reconstructed nowhere, so reifying it further would be the
//!   every-long-signature reading of the rule the convention rejects.
//!   `TabLayoutEntry` itself *is* the reified seam: it is the value the GTK
//!   adapter and the pure policy both name.
//! * **Zero actuation seams added.** Slot 5b's budgeted seam stays unspent.

pub mod policy;

mod close_execution;
mod surfaces;

#[cfg(feature = "test-utils")]
pub mod evidence;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

impl LushtextWindow {
    /// Install the native Adwaita tab context menu and its actions.
    ///
    /// The menu model lives on `AdwTabView` itself so right-click handling stays
    /// toolkit-owned instead of relying on custom tab-hit testing.
    pub(super) fn setup_tab_management(&self) {
        surfaces::register_tab_context_actions(self);
        self.imp()
            .tab_view
            .set_menu_model(Some(&self.imp().tab_management.context_menu));

        let window_weak = self.downgrade();
        self.imp().tab_view.connect_setup_menu(move |_view, page| {
            if let Some(window) = window_weak.upgrade() {
                surfaces::refresh_tab_context_menu(&window, page);
            }
        });

        let window_weak = self.downgrade();
        self.imp().tab_view.connect_page_reordered(move |_, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.save_session_debounced();
                surfaces::refresh_tab_context_menu(&window, None);
            }
        });

        for i in 0..self.imp().tab_view.n_pages() {
            let page = self.imp().tab_view.nth_page(i);
            self.configure_tab_page(&page);
        }
        surfaces::refresh_tab_context_menu(self, None);
    }

    /// Attach pin-state bookkeeping to a newly created page exactly once.
    pub(crate) fn configure_tab_page(&self, page: &libadwaita::TabPage) {
        let page_key = surfaces::tab_page_key(page);
        if !self
            .imp()
            .tab_management
            .configured_pages
            .borrow_mut()
            .insert(page_key)
        {
            return;
        }

        surfaces::refresh_tab_page_indicator(page);

        let window_weak = self.downgrade();
        let page_weak = page.downgrade();
        page.connect_pinned_notify(move |_| {
            if let Some(window) = window_weak.upgrade()
                && let Some(page) = page_weak.upgrade()
            {
                surfaces::refresh_tab_page_indicator(&page);
                window.save_session_debounced();
            }
        });
    }

    /// Apply a restored pinned state without surfacing user-facing feedback.
    pub(crate) fn restore_tab_pinned_state(&self, page: &libadwaita::TabPage, pinned: bool) {
        if page.is_pinned() != pinned {
            self.imp().tab_view.set_page_pinned(page, pinned);
        }
        surfaces::refresh_tab_page_indicator(page);
    }

    /// Entry point: move the menu target between the pinned and unpinned
    /// segments.
    pub(crate) fn toggle_pinned_tab(&self) {
        let Some((target, _, _, _)) = surfaces::current_target_context(self) else {
            surfaces::refresh_tab_context_menu(self, None);
            return;
        };

        let pinned = !target.is_pinned();
        self.imp().tab_view.set_page_pinned(&target, pinned);
        self.publish_status_message(
            &policy::pin_toggle_message(&surfaces::tab_display_title(&target), pinned),
            MessageKind::Info,
        );
        surfaces::refresh_tab_context_menu(self, None);
    }

    /// Entry point: move the menu target one slot toward the pinned edge.
    pub(crate) fn move_tab_left(&self) {
        self.move_target_tab(true);
    }

    /// Entry point: move the menu target one slot away from the pinned edge.
    pub(crate) fn move_tab_right(&self) {
        self.move_target_tab(false);
    }

    /// The shared reorder stage: both directions differ only in the policy
    /// bound they consult and the `AdwTabView` call they make.
    fn move_target_tab(&self, toward_pinned_edge: bool) {
        let Some((target, _pages, layout, target_index)) = surfaces::current_target_context(self)
        else {
            surfaces::refresh_tab_context_menu(self, None);
            return;
        };
        let permitted = if toward_pinned_edge {
            policy::can_move_left(&layout, target_index)
        } else {
            policy::can_move_right(&layout, target_index)
        };
        if !permitted {
            surfaces::refresh_tab_context_menu(self, None);
            return;
        }

        let moved = if toward_pinned_edge {
            self.imp().tab_view.reorder_backward(&target)
        } else {
            self.imp().tab_view.reorder_forward(&target)
        };
        if moved {
            self.publish_status_message(
                &policy::reorder_message(&surfaces::tab_display_title(&target), toward_pinned_edge),
                MessageKind::Info,
            );
        }
        surfaces::refresh_tab_context_menu(self, None);
    }

    /// Entry point: close all unpinned tabs except the menu target.
    pub(crate) fn close_other_tabs(&self) {
        self.close_eligible_tabs(policy::eligible_close_other_positions);
    }

    /// Entry point: close all unpinned tabs strictly after the menu target.
    pub(crate) fn close_tabs_to_the_right(&self) {
        self.close_eligible_tabs(policy::eligible_close_right_positions);
    }

    /// Resolve an eligibility rule into pages and hand them to the close stage
    /// order, which owns the refuse/ask-once/authorize/detach sequence.
    fn close_eligible_tabs(&self, eligible: fn(&[policy::TabLayoutEntry], usize) -> Vec<usize>) {
        let Some((_target, pages, layout, target_index)) = surfaces::current_target_context(self)
        else {
            surfaces::refresh_tab_context_menu(self, None);
            return;
        };
        let targets = eligible(&layout, target_index)
            .into_iter()
            .map(|index| pages[index].clone())
            .collect::<Vec<_>>();
        self.confirm_and_close_tab_pages(targets);
    }
}
