// SPDX-License-Identifier: GPL-3.0-or-later

//! Local-history browser, restore, and rename-migration workflows.
//!
//! Automatic capture stays tab-local in `ui/editor_page/`, while this window
//! workflow owns the deliberate browse surface, action availability, restore
//! safety messaging, and lineage migration after sidebar renames.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use crate::model::local_history::{LocalHistorySnapshot, LocalHistorySnapshotMeta};
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{async_task, json_store, local_history_service};
use crate::ui::editor_page::{LushtextEditorPage, PendingWarningAction};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// UI state for one open local-history browser dialog.
struct LocalHistoryBrowserState {
    /// Window that owns the dialog and receives status updates.
    window: LushtextWindow,
    /// Active editor the browser belongs to.
    editor: LushtextEditorPage,
    /// Saved path whose lineage is being browsed.
    path: PathBuf,
    /// Dialog containing the browser widgets.
    dialog: libadwaita::Dialog,
    /// Adaptive split view used for wide and narrow dialog layouts.
    split_view: libadwaita::NavigationSplitView,
    /// Snapshot list shown newest-first.
    list_box: gtk4::ListBox,
    /// Header label for the selected snapshot.
    preview_title: gtk4::Label,
    /// Secondary metadata label for the selected snapshot.
    preview_meta: gtk4::Label,
    /// Buffer backing the read-only preview text view.
    preview_buffer: gtk4::TextBuffer,
    /// Stack switching between loading, error, and content preview states.
    preview_stack: gtk4::Stack,
    /// Restore action for the selected snapshot.
    restore_button: gtk4::Button,
    /// Copy action for the selected snapshot text.
    copy_button: gtk4::Button,
    /// Back button shown when the adaptive split view collapses.
    back_button: gtk4::Button,
    /// Snapshot metadata backing the current list rows.
    snapshots: Vec<LocalHistorySnapshotMeta>,
    /// Last fully loaded snapshot preview.
    loaded_snapshot: RefCell<Option<LocalHistorySnapshot>>,
    /// Generation counter suppressing stale preview loads when selection changes quickly.
    preview_generation: Cell<u32>,
}

/// State passed through the restore-safety background capture.
struct RestoreWorkState {
    /// Browser widgets that should be updated when the safety snapshot finishes.
    browser: Rc<LocalHistoryBrowserState>,
    /// Current buffer text saved for the immediate undo affordance.
    undo_text: String,
    /// Historical text that should replace the buffer on success.
    restore_text: String,
}

