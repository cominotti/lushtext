// SPDX-License-Identifier: GPL-3.0-or-later

//! Preferences > Data > Preserved Drafts: the review surface for the drafts
//! set-aside area.
//!
//! The group lists the newest preserved bodies with per-row Open and Delete,
//! a summary row with the whole area's count, size, soft-bound state, and a
//! truncation note, and one bulk "Delete All Preserved Drafts…" action. Every
//! deletion here is a confirmed user decision: the bulk confirmation states
//! the exact count and size it deletes, and removes only the bodies it listed,
//! as fingerprinted when the group was rendered
//! (`draft_service::set_aside::delete_confirmed`). No body text is shown,
//! logged, or announced.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then_weak;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::services::draft_service::set_aside::SetAsideBulkDeletion;
use crate::services::draft_service::set_aside_retention::{
    BoundStatus, SetAsideFingerprint, SetAsideTotals, bound_status,
};
use crate::services::draft_service::{self, SetAsideDraftListing};
use crate::services::json_store;
use crate::ui::accessibility;

use super::LushtextPreferences;

/// Name of the Data page in the Preferences template.
pub const DATA_PAGE_NAME: &str = "data";
/// Tallest the preserved-draft list grows before it scrolls inside the group.
const SET_ASIDE_LIST_MAX_HEIGHT: i32 = 320;

/// Title for one preserved set-aside draft row.
fn set_aside_row_title(draft: &draft_service::SetAsideDraft) -> String {
    match (&draft.original_path, draft.untitled) {
        (Some(path), _) => path.display().to_string(),
        (None, true) => "Untitled document".to_string(),
        (None, false) => "Unknown file".to_string(),
    }
}

/// Subtitle for one preserved set-aside draft row: when and how much.
fn set_aside_row_subtitle(draft: &draft_service::SetAsideDraft) -> String {
    let kept = i64::try_from(draft.body.stamp_secs)
        .ok()
        .and_then(|secs| glib::DateTime::from_unix_local(secs).ok())
        .and_then(|time| time.format("%Y-%m-%d %H:%M").ok())
        .map_or_else(|| "an unknown time".to_string(), |time| time.to_string());
    format!(
        "Kept {kept} · {}",
        glib::format_size(draft.body.byte_size())
    )
}

/// `1204` as `1,204`.
pub(crate) fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `1 preserved draft`, `37 preserved drafts`.
pub(crate) fn drafts_phrase(count: u64) -> String {
    if count == 1 {
        "1 preserved draft".to_string()
    } else {
        format!("{} preserved drafts", grouped(count))
    }
}

/// `at least ` when a scan stopped at its budget, so its totals are a lower
/// bound; empty otherwise.
pub(crate) const fn lower_bound_prefix(totals: SetAsideTotals) -> &'static str {
    if totals.complete { "" } else { "at least " }
}

/// Rows the group lists.
fn shown_count(listing: &SetAsideDraftListing) -> u64 {
    u64::try_from(listing.drafts.len()).unwrap_or(u64::MAX)
}

/// `newest 256 of [at least ]1,204`.
fn newest_of_total(listing: &SetAsideDraftListing) -> String {
    format!(
        "newest {} of {}{}",
        grouped(shown_count(listing)),
        lower_bound_prefix(listing.totals),
        grouped(listing.totals.count)
    )
}

/// Summary row title: the area's count, or its lower bound.
fn summary_title(listing: &SetAsideDraftListing) -> String {
    let phrase = drafts_phrase(listing.totals.count);
    if listing.totals.complete {
        phrase
    } else {
        format!("At least {phrase}")
    }
}

/// Summary row subtitle: size, soft-bound state, and truncation.
fn summary_subtitle(listing: &SetAsideDraftListing) -> String {
    let size = glib::format_size(listing.totals.bytes);
    let mut parts = vec![if listing.totals.complete {
        size.to_string()
    } else {
        format!("At least {size}")
    }];
    if matches!(bound_status(Some(listing.totals)), BoundStatus::Over { .. }) {
        parts.push("over the suggested limit".to_string());
    }
    if listing.is_truncated() {
        parts.push(format!("showing the {}", newest_of_total(listing)));
    }
    parts.join(" · ")
}

