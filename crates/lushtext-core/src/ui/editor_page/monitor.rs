// SPDX-License-Identifier: GPL-3.0-or-later

//! External file-monitor wiring for one editor tab.
//!
//! This stays in the driving-adapter layer because it owns `gio::FileMonitor`
//! signals and the GTK-thread debounce logic that translates them into inline
//! editor notifications.

use std::time::Duration;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{async_task, editor_io};

use super::LushtextEditorPage;

impl LushtextEditorPage {
    /// Start watching the file for external modifications.
    pub fn start_file_monitor(&self) {
        self.stop_file_monitor();
        let Some(ref path) = *self.imp().file_path.borrow() else {
            return;
        };

        let file = gio::File::for_path(path);
        let monitor = match file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            Ok(monitor) => monitor,
            Err(error) => {
                tracing::warn!("Failed to start file monitor: {error}");
                return;
            }
        };

        let editor_weak = self.downgrade();
        monitor.connect_changed(move |_, _, _, event| {
            if !matches!(
                event,
                gio::FileMonitorEvent::Changed | gio::FileMonitorEvent::Created
            ) {
                return;
            }
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };

            let generation = editor
                .imp()
                .monitor
                .monitor_generation
                .get()
                .wrapping_add(1);
            editor.imp().monitor.monitor_generation.set(generation);

            let editor_weak = editor.downgrade();
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().monitor.monitor_generation.get() != generation {
                    return;
                }
                let Some(ref path) = *editor.imp().file_path.borrow() else {
                    return;
                };
                let last_known = editor.imp().monitor.last_known_mtime.get();
                if last_known.is_none() {
                    return;
                }
                let path = path.clone();
                async_task::spawn_blocking_then(
                    editor.clone(),
                    move || editor_io::mtime_secs(&path),
                    move |editor, current_mtime| {
                        if current_mtime != last_known {
                            editor.emit_inline_notification(InlineActionNotification {
                                style: InlineNotificationStyle::Warning,
                                title: "File Has Changed on Disk".to_string(),
                                body: "The file was modified by another program.".to_string(),
                                primary_button: Some("_Discard Changes and Reload".to_string()),
                                secondary_button: None,
                            });
                        }
                    },
                );
            });
        });

        *self.imp().monitor.file_monitor.borrow_mut() = Some(monitor);
    }

    /// Stop watching the file.
    pub fn stop_file_monitor(&self) {
        if let Some(monitor) = self.imp().monitor.file_monitor.take() {
            monitor.cancel();
        }
    }
}
