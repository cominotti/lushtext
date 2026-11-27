// SPDX-License-Identifier: GPL-3.0-or-later

//! Presenting the browser, and installing one snapshot preview into it.
//!
//! Stage-order-qualified `preview_execution` rather than plain `execution`: this
//! workflow owns two execution-shaped coordination jobs in its browse stage
//! order — showing a snapshot, and restoring one — and neither is a stable
//! sibling being renamed for symmetry, so both take the qualifier.
//!
//! Everything bounded about the preview lives here: one-active/one-latest body
//! reads through the shared coordinator, a disposal reservation per read, and a
//! paragraph-aligned sliced install for a body too large for one GTK turn.

use lushtext_model::document::paragraph_boundary::DocumentProfile;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gtk_lush_tasks::spawn_blocking_then;
use lushtext_services::persistence::json_store;

use crate::ui::folded_text::{FoldedTextSurface, InstallSlice, SurfaceText, install_slice};
use crate::ui::protected_fold::{BoundaryLedger, FoldedInstall};
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, SidebarItemExt};
use std::sync::Arc;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use crate::ui::{accessibility, buffer_snapshot};
use lushtext_model::persistence::local_history::{LocalHistorySnapshot, LocalHistorySnapshotMeta};
use lushtext_model::values::{HistoryPreviewGeneration, Sp};
use lushtext_services::persistence::local_history_service;

use super::policy;
use crate::ui::window::LushtextWindow;

pub(super) type GuardedLocalHistorySnapshot =
    crate::ui::plain_disposal::DisposalOwned<LocalHistorySnapshot>;

/// The snapshot the preview shows, shared with the folded preview's copy
/// workers (`ui/folded_text.rs`). A restore takes it back owned
/// (`LocalHistoryBrowserState::with_owned_loaded_snapshot`).
pub(super) type LoadedSnapshot = Arc<GuardedLocalHistorySnapshot>;

impl SurfaceText for GuardedLocalHistorySnapshot {
    fn surface_text(&self) -> &str {
        &self.text
    }
}

/// How long a restore waits before taking the snapshot again while a copy
/// worker still holds it (a copy cuts its range in milliseconds).
const OWNED_SNAPSHOT_RETRY_MS: u64 = 4;

pub(super) enum GuardedLocalHistoryPreviewLoadOutcome {
    /// The snapshot and its paragraph profile, computed on the reading worker
    /// so a restore can join it into the editor's document scale.
    Loaded(GuardedLocalHistorySnapshot, Box<DocumentProfile>),
    Missing,
    Cancelled,
}

/// Shrink one loaded snapshot's disposal reservation to its real weight, on the
/// worker that produced it.
///
/// The reservation is taken conservatively before the read, because the body's
/// size is not known until it is read; shrinking here returns the difference to
/// the lane instead of holding a 64 MiB claim for a 2 KiB snapshot.
pub(super) fn guard_local_history_preview_on_worker(
    result: Result<
        local_history_service::LocalHistoryPreviewLoadOutcome,
        local_history_service::LocalHistoryError,
    >,
    reservation: crate::ui::plain_disposal::DisposalReservation,
) -> Result<GuardedLocalHistoryPreviewLoadOutcome, local_history_service::LocalHistoryError> {
    match result? {
        local_history_service::LocalHistoryPreviewLoadOutcome::Loaded(snapshot) => {
            let weight = u64::try_from(
                std::mem::size_of::<LocalHistorySnapshot>()
                    .saturating_add(snapshot.text.capacity())
                    .saturating_add(snapshot.meta.snapshot_id.capacity())
                    .saturating_add(snapshot.meta.content_hash.capacity()),
            )
            .unwrap_or(u64::MAX);
            debug_assert!(weight <= policy::PREVIEW_RESERVATION_BYTES);
            let profile = DocumentProfile::of_text(&snapshot.text);
            Ok(GuardedLocalHistoryPreviewLoadOutcome::Loaded(
                reservation.shrink_to_and_own(weight, snapshot),
                Box::new(profile),
            ))
        }
        local_history_service::LocalHistoryPreviewLoadOutcome::Missing => {
            Ok(GuardedLocalHistoryPreviewLoadOutcome::Missing)
        }
        local_history_service::LocalHistoryPreviewLoadOutcome::Cancelled => {
            Ok(GuardedLocalHistoryPreviewLoadOutcome::Cancelled)
        }
    }
}

