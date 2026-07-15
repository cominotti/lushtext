// SPDX-License-Identifier: GPL-3.0-or-later

//! Local-history browser, restore, and rename-migration workflows.
//!
//! Automatic capture stays tab-local in `ui/editor_page/`, while this window
//! workflow owns the deliberate browse surface, action availability, restore
//! safety messaging, and lineage migration after sidebar renames.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::AtomicU64;
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, SidebarItemExt};

use crate::model::buffer_replacement::{
    REPLACEMENT_INSERT_SLICE_BYTES, SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES,
    next_replacement_boundary,
};
use crate::model::local_history::{LocalHistorySnapshot, LocalHistorySnapshotMeta};
use crate::model::migration_ledger::MigrationKind;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{
    filesystem::metadata as fs_metadata, json_store, local_history_service, migration_ledger,
};
use crate::ui::editor_page::{
    BufferReplacementOutcome, BufferReplacementRequest, BufferReplacementTicket,
    BufferReplacementWorkflow, LushtextEditorPage, PendingWarningAction,
};
use crate::ui::status_bar::MessageKind;
use crate::ui::{accessibility, buffer_snapshot};

use super::LushtextWindow;

#[cfg(feature = "test-utils")]
pub use crate::services::local_history_service::set_local_history_preview_read_delay_for_test;

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
/// Maximum preview body installed synchronously in one GTK turn.
const LOCAL_HISTORY_PREVIEW_DIRECT_THRESHOLD_BYTES: usize = SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES;
/// Maximum UTF-8 bytes inserted by one scheduled preview slice.
const LOCAL_HISTORY_PREVIEW_INSTALL_SLICE_BYTES: usize = REPLACEMENT_INSERT_SLICE_BYTES;

#[cfg(feature = "test-utils")]
static LOCAL_HISTORY_PREVIEW_INSTALL_SLICES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static LOCAL_HISTORY_PREVIEW_INSTALL_CANCELLATIONS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static LOCAL_HISTORY_PREVIEW_INSTALL_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Delay successive preview slices without blocking GTK for deterministic tests.
#[cfg(feature = "test-utils")]
pub fn set_local_history_preview_install_delay_for_test(delay_ms: u64) {
    LOCAL_HISTORY_PREVIEW_INSTALL_DELAY_MS.store(delay_ms, Ordering::Release);
}

#[cfg(feature = "test-utils")]
fn local_history_preview_install_delay_for_test() -> u64 {
    LOCAL_HISTORY_PREVIEW_INSTALL_DELAY_MS.load(Ordering::Acquire)
}

#[cfg(not(feature = "test-utils"))]
fn local_history_preview_install_delay_for_test() -> u64 {
    0
}

/// Scalar local-history preview installation evidence.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalHistoryPreviewInstallSnapshot {
    /// GTK insertion slices completed by this test process.
    pub slices: usize,
    /// Sliced installations cancelled by supersession or disposal.
    pub cancellations: usize,
}

/// Return process-local preview installation evidence.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn local_history_preview_install_snapshot_for_test() -> LocalHistoryPreviewInstallSnapshot {
    LocalHistoryPreviewInstallSnapshot {
        slices: LOCAL_HISTORY_PREVIEW_INSTALL_SLICES.load(Ordering::Acquire),
        cancellations: LOCAL_HISTORY_PREVIEW_INSTALL_CANCELLATIONS.load(Ordering::Acquire),
    }
}

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
    /// One-active/one-latest ownership for snapshot body reads.
    preview_loads: RefCell<local_history_service::LocalHistoryPreviewCoordinator>,
    /// Current browser-local sliced GTK installation, when required.
    preview_install: RefCell<Option<LocalHistoryPreviewInstallSession>>,
    /// Whether dialog teardown invalidated all preview work.
    disposed: Cell<bool>,
    /// Active safety capture for a pending restore, disposed with the dialog.
    restore_snapshot: RefCell<Option<buffer_snapshot::BufferSnapshotHandle>>,
}

struct LocalHistoryPreviewInstallSession {
    generation: u64,
    snapshot: Option<LocalHistorySnapshot>,
    offset: usize,
    source_id: Option<glib::SourceId>,
}