impl LushtextWindow {
    /// Open the local-history browser for the active saved document.
    pub(super) fn show_local_history_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let Some(path) = editor.file_path() else {
            self.publish_status_message(
                "Local history requires a saved file",
                MessageKind::Warning,
            );
            return;
        };
        self.show_local_history_for_path(&path);
    }

    /// Open local history for an explicit saved file path, selecting or opening its tab first.
    pub(super) fn show_local_history_for_path(&self, path: &Path) {
        let availability = local_history_availability_for_path(path);
        if !availability.allows_browsing() {
            self.publish_status_message(
                "Local history is unavailable for files above 50 MB",
                MessageKind::Warning,
            );
            return;
        }

        self.open_document(path);
        let Some(editor) = self.active_editor() else {
            self.publish_status_message(
                "Local history could not find an editor for that file",
                MessageKind::Warning,
            );
            return;
        };
        let Some(editor_path) = editor.file_path() else {
            self.publish_status_message(
                "Local history requires a saved file",
                MessageKind::Warning,
            );
            return;
        };

        async_task::spawn_blocking_then(
            (self.clone(), editor, editor_path.clone()),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::list_snapshots_for_path(&data_dir, &editor_path)
            },
            |(window, editor, path), result| match result {
                Ok(snapshots) => window.present_local_history_browser(editor, path, snapshots),
                Err(error) => {
                    tracing::error!("Failed to list local-history snapshots: {error}");
                    window.publish_status_message(
                        "Local history could not be loaded",
                        MessageKind::Error,
                    );
                }
            },
        );
    }

    /// Recompute whether the local-history action should be enabled.
    pub(super) fn update_local_history_action(&self) {
        if let Some(action) = self.lookup_action("show-local-history")
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            let enabled = self.active_editor().is_some_and(|editor| {
                editor.file_path().is_some()
                    && editor.local_history_availability().allows_browsing()
            });
            simple.set_enabled(enabled);
        }
    }

    /// Migrate local-history lineages after an in-app sidebar rename.
    pub(super) fn migrate_local_history_after_rename(&self, old_path: &Path, new_path: &Path) {
        let old_path = old_path.to_path_buf();
        let new_path = new_path.to_path_buf();
        let old_for_move = old_path.clone();
        let new_for_move = new_path.clone();
        let window_weak = self.downgrade();
        async_task::spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::move_path_tree(&data_dir, &old_for_move, &new_for_move)
            },
            move |(), result| {
                if let Err(error) = result {
                    tracing::error!(
                        "Failed to migrate local history for {} -> {}: {error}",
                        old_path.display(),
                        new_path.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename succeeded, but local history could not be moved",
                            MessageKind::Warning,
                        );
                    }
                }
            },
        );
    }

    /// Apply the browser's immediate undo affordance after a restore.
    pub(super) fn undo_local_history_restore(&self, editor: &LushtextEditorPage) {
        let Some(undo_text) = editor.take_local_history_restore_undo_text() else {
            self.publish_status_message(
                "There is no local-history restore to undo",
                MessageKind::Warning,
            );
            return;
        };

        editor.replace_buffer_with_local_history_text(&undo_text);
        if let Some(path) = editor.file_path() {
            self.resolve_notes_for_editor(editor, &path);
        }
        self.dismiss_editor_notifications(editor);
        self.publish_status_message("Local-history restore undone", MessageKind::Info);
        self.refresh_status_bar();
    }

    fn present_local_history_browser(
        &self,
        editor: LushtextEditorPage,
        path: PathBuf,
        snapshots: Vec<LocalHistorySnapshotMeta>,
    ) {
        if snapshots.is_empty() {
            Self::build_empty_local_history_dialog(&path).present(Some(self));
            return;
        }

        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(1120)
            .content_height(760)
            .follows_content_size(true)
            .build();

        let list_box = gtk4::ListBox::new();
        list_box.set_selection_mode(gtk4::SelectionMode::Single);
        list_box.add_css_class("boxed-list");

        let preview_title = gtk4::Label::new(Some("Loading snapshot…"));
        preview_title.set_halign(gtk4::Align::Start);
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-4");

        let preview_meta = gtk4::Label::new(None);
        preview_meta.set_halign(gtk4::Align::Start);
        preview_meta.set_xalign(0.0);
        preview_meta.add_css_class("dim-label");
        preview_meta.set_wrap(true);

        let preview_buffer = gtk4::TextBuffer::new(None);
        let preview_view = gtk4::TextView::new();
        preview_view.set_buffer(Some(&preview_buffer));
        preview_view.set_editable(false);
        preview_view.set_cursor_visible(false);
        preview_view.set_wrap_mode(gtk4::WrapMode::None);
        preview_view.add_css_class("monospace");
        // Dialog shell margins do not pad the document itself, so the preview
        // text view needs its own inner spacing to avoid rendering flush
        // against the scrolled frame.
        preview_view.set_left_margin(14);
        preview_view.set_right_margin(14);
        preview_view.set_top_margin(12);
        preview_view.set_bottom_margin(12);

        let preview_stack = gtk4::Stack::new();
        preview_stack.set_hexpand(true);
        preview_stack.set_vexpand(true);
        preview_stack.add_named(&loading_preview_widget(), Some("loading"));
        preview_stack.add_named(&preview_error_widget("Preview unavailable"), Some("error"));
        let preview_scroll = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&preview_view)
            .build();
        preview_stack.add_named(&preview_scroll, Some("content"));
        preview_stack.set_visible_child_name("loading");

        let restore_button = gtk4::Button::with_label("Restore");
        restore_button.add_css_class("suggested-action");
        restore_button.set_sensitive(false);
        let copy_button = gtk4::Button::with_label("Copy");
        copy_button.set_sensitive(false);

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Snapshots")
            .visible(false)
            .build();

        let split_view = libadwaita::NavigationSplitView::new();
        split_view.set_min_sidebar_width(300.0);
        split_view.set_max_sidebar_width(420.0);
        split_view.set_sidebar(Some(&libadwaita::NavigationPage::new(
            &build_history_sidebar(&path, &list_box),
            "Snapshots",
        )));
        split_view.set_content(Some(&libadwaita::NavigationPage::new(
            &build_history_preview_page(
                &back_button,
                &preview_title,
                &preview_meta,
                &preview_stack,
                &copy_button,
                &restore_button,
            ),
            "Preview",
        )));
        split_view.set_show_content(false);
        dialog.set_child(Some(&split_view));

        let state = Rc::new(LocalHistoryBrowserState {
            window: self.clone(),
            editor,
            path,
            dialog,
            split_view,
            list_box,
            preview_title,
            preview_meta,
            preview_buffer,
            preview_stack,
            restore_button,
            copy_button,
            back_button,
            snapshots,
            loaded_snapshot: RefCell::new(None),
            preview_generation: Cell::new(0),
        });

        populate_history_rows(&state);
        state.back_button.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                state.split_view.set_show_content(false);
            }
        });
        state.split_view.connect_collapsed_notify({
            let state = Rc::clone(&state);
            move |split| {
                state.back_button.set_visible(split.is_collapsed());
            }
        });
        state.copy_button.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let Some(snapshot) = state.loaded_snapshot.borrow().clone() else {
                    return;
                };
                gtk4::prelude::RootExt::display(&state.window)
                    .clipboard()
                    .set_text(&snapshot.text);
                state
                    .window
                    .publish_status_message("Snapshot copied to the clipboard", MessageKind::Info);
            }
        });
        state.restore_button.connect_clicked({
            let state = Rc::clone(&state);
            move |_| {
                let Some(snapshot) = state.loaded_snapshot.borrow().clone() else {
                    return;
                };
                state.restore_button.set_sensitive(false);
                state.copy_button.set_sensitive(false);
                LushtextWindow::restore_local_history_snapshot(Rc::clone(&state), snapshot);
            }
        });

        if let Some(first_row) = state.list_box.row_at_index(0) {
            state.list_box.select_row(Some(&first_row));
            state.load_preview_for_row(&first_row, false);
        }
        state.list_box.connect_row_selected({
            let state = Rc::clone(&state);
            move |_list, row| {
                let Some(row) = row else { return };
                state.load_preview_for_row(row, true);
            }
        });
        state.list_box.connect_row_activated({
            let state = Rc::clone(&state);
            move |_list, row| {
                state.load_preview_for_row(row, true);
            }
        });

        state.dialog.present(Some(self));
    }

    fn build_empty_local_history_dialog(path: &Path) -> libadwaita::Dialog {
        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(560)
            .content_height(360)
            .follows_content_size(true)
            .build();

        let status = libadwaita::StatusPage::builder()
            .icon_name("document-open-recent-symbolic")
            .title("No local history yet")
            .description(format!(
                "{}\n\nSaved snapshots will appear after you edit or save this document.",
                path.display()
            ))
            .build();
        dialog.set_child(Some(&status));
        dialog
    }

    fn restore_local_history_snapshot(
        browser: Rc<LocalHistoryBrowserState>,
        snapshot: LocalHistorySnapshot,
    ) {
        let buffer = browser.editor.buffer();
        let undo_text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        let restore_text = snapshot.text;
        let path = browser.path.clone();

        async_task::spawn_blocking_then(
            RestoreWorkState {
                browser,
                undo_text: undo_text.clone(),
                restore_text,
            },
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::capture_snapshot_for_path(
                    &data_dir,
                    &path,
                    &undo_text,
                    crate::model::local_history::LocalHistorySnapshotOrigin::RestoreSafety,
                    crate::services::local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
                )
            },
            move |state, result| {
                if let Err(error) = result {
                    tracing::error!("Failed to capture local-history safety snapshot: {error}");
                    state.browser.restore_button.set_sensitive(true);
                    state.browser.copy_button.set_sensitive(true);
                    state.browser.window.publish_status_message(
                        "Local history restore could not be prepared safely",
                        MessageKind::Error,
                    );
                    return;
                }

                state
                    .browser
                    .editor
                    .set_local_history_restore_undo_text(Some(state.undo_text));
                state
                    .browser
                    .editor
                    .replace_buffer_with_local_history_text(&state.restore_text);
                state
                    .browser
                    .window
                    .dismiss_editor_notifications(&state.browser.editor);
                state
                    .browser
                    .window
                    .resolve_notes_for_editor(&state.browser.editor, state.browser.path.as_path());
                state.browser.editor.emit_inline_notification_with_warning_action(
                    InlineActionNotification {
                        style: InlineNotificationStyle::Warning,
                        title: "Restored from Local History".to_string(),
                        body: "The previous buffer state was saved as a safety snapshot. Use Undo Restore to switch back immediately.".to_string(),
                        primary_button: Some("Undo Restore".to_string()),
                        secondary_button: None,
                    },
                    PendingWarningAction::UndoLocalHistoryRestore,
                );
                state
                    .browser
                    .window
                    .publish_status_message("Snapshot restored into the editor", MessageKind::Info);
                state.browser.window.refresh_status_bar();
                state.browser.dialog.close();
            },
        );
    }
}