pub(super) static PREVIEW_INSTALL_SLICES: AtomicUsize = AtomicUsize::new(0);
pub(super) static PREVIEW_INSTALL_CANCELLATIONS: AtomicUsize = AtomicUsize::new(0);
pub(super) static PREVIEW_CLEAR_TURNS: AtomicUsize = AtomicUsize::new(0);
pub(super) static PREVIEW_LARGEST_CLEAR_TURN: AtomicUsize = AtomicUsize::new(0);
pub(super) static PREVIEW_LONGEST_CLEAR_TURN_US: AtomicUsize = AtomicUsize::new(0);
pub(super) static PREVIEW_BROKEN_INSTALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
fn preview_install_delay() -> u64 {
    super::test_policy::preview_install_delay_ms()
}

#[cfg(not(feature = "test-utils"))]
const fn preview_install_delay() -> u64 {
    0
}

fn record_preview_install_cancellation() {
    PREVIEW_INSTALL_CANCELLATIONS.fetch_add(1, Ordering::Release);
}

/// UI state for one open local-history browser dialog.
pub(super) struct LocalHistoryBrowserState {
    pub(super) window: LushtextWindow,
    pub(super) editor: LushtextEditorPage,
    pub(super) path: PathBuf,
    pub(super) dialog: libadwaita::Dialog,
    split_view: libadwaita::NavigationSplitView,
    sidebar: libadwaita::Sidebar,
    preview_title: gtk4::Label,
    preview_meta: gtk4::Label,
    preview_buffer: gtk4::TextBuffer,
    preview_stack: gtk4::Stack,
    pub(super) restore_button: gtk4::Button,
    pub(super) copy_button: gtk4::Button,
    back_button: gtk4::Button,
    snapshots: Vec<LocalHistorySnapshotMeta>,
    /// Copy routing of the preview view: copies of a folded snapshot are cut
    /// from the snapshot, not from the buffer (`ui/folded_text.rs`).
    preview_copy: Rc<FoldedTextSurface>,
    /// One-active/one-latest ownership for snapshot body reads.
    preview_loads: RefCell<local_history_service::LocalHistoryPreviewCoordinator>,
    /// One paced capacity wakeup for the selected snapshot.
    preview_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    /// Lifecycle phase of the selected preview (admission, install, loaded, or disposed).
    preview_phase: RefCell<PreviewPhase>,
    /// Boundary marks a sliced protected preview left in `preview_buffer`.
    preview_boundaries: BoundaryLedger,
    /// Bounded clear phase for a large preview buffer.
    clear_phase: RefCell<ClearPhase>,
    /// Restore-safety phase (idle, waiting for progress capacity, or capturing).
    restore_phase: RefCell<RestorePhase>,
    /// One paced progress-capacity wakeup for the pending restore intent.
    pub(super) restore_capacity_wakeup: crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
}

enum PreviewPhase {
    Idle,
    AwaitingAdmission(local_history_service::LocalHistoryPreviewStart),
    Installing {
        session: LocalHistoryPreviewInstallSession,
        profile: DocumentProfile,
    },
    Loaded {
        snapshot: LoadedSnapshot,
        profile: DocumentProfile,
    },
    RestoringSnapshot {
        profile: DocumentProfile,
    },
    Disposed,
}

enum ClearPhase {
    Idle,
    Running(glib::SourceId),
}

enum RestorePhase {
    Idle,
    AwaitingCapacity,
    Capturing(buffer_snapshot::BufferSnapshotHandle),
}

struct LocalHistoryPreviewInstallSession {
    generation: HistoryPreviewGeneration,
    snapshot: LoadedSnapshot,
    offset: usize,
    source_id: Option<glib::SourceId>,
    /// The sliced install of a snapshot whose longest paragraph exceeds
    /// `L_PROTECT_EDIT` (`ui::protected_fold::FoldedInstall`, C3).
    fold: Option<FoldedInstall>,
}