/// State passed through the restore-safety background capture.
struct RestoreWorkState {
    /// Browser widgets that should be updated when the safety snapshot finishes.
    browser: Rc<LocalHistoryBrowserState>,
    /// Current buffer text saved for the immediate undo affordance.
    undo_text: String,
    /// Historical snapshot whose body should replace the buffer on success.
    restore_snapshot: LocalHistorySnapshot,
    /// Editor/path/edit identity captured with the safety body.
    ticket: LocalHistoryReplacementTicket,
}

#[derive(Clone, Copy)]
struct LocalHistoryReplacementTicket {
    editor_generation: u64,
    path_generation: u64,
    edit_generation: u64,
}

impl LocalHistoryReplacementTicket {
    fn capture(editor: &LushtextEditorPage) -> Self {
        let state = &editor.imp().local_history;
        Self {
            editor_generation: state.editor_generation.get(),
            path_generation: state.path_generation.get(),
            edit_generation: state.edit_generation.get(),
        }
    }

    fn is_current(self, editor: &LushtextEditorPage) -> bool {
        let state = &editor.imp().local_history;
        state.editor_generation.get() == self.editor_generation
            && state.path_generation.get() == self.path_generation
            && state.edit_generation.get() == self.edit_generation
    }
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
        spawn_blocking_then(
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
        spawn_blocking_then(
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
        spawn_blocking_then(
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
        let ticket = LocalHistoryReplacementTicket::capture(editor);
        let freshness_editor = editor.downgrade();
        let terminal_editor = editor.downgrade();
        let cancelled_editor = editor.downgrade();
        let window_weak = self.downgrade();
        let request = BufferReplacementRequest::new(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::LocalHistoryUndo,
                generation: ticket.edit_generation,
            },
            undo_text,
            move |_| {
                freshness_editor
                    .upgrade()
                    .is_some_and(|editor| ticket.is_current(&editor))
            },
            move |outcome| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let Some(editor) = terminal_editor.upgrade() else {
                    return;
                };
                if !matches!(
                    outcome,
                    BufferReplacementOutcome::Complete {
                        ticket: BufferReplacementTicket {
                            workflow: BufferReplacementWorkflow::LocalHistoryUndo,
                            generation,
                        },
                        ..
                    } if generation == ticket.edit_generation && ticket.is_current(&editor)
                ) {
                    return;
                }
                editor.finish_local_history_buffer_replacement();
                if let Some(path) = editor.file_path() {
                    window.resolve_notes_for_editor(&editor, &path);
                }
                window.dismiss_editor_notifications(&editor);
                window.publish_status_message("Local-history restore undone", MessageKind::Info);
                window.refresh_status_bar();
            },
        )
        .return_body_on_cancel(move |body| {
            if let Some(editor) = cancelled_editor.upgrade()
                && ticket.is_current(&editor)
            {
                editor.set_local_history_restore_undo_text(Some(body));
                editor.set_pending_warning_action(Some(
                    PendingWarningAction::UndoLocalHistoryRestore,
                ));
            }
        });
        editor.replace_buffer_bounded(request);
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
        accessibility::set_role(&sidebar, gtk4::AccessibleRole::List);
        sidebar.set_mode(libadwaita::SidebarMode::Sidebar);
        sidebar.set_vexpand(true);
        accessibility::set_labelled_description(
            &sidebar,
            "Local history snapshots",
            "Choose a saved snapshot for the active document",
        );

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
        accessibility::set_labelled_description(
            &preview_view,
            "Snapshot text preview",
            "Read-only text captured in the selected local-history snapshot",
        );
        accessibility::set_read_only(&preview_view, true);
        accessibility::set_multi_line(&preview_view, true);
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
        accessibility::set_role(&preview_stack, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &preview_stack,
            "Local history preview",
            "Read-only preview for the selected local-history snapshot",
        );
        set_local_history_preview_accessibility(&preview_stack, "Loading snapshot", true, false);
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
        set_local_history_action_enabled(&restore_button, false);
        accessibility::set_labelled_description(
            &restore_button,
            "Restore selected snapshot",
            "Replace the editor contents with the selected snapshot",
        );
        let copy_button = gtk4::Button::with_label("Copy");
        set_local_history_action_enabled(&copy_button, false);
        accessibility::set_labelled_description(
            &copy_button,
            "Copy selected snapshot",
            "Copy the selected snapshot text to the clipboard",
        );

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Snapshots")
            .visible(false)
            .build();
        accessibility::set_labelled_description(
            &back_button,
            "Back to snapshots",
            "Return to the local-history snapshot list",
        );

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
            preview_loads: RefCell::default(),
            preview_install: RefCell::new(None),
            disposed: Cell::new(false),
            restore_snapshot: RefCell::new(None),
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
                let snapshot = state.loaded_snapshot.borrow();
                let Some(snapshot) = snapshot.as_ref() else {
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
                let Some(snapshot) = state.loaded_snapshot.borrow_mut().take() else {
                    return;
                };
                set_local_history_action_enabled(&state.restore_button, false);
                set_local_history_action_enabled(&state.copy_button, false);
                LushtextWindow::restore_local_history_snapshot(&state, snapshot);
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
                if let Some(state) = state_holder.borrow().as_ref()
                    && let Some(snapshot) = state.restore_snapshot.take()
                {
                    snapshot.dispose();
                }
                if let Some(state) = state_holder.borrow().as_ref() {
                    state.dispose_preview_runtime();
                }
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
        accessibility::set_role(&status, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &status,
            "No local history yet",
            "Saved snapshots will appear after this document is edited or saved",
        );
        dialog.set_child(Some(&status));
        dialog
    }

    fn restore_local_history_snapshot(
        browser: &Rc<LocalHistoryBrowserState>,
        snapshot: LocalHistorySnapshot,
    ) {
        let buffer = browser.editor.buffer();
        let browser_for_restore = Rc::clone(browser);
        let run_restore = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let browser = browser_for_restore;
            browser.restore_snapshot.take();
            let buffer_snapshot::BufferSnapshotOutcome::Captured(undo_text) = outcome else {
                browser.loaded_snapshot.replace(Some(snapshot));
                set_local_history_action_enabled(&browser.restore_button, true);
                set_local_history_action_enabled(&browser.copy_button, true);
                return;
            };
            let path = browser.path.clone();
            let ticket = LocalHistoryReplacementTicket::capture(&browser.editor);
            spawn_blocking_then(
                RestoreWorkState {
                    browser,
                    undo_text: undo_text.clone(),
                    restore_snapshot: snapshot,
                    ticket,
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
                        set_local_history_action_enabled(&state.browser.restore_button, true);
                        set_local_history_action_enabled(
                            &state.browser.copy_button,
                            !state.restore_snapshot.text.is_empty(),
                        );
                        state
                            .browser
                            .loaded_snapshot
                            .replace(Some(state.restore_snapshot));
                        state.browser.window.publish_status_message(
                            "Local history restore could not be prepared safely",
                            MessageKind::Error,
                        );
                        return;
                    }
                    if !state.ticket.is_current(&state.browser.editor) {
                        set_local_history_action_enabled(&state.browser.restore_button, true);
                        set_local_history_action_enabled(
                            &state.browser.copy_button,
                            !state.restore_snapshot.text.is_empty(),
                        );
                        state
                            .browser
                            .loaded_snapshot
                            .replace(Some(state.restore_snapshot));
                        return;
                    }

                    let freshness_editor = state.browser.editor.downgrade();
                    let terminal_editor = state.browser.editor.downgrade();
                    let browser = Rc::clone(&state.browser);
                    let cancelled_browser = Rc::clone(&state.browser);
                    let undo_text = state.undo_text;
                    let ticket = state.ticket;
                    let restore_meta = state.restore_snapshot.meta;
                    let restore_text = state.restore_snapshot.text;
                    state.browser.editor.replace_buffer_bounded(
                        BufferReplacementRequest::new(
                            BufferReplacementTicket {
                                workflow: BufferReplacementWorkflow::LocalHistoryRestore,
                                generation: ticket.edit_generation,
                            },
                            restore_text,
                            move |_| {
                                freshness_editor
                                    .upgrade()
                                    .is_some_and(|editor| ticket.is_current(&editor))
                            },
                            move |outcome| {
                                let Some(editor) = terminal_editor.upgrade() else {
                                    return;
                                };
                                if !matches!(
                                    outcome,
                                    BufferReplacementOutcome::Complete {
                                        ticket: BufferReplacementTicket {
                                            workflow: BufferReplacementWorkflow::LocalHistoryRestore,
                                            generation,
                                        },
                                        ..
                                    } if generation == ticket.edit_generation && ticket.is_current(&editor)
                                ) {
                                    set_local_history_action_enabled(&browser.restore_button, true);
                                    set_local_history_action_enabled(
                                        &browser.copy_button,
                                        browser
                                            .loaded_snapshot
                                            .borrow()
                                            .as_ref()
                                            .is_some_and(|snapshot| !snapshot.text.is_empty()),
                                    );
                                    return;
                                }
                                editor.set_local_history_restore_undo_text(Some(undo_text));
                                editor.finish_local_history_buffer_replacement();
                                browser.window.dismiss_editor_notifications(&editor);
                                browser
                                    .window
                                    .resolve_notes_for_editor(&editor, browser.path.as_path());
                                editor.emit_inline_notification_with_warning_action(
                                    InlineActionNotification {
                                        style: InlineNotificationStyle::Warning,
                                        title: "Restored from Local History".to_string(),
                                        body: "The previous buffer state was saved as a safety snapshot. Use Undo Restore to switch back immediately.".to_string(),
                                        primary_button: Some("Undo Restore".to_string()),
                                        secondary_button: None,
                                    },
                                    PendingWarningAction::UndoLocalHistoryRestore,
                                );
                                browser.window.publish_status_message(
                                    "Snapshot restored into the editor",
                                    MessageKind::Info,
                                );
                                browser.window.refresh_status_bar();
                                browser.dialog.close();
                            },
                        )
                        .return_body_on_cancel(move |text| {
                            cancelled_browser.loaded_snapshot.replace(Some(
                                LocalHistorySnapshot {
                                    meta: restore_meta,
                                    text,
                                },
                            ));
                        }),
                    );
                },
            );
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async(buffer, run_restore);
            browser.restore_snapshot.replace(Some(snapshot));
        } else {
            run_restore(buffer_snapshot::BufferSnapshotOutcome::Captured(
                buffer_snapshot::snapshot_buffer_text_direct(&buffer),
            ));
        }
    }
}

