// SPDX-License-Identifier: GPL-3.0-or-later

//! Preferences Data page workflow for app-owned metadata format scans.
//!
//! The preferences implementation owns the template children, while this module
//! owns the background scan/convert commands and their read-model projection
//! back into the Data page rows.

use std::time::{Duration, Instant};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then_weak;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::services::{draft_service, format_upgrade, json_store};
use crate::ui::accessibility;

use super::LushtextPreferences;

/// Maximum rows rendered per metadata group before showing an omitted-count row.
const DATA_DETAILS_MAX_ROWS_PER_GROUP: usize = 32;
/// Minimum time a successful fast scan remains visibly "checking" in Preferences.
///
/// One second is long enough for users to perceive that Refresh did work while
/// staying short enough that genuinely slow scans are not delayed further.
const DATA_SCAN_MINIMUM_VISIBLE_DURATION: Duration = Duration::from_secs(1);

/// Background conversion result returned to the Preferences Data page on the GTK main thread.
enum DataConvertWorkerResult {
    NoConvert(format_upgrade::FormatPlan),
    Applied {
        plan: format_upgrade::FormatPlan,
        result: Result<format_upgrade::FormatApplyOutcome, String>,
    },
}

/// Presentation policy for a Data page scan result.
#[derive(Clone, Copy)]
enum DataScanPresentation {
    /// Initial and post-convert scans should reveal actionable state as soon as it is ready.
    Immediate,
    /// Manual Refresh keeps the checking state visible long enough to prove the click did work.
    VisibleDwell,
}

impl LushtextPreferences {
    /// Run the startup or post-conversion scan without extra visible dwell.
    pub(super) fn run_data_scan_immediate(&self) {
        self.run_data_scan(DataScanPresentation::Immediate);
    }

    /// Run a manual scan that keeps the checking state visible for a short dwell.
    pub(super) fn run_data_scan_visible_dwell(&self) {
        self.run_data_scan(DataScanPresentation::VisibleDwell);
    }

    /// Run a Data-page scan on a background thread.
    fn run_data_scan(&self, presentation: DataScanPresentation) {
        let imp = self.imp();
        if imp.data_operation_inflight.replace(true) {
            return;
        }
        self.show_data_operation_in_progress("Verifying app data formats");

        let prefs = self.clone();
        let scan_started_at = Instant::now();
        let minimum_visible_duration = match presentation {
            DataScanPresentation::Immediate => Duration::ZERO,
            DataScanPresentation::VisibleDwell => DATA_SCAN_MINIMUM_VISIBLE_DURATION,
        };
        spawn_blocking_then_weak(
            &prefs,
            || {
                let data_dir = json_store::data_dir();
                let inventory = format_upgrade::scan(&data_dir);
                format_upgrade::build_plan(&inventory)
            },
            move |prefs, plan| {
                let remaining = minimum_visible_duration.saturating_sub(scan_started_at.elapsed());
                if remaining.is_zero() {
                    prefs.complete_data_scan(&plan);
                } else {
                    let prefs_weak = prefs.downgrade();
                    // GLib timeout callbacks run on the GTK main loop; the
                    // weak dialog reference prevents a delayed fast-scan result
                    // from keeping a closed Preferences dialog alive.
                    glib::timeout_add_local_once(remaining, move || {
                        if let Some(prefs) = prefs_weak.upgrade() {
                            prefs.complete_data_scan(&plan);
                        }
                    });
                }
            },
        );
    }

