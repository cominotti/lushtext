// SPDX-License-Identifier: GPL-3.0-or-later

//! Accessible-state projection for the search panel.
//!
//! Search, retirement, preview, and apply all change several widgets from
//! different modules. Projecting busy/invalid/hidden/value state in one place
//! keeps every caller from carrying a parallel set of rules, and keeps that
//! widget mutation out of the workflow facade.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-SEARCH-REPLACE` — it projects the workflow onto widgets (accessible-state projection) — so under
//! `gtk-adapter-module-boundaries` it is outside the five-name role taxonomy,
//! takes none of those names, and owns no `policy.rs` and no `evidence.rs`. Its
//! behavior obligations are unchanged. Named in that workflow's matrix row.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::accessibility;

use super::LushtextSearchPanel;
#[cfg(feature = "test-utils")]
use super::item::SearchResultItem;

impl LushtextSearchPanel {
    /// Project the panel's live workflow state into GTK accessible metadata.
    pub(crate) fn refresh_accessibility_state(&self) {
        let imp = self.imp();
        let searching = imp.runtime.searching.get();
        let preview_pending = imp.preview.preview_pending.get();
        let replace_transaction_pending = imp.preview.replace_transaction_pending.get();
        let count_text = imp.count_label.text();
        let count_value = if count_text.is_empty() {
            "No workspace search results".to_string()
        } else {
            count_text.to_string()
        };

        accessibility::set_busy(&*imp.search_entry, searching);
        accessibility::set_busy(
            &*imp.results_list,
            searching || preview_pending || replace_transaction_pending,
        );
        accessibility::set_busy(
            &*imp.replace_all_button,
            preview_pending || replace_transaction_pending,
        );
        accessibility::set_invalid(&*imp.search_entry, imp.error_label.is_visible());
        accessibility::set_hidden(
            &*imp.results_list,
            !imp.results_body_revealer.reveals_child(),
        );
        accessibility::set_value_text(&*imp.count_label, &count_value);

        for toggle in [
            &*imp.case_toggle,
            &*imp.regex_toggle,
            &*imp.word_toggle,
            &*imp.more_toggle,
            &*imp.gitignore_toggle,
        ] {
            accessibility::set_pressed(toggle, toggle.is_active());
        }
        accessibility::set_expanded(&*imp.more_toggle, Some(imp.more_toggle.is_active()));

        accessibility::set_disabled(
            &*imp.replace_all_button,
            !imp.replace_all_button.is_sensitive(),
        );
        accessibility::set_disabled(
            &*imp.undo_button,
            !imp.undo_button.is_sensitive() || !self.has_undo_backup(),
        );
        accessibility::set_hidden(&*imp.undo_button, !imp.undo_button.is_visible());
        accessibility::set_hidden(&*imp.save_button, !imp.save_button.is_visible());

        let replace_label = imp
            .replace_all_button
            .label()
            .unwrap_or_else(|| "Replace All".into());
        accessibility::set_value_text(&*imp.replace_all_button, replace_label.as_str());

        if !searching && !preview_pending && !count_text.is_empty() {
            imp.results_announcement_throttler.announce_if_allowed(
                &*imp.count_label,
                accessibility::AnnouncementLane::DebouncedResults,
                "workspace-search-results",
                count_text.as_str(),
            );
        }
    }

    /// Test seam for forcing the same accessibility projection used by search,
    /// retirement, and replace workflows after a widget test mutates state.
    #[cfg(feature = "test-utils")]
    pub fn refresh_accessibility_state_for_test(&self) {
        self.refresh_accessibility_state();
    }
}

/// Test seam for asserting the search-result row metadata used by the list factory.
#[cfg(feature = "test-utils")]
pub fn apply_search_result_row_accessibility_for_test(
    row_widget: &gtk4::TreeExpander,
    result_item: &SearchResultItem,
    expanded: Option<bool>,
) {
    super::list_factory::apply_result_row_accessibility(row_widget, result_item, expanded);
}