impl LocalHistoryBrowserState {
    fn load_preview_for_index(self: &Rc<Self>, index: usize, user_selected: bool) {
        let Some(meta) = self.snapshots.get(index).cloned() else {
            return;
        };

        self.cancel_preview_install();
        if let Some(snapshot) = self.loaded_snapshot.take() {
            retire_local_history_snapshot(snapshot);
        }
        self.preview_title.set_label("Loading snapshot…");
        self.preview_meta
            .set_label(&format_snapshot_meta(meta.origin, meta.byte_len));
        self.preview_buffer.set_text("");
        self.preview_stack.set_visible_child_name("loading");
        set_local_history_preview_accessibility(
            &self.preview_stack,
            "Loading snapshot",
            true,
            false,
        );
        set_local_history_action_enabled(&self.restore_button, false);
        set_local_history_action_enabled(&self.copy_button, false);

        if user_selected {
            // `show-content` is only visible while collapsed, but setting it
            // before the adaptive layout settles preserves the user's
            // navigation request during resize and widget-test transitions.
            self.split_view.set_show_content(true);
        }

        let start = self.preview_loads.borrow_mut().submit(
            local_history_service::LocalHistoryPreviewRequest {
                path: self.path.clone(),
                snapshot_id: meta.snapshot_id,
            },
        );
        if let Some(start) = start {
            self.start_preview_load(start);
        }
    }

