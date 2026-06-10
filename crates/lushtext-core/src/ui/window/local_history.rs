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
use libadwaita::prelude::{AdwDialogExt, SidebarItemExt};

use crate::model::local_history::{LocalHistorySnapshot, LocalHistorySnapshotMeta};
use crate::model::migration_ledger::MigrationKind;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{
    async_task, filesystem::metadata as fs_metadata, json_store, local_history_service,
    migration_ledger,
};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::{LushtextEditorPage, PendingWarningAction};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Leave a visible gutter around the local-history viewer so it still reads as
/// a parent-owned secondary surface instead of another primary window.
const LOCAL_HISTORY_VIEWER_PARENT_MARGIN_SP: i32 = 48;
/// Wide local-history browsing should use most of the parent width.
const LOCAL_HISTORY_VIEWER_WIDTH_FRACTION: f64 = 0.9;
/// Wide local-history browsing should use most of the parent height.
const LOCAL_HISTORY_VIEWER_HEIGHT_FRACTION: f64 = 0.88;
/// Wide local-history browsing should stay comfortably readable on desktops.
const LOCAL_HISTORY_VIEWER_MIN_WIDTH_SP: i32 = 1080;
/// Wide local-history browsing should stop growing once it already feels like a viewer.
const LOCAL_HISTORY_VIEWER_MAX_WIDTH_SP: i32 = 1680;
/// Wide local-history browsing should keep enough height for reading snapshot text.
const LOCAL_HISTORY_VIEWER_MIN_HEIGHT_SP: i32 = 720;
/// Wide local-history browsing should stop growing once the preview has ample height.
const LOCAL_HISTORY_VIEWER_MAX_HEIGHT_SP: i32 = 1080;
/// The snapshot list should stay readable without competing evenly with the preview.
const LOCAL_HISTORY_VIEWER_MIN_SIDEBAR_WIDTH_SP: f64 = 260.0;
/// The snapshot list should behave like a browse rail, not a co-equal pane.
const LOCAL_HISTORY_VIEWER_MAX_SIDEBAR_WIDTH_SP: f64 = 340.0;
/// Compact empty-history width mirrors the Notes empty browser so status pages
/// have a readable line length instead of collapsing to their natural text size.
const EMPTY_LOCAL_HISTORY_WIDTH_SP: i32 = 640;
/// Compact empty-history height fits the normal status-page icon, title, and
/// description without introducing a scrollbar.
const EMPTY_LOCAL_HISTORY_HEIGHT_SP: i32 = 480;

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
    /// Adwaita sidebar rail showing snapshots newest-first.
    sidebar: libadwaita::Sidebar,
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