    /// Run a supported conversion from the Data page.
    pub(super) fn run_data_convert(&self) {
        let imp = self.imp();
        if imp.data_operation_inflight.replace(true) {
            return;
        }
        if !imp.data_last_scan_offers_convert.get() {
            imp.data_operation_inflight.set(false);
            return;
        }

        self.show_data_operation_in_progress("Updating supported older app data");

        let prefs = self.clone();
        spawn_blocking_then_weak(
            &prefs,
            move || {
                let data_dir = json_store::data_dir();
                let inventory = format_upgrade::scan(&data_dir);
                let plan = format_upgrade::build_plan(&inventory);
                if !plan.offers_convert() {
                    return DataConvertWorkerResult::NoConvert(plan);
                }
                let result =
                    format_upgrade::apply_plan(&data_dir, &plan).map_err(|error| error.to_string());
                DataConvertWorkerResult::Applied { plan, result }
            },
            |prefs, result| {
                let imp = prefs.imp();
                imp.data_operation_inflight.set(false);
                imp.data_scan_button.set_sensitive(true);
                match result {
                    DataConvertWorkerResult::NoConvert(plan) => {
                        let detail = if plan.has_no_action() {
                            None
                        } else {
                            Some("No supported conversion is available for the current scan")
                        };
                        prefs.render_data_plan(&plan, detail);
                    }
                    DataConvertWorkerResult::Applied {
                        result: Ok(outcome),
                        ..
                    } if outcome.is_success() => prefs.run_data_scan_immediate(),
                    DataConvertWorkerResult::Applied {
                        plan,
                        result: Ok(outcome),
                    } => {
                        let detail = outcome.failures.first().map(|failure| {
                            format!("{}: {}", failure.path.display(), failure.detail)
                        });
                        prefs.render_data_plan(&plan, detail.as_deref());
                    }
                    DataConvertWorkerResult::Applied {
                        plan,
                        result: Err(detail),
                    } => {
                        prefs.render_data_plan(&plan, Some(&detail));
                    }
                }
            },
        );
    }

    /// Render one scanned format plan into Data page status, details, and action state.
    fn render_data_plan(&self, plan: &format_upgrade::FormatPlan, failure: Option<&str>) {
        let imp = self.imp();
        let status = data_plan_status(plan, failure);
        let offers_convert = plan.offers_convert();
        let verified_current = failure.is_none() && plan.has_no_action();
        imp.data_status_row.set_subtitle(&status);
        imp.data_current_indicator.set_visible(verified_current);
        imp.data_actions_group.set_visible(offers_convert);
        imp.data_convert_row.set_visible(offers_convert);
        imp.data_convert_button.set_sensitive(offers_convert);
        imp.data_convert_button.set_label(if failure.is_some() {
            "Retry"
        } else {
            "Convert"
        });
        imp.data_last_scan_offers_convert.set(offers_convert);
        self.render_data_details(plan, failure);
        self.refresh_data_accessibility_state();
        self.announce_data_plan_status(&status, failure);
    }

    /// Present an active Data-page operation before its background result lands.
    fn show_data_operation_in_progress(&self, subtitle: &str) {
        let imp = self.imp();
        imp.data_current_indicator.set_visible(false);
        imp.data_scan_button.set_sensitive(false);
        imp.data_convert_button.set_sensitive(false);
        imp.data_status_row.set_subtitle(subtitle);
        self.refresh_data_accessibility_state();
    }

    /// Accept a completed scan after any visible verification dwell has elapsed.
    fn complete_data_scan(&self, plan: &format_upgrade::FormatPlan) {
        let imp = self.imp();
        imp.data_operation_inflight.set(false);
        imp.data_scan_button.set_sensitive(true);
        self.render_data_plan(plan, None);
    }

    /// Announce the compact Data-page result row after a scan or apply attempt.
    fn announce_data_plan_status(&self, status: &str, failure: Option<&str>) {
        let imp = self.imp();
        let lane = if failure.is_some() {
            accessibility::AnnouncementLane::Alert
        } else {
            accessibility::AnnouncementLane::StatusUpdate
        };
        let key = if failure.is_some() {
            "app-data-format-failed"
        } else {
            "app-data-format-scan"
        };
        imp.data_announcement_throttler.announce_if_allowed(
            &*imp.data_status_row,
            lane,
            key,
            &format!("App data format scan complete: {status}"),
        );
    }