/// Body of the Delete All confirmation: the exact count and size it deletes.
fn delete_all_body(listing: &SetAsideDraftListing) -> String {
    let size = glib::format_size(listed_bytes(listing));
    let what = if listing.is_truncated() {
        format!(
            "The {} preserved drafts ({size}) will be permanently deleted; the rest stay for a later pass.",
            newest_of_total(listing)
        )
    } else {
        format!(
            "{} ({size}) will be permanently deleted.",
            drafts_phrase(shown_count(listing))
        )
    };
    format!("{what} This cannot be undone. Drafts preserved after this dialog opened are kept.")
}

/// Total size of the rows the group lists (what Delete All confirms).
fn listed_bytes(listing: &SetAsideDraftListing) -> u64 {
    listing.drafts.iter().fold(0u64, |total, draft| {
        total.saturating_add(draft.body.byte_size())
    })
}

/// What a finished bulk deletion reports, as a short status sentence.
fn bulk_deletion_message(outcome: SetAsideBulkDeletion) -> String {
    let mut message = format!("Deleted {}", drafts_phrase(outcome.deleted));
    if outcome.kept > 0 {
        message.push_str(&format!(
            "; kept {} that changed after you confirmed",
            grouped(outcome.kept)
        ));
    }
    if outcome.failed > 0 {
        message.push_str(&format!(
            "; {} could not be deleted",
            grouped(outcome.failed)
        ));
    }
    if outcome.durability_unconfirmed {
        message.push_str("; the deletion may not survive a crash");
    }
    message
}

/// A destructive confirmation whose default and close response is Cancel;
/// `on_confirm` runs only when the user picks `confirm_label`.
fn destructive_confirmation(
    heading: &str,
    body: &str,
    confirm_label: &str,
    on_confirm: impl Fn() + 'static,
) -> libadwaita::AlertDialog {
    let dialog = libadwaita::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("delete", confirm_label);
    dialog.set_response_appearance("delete", libadwaita::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None::<&str>, move |_, response| {
        if response == "delete" {
            on_confirm();
        }
    });
    dialog
}

impl LushtextPreferences {
    /// List the preserved set-aside drafts off GTK and project them into the
    /// Data page's Preserved Drafts group, which stays hidden while empty.
    pub fn refresh_set_aside_drafts(&self) {
        spawn_blocking_then_weak(
            self,
            || draft_service::list_set_aside_drafts(&json_store::data_dir()),
            |prefs, listed| match listed {
                Ok(listing) => prefs.render_set_aside_drafts(listing),
                Err(error) => {
                    tracing::warn!("Could not list preserved drafts: {error}");
                    prefs.render_set_aside_drafts(SetAsideDraftListing::default());
                }
            },
        );
    }

    /// Present the Data page with the Preserved Drafts group in view: its
    /// summary row takes focus as soon as the listing lands.
    pub fn show_preserved_drafts(&self) {
        self.set_visible_page_name(DATA_PAGE_NAME);
        self.imp().data_set_aside_focus_pending.set(true);
        self.focus_set_aside_summary_if_pending();
    }

    fn focus_set_aside_summary_if_pending(&self) {
        let imp = self.imp();
        if !imp.data_set_aside_focus_pending.get() {
            return;
        }
        if imp.data_set_aside_listing.borrow().is_some() {
            imp.data_set_aside_focus_pending.set(false);
            imp.data_set_aside_summary.grab_focus();
        }
    }

