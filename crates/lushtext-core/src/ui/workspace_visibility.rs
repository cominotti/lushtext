// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-side reader for the workspace entry visibility preference.
//!
//! The rule itself is GTK-free domain policy in
//! `model::workspace_visibility`; this module only turns the two GSettings
//! keys into that value once per refresh pass or rebuild, so services and
//! workers never touch `gio::Settings`.

use gtk4::gio;
use gtk4::prelude::*;

use crate::config::keys;
use crate::model::workspace_visibility::WorkspaceEntryVisibility;

/// Build the current visibility rule from the two persisted keys.
#[must_use]
pub(crate) fn workspace_entry_visibility(settings: &gio::Settings) -> WorkspaceEntryVisibility {
    WorkspaceEntryVisibility::new(
        settings.boolean(keys::WORKSPACE_SHOW_HIDDEN_FILES),
        excluded_names(settings),
    )
}

/// Read the persisted excluded-name array in stored order.
#[must_use]
pub(crate) fn excluded_names(settings: &gio::Settings) -> Vec<String> {
    settings
        .strv(keys::WORKSPACE_EXCLUDED_NAMES)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Persist a complete excluded-name array.
pub(crate) fn set_excluded_names(settings: &gio::Settings, names: &[String]) {
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    if let Err(error) = settings.set_strv(keys::WORKSPACE_EXCLUDED_NAMES, names.as_slice()) {
        tracing::error!("Failed to persist excluded names: {error}");
    }
}

/// Connect `handler` to changes of either visibility key.
///
/// Returns the two handler ids so the caller can own them in a `SignalBag`.
pub(crate) fn connect_visibility_changed<F>(
    settings: &gio::Settings,
    handler: F,
) -> [glib::SignalHandlerId; 2]
where
    F: Fn() + Clone + 'static,
{
    let show_hidden = {
        let handler = handler.clone();
        settings.connect_changed(Some(keys::WORKSPACE_SHOW_HIDDEN_FILES), move |_, _| {
            handler();
        })
    };
    let excluded = settings.connect_changed(Some(keys::WORKSPACE_EXCLUDED_NAMES), move |_, _| {
        handler();
    });
    [show_hidden, excluded]
}