impl LushtextWindow {
    pub(super) fn present_local_history_browser(
        &self,
        editor: LushtextEditorPage,
        path: PathBuf,
        snapshots: Vec<LocalHistorySnapshotMeta>,
    ) {
        let snapshots = policy::filter_visible_snapshots(snapshots);
        if snapshots.is_empty() {
            Self::build_empty_local_history_dialog(&path).present(Some(self));
            return;
        }

        let (dialog_width, dialog_height) = policy::viewer_dialog_size(
            policy::current_window_dimension(Sp::new(self.width()), Sp::new(self.default_size().0)),
            policy::current_window_dimension(
                Sp::new(self.height()),
                Sp::new(self.default_size().1),
            ),
        );
        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(dialog_width.as_sp())
            .content_height(dialog_height.as_sp())
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
        crate::ui::font_zoom::follow_editor_font_monospace(&preview_view);
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
        let preview_copy = FoldedTextSurface::attach(&preview_view);

        let preview_stack = gtk4::Stack::new();
        preview_stack.set_hexpand(true);
        preview_stack.set_vexpand(true);
        accessibility::set_role(&preview_stack, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &preview_stack,
            "Local history preview",
            "Read-only preview for the selected local-history snapshot",
        );
        preview_stack.add_named(&loading_preview_widget(), Some("loading"));
        preview_stack.add_named(
            &preview_status_page(
                "document-new-symbolic",
                "This snapshot was empty",
                "No text had been saved at this point. For “Before edits” entries, this can mean the file was empty before the current unsaved changes began.",
                "No text had been saved at this point",
                false,
            ),
            Some("empty"),
        );
        preview_stack.add_named(
            &preview_status_page(
                "dialog-warning-symbolic",
                "Preview unavailable",
                "This snapshot could not be loaded right now.",
                "This snapshot could not be loaded right now",
                true,
            ),
            Some("error"),
        );
        let preview_scroll = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&preview_view)
            .build();
        preview_stack.add_named(&preview_scroll, Some("content"));
        set_preview_stack_state(
            &preview_stack,
            PreviewStackState::Loading,
            "Loading snapshot",
        );

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
        split_view.set_min_sidebar_width(f64::from(policy::VIEWER_MIN_SIDEBAR_WIDTH_SP.as_sp()));
        split_view.set_max_sidebar_width(f64::from(policy::VIEWER_MAX_SIDEBAR_WIDTH_SP.as_sp()));
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
            preview_copy,
            preview_loads: RefCell::default(),
            preview_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup::default(),
            preview_phase: RefCell::new(PreviewPhase::Idle),
            preview_boundaries: BoundaryLedger::default(),
            clear_phase: RefCell::new(ClearPhase::Idle),
            restore_phase: RefCell::new(RestorePhase::Idle),
            restore_capacity_wakeup:
                crate::ui::plain_disposal::ProgressDisposalCapacityWakeup::default(),
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
                let Some(snapshot) = state.loaded_snapshot() else {
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
                if state.loaded_snapshot().is_none() {
                    return;
                }
                set_local_history_action_enabled(&state.restore_button, false);
                set_local_history_action_enabled(&state.copy_button, false);
                state.with_owned_loaded_snapshot(|state, snapshot| {
                    LushtextWindow::restore_local_history_snapshot(state, snapshot);
                });
            }
        });

        state.sidebar.set_selected(0);
        state.load_preview_for_index(0, false);
        state.sidebar.connect_selected_item_notify({
            let state = Rc::downgrade(&state);
            move |sidebar| {
                if let Some(state) = state.upgrade()
                    && let Some(item) = sidebar.selected_item()
                    && let Ok(index) = usize::try_from(item.index())
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
                if let Some(state) = state_holder.borrow().as_ref() {
                    state.dispose_restore_capture();
                    state.dispose_preview_runtime();
                }
                state_holder.borrow_mut().take();
            }
        });

        state.dialog.present(Some(self));
    }

    pub(super) fn build_empty_local_history_dialog(path: &Path) -> libadwaita::Dialog {
        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(policy::EMPTY_WIDTH_SP.as_sp())
            .content_height(policy::EMPTY_HEIGHT_SP.as_sp())
            // `AdwStatusPage` has a narrow natural request; following content size
            // would collapse this empty-state browser instead of using the target.
            .follows_content_size(false)
            .build();

        let status = preview_status_page(
            "document-open-recent-symbolic",
            "No local history yet",
            &format!(
                "{}\n\nSaved snapshots will appear after you edit or save this document.",
                path.display()
            ),
            "Saved snapshots will appear after this document is edited or saved",
            false,
        );
        status.set_hexpand(true);
        status.set_vexpand(true);
        dialog.set_child(Some(&status));
        dialog
    }
}