    /// Rebuild the bounded per-file detail list for the current Data page plan.
    fn render_data_details(&self, plan: &format_upgrade::FormatPlan, failure: Option<&str>) {
        let imp = self.imp();
        while let Some(child) = imp.data_details_list.first_child() {
            imp.data_details_list.remove(&child);
        }

        if let Some(detail) = failure {
            let row = libadwaita::ActionRow::builder()
                .title("Last Attempt Failed")
                .subtitle(detail)
                .build();
            accessibility::apply_row_accessibility(
                &row,
                accessibility::RowAccessibility::new("Last app data update attempt failed")
                    .description(detail),
            );
            imp.data_details_list.append(&row);
        }

        if plan.has_no_action() {
            let row = libadwaita::ActionRow::builder()
                .title("Current")
                .subtitle("No app data files require a format update")
                .build();
            accessibility::apply_row_accessibility(
                &row,
                accessibility::RowAccessibility::new("App data format current")
                    .description("No app data files require a format update"),
            );
            imp.data_details_list.append(&row);
            return;
        }

        let mut position = 1i32;
        let total_rows = data_details_row_count(plan, failure);
        for group in &plan.groups {
            for planned in group.actions.iter().take(DATA_DETAILS_MAX_ROWS_PER_GROUP) {
                let title = format!(
                    "{}: {}",
                    group.metadata_kind.label(),
                    planned.item.path.display()
                );
                let subtitle = action_summary(planned);
                let row = libadwaita::ActionRow::builder()
                    .title(&title)
                    .subtitle(&subtitle)
                    .build();
                accessibility::apply_row_accessibility(
                    &row,
                    accessibility::RowAccessibility::new(&title)
                        .description(&subtitle)
                        .position(position, total_rows),
                );
                imp.data_details_list.append(&row);
                position += 1;
            }
            let omitted = group
                .actions
                .len()
                .saturating_sub(DATA_DETAILS_MAX_ROWS_PER_GROUP);
            if omitted > 0 {
                let title = format!("{}: {} more item(s)", group.metadata_kind.label(), omitted);
                let subtitle = "Additional matching app data is included in the action";
                let row = libadwaita::ActionRow::builder()
                    .title(&title)
                    .subtitle("Additional matching app data is included in the action")
                    .build();
                accessibility::apply_row_accessibility(
                    &row,
                    accessibility::RowAccessibility::new(&title)
                        .description(subtitle)
                        .position(position, total_rows),
                );
                imp.data_details_list.append(&row);
                position += 1;
            }
        }
    }

    /// Refresh dynamic accessibility state for the Preferences Data page.
    pub(super) fn refresh_data_accessibility_state(&self) {
        let imp = self.imp();
        let status = imp
            .data_status_row
            .subtitle()
            .unwrap_or_else(|| "Checking app data formats".into());
        accessibility::set_value_text(&*imp.data_status_row, status.as_str());
        accessibility::set_busy(&*imp.data_status_row, imp.data_operation_inflight.get());
        accessibility::set_busy(&*imp.data_scan_button, imp.data_operation_inflight.get());
        accessibility::set_disabled(&*imp.data_scan_button, !imp.data_scan_button.is_sensitive());
        accessibility::set_disabled(
            &*imp.data_convert_button,
            !imp.data_convert_button.is_sensitive(),
        );
        accessibility::set_hidden(&*imp.data_convert_row, !imp.data_convert_row.is_visible());
        accessibility::set_hidden(
            &*imp.data_current_indicator,
            !imp.data_current_indicator.is_visible(),
        );
        accessibility::set_value_text(&*imp.transparency_button, &imp.transparency_label.text());
    }
}

/// Summarize a scan/apply result in the compact Data page status row.
fn data_plan_status(plan: &format_upgrade::FormatPlan, failure: Option<&str>) -> String {
    if let Some(detail) = failure {
        return format!("Data update failed: {detail}");
    }
    if plan.has_no_action() {
        "Data format is current".to_string()
    } else if plan.has_future_version_blocker() {
        "Some app data was created by a newer LushText".to_string()
    } else if plan.offers_convert() {
        "Supported older app data can be updated".to_string()
    } else {
        "Some app data needs preservation or recovery".to_string()
    }
}