    fn start_preview_load(self: &Rc<Self>, start: local_history_service::LocalHistoryPreviewStart) {
        let local_history_service::LocalHistoryPreviewStart {
            generation,
            request,
            cancellation,
        } = start;
        let state_weak = Rc::downgrade(self);
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::load_snapshot_for_path_cancellable(
                    &data_dir,
                    &request.path,
                    &request.snapshot_id,
                    &cancellation,
                )
            },
            move |(), result| {
                let Some(state) = state_weak.upgrade() else {
                    retire_local_history_preview_result(result);
                    return;
                };
                state.finish_preview_load(generation, result);
            },
        );
    }

    fn finish_preview_load(
        self: &Rc<Self>,
        generation: u64,
        result: anyhow::Result<local_history_service::LocalHistoryPreviewLoadOutcome>,
    ) {
        let (accepted, next) = {
            let mut loads = self.preview_loads.borrow_mut();
            let accepted = loads.is_current(generation) && !self.disposed.get();
            let next = loads.finish(generation);
            (accepted, next)
        };
        if accepted {
            match result {
                Ok(local_history_service::LocalHistoryPreviewLoadOutcome::Loaded(snapshot)) => {
                    self.begin_preview_install(generation, snapshot);
                }
                Ok(local_history_service::LocalHistoryPreviewLoadOutcome::Missing) => {
                    self.preview_title.set_label("Snapshot missing");
                    self.preview_meta.set_label("");
                    self.preview_stack.set_visible_child_name("error");
                    set_local_history_preview_accessibility(
                        &self.preview_stack,
                        "Snapshot missing",
                        false,
                        true,
                    );
                }
                Ok(local_history_service::LocalHistoryPreviewLoadOutcome::Cancelled) => {}
                Err(error) => {
                    tracing::error!("Failed to load local-history preview: {error}");
                    self.preview_title.set_label("Preview unavailable");
                    self.preview_meta.set_label("");
                    self.preview_stack.set_visible_child_name("error");
                    set_local_history_preview_accessibility(
                        &self.preview_stack,
                        "Preview unavailable",
                        false,
                        true,
                    );
                }
            }
        } else {
            retire_local_history_preview_result(result);
        }
        if let Some(next) = next {
            self.start_preview_load(next);
        }
    }

    fn begin_preview_install(self: &Rc<Self>, generation: u64, snapshot: LocalHistorySnapshot) {
        self.preview_title
            .set_label(&format_history_time(snapshot.meta.captured_at_millis));
        self.preview_meta.set_label(&format_snapshot_meta(
            snapshot.meta.origin,
            snapshot.meta.byte_len,
        ));
        if snapshot.text.is_empty() {
            self.preview_buffer.set_text("");
            self.preview_stack.set_visible_child_name("empty");
            set_local_history_preview_accessibility(
                &self.preview_stack,
                "This snapshot was empty",
                false,
                false,
            );
            set_local_history_action_enabled(&self.copy_button, false);
            set_local_history_action_enabled(&self.restore_button, true);
            self.loaded_snapshot.replace(Some(snapshot));
            return;
        }
        if snapshot.text.len() <= LOCAL_HISTORY_PREVIEW_DIRECT_THRESHOLD_BYTES {
            self.preview_buffer.set_text(&snapshot.text);
            self.finish_preview_install(snapshot);
            return;
        }

        self.preview_buffer.set_text("");
        self.preview_install
            .replace(Some(LocalHistoryPreviewInstallSession {
                generation,
                snapshot: Some(snapshot),
                offset: 0,
                source_id: None,
            }));
        self.schedule_preview_install_slice();
    }

    fn schedule_preview_install_slice(self: &Rc<Self>) {
        let state_weak = Rc::downgrade(self);
        let delay_ms = local_history_preview_install_delay_for_test();
        let source_id = if delay_ms == 0 {
            glib::idle_add_local_once(move || {
                if let Some(state) = state_weak.upgrade() {
                    state.run_preview_install_slice();
                }
            })
        } else {
            glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
                if let Some(state) = state_weak.upgrade() {
                    state.run_preview_install_slice();
                }
            })
        };
        if let Some(session) = self.preview_install.borrow_mut().as_mut() {
            session.source_id = Some(source_id);
        } else {
            source_id.remove();
        }
    }

    fn run_preview_install_slice(self: &Rc<Self>) {
        let Some(mut session) = self.preview_install.take() else {
            return;
        };
        session.source_id = None;
        if self.disposed.get() || !self.preview_loads.borrow().is_current(session.generation) {
            if let Some(snapshot) = session.snapshot.take() {
                retire_local_history_snapshot(snapshot);
            }
            record_local_history_preview_install_cancellation();
            return;
        }
        let Some(snapshot) = session.snapshot.as_ref() else {
            return;
        };
        let end = next_replacement_boundary(&snapshot.text, session.offset);
        debug_assert!(
            end.saturating_sub(session.offset) <= LOCAL_HISTORY_PREVIEW_INSTALL_SLICE_BYTES
        );
        let mut end_iter = self.preview_buffer.end_iter();
        self.preview_buffer
            .insert(&mut end_iter, &snapshot.text[session.offset..end]);
        session.offset = end;
        record_local_history_preview_install_slice();

        if session.offset == snapshot.text.len() {
            let snapshot = session.snapshot.take().expect("install snapshot exists");
            self.finish_preview_install(snapshot);
        } else {
            self.preview_install.replace(Some(session));
            self.schedule_preview_install_slice();
        }
    }

    fn finish_preview_install(&self, snapshot: LocalHistorySnapshot) {
        self.preview_stack.set_visible_child_name("content");
        set_local_history_preview_accessibility(
            &self.preview_stack,
            &format!(
                "Snapshot from {}",
                format_history_time(snapshot.meta.captured_at_millis)
            ),
            false,
            false,
        );
        set_local_history_action_enabled(&self.copy_button, true);
        set_local_history_action_enabled(&self.restore_button, true);
        self.loaded_snapshot.replace(Some(snapshot));
    }

    fn cancel_preview_install(&self) {
        let Some(mut session) = self.preview_install.take() else {
            return;
        };
        if let Some(source_id) = session.source_id.take() {
            source_id.remove();
        }
        if let Some(snapshot) = session.snapshot.take() {
            retire_local_history_snapshot(snapshot);
        }
        record_local_history_preview_install_cancellation();
    }

    fn dispose_preview_runtime(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.preview_loads.borrow_mut().invalidate();
        self.cancel_preview_install();
        if let Some(snapshot) = self.loaded_snapshot.take() {
            retire_local_history_snapshot(snapshot);
        }
        set_local_history_action_enabled(&self.restore_button, false);
        set_local_history_action_enabled(&self.copy_button, false);
    }
}

