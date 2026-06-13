// SPDX-License-Identifier: GPL-3.0-or-later

//! Startup format preflight and normal metadata-consumer handoff.
//!
//! The window shell can render immediately, but workspace/session/draft
//! consumers must wait until app-owned metadata is known to be current or the
//! user chooses a safe upgrade action.

use std::path::Path;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt, PreferencesGroupExt};

use crate::services::{format_upgrade, json_store};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Stable response id for the Convert action in the startup compatibility dialog.
const RESPONSE_CONVERT: &str = "convert";
/// Stable response id for preserving incompatible data and continuing fresh.
const RESPONSE_START_FRESH: &str = "start-fresh";
/// Stable response id that exits without changing app data.
const RESPONSE_QUIT: &str = "quit";

/// Result carried from startup-format background work back to the GTK main thread.
enum StartupFormatApplyWorkerResult {
    NoDecisionNeeded,
    Applied {
        plan: format_upgrade::FormatPlan,
        mode: format_upgrade::FormatApplyMode,
        result: Result<format_upgrade::FormatApplyOutcome, String>,
    },
}

impl LushtextWindow {
    /// Start the app-data preflight that gates normal startup metadata consumers.
    ///
    /// This runs filesystem work on a background thread. Current or missing v1
    /// metadata continues silently; actionable critical metadata presents a
    /// compatibility dialog before workspace/session/draft restore can run.
    pub(super) fn begin_startup_data_flow(&self) {
        let flow = &self.imp().startup_data_flow;
        if flow.completed.get() || flow.running.replace(true) {
            return;
        }

        spawn_blocking_then(
            self.clone(),
            || {
                let data_dir = json_store::data_dir();
                let inventory = format_upgrade::scan(&data_dir);
                format_upgrade::build_plan(&inventory)
            },
            |window, plan| {
                window.imp().startup_data_flow.running.set(false);
                if plan.requires_startup_decision() {
                    window.present_startup_format_dialog(&plan, None);
                } else {
                    window.continue_startup_data_flow();
                }
            },
        );
    }

    /// Release startup-gated metadata consumers after preflight or upgrade resolves.
    ///
    /// Runs on the GTK main thread; loads workspaces, flushes queued activation
    /// opens, restores session/drafts, and starts draft autosave.
    pub(super) fn continue_startup_data_flow(&self) {
        let flow = &self.imp().startup_data_flow;
        if flow.completed.replace(true) {
            return;
        }

        self.reconcile_pending_migrations_on_startup();
        self.imp().sidebar.load_workspaces();
        self.refresh_workspace_scope_consumers();
        self.flush_pending_activation_opens();
        self.load_session_and_drafts();
        self.start_autosave_timer();
    }

    /// Queue explicit desktop/CLI opens until the startup gate is resolved.
    pub(super) fn queue_activation_open_if_startup_pending(&self, path: &Path) -> bool {
        let flow = &self.imp().startup_data_flow;
        if flow.completed.get() {
            return false;
        }
        flow.pending_activation_paths
            .borrow_mut()
            .push(path.to_path_buf());
        true
    }

    fn flush_pending_activation_opens(&self) {
        let paths = self.imp().startup_data_flow.pending_activation_paths.take();
        for path in paths {
            self.open_document_from_activation(&path);
        }
    }