/// Count the rows rendered into the Data-page details list for position metadata.
fn data_details_row_count(plan: &format_upgrade::FormatPlan, failure: Option<&str>) -> i32 {
    let failure_rows = i32::from(failure.is_some());
    if plan.has_no_action() {
        return failure_rows + 1;
    }

    let planned_rows = plan.groups.iter().fold(0usize, |count, group| {
        let visible = group.actions.len().min(DATA_DETAILS_MAX_ROWS_PER_GROUP);
        let omitted = usize::from(group.actions.len() > DATA_DETAILS_MAX_ROWS_PER_GROUP);
        count + visible + omitted
    });
    i32::try_from(planned_rows)
        .unwrap_or(i32::MAX - failure_rows)
        .saturating_add(failure_rows)
}

/// Convert one planned item into the short subtitle shown in the details list.
fn action_summary(planned: &format_upgrade::FormatPlannedItem) -> String {
    match &planned.action {
        format_upgrade::FormatPlanAction::NoAction => "No action required".to_string(),
        format_upgrade::FormatPlanAction::ConvertToLatest {
            from_version,
            to_version,
        } => format!("Convert from v{from_version} to v{to_version}"),
        format_upgrade::FormatPlanAction::StartFreshOnly => {
            "No converter is available; Start Fresh preserves this data first".to_string()
        }
        format_upgrade::FormatPlanAction::ReportOnly => {
            "Recovery metadata will preserve or report this issue".to_string()
        }
    }
}

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
    format!("Kept {kept} · {}", glib::format_size(draft.body.byte_size))
}

impl LushtextPreferences {
    /// List the preserved set-aside drafts off GTK and project them into the
    /// Data page's Preserved Drafts group, which stays hidden while empty.
    pub fn refresh_set_aside_drafts(&self) {
        spawn_blocking_then_weak(
            self,
            || draft_service::list_set_aside_drafts(&json_store::data_dir()),
            |prefs, listed| match listed {
                Ok(drafts) => prefs.render_set_aside_drafts(&drafts),
                Err(error) => {
                    tracing::warn!("Could not list preserved drafts: {error}");
                    prefs.render_set_aside_drafts(&[]);
                }
            },
        );
    }

    fn render_set_aside_drafts(&self, drafts: &[draft_service::SetAsideDraft]) {
        let imp = self.imp();
        for row in imp.data_set_aside_rows.take() {
            imp.data_set_aside_group.remove(&row);
        }
        imp.data_set_aside_group.set_visible(!drafts.is_empty());
        let total = i32::try_from(drafts.len()).unwrap_or(i32::MAX);
        let mut rows = Vec::with_capacity(drafts.len());
        for (position, draft) in (1i32..).zip(drafts) {
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
            let path = draft.body.path.clone();
            let prefs_weak = self.downgrade();
            let confirm_title = title.clone();
            delete.connect_clicked(move |_| {
                if let Some(prefs) = prefs_weak.upgrade() {
                    prefs.confirm_delete_set_aside_draft(&path, &confirm_title);
                }
            });
            row.add_suffix(&open);
            row.add_suffix(&delete);
            imp.data_set_aside_group.add(&row);
            rows.push(row);
        }
        imp.data_set_aside_rows.replace(rows);
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

    /// Ask before deleting one preserved draft: it may be the only copy.
    fn confirm_delete_set_aside_draft(&self, path: &std::path::Path, title: &str) {
        let dialog = libadwaita::AlertDialog::builder()
            .heading("Delete Preserved Draft?")
            .body(format!(
                "The unsaved changes kept for {title} will be permanently deleted."
            ))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let prefs_weak = self.downgrade();
        let path = path.to_path_buf();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response == "delete"
                && let Some(prefs) = prefs_weak.upgrade()
            {
                prefs.delete_set_aside_draft(&path);
            }
        });
        dialog.present(Some(self));
    }

    /// Delete one preserved draft durably after the user confirmed it, then
    /// refresh the group.
    pub fn delete_set_aside_draft(&self, path: &std::path::Path) {
        let path = path.to_path_buf();
        spawn_blocking_then_weak(
            self,
            move || draft_service::set_aside::delete(&json_store::data_dir(), &path),
            |prefs, deleted| {
                if let Err(error) = deleted {
                    tracing::warn!("Could not delete a preserved draft: {error}");
                }
                prefs.refresh_set_aside_drafts();
            },
        );
    }
}