impl LocalHistoryBrowserState {
    fn load_preview_for_row(self: &Rc<Self>, row: &gtk4::ListBoxRow, user_selected: bool) {
        let index = row.index();
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(meta) = self.snapshots.get(index).cloned() else {
            return;
        };

        self.loaded_snapshot.borrow_mut().take();
        self.preview_title.set_label("Loading snapshot…");
        self.preview_meta.set_label(&format!(
            "{} · {}",
            meta.origin.label(),
            format_bytes(meta.byte_len)
        ));
        self.preview_buffer.set_text("");
        self.preview_stack.set_visible_child_name("loading");
        self.restore_button.set_sensitive(false);
        self.copy_button.set_sensitive(false);

        if user_selected && self.split_view.is_collapsed() {
            self.split_view.set_show_content(true);
        }

        let generation = self.preview_generation.get().wrapping_add(1);
        self.preview_generation.set(generation);
        async_task::spawn_blocking_then(
            Rc::clone(self),
            {
                let path = self.path.clone();
                let snapshot_id = meta.snapshot_id.clone();
                move || {
                    let data_dir = json_store::data_dir();
                    local_history_service::load_snapshot_for_path(&data_dir, &path, &snapshot_id)
                }
            },
            move |state, result| {
                if state.preview_generation.get() != generation {
                    return;
                }

                match result {
                    Ok(Some(snapshot)) => {
                        state
                            .preview_title
                            .set_label(&format_history_time(snapshot.meta.captured_at_millis));
                        state.preview_meta.set_label(&format!(
                            "{} · {}",
                            snapshot.meta.origin.label(),
                            format_bytes(snapshot.meta.byte_len)
                        ));
                        state.preview_buffer.set_text(&snapshot.text);
                        state.preview_stack.set_visible_child_name("content");
                        state.loaded_snapshot.replace(Some(snapshot));
                        state.restore_button.set_sensitive(true);
                        state.copy_button.set_sensitive(true);
                    }
                    Ok(None) => {
                        state.preview_title.set_label("Snapshot missing");
                        state.preview_meta.set_label("");
                        state.preview_stack.set_visible_child_name("error");
                    }
                    Err(error) => {
                        tracing::error!("Failed to load local-history preview: {error}");
                        state.preview_title.set_label("Preview unavailable");
                        state.preview_meta.set_label("");
                        state.preview_stack.set_visible_child_name("error");
                    }
                }
            },
        );
    }
}