/// Background outcome for opening local history from a path rather than an already-loaded tab.
enum LocalHistoryPathLoadOutcome {
    /// The target exceeds the editor's local-history size policy.
    Unavailable,
    /// Snapshot metadata was loaded and can be presented once the tab is selected.
    Loaded {
        /// Saved file path whose lineage was loaded.
        path: PathBuf,
        /// Snapshot metadata for the browser sidebar.
        snapshots: Vec<LocalHistorySnapshotMeta>,
        /// Recovery diagnostics found while loading the lineage.
        diagnostics: Vec<RecoveryDiagnostic>,
    },
    /// Snapshot metadata could not be read.
    Failed(String),
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
        if !editor.local_history_availability().allows_browsing() {
            self.publish_status_message(
                "Local history is unavailable for files above 50 MB",
                MessageKind::Warning,
            );
            return;
        }
        self.load_local_history_for_editor(editor, path);
    }

    /// Open local history for an explicit saved file path, selecting or opening its tab first.
    pub(super) fn show_local_history_for_path(&self, path: &Path) {
        let path = path.to_path_buf();
        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let availability = fs_metadata::file_facts(&path).ok().map_or(
                    local_history_service::LocalHistoryAvailability::Unavailable,
                    |facts| {
                        local_history_service::availability_for_size_check(
                            crate::services::file_limits::FileSizeCheck::classify(facts.byte_size),
                        )
                    },
                );
                if !availability.allows_browsing() {
                    return LocalHistoryPathLoadOutcome::Unavailable;
                }
                let data_dir = json_store::data_dir();
                match local_history_service::list_snapshots_for_path_recovering(&data_dir, &path) {
                    Ok(listing) => LocalHistoryPathLoadOutcome::Loaded {
                        path,
                        snapshots: listing.snapshots,
                        diagnostics: listing.diagnostics,
                    },
                    Err(error) => LocalHistoryPathLoadOutcome::Failed(error.to_string()),
                }
            },
            |window, result| match result {
                LocalHistoryPathLoadOutcome::Unavailable => {
                    window.publish_status_message(
                        "Local history is unavailable for files above 50 MB",
                        MessageKind::Warning,
                    );
                }
                LocalHistoryPathLoadOutcome::Loaded {
                    path,
                    snapshots,
                    diagnostics,
                } => {
                    window.open_document(&path);
                    let Some(editor) = window.active_editor() else {
                        window.publish_status_message(
                            "Local history could not find an editor for that file",
                            MessageKind::Warning,
                        );
                        return;
                    };
                    let editor_path = editor.file_path().unwrap_or(path);
                    window.present_local_history_browser(editor, editor_path, snapshots);
                    window.publish_local_history_recovery_diagnostics(&diagnostics);
                }
                LocalHistoryPathLoadOutcome::Failed(error) => {
                    tracing::error!("Failed to list local-history snapshots: {error}");
                    window.publish_status_message(
                        "Local history could not be loaded",
                        MessageKind::Error,
                    );
                }
            },
        );
    }

    /// Load snapshot metadata for an already-open eligible editor.
    fn load_local_history_for_editor(&self, editor: LushtextEditorPage, path: PathBuf) {
        async_task::spawn_blocking_then(
            (self.clone(), editor, path.clone()),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::list_snapshots_for_path_recovering(&data_dir, &path)
            },
            |(window, editor, path), result| match result {
                Ok(listing) => {
                    window.present_local_history_browser(editor, path, listing.snapshots);
                    window.publish_local_history_recovery_diagnostics(&listing.diagnostics);
                }
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

    fn publish_local_history_recovery_diagnostics(&self, diagnostics: &[RecoveryDiagnostic]) {
        if diagnostics.is_empty() {
            return;
        }
        for diagnostic in diagnostics {
            tracing::warn!("{}", diagnostic.summary());
        }
        self.publish_status_message(
            "Some local-history metadata needed recovery",
            MessageKind::Warning,
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
                let generation = migration_ledger::record_pending(
                    &data_dir,
                    &old_for_move,
                    &new_for_move,
                    &[MigrationKind::LocalHistory],
                )?;
                migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::LocalHistory,
                    || {
                        local_history_service::move_path_tree(
                            &data_dir,
                            &old_for_move,
                            &new_for_move,
                        )
                    },
                )
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
        let snapshots = filter_visible_local_history_snapshots(snapshots);
        if snapshots.is_empty() {
            Self::build_empty_local_history_dialog(&path).present(Some(self));
            return;
        }

        let (dialog_width, dialog_height) = local_history_viewer_dialog_size(self);
        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(dialog_width)
            .content_height(dialog_height)
            // Keep the viewer at the configured desktop-scale size instead of
            // shrinking back down to the child widget's natural request.
            .follows_content_size(false)
            .build();

        let sidebar = libadwaita::Sidebar::new();
        sidebar.set_accessible_role(gtk4::AccessibleRole::List);
        sidebar.set_mode(libadwaita::SidebarMode::Sidebar);
        sidebar.set_vexpand(true);
        sidebar.update_property(&[
            gtk4::accessible::Property::Label("Local history snapshots"),
            gtk4::accessible::Property::Description(
                "Choose a saved snapshot for the active document",
            ),
        ]);

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
        preview_stack.add_named(&empty_snapshot_widget(), Some("empty"));
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
        restore_button.update_property(&[gtk4::accessible::Property::Label(
            "Restore selected snapshot",
        )]);
        let copy_button = gtk4::Button::with_label("Copy");
        copy_button.set_sensitive(false);
        copy_button.update_property(&[gtk4::accessible::Property::Label("Copy selected snapshot")]);

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Snapshots")
            .visible(false)
            .build();
        back_button.update_property(&[gtk4::accessible::Property::Label("Back to snapshots")]);

        let split_view = libadwaita::NavigationSplitView::new();
        split_view.set_min_sidebar_width(LOCAL_HISTORY_VIEWER_MIN_SIDEBAR_WIDTH_SP);
        split_view.set_max_sidebar_width(LOCAL_HISTORY_VIEWER_MAX_SIDEBAR_WIDTH_SP);
        split_view.set_sidebar(Some(&libadwaita::NavigationPage::new(
            &build_history_sidebar(&path, &sidebar),
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
            sidebar,
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

        populate_history_sidebar(&state);
        state.back_button.connect_clicked({
            let state = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = state.upgrade() {
                    state.split_view.set_show_content(false);
                }
            }
        });
        // Collapsed adaptive navigation owns back-button visibility. The
        // binding seeds the initial dialog layout and stays live as breakpoints
        // change without storing a signal handler ID.
        state
            .split_view
            .bind_property("collapsed", &state.back_button, "visible")
            .sync_create()
            .build();
        state.copy_button.connect_clicked({
            let state = Rc::downgrade(&state);
            move |_| {
                let Some(state) = state.upgrade() else {
                    return;
                };
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
            let state = Rc::downgrade(&state);
            move |_| {
                let Some(state) = state.upgrade() else {
                    return;
                };
                let Some(snapshot) = state.loaded_snapshot.borrow().clone() else {
                    return;
                };
                state.restore_button.set_sensitive(false);
                state.copy_button.set_sensitive(false);
                LushtextWindow::restore_local_history_snapshot(Rc::clone(&state), snapshot);
            }
        });

        state.sidebar.set_selected(0);
        state.load_preview_for_index(0, false);
        state.sidebar.connect_selected_item_notify({
            let state = Rc::downgrade(&state);
            move |sidebar| {
                if let Some(state) = state.upgrade()
                    && let Some(index) = history_sidebar_item_index(sidebar.selected_item())
                {
                    state.load_preview_for_index(index, true);
                }
            }
        });
        state.sidebar.connect_activated({
            let state = Rc::downgrade(&state);
            move |_sidebar, index| {
                if let Some(state) = state.upgrade()
                    && let Ok(index) = usize::try_from(index)
                {
                    state.load_preview_for_index(index, true);
                }
            }
        });

        // The dialog owns this holder while it is visible, keeping browser
        // state alive without child-widget signal closures strongly owning the
        // whole dialog subtree. The `closed` signal drops the state and breaks
        // the temporary dialog -> holder -> state -> dialog cycle.
        let state_holder = Rc::new(RefCell::new(Some(Rc::clone(&state))));
        state.dialog.connect_closed({
            let state_holder = Rc::clone(&state_holder);
            move |_| {
                state_holder.borrow_mut().take();
            }
        });

        state.dialog.present(Some(self));
    }

    fn build_empty_local_history_dialog(path: &Path) -> libadwaita::Dialog {
        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(EMPTY_LOCAL_HISTORY_WIDTH_SP)
            .content_height(EMPTY_LOCAL_HISTORY_HEIGHT_SP)
            // `AdwStatusPage` has a narrow natural request; following content size
            // would collapse this empty-state browser instead of using the target.
            .follows_content_size(false)
            .build();

        let status = libadwaita::StatusPage::builder()
            .icon_name("document-open-recent-symbolic")
            .title("No local history yet")
            .description(format!(
                "{}\n\nSaved snapshots will appear after you edit or save this document.",
                path.display()
            ))
            .build();
        status.set_hexpand(true);
        status.set_vexpand(true);
        dialog.set_child(Some(&status));
        dialog
    }

    fn restore_local_history_snapshot(
        browser: Rc<LocalHistoryBrowserState>,
        snapshot: LocalHistorySnapshot,
    ) {
        let buffer = browser.editor.buffer();
        let restore_text = snapshot.text;
        let run_restore = move |undo_text: String| {
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
                    state.browser.window.resolve_notes_for_editor(
                        &state.browser.editor,
                        state.browser.path.as_path(),
                    );
                    state
                        .browser
                        .editor
                        .emit_inline_notification_with_warning_action(
                            InlineActionNotification {
                                style: InlineNotificationStyle::Warning,
                                title: "Restored from Local History".to_string(),
                                body: "The previous buffer state was saved as a safety snapshot. Use Undo Restore to switch back immediately.".to_string(),
                                primary_button: Some("Undo Restore".to_string()),
                                secondary_button: None,
                            },
                            PendingWarningAction::UndoLocalHistoryRestore,
                        );
                    state.browser.window.publish_status_message(
                        "Snapshot restored into the editor",
                        MessageKind::Info,
                    );
                    state.browser.window.refresh_status_bar();
                    state.browser.dialog.close();
                },
            );
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
            buffer_snapshot::snapshot_buffer_text_async(buffer, run_restore);
        } else {
            run_restore(buffer_snapshot::snapshot_buffer_text_direct(&buffer));
        }
    }
}

impl LocalHistoryBrowserState {
    fn load_preview_for_index(self: &Rc<Self>, index: usize, user_selected: bool) {
        let Some(meta) = self.snapshots.get(index).cloned() else {
            return;
        };

        self.loaded_snapshot.borrow_mut().take();
        self.preview_title.set_label("Loading snapshot…");
        self.preview_meta
            .set_label(&format_snapshot_meta(meta.origin, meta.byte_len));
        self.preview_buffer.set_text("");
        self.preview_stack.set_visible_child_name("loading");
        self.restore_button.set_sensitive(false);
        self.copy_button.set_sensitive(false);

        if user_selected {
            // `show-content` is only visible while collapsed, but setting it
            // before the adaptive layout settles preserves the user's
            // navigation request during resize and widget-test transitions.
            self.split_view.set_show_content(true);
        }

        let generation = self.preview_generation.get().wrapping_add(1);
        self.preview_generation.set(generation);
        async_task::spawn_blocking_then(
            Rc::clone(self),
            {
                let path = self.path.clone();
                let snapshot_id = meta.snapshot_id;
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
                        state.preview_meta.set_label(&format_snapshot_meta(
                            snapshot.meta.origin,
                            snapshot.meta.byte_len,
                        ));
                        if snapshot.text.is_empty() {
                            state.preview_buffer.set_text("");
                            state.preview_stack.set_visible_child_name("empty");
                            state.copy_button.set_sensitive(false);
                        } else {
                            state.preview_buffer.set_text(&snapshot.text);
                            state.preview_stack.set_visible_child_name("content");
                            state.copy_button.set_sensitive(true);
                        }
                        state.loaded_snapshot.replace(Some(snapshot));
                        state.restore_button.set_sensitive(true);
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

/// Compute the current main-window size, falling back to the configured default
/// geometry before the window is mapped in widget tests or at startup.
fn current_window_size(window: &LushtextWindow) -> (i32, i32) {
    let (default_width, default_height) = window.default_size();
    (
        current_window_dimension(window.width(), default_width),
        current_window_dimension(window.height(), default_height),
    )
}

/// Clamp one dialog axis so the viewer uses most of the parent window without
/// outgrowing it on either small or large desktops.
#[expect(
    clippy::cast_possible_truncation,
    reason = "The proportional viewer size is clamped back into GTK i32 geometry bounds"
)]
fn parent_relative_dialog_axis_size(
    parent_axis: i32,
    target_fraction: f64,
    min_axis: i32,
    max_axis: i32,
) -> i32 {
    let parent_axis = parent_axis.max(1);
    let bounded_parent = (parent_axis - LOCAL_HISTORY_VIEWER_PARENT_MARGIN_SP).max(1);
    let proportional = (f64::from(parent_axis) * target_fraction).round() as i32;
    proportional.clamp(min_axis, max_axis).min(bounded_parent)
}

/// Size the populated local-history browser like a large viewer while keeping
/// the dialog visibly smaller than its parent window.
fn local_history_viewer_dialog_size(window: &LushtextWindow) -> (i32, i32) {
    let (parent_width, parent_height) = current_window_size(window);
    (
        parent_relative_dialog_axis_size(
            parent_width,
            LOCAL_HISTORY_VIEWER_WIDTH_FRACTION,
            LOCAL_HISTORY_VIEWER_MIN_WIDTH_SP,
            LOCAL_HISTORY_VIEWER_MAX_WIDTH_SP,
        ),
        parent_relative_dialog_axis_size(
            parent_height,
            LOCAL_HISTORY_VIEWER_HEIGHT_FRACTION,
            LOCAL_HISTORY_VIEWER_MIN_HEIGHT_SP,
            LOCAL_HISTORY_VIEWER_MAX_HEIGHT_SP,
        ),
    )
}

/// Resolve one current-vs-default axis without forcing callers to repeat the
/// same width/height fallback logic.
fn current_window_dimension(current_axis: i32, default_axis: i32) -> i32 {
    if current_axis > 0 {
        current_axis
    } else {
        default_axis.max(1)
    }
}

fn populate_history_sidebar(state: &LocalHistoryBrowserState) {
    let section = libadwaita::SidebarSection::new();
    section.set_title(Some("Snapshots"));
    for meta in &state.snapshots {
        section.append(history_sidebar_item(meta));
    }
    state.sidebar.append(section);
}

fn history_sidebar_item(meta: &LocalHistorySnapshotMeta) -> libadwaita::SidebarItem {
    let title = format_history_time(meta.captured_at_millis);
    let subtitle = format_snapshot_meta(meta.origin, meta.byte_len);
    libadwaita::SidebarItem::builder()
        .title(title)
        .subtitle(subtitle.clone())
        .tooltip(subtitle)
        .icon_name("document-open-recent-symbolic")
        .build()
}

fn history_sidebar_item_index(item: Option<libadwaita::SidebarItem>) -> Option<usize> {
    item.and_then(|item| usize::try_from(item.index()).ok())
}

/// Hide legacy empty baseline rows that were repeatedly created by the older
/// draft-restore workflow while leaving the stored history untouched on disk.
fn filter_visible_local_history_snapshots(
    snapshots: Vec<LocalHistorySnapshotMeta>,
) -> Vec<LocalHistorySnapshotMeta> {
    let empty_baseline_count = snapshots
        .iter()
        .filter(|meta| is_empty_baseline_snapshot(meta))
        .count();
    let non_empty_periodic_count = snapshots
        .iter()
        .filter(|meta| {
            meta.origin == crate::model::local_history::LocalHistorySnapshotOrigin::Periodic
                && meta.byte_len > 0
        })
        .count();

    snapshots
        .into_iter()
        .filter(|meta| {
            !should_hide_legacy_empty_baseline(meta, empty_baseline_count, non_empty_periodic_count)
        })
        .collect()
}

fn should_hide_legacy_empty_baseline(
    meta: &LocalHistorySnapshotMeta,
    empty_baseline_count: usize,
    non_empty_periodic_count: usize,
) -> bool {
    is_empty_baseline_snapshot(meta) && empty_baseline_count >= 2 && non_empty_periodic_count >= 2
}

fn is_empty_baseline_snapshot(meta: &LocalHistorySnapshotMeta) -> bool {
    meta.origin == crate::model::local_history::LocalHistorySnapshotOrigin::Baseline
        && meta.byte_len == 0
}

fn build_history_sidebar(path: &Path, sidebar: &libadwaita::Sidebar) -> gtk4::Box {
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
        .child(sidebar)
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

fn loading_preview_widget() -> gtk4::Widget {
    let label = gtk4::Label::new(Some("Loading preview…"));
    label.set_hexpand(true);
    label.set_vexpand(true);
    label.set_halign(gtk4::Align::Center);
    label.set_valign(gtk4::Align::Center);
    label.upcast()
}

fn empty_snapshot_widget() -> gtk4::Widget {
    libadwaita::StatusPage::builder()
        .icon_name("document-new-symbolic")
        .title("This snapshot was empty")
        .description(
            "No text had been saved at this point. For “Before edits” entries, this can mean the file was empty before the current unsaved changes began.",
        )
        .build()
        .upcast()
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

fn format_snapshot_meta(
    origin: crate::model::local_history::LocalHistorySnapshotOrigin,
    byte_len: u64,
) -> String {
    if byte_len == 0 {
        format!("{} · Empty file", origin.label())
    } else {
        format!("{} · {}", origin.label(), format_bytes(byte_len))
    }
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