    fn present_startup_format_dialog(
        &self,
        plan: &format_upgrade::FormatPlan,
        previous_error: Option<&str>,
    ) {
        let future = plan.has_future_version_blocker();
        // Any future-version blocker makes Convert unsafe for the whole
        // startup gate; converting older files cannot make newer metadata
        // readable by this binary.
        let (heading, body) = startup_dialog_text(future);
        let details = startup_dialog_details(plan, previous_error, future);
        let dialog = libadwaita::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .extra_child(&details)
            .build();

        if future {
            dialog.add_response(RESPONSE_QUIT, "_Quit");
            dialog.add_response(RESPONSE_START_FRESH, "_Start Fresh");
            dialog.set_response_appearance(
                RESPONSE_START_FRESH,
                libadwaita::ResponseAppearance::Destructive,
            );
            dialog.set_default_response(Some(RESPONSE_QUIT));
            dialog.set_close_response(RESPONSE_QUIT);
        } else {
            dialog.add_response(RESPONSE_QUIT, "_Quit");
            dialog.add_response(RESPONSE_START_FRESH, "_Start Fresh");
            dialog.add_response(
                RESPONSE_CONVERT,
                if previous_error.is_some() {
                    "_Retry Convert"
                } else {
                    "_Convert"
                },
            );
            dialog.set_response_appearance(
                RESPONSE_START_FRESH,
                libadwaita::ResponseAppearance::Destructive,
            );
            dialog.set_response_appearance(
                RESPONSE_CONVERT,
                libadwaita::ResponseAppearance::Suggested,
            );
            dialog.set_default_response(Some(RESPONSE_CONVERT));
            dialog.set_close_response(RESPONSE_QUIT);
        }

        let window_weak = self.downgrade();
        // AlertDialog responses arrive through a GObject signal; this closure
        // disables duplicate responses and routes the selected startup action.
        dialog.connect_response(None::<&str>, move |dialog, response| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match response {
                RESPONSE_CONVERT => {
                    set_dialog_responses_enabled(dialog, false);
                    window.run_startup_format_apply(format_upgrade::FormatApplyMode::Convert);
                }
                RESPONSE_START_FRESH => {
                    set_dialog_responses_enabled(dialog, false);
                    window.run_startup_format_apply(format_upgrade::FormatApplyMode::StartFresh);
                }
                _ => quit_window_application(&window),
            }
        });
        dialog.present(Some(self));
    }

    fn run_startup_format_apply(&self, mode: format_upgrade::FormatApplyMode) {
        spawn_blocking_then(
            self.clone(),
            move || {
                // Re-scan in the worker instead of applying the dialog's
                // snapshot; app data may have changed while the dialog was
                // open, and apply needs fresh file facts.
                let data_dir = json_store::data_dir();
                let inventory = format_upgrade::scan(&data_dir);
                let plan = format_upgrade::build_plan(&inventory);
                if !plan.requires_startup_decision() {
                    return StartupFormatApplyWorkerResult::NoDecisionNeeded;
                }
                let result = match mode {
                    format_upgrade::FormatApplyMode::Convert => {
                        format_upgrade::apply_plan(&data_dir, &plan)
                    }
                    format_upgrade::FormatApplyMode::StartFresh => {
                        format_upgrade::start_fresh(&data_dir, &plan)
                    }
                }
                .map_err(|error| error.to_string());
                StartupFormatApplyWorkerResult::Applied { plan, mode, result }
            },
            |window, result| match result {
                StartupFormatApplyWorkerResult::NoDecisionNeeded => {
                    window.continue_startup_data_flow();
                }
                StartupFormatApplyWorkerResult::Applied {
                    mode,
                    result: Ok(outcome),
                    ..
                } if outcome.is_success() => {
                    match mode {
                        format_upgrade::FormatApplyMode::Convert => window.publish_status_message(
                            &format!("Updated {} data file(s)", outcome.converted_count),
                            MessageKind::Info,
                        ),
                        format_upgrade::FormatApplyMode::StartFresh => {
                            window.publish_status_message(
                                &format!("Preserved {} data file(s)", outcome.start_fresh_count),
                                MessageKind::Warning,
                            );
                        }
                    }
                    window.continue_startup_data_flow();
                }
                StartupFormatApplyWorkerResult::Applied {
                    plan,
                    result: Ok(outcome),
                    ..
                } => {
                    let detail = outcome.failures.first().map_or_else(
                        || "The data update did not complete.".to_string(),
                        |failure| format!("{}: {}", failure.path.display(), failure.detail),
                    );
                    window.present_startup_format_dialog(&plan, Some(&detail));
                }
                StartupFormatApplyWorkerResult::Applied {
                    plan,
                    result: Err(detail),
                    ..
                } => {
                    window.present_startup_format_dialog(&plan, Some(&detail));
                }
            },
        );
    }
}

fn startup_dialog_text(future: bool) -> (&'static str, &'static str) {
    if future {
        (
            "Data Was Created by a Newer LushText",
            "This app data was saved by a newer LushText, so this version cannot read it safely.",
        )
    } else {
        (
            "Older LushText Data Can Be Updated",
            "LushText found older app data with a supported upgrade path. It must be handled before restoring workspaces, sessions, drafts, notes, or undo state.",
        )
    }
}