    /// Build the group's fixed rows once: the summary, the bulk action, and
    /// the bounded scroller holding one row per listed body.
    pub(super) fn setup_set_aside_group(&self) {
        let imp = self.imp();
        imp.data_set_aside_summary.set_subtitle_lines(3);
        imp.data_set_aside_summary.set_focusable(true);
        imp.data_set_aside_delete_all
            .set_title("Delete All Preserved Drafts…");
        imp.data_set_aside_delete_all
            .add_css_class("destructive-action");
        accessibility::set_labelled_description(
            &imp.data_set_aside_delete_all,
            "Delete All Preserved Drafts…",
            "Asks first, stating how many drafts and how much space",
        );
        let prefs_weak = self.downgrade();
        imp.data_set_aside_delete_all.connect_activated(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                // The dialog is returned for tests; it owns its own response.
                let _ = prefs.confirm_delete_all_set_aside_drafts();
            }
        });
        imp.data_set_aside_list
            .set_selection_mode(gtk4::SelectionMode::None);
        imp.data_set_aside_list.add_css_class("boxed-list");
        accessibility::set_role(&imp.data_set_aside_list, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &imp.data_set_aside_list,
            "Preserved drafts",
            "The newest preserved drafts, each with Open and Delete",
        );
        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(SET_ASIDE_LIST_MAX_HEIGHT)
            .child(&imp.data_set_aside_list)
            .build();
        imp.data_set_aside_group.add(&imp.data_set_aside_summary);
        imp.data_set_aside_group.add(&imp.data_set_aside_delete_all);
        imp.data_set_aside_group.add(&scroll);
    }

    fn render_set_aside_drafts(&self, listing: SetAsideDraftListing) {
        let imp = self.imp();
        imp.data_set_aside_list.remove_all();
        let empty = listing.drafts.is_empty();
        imp.data_set_aside_group.set_visible(!empty);
        if empty {
            imp.data_set_aside_listing.replace(None);
            return;
        }
        let title = summary_title(&listing);
        let subtitle = summary_subtitle(&listing);
        imp.data_set_aside_summary.set_title(&title);
        imp.data_set_aside_summary.set_subtitle(&subtitle);
        accessibility::set_labelled_description(&imp.data_set_aside_summary, &title, &subtitle);
        let drafts = &listing.drafts;
        let total = i32::try_from(drafts.len()).unwrap_or(i32::MAX);
        for (position, draft) in (1i32..).zip(drafts) {
            imp.data_set_aside_list
                .append(&self.set_aside_row(draft, position, total));
        }
        imp.data_set_aside_listing.replace(Some(listing));
        self.focus_set_aside_summary_if_pending();
    }

    fn set_aside_row(
        &self,
        draft: &draft_service::SetAsideDraft,
        position: i32,
        total: i32,
    ) -> libadwaita::ActionRow {
        let title = set_aside_row_title(draft);
        let subtitle = set_aside_row_subtitle(draft);
        let row = libadwaita::ActionRow::builder()
            .title(&title)
            .subtitle(&subtitle)
            .title_lines(2)
            .build();
        accessibility::apply_row_accessibility(
            &row,
            accessibility::RowAccessibility::new(&format!("Preserved draft of {title}"))
                .description(&subtitle)
                .position(position, total),
        );
        let open = gtk4::Button::builder()
            .label("Open")
            .valign(gtk4::Align::Center)
            .tooltip_text("Open these changes in a new tab")
            .build();
        open.add_css_class("flat");
        accessibility::set_label(&open, &format!("Open preserved draft of {title}"));
        let delete = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk4::Align::Center)
            .tooltip_text("Delete these preserved changes")
            .build();
        delete.add_css_class("flat");
        accessibility::set_label(&delete, &format!("Delete preserved draft of {title}"));
        let path = draft.body.path.clone();
        let prefs_weak = self.downgrade();
        open.connect_clicked(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs.open_set_aside_draft(&path);
            }
        });
        let fingerprint = draft.body.fingerprint.clone();
        let prefs_weak = self.downgrade();
        delete.connect_clicked(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs.confirm_delete_set_aside_draft(&fingerprint, &title);
            }
        });
        row.add_suffix(&open);
        row.add_suffix(&delete);
        row
    }

    /// Open one preserved draft's text in a new untitled tab of the window this
    /// dialog belongs to; the set-aside copy stays until the user deletes it.
    pub fn open_set_aside_draft(&self, path: &std::path::Path) {
        let path = path.to_path_buf();
        spawn_blocking_then_weak(
            self,
            move || draft_service::set_aside::read(&json_store::data_dir(), &path),
            |prefs, text| match text {
                Ok(text) => {
                    if let Some(window) = prefs
                        .root()
                        .and_downcast::<crate::ui::window::LushtextWindow>()
                    {
                        window.open_set_aside_draft(text);
                        prefs.close();
                    }
                }
                Err(error) => {
                    tracing::warn!("Could not open a preserved draft: {error}");
                    accessibility::announce_with_lane(
                        &*prefs.imp().data_set_aside_group,
                        "The preserved draft could not be opened",
                        accessibility::AnnouncementLane::Alert,
                    );
                }
            },
        );
    }

    /// Ask before deleting one preserved draft: it may be the only copy. The
    /// row's fingerprint, as listed, is what the confirmation deletes.
    fn confirm_delete_set_aside_draft(&self, fingerprint: &SetAsideFingerprint, title: &str) {
        let prefs_weak = self.downgrade();
        let fingerprint = fingerprint.clone();
        let dialog = destructive_confirmation(
            "Delete Preserved Draft?",
            &format!("The unsaved changes kept for {title} will be permanently deleted."),
            "Delete",
            move || {
                if let Some(prefs) = prefs_weak.upgrade() {
                    prefs.delete_set_aside_draft(fingerprint.clone());
                }
            },
        );
        dialog.present(Some(self));
    }

    /// Delete one preserved draft durably after the user confirmed it, only
    /// while it is unchanged since it was listed, then refresh the group.
    pub fn delete_set_aside_draft(&self, fingerprint: SetAsideFingerprint) {
        spawn_blocking_then_weak(
            self,
            move || {
                draft_service::set_aside::delete_confirmed(
                    &json_store::data_dir(),
                    std::slice::from_ref(&fingerprint),
                )
            },
            |prefs, outcome| {
                if outcome.deleted == 0 {
                    let message = if outcome.failed > 0 {
                        "The preserved draft could not be deleted"
                    } else {
                        "The preserved draft changed or was removed after you confirmed; nothing was deleted"
                    };
                    accessibility::announce_with_lane(
                        &*prefs.imp().data_set_aside_group,
                        message,
                        accessibility::AnnouncementLane::Alert,
                    );
                }
                prefs.refresh_set_aside_drafts();
            },
        );
    }

    /// Ask before deleting every listed preserved draft, stating the exact
    /// count and size. The confirmed set is fixed now, from the rows this
    /// group shows: a body set aside, or changed, after this point is kept.
    /// Cancel is the default and the close response. Returns the dialog, or
    /// `None` when nothing is listed.
    #[must_use]
    pub fn confirm_delete_all_set_aside_drafts(&self) -> Option<libadwaita::AlertDialog> {
        let (body, confirmed) = {
            let listing = self.imp().data_set_aside_listing.borrow();
            let listing = listing
                .as_ref()
                .filter(|listing| !listing.drafts.is_empty())?;
            let confirmed: Vec<SetAsideFingerprint> = listing
                .drafts
                .iter()
                .map(|draft| draft.body.fingerprint.clone())
                .collect();
            (delete_all_body(listing), confirmed)
        };
        let prefs_weak = self.downgrade();
        let confirmed = std::cell::RefCell::new(Some(confirmed));
        let dialog = destructive_confirmation(
            "Delete All Preserved Drafts?",
            &body,
            "Delete All",
            move || {
                if let Some(prefs) = prefs_weak.upgrade()
                    && let Some(confirmed) = confirmed.take()
                {
                    prefs.delete_confirmed_set_aside_drafts(confirmed);
                }
            },
        );
        dialog.present(Some(self));
        Some(dialog)
    }

    /// Delete exactly the confirmed bodies off GTK, then announce the result
    /// and refresh the group.
    fn delete_confirmed_set_aside_drafts(&self, confirmed: Vec<SetAsideFingerprint>) {
        spawn_blocking_then_weak(
            self,
            move || draft_service::set_aside::delete_confirmed(&json_store::data_dir(), &confirmed),
            |prefs, outcome| {
                accessibility::announce_with_lane(
                    &*prefs.imp().data_set_aside_group,
                    &bulk_deletion_message(outcome),
                    accessibility::AnnouncementLane::StatusUpdate,
                );
                prefs.refresh_set_aside_drafts();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_group_digits_and_bulk_messages_name_every_outcome() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_204), "1,204");
        assert_eq!(grouped(10_000_000), "10,000,000");
        assert_eq!(
            bulk_deletion_message(SetAsideBulkDeletion {
                deleted: 1,
                ..SetAsideBulkDeletion::default()
            }),
            "Deleted 1 preserved draft"
        );
        assert_eq!(
            bulk_deletion_message(SetAsideBulkDeletion {
                deleted: 3,
                kept: 1,
                failed: 2,
                durability_unconfirmed: true,
            }),
            "Deleted 3 preserved drafts; kept 1 that changed after you confirmed; 2 could not \
             be deleted; the deletion may not survive a crash"
        );
    }
}