fn retire_local_history_preview_result(
    result: anyhow::Result<local_history_service::LocalHistoryPreviewLoadOutcome>,
) {
    if let Ok(local_history_service::LocalHistoryPreviewLoadOutcome::Loaded(snapshot)) = result {
        retire_local_history_snapshot(snapshot);
    }
}

fn retire_local_history_snapshot(snapshot: LocalHistorySnapshot) {
    spawn_blocking_then((), move || drop(snapshot), |(), ()| {});
}

fn record_local_history_preview_install_slice() {
    #[cfg(feature = "test-utils")]
    LOCAL_HISTORY_PREVIEW_INSTALL_SLICES.fetch_add(1, Ordering::Release);
}

fn record_local_history_preview_install_cancellation() {
    #[cfg(feature = "test-utils")]
    LOCAL_HISTORY_PREVIEW_INSTALL_CANCELLATIONS.fetch_add(1, Ordering::Release);
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

/// Keep GTK sensitivity and explicit accessible disabled state in sync.
fn set_local_history_action_enabled(button: &gtk4::Button, enabled: bool) {
    button.set_sensitive(enabled);
    accessibility::set_disabled(button, !enabled);
}

/// Update the preview stack's workflow state after selection or preview loading.
fn set_local_history_preview_accessibility(
    stack: &gtk4::Stack,
    value_text: &str,
    busy: bool,
    invalid: bool,
) {
    accessibility::set_value_text(stack, value_text);
    accessibility::set_busy(stack, busy);
    accessibility::set_invalid(stack, invalid);
}

fn loading_preview_widget() -> gtk4::Widget {
    let label = gtk4::Label::new(Some("Loading preview…"));
    label.set_hexpand(true);
    label.set_vexpand(true);
    label.set_halign(gtk4::Align::Center);
    label.set_valign(gtk4::Align::Center);
    accessibility::set_role(&label, gtk4::AccessibleRole::Status);
    accessibility::set_label(&label, "Loading preview");
    label.upcast()
}

fn empty_snapshot_widget() -> gtk4::Widget {
    let status = libadwaita::StatusPage::builder()
        .icon_name("document-new-symbolic")
        .title("This snapshot was empty")
        .description(
            "No text had been saved at this point. For “Before edits” entries, this can mean the file was empty before the current unsaved changes began.",
        )
        .build();
    accessibility::set_role(&status, gtk4::AccessibleRole::Status);
    accessibility::set_labelled_description(
        &status,
        "This snapshot was empty",
        "No text had been saved at this point",
    );
    status.upcast()
}

fn preview_error_widget(title: &str) -> gtk4::Widget {
    let status = libadwaita::StatusPage::builder()
        .icon_name("dialog-warning-symbolic")
        .title(title)
        .description("This snapshot could not be loaded right now.")
        .build();
    accessibility::set_role(&status, gtk4::AccessibleRole::Status);
    accessibility::set_labelled_description(
        &status,
        title,
        "This snapshot could not be loaded right now",
    );
    accessibility::set_invalid(&status, true);
    status.upcast()
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