impl LocalHistoryBrowserState {
    fn is_disposed(&self) -> bool {
        matches!(*self.preview_phase.borrow(), PreviewPhase::Disposed)
    }

    fn loaded_snapshot(&self) -> Option<LoadedSnapshot> {
        match &*self.preview_phase.borrow() {
            PreviewPhase::Loaded { snapshot, .. } => Some(Arc::clone(snapshot)),
            _ => None,
        }
    }

    pub(super) fn has_non_empty_loaded_snapshot(&self) -> bool {
        matches!(
            &*self.preview_phase.borrow(),
            PreviewPhase::Loaded { snapshot, .. } if !snapshot.text.is_empty()
        )
    }

    pub(super) fn loaded_profile(&self) -> DocumentProfile {
        match &*self.preview_phase.borrow() {
            PreviewPhase::Installing { profile, .. }
            | PreviewPhase::Loaded { profile, .. }
            | PreviewPhase::RestoringSnapshot { profile } => profile.clone(),
            PreviewPhase::Idle | PreviewPhase::AwaitingAdmission(_) | PreviewPhase::Disposed => {
                DocumentProfile::EMPTY
            }
        }
    }

    pub(super) fn set_restore_awaiting_capacity(&self) {
        self.restore_phase.replace(RestorePhase::AwaitingCapacity);
    }

    pub(super) fn take_restore_awaiting_capacity(&self) -> bool {
        if !matches!(*self.restore_phase.borrow(), RestorePhase::AwaitingCapacity) {
            return false;
        }
        self.restore_phase.replace(RestorePhase::Idle);
        true
    }

    pub(super) fn set_restore_capturing(&self, snapshot: buffer_snapshot::BufferSnapshotHandle) {
        self.restore_phase
            .replace(RestorePhase::Capturing(snapshot));
    }

    pub(super) fn finish_restore_capture(&self) {
        self.restore_phase.replace(RestorePhase::Idle);
    }

    fn dispose_restore_capture(&self) {
        if let RestorePhase::Capturing(snapshot) = self.restore_phase.replace(RestorePhase::Idle) {
            snapshot.dispose();
        }
    }

    fn load_preview_for_index(self: &Rc<Self>, index: usize, user_selected: bool) {
        let Some(meta) = self.snapshots.get(index).cloned() else {
            return;
        };
        if self.is_disposed() {
            return;
        }

        self.dispose_restore_capture();
        self.restore_capacity_wakeup.cancel();
        self.retire_active_preview();
        self.preview_title.set_label("Loading snapshot…");
        self.preview_meta
            .set_label(&policy::format_snapshot_meta(meta.origin, meta.byte_len));
        self.clear_preview_buffer();
        set_preview_stack_state(
            &self.preview_stack,
            PreviewStackState::Loading,
            "Loading snapshot",
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
        } else {
            self.finish_cancelled_preview_admission();
        }
    }