fn populate_history_rows(state: &LocalHistoryBrowserState) {
    for meta in &state.snapshots {
        let row = gtk4::ListBoxRow::new();
        row.set_selectable(true);
        row.set_activatable(true);
        row.set_child(Some(&history_row_widget(meta)));
        state.list_box.append(&row);
    }
}

fn build_history_sidebar(path: &Path, list_box: &gtk4::ListBox) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);

    let title = gtk4::Label::new(Some("Snapshots"));
    title.set_halign(gtk4::Align::Start);
    title.set_xalign(0.0);
    title.add_css_class("title-4");
    content.append(&title);

    let subtitle = gtk4::Label::new(Some(&path.display().to_string()));
    subtitle.set_halign(gtk4::Align::Start);
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    content.append(&subtitle);

    let scroll = gtk4::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(list_box)
        .build();
    content.append(&scroll);

    content
}

fn build_history_preview_page(
    back_button: &gtk4::Button,
    preview_title: &gtk4::Label,
    preview_meta: &gtk4::Label,
    preview_stack: &gtk4::Stack,
    copy_button: &gtk4::Button,
    restore_button: &gtk4::Button,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(back_button);

    let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    title_box.append(preview_title);
    title_box.append(preview_meta);
    header.append(&title_box);
    content.append(&header);

    content.append(preview_stack);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    actions.set_halign(gtk4::Align::End);
    actions.append(copy_button);
    actions.append(restore_button);
    content.append(&actions);

    content
}