/// Build the dialog's scannable grouped content instead of crowding the body text.
fn startup_dialog_details(
    plan: &format_upgrade::FormatPlan,
    previous_error: Option<&str>,
    future: bool,
) -> gtk4::Box {
    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(6)
        .build();

    let options_group = libadwaita::PreferencesGroup::builder()
        .title("Options")
        .build();
    if future {
        options_group.add(&startup_dialog_row(
            "Quit",
            "Close LushText without changing app data.",
        ));
        options_group.add(&startup_dialog_row("Start Fresh", start_fresh_summary()));
    } else {
        options_group.add(&startup_dialog_row(
            if previous_error.is_some() {
                "Retry Convert"
            } else {
                "Convert"
            },
            "Back up affected files, then update supported older data to the current format.",
        ));
        options_group.add(&startup_dialog_row("Start Fresh", start_fresh_summary()));
        options_group.add(&startup_dialog_row(
            "Quit",
            "Close LushText without changing app data.",
        ));
    }
    content.append(&options_group);

    if let Some(detail) = previous_error {
        let error_group = libadwaita::PreferencesGroup::builder()
            .title("Last Attempt")
            .build();
        error_group.add(&startup_dialog_row("Failed", detail));
        content.append(&error_group);
    }

    let affected_group = libadwaita::PreferencesGroup::builder()
        .title("Affected Data")
        .build();
    append_affected_data_rows(&affected_group, plan);
    content.append(&affected_group);

    content
}

fn append_affected_data_rows(
    affected_group: &libadwaita::PreferencesGroup,
    plan: &format_upgrade::FormatPlan,
) {
    let mut appended = false;
    for plan_group in &plan.groups {
        if plan_group.actions.is_empty() {
            continue;
        }
        let title = metadata_title(plan_group.metadata_kind.label());
        let subtitle = affected_data_summary(plan_group);
        affected_group.add(&startup_dialog_row(title, subtitle));
        appended = true;
    }
    if !appended {
        affected_group.add(&startup_dialog_row("Current", "No data files need action."));
    }
}

fn affected_data_summary(group: &format_upgrade::FormatPlanGroup) -> String {
    let mut convertible = 0;
    let mut future = 0;
    let mut recovery = 0;

    for planned in &group.actions {
        if matches!(
            planned.action,
            format_upgrade::FormatPlanAction::ConvertToLatest { .. }
        ) {
            convertible += 1;
        } else if planned.item.classification.is_future_version() {
            future += 1;
        } else {
            recovery += 1;
        }
    }

    let mut parts = Vec::new();
    if convertible > 0 {
        parts.push(format!(
            "{} can be converted to the current format",
            format_item_count(convertible)
        ));
    }
    if future > 0 {
        let verb = if future == 1 { "was" } else { "were" };
        parts.push(format!(
            "{} {verb} created by a newer LushText",
            format_item_count(future)
        ));
    }
    if recovery > 0 {
        parts.push(format!(
            "{} needs preservation or recovery before startup continues",
            format_item_count(recovery)
        ));
    }
    format!("{}.", parts.join("; "))
}

fn startup_dialog_row(
    title: impl Into<glib::GString>,
    subtitle: impl Into<glib::GString>,
) -> libadwaita::ActionRow {
    let row = libadwaita::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .title_lines(0)
        .subtitle_lines(0)
        .build();
    row.set_activatable(false);
    row.set_selectable(false);
    row
}

fn start_fresh_summary() -> String {
    format!(
        "Back up affected files in {}, remove them from active app data, and continue with current defaults.",
        format_upgrade::FORMAT_UPGRADE_BACKUP_DIR
    )
}

fn metadata_title(label: &str) -> String {
    label
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut title = first.to_ascii_uppercase().to_string();
            title.push_str(chars.as_str());
            title
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_item_count(count: usize) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{count} items")
    }
}

fn set_dialog_responses_enabled(dialog: &libadwaita::AlertDialog, enabled: bool) {
    for response in [RESPONSE_CONVERT, RESPONSE_START_FRESH, RESPONSE_QUIT] {
        if dialog.has_response(response) {
            dialog.set_response_enabled(response, enabled);
        }
    }
}

fn quit_window_application(window: &LushtextWindow) {
    if let Some(app) = window.application() {
        app.quit();
    } else {
        window.close();
    }
}