    fn start_preview_load(self: &Rc<Self>, start: local_history_service::LocalHistoryPreviewStart) {
        if start.cancellation.is_cancelled() {
            self.finish_preview_load(
                start.generation,
                Ok(GuardedLocalHistoryPreviewLoadOutcome::Cancelled),
            );
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let Some(reservation) =
            crate::ui::plain_disposal::try_reserve_for_gtk(policy::PREVIEW_RESERVATION_BYTES)
        else {
            debug_assert!(!matches!(
                *self.preview_phase.borrow(),
                PreviewPhase::AwaitingAdmission(_)
            ));
            self.preview_phase
                .replace(PreviewPhase::AwaitingAdmission(start));
            let state_weak = Rc::downgrade(self);
            self.preview_capacity_wakeup.arm(observed_epoch, move || {
                if let Some(state) = state_weak.upgrade() {
                    state.retry_preview_admission();
                }
            });
            self.preview_title.set_label("Preview deferred");
            self.preview_meta
                .set_label("Waiting for memory pressure to clear");
            set_preview_stack_state(
                &self.preview_stack,
                PreviewStackState::Loading,
                "Snapshot preview deferred by memory pressure",
            );
            return;
        };

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
                guard_local_history_preview_on_worker(
                    local_history_service::load_snapshot_for_path_cancellable(
                        &data_dir,
                        &request.path,
                        &request.snapshot_id,
                        &cancellation,
                    ),
                    reservation,
                )
            },
            move |(), result| {
                let Some(state) = state_weak.upgrade() else {
                    drop(result);
                    return;
                };
                state.finish_preview_load(generation, result);
            },
        );
    }

    fn retry_preview_admission(self: &Rc<Self>) {
        match self.preview_phase.replace(PreviewPhase::Idle) {
            PreviewPhase::AwaitingAdmission(start) => self.start_preview_load(start),
            other => {
                self.preview_phase.replace(other);
            }
        }
    }

    fn finish_cancelled_preview_admission(self: &Rc<Self>) {
        match self.preview_phase.replace(PreviewPhase::Idle) {
            PreviewPhase::AwaitingAdmission(start) if start.cancellation.is_cancelled() => {
                self.preview_capacity_wakeup.cancel();
                self.finish_preview_load(
                    start.generation,
                    Ok(GuardedLocalHistoryPreviewLoadOutcome::Cancelled),
                );
            }
            other => {
                self.preview_phase.replace(other);
            }
        }
    }

    fn finish_preview_load(
        self: &Rc<Self>,
        generation: HistoryPreviewGeneration,
        result: Result<
            GuardedLocalHistoryPreviewLoadOutcome,
            local_history_service::LocalHistoryError,
        >,
    ) {
        let (accepted, next) = {
            let mut loads = self.preview_loads.borrow_mut();
            let accepted = loads.is_current(generation) && !self.is_disposed();
            let next = loads.finish(generation);
            (accepted, next)
        };
        if accepted {
            match result {
                Ok(GuardedLocalHistoryPreviewLoadOutcome::Loaded(snapshot, profile)) => {
                    self.begin_preview_install(generation, snapshot, *profile);
                }
                Ok(GuardedLocalHistoryPreviewLoadOutcome::Missing) => {
                    self.show_preview_error("Snapshot missing");
                }
                Ok(GuardedLocalHistoryPreviewLoadOutcome::Cancelled) => {}
                Err(error) => {
                    tracing::error!("Failed to load local-history preview: {error}");
                    self.show_preview_error("Preview unavailable");
                }
            }
        } else {
            drop(result);
        }
        if let Some(next) = next {
            self.start_preview_load(next);
        }
    }

    /// Show the preview pane's error state under one title.
    fn show_preview_error(&self, title: &str) {
        self.preview_title.set_label(title);
        self.preview_meta.set_label("");
        set_preview_stack_state(&self.preview_stack, PreviewStackState::Error, title);
    }

    /// Install a snapshot into the preview, folding paragraphs over `L_PROTECT_EDIT`.
    fn begin_preview_install(
        self: &Rc<Self>,
        generation: HistoryPreviewGeneration,
        snapshot: GuardedLocalHistorySnapshot,
        profile: DocumentProfile,
    ) {
        let fold_plan = profile.fold_plan.clone();
        let snapshot: LoadedSnapshot = Arc::new(snapshot);
        self.preview_copy
            .set_folded_source(fold_plan.clone().map(|plan| {
                let weak = Arc::downgrade(&snapshot);
                let source: std::sync::Weak<dyn SurfaceText> = weak;
                (source, plan)
            }));
        self.preview_title
            .set_label(&format_history_time(snapshot.meta.captured_at_millis));
        self.preview_meta.set_label(&policy::format_snapshot_meta(
            snapshot.meta.origin,
            snapshot.meta.byte_len,
        ));
        match policy::preview_install_plan(snapshot.text.len()) {
            policy::PreviewInstallPlan::Empty => {
                self.clear_preview_buffer();
                set_preview_stack_state(
                    &self.preview_stack,
                    PreviewStackState::Empty,
                    "This snapshot was empty",
                );
                // Copy stays disabled when empty, while Restore stays enabled.
                set_local_history_action_enabled(&self.copy_button, false);
                set_local_history_action_enabled(&self.restore_button, true);
                self.preview_phase
                    .replace(PreviewPhase::Loaded { snapshot, profile });
            }
            policy::PreviewInstallPlan::Direct
                if fold_plan.is_none() && self.clear_preview_buffer() =>
            {
                self.preview_buffer.set_text(&snapshot.text);
                self.finish_preview_install(snapshot, profile);
            }
            policy::PreviewInstallPlan::Direct | policy::PreviewInstallPlan::Sliced => {
                self.clear_preview_buffer();
                let fold = fold_plan
                    .map(|plan| FoldedInstall::new(plan, self.preview_buffer.upcast_ref(), false));
                self.preview_phase.replace(PreviewPhase::Installing {
                    session: LocalHistoryPreviewInstallSession {
                        generation,
                        snapshot,
                        offset: 0,
                        source_id: None,
                        fold,
                    },
                    profile,
                });
                self.schedule_preview_install_slice();
            }
        }
    }

    fn clear_preview_buffer(self: &Rc<Self>) -> bool {
        if matches!(*self.clear_phase.borrow(), ClearPhase::Running(_)) {
            return false;
        }
        if self.preview_boundaries.is_empty()
            && self.preview_buffer.char_count() <= PREVIEW_DIRECT_CLEAR_CHARS
        {
            self.preview_buffer.set_text("");
            return true;
        }
        self.schedule_preview_clear_turn();
        false
    }

    fn schedule_preview_clear_turn(self: &Rc<Self>) {
        let state_weak = Rc::downgrade(self);
        let source = glib::idle_add_local_once(move || {
            if let Some(state) = state_weak.upgrade() {
                state.clear_phase.replace(ClearPhase::Idle);
                state.run_preview_clear_turn();
            }
        });
        if let ClearPhase::Running(old) = self.clear_phase.replace(ClearPhase::Running(source)) {
            old.remove();
        }
    }

    fn run_preview_clear_turn(self: &Rc<Self>) {
        if self.is_disposed() {
            return;
        }
        let started_us = glib::monotonic_time();
        let buffer: &gtk4::TextBuffer = self.preview_buffer.upcast_ref();
        let deleted = clear_preview_turn(buffer, &self.preview_boundaries);
        let ended_us = glib::monotonic_time();
        crate::ui::responsiveness_probe::record_span(
            "local_history_preview_clear_turn",
            started_us,
            ended_us,
        );
        PREVIEW_LONGEST_CLEAR_TURN_US.fetch_max(
            usize::try_from(ended_us.saturating_sub(started_us)).unwrap_or(usize::MAX),
            Ordering::AcqRel,
        );
        PREVIEW_CLEAR_TURNS.fetch_add(1, Ordering::Release);
        PREVIEW_LARGEST_CLEAR_TURN.fetch_max(deleted, Ordering::AcqRel);
        if buffer.char_count() > 0 || !self.preview_boundaries.is_empty() {
            self.schedule_preview_clear_turn();
        }
    }

    fn schedule_preview_install_slice(self: &Rc<Self>) {
        let state_weak = Rc::downgrade(self);
        let delay_ms = preview_install_delay();
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
        if let PreviewPhase::Installing { session, .. } = &mut *self.preview_phase.borrow_mut() {
            session.source_id = Some(source_id);
        } else {
            source_id.remove();
        }
    }

    fn run_preview_install_slice(self: &Rc<Self>) {
        let (mut session, profile) = match self.preview_phase.replace(PreviewPhase::Idle) {
            PreviewPhase::Installing { session, profile } => (session, profile),
            other => {
                self.preview_phase.replace(other);
                return;
            }
        };
        session.source_id = None;
        if self.is_disposed() || !self.preview_loads.borrow().is_current(session.generation) {
            drop(session.snapshot);
            record_preview_install_cancellation();
            return;
        }
        if matches!(*self.clear_phase.borrow(), ClearPhase::Running(_)) {
            self.preview_phase
                .replace(PreviewPhase::Installing { session, profile });
            self.schedule_preview_install_slice();
            return;
        }
        let text = session.snapshot.text.as_str();
        // One hidden-tail insert per main-loop iteration, process-wide
        // (`ui/protected_fold.rs`): a deferred turn waits for the next one.
        match install_slice(
            self.preview_buffer.upcast_ref(),
            text,
            &mut session.offset,
            &mut session.fold,
            &self.preview_boundaries,
        ) {
            InstallSlice::Deferred => {
                self.preview_phase
                    .replace(PreviewPhase::Installing { session, profile });
                self.schedule_preview_install_slice();
                return;
            }
            InstallSlice::Broken => {
                drop(session.snapshot);
                PREVIEW_BROKEN_INSTALLS.fetch_add(1, Ordering::Release);
                record_preview_install_cancellation();
                self.clear_preview_buffer();
                self.show_preview_error("Snapshot Could Not Be Shown");
                return;
            }
            InstallSlice::Inserted => {}
        }
        PREVIEW_INSTALL_SLICES.fetch_add(1, Ordering::Release);

        if policy::preview_install_is_complete(session.offset, session.snapshot.text.len()) {
            self.finish_preview_install(session.snapshot, profile);
        } else {
            self.preview_phase
                .replace(PreviewPhase::Installing { session, profile });
            self.schedule_preview_install_slice();
        }
    }

    fn finish_preview_install(&self, snapshot: LoadedSnapshot, profile: DocumentProfile) {
        set_preview_stack_state(
            &self.preview_stack,
            PreviewStackState::Content,
            &format!(
                "Snapshot from {}",
                format_history_time(snapshot.meta.captured_at_millis)
            ),
        );
        set_local_history_action_enabled(&self.copy_button, true);
        set_local_history_action_enabled(&self.restore_button, true);
        self.preview_phase
            .replace(PreviewPhase::Loaded { snapshot, profile });
    }

    fn retire_active_preview(&self) {
        if matches!(*self.preview_phase.borrow(), PreviewPhase::Disposed) {
            return;
        }
        retire_preview_phase(self.preview_phase.replace(PreviewPhase::Idle));
    }

    /// Put a snapshot a restore did not consume back as the loaded one, and
    /// route the folded preview's copies through it again.
    pub(super) fn return_loaded_snapshot(&self, snapshot: GuardedLocalHistorySnapshot) {
        if self.is_disposed() {
            return;
        }
        let snapshot: LoadedSnapshot = Arc::new(snapshot);
        let profile = self.loaded_profile();
        self.preview_copy
            .set_folded_source(profile.fold_plan.clone().map(|plan| {
                let weak = Arc::downgrade(&snapshot);
                let source: std::sync::Weak<dyn SurfaceText> = weak;
                (source, plan)
            }));
        self.preview_phase
            .replace(PreviewPhase::Loaded { snapshot, profile });
    }

    /// Run `then` with the loaded snapshot taken back owned (a restore moves
    /// it into the editor), retrying if a folded copy worker still holds a ref.
    pub(super) fn with_owned_loaded_snapshot(
        self: &Rc<Self>,
        then: impl FnOnce(&Rc<Self>, GuardedLocalHistorySnapshot) + 'static,
    ) {
        let (snapshot, profile) = match self.preview_phase.replace(PreviewPhase::Idle) {
            PreviewPhase::Loaded { snapshot, profile } => (snapshot, profile),
            other => {
                self.preview_phase.replace(other);
                return;
            }
        };
        match Arc::try_unwrap(snapshot) {
            Ok(snapshot) => {
                self.preview_phase
                    .replace(PreviewPhase::RestoringSnapshot { profile });
                self.preview_copy.set_folded_source(None);
                then(self, snapshot);
            }
            Err(shared) => {
                self.preview_phase.replace(PreviewPhase::Loaded {
                    snapshot: shared,
                    profile,
                });
                let weak = Rc::downgrade(self);
                glib::timeout_add_local_once(
                    Duration::from_millis(OWNED_SNAPSHOT_RETRY_MS),
                    move || {
                        if let Some(state) = weak.upgrade()
                            && !state.is_disposed()
                        {
                            state.with_owned_loaded_snapshot(then);
                        }
                    },
                );
            }
        }
    }

    fn dispose_preview_runtime(&self) {
        if self.is_disposed() {
            return;
        }
        self.preview_loads.borrow_mut().invalidate();
        self.preview_copy.set_folded_source(None);
        self.preview_capacity_wakeup.cancel();
        self.dispose_restore_capture();
        self.restore_capacity_wakeup.cancel();
        if let ClearPhase::Running(source) = self.clear_phase.replace(ClearPhase::Idle) {
            source.remove();
        }
        retire_preview_phase(self.preview_phase.replace(PreviewPhase::Disposed));
        set_local_history_action_enabled(&self.restore_button, false);
        set_local_history_action_enabled(&self.copy_button, false);
    }
}