fn history_row_widget(meta: &LocalHistorySnapshotMeta) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.set_margin_top(10);
    content.set_margin_bottom(10);

    let title = gtk4::Label::new(Some(&format_history_time(meta.captured_at_millis)));
    title.set_halign(gtk4::Align::Start);
    title.set_xalign(0.0);
    title.add_css_class("heading");
    content.append(&title);

    let subtitle = gtk4::Label::new(Some(&format!(
        "{} · {}",
        meta.origin.label(),
        format_bytes(meta.byte_len)
    )));
    subtitle.set_halign(gtk4::Align::Start);
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("dim-label");
    subtitle.set_wrap(true);
    content.append(&subtitle);

    content
}

fn loading_preview_widget() -> gtk4::Widget {
    let label = gtk4::Label::new(Some("Loading preview…"));
    label.set_hexpand(true);
    label.set_vexpand(true);
    label.set_halign(gtk4::Align::Center);
    label.set_valign(gtk4::Align::Center);
    label.upcast()
}

fn preview_error_widget(title: &str) -> gtk4::Widget {
    libadwaita::StatusPage::builder()
        .icon_name("dialog-warning-symbolic")
        .title(title)
        .description("This snapshot could not be loaded right now.")
        .build()
        .upcast()
}

fn format_history_time(captured_at_millis: u64) -> String {
    glib::DateTime::from_unix_local((captured_at_millis / 1000) as i64)
        .ok()
        .map_or_else(
            || "Unknown time".to_string(),
            |datetime| {
                datetime.format("%Y-%m-%d %H:%M").map_or_else(
                    |_| "Unknown time".to_string(),
                    |formatted| formatted.to_string(),
                )
            },
        )
}

fn format_bytes(byte_len: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;

    if byte_len >= MIB {
        format!("{:.1} MB", byte_len as f64 / MIB as f64)
    } else if byte_len >= KIB {
        format!("{:.1} KB", byte_len as f64 / KIB as f64)
    } else {
        format!("{byte_len} B")
    }
}

fn local_history_availability_for_path(
    path: &Path,
) -> local_history_service::LocalHistoryAvailability {
    std::fs::metadata(path).ok().map_or(
        local_history_service::LocalHistoryAvailability::Unavailable,
        |metadata| {
            local_history_service::availability_for_size_check(
                crate::services::file_limits::FileSizeCheck::classify(metadata.len()),
            )
        },
    )
}