fn retire_preview_phase(phase: PreviewPhase) {
    match phase {
        PreviewPhase::Installing { mut session, .. } => {
            if let Some(source_id) = session.source_id.take() {
                source_id.remove();
            }
            drop(session.snapshot);
            record_preview_install_cancellation();
        }
        PreviewPhase::Loaded { snapshot, .. } => drop(snapshot),
        PreviewPhase::Idle
        | PreviewPhase::AwaitingAdmission(_)
        | PreviewPhase::RestoringSnapshot { .. }
        | PreviewPhase::Disposed => {}
    }
}

/// Characters a preview buffer may hold and still be emptied in one
/// `set_text("")`: one ordinary clear slice of the editor's replacement.
///
/// Kind: cap; a power of two, rounded down.
const PREVIEW_DIRECT_CLEAR_CHARS: i32 =
    lushtext_model::document::buffer_replacement::REPLACEMENT_CLEAR_SLICE_CHARS;

/// One bounded clear turn of the preview buffer: the next boundary-ledger
/// ranges while the ledger holds any, then a slice of at most one clear slice
/// of characters extended to the next paragraph start (a whole paragraph is
/// at most one unmarked protected tail slice, or one visible paragraph).
/// Returns the characters (or, for a ledger turn, bytes) deleted.
fn clear_preview_turn(buffer: &gtk4::TextBuffer, ledger: &BoundaryLedger) -> usize {
    if let Some(turn) = ledger.clear_turn(buffer) {
        return usize::try_from(turn.deleted_bytes).unwrap_or(usize::MAX);
    }
    let count =
        lushtext_model::document::buffer_replacement::next_clear_char_count(buffer.char_count());
    if count == 0 {
        return 0;
    }
    let mut start = buffer.start_iter();
    let mut end = start;
    let _ = end.forward_chars(count);
    if !end.is_end() && !end.starts_line() {
        let _ = end.forward_line();
    }
    let deleted = usize::try_from(end.offset()).unwrap_or(0);
    buffer.delete(&mut start, &mut end);
    deleted
}

fn populate_history_sidebar(state: &LocalHistoryBrowserState) {
    let section = libadwaita::SidebarSection::new();
    section.set_title(Some("Snapshots"));
    for meta in &state.snapshots {
        let subtitle = policy::format_snapshot_meta(meta.origin, meta.byte_len);
        section.append(
            libadwaita::SidebarItem::builder()
                .title(format_history_time(meta.captured_at_millis))
                .subtitle(subtitle.clone())
                .tooltip(subtitle)
                .icon_name("document-open-recent-symbolic")
                .build(),
        );
    }
    state.sidebar.append(section);
}

fn padded_vertical_box() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content
}

fn build_history_sidebar(path: &Path, sidebar: &libadwaita::Sidebar) -> gtk4::Box {
    let content = padded_vertical_box();

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
    let content = padded_vertical_box();

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
pub(super) fn set_local_history_action_enabled(button: &gtk4::Button, enabled: bool) {
    button.set_sensitive(enabled);
    accessibility::set_disabled(button, !enabled);
}

#[derive(Clone, Copy)]
enum PreviewStackState {
    Loading,
    Empty,
    Error,
    Content,
}

/// Update the preview stack's visible child and accessible workflow state.
fn set_preview_stack_state(stack: &gtk4::Stack, kind: PreviewStackState, value_text: &str) {
    let (child, busy, invalid) = match kind {
        PreviewStackState::Loading => ("loading", true, false),
        PreviewStackState::Empty => ("empty", false, false),
        PreviewStackState::Error => ("error", false, true),
        PreviewStackState::Content => ("content", false, false),
    };
    stack.set_visible_child_name(child);
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

fn preview_status_page(
    icon_name: &str,
    title: &str,
    description: &str,
    accessible_description: &str,
    invalid: bool,
) -> gtk4::Widget {
    let status = libadwaita::StatusPage::builder()
        .icon_name(icon_name)
        .title(title)
        .description(description)
        .build();
    accessibility::set_role(&status, gtk4::AccessibleRole::Status);
    accessibility::set_labelled_description(&status, title, accessible_description);
    if invalid {
        accessibility::set_invalid(&status, true);
    }
    status.upcast()
}

fn format_history_time(captured_at_millis: u64) -> String {
    i64::try_from(captured_at_millis / 1000)
        .ok()
        .and_then(|secs| glib::DateTime::from_unix_local(secs).ok())
        .and_then(|dt| dt.format("%Y-%m-%d %H:%M").ok())
        .map_or_else(|| "Unknown time".to_string(), Into::into)
}
