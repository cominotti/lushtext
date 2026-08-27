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

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::services::json_store;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, SidebarItemExt};

use crate::model::buffer_replacement::next_replacement_boundary;
use crate::model::local_history::{LocalHistorySnapshot, LocalHistorySnapshotMeta};
use crate::services::local_history_service;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use crate::ui::{accessibility, buffer_snapshot};

use super::policy;
use crate::ui::window::LushtextWindow;

/// Current window width, falling back to the configured default before the
/// window is mapped in widget tests or at startup.
fn current_window_width(window: &LushtextWindow) -> i32 {
    policy::current_window_dimension(window.width(), window.default_size().0)
}

/// Current window height, with the same pre-map fallback.
fn current_window_height(window: &LushtextWindow) -> i32 {
    policy::current_window_dimension(window.height(), window.default_size().1)
}

pub(super) type GuardedLocalHistorySnapshot =
    crate::ui::plain_disposal::DisposalOwned<LocalHistorySnapshot>;

pub(super) enum GuardedLocalHistoryPreviewLoadOutcome {
    Loaded(GuardedLocalHistorySnapshot),
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
    result: anyhow::Result<local_history_service::LocalHistoryPreviewLoadOutcome>,
    reservation: crate::ui::plain_disposal::DisposalReservation,
) -> anyhow::Result<GuardedLocalHistoryPreviewLoadOutcome> {
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
            Ok(GuardedLocalHistoryPreviewLoadOutcome::Loaded(
                reservation.shrink_to_and_own(weight, snapshot),
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
#[cfg(feature = "test-utils")]
fn preview_install_delay() -> u64 {
    super::test_policy::preview_install_delay_ms()
}

#[cfg(not(feature = "test-utils"))]
const fn preview_install_delay() -> u64 {
    0
}

fn record_preview_install_slice() {
    PREVIEW_INSTALL_SLICES.fetch_add(1, Ordering::Release);
}

fn record_preview_install_cancellation() {
    PREVIEW_INSTALL_CANCELLATIONS.fetch_add(1, Ordering::Release);
}

/// UI state for one open local-history browser dialog.
pub(super) struct LocalHistoryBrowserState {
    /// Window that owns the dialog and receives status updates.
    pub(super) window: LushtextWindow,
    /// Active editor the browser belongs to.
    pub(super) editor: LushtextEditorPage,
    /// Saved path whose lineage is being browsed.
    pub(super) path: PathBuf,
    /// Dialog containing the browser widgets.
    pub(super) dialog: libadwaita::Dialog,
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
    pub(super) restore_button: gtk4::Button,
    /// Copy action for the selected snapshot text.
    pub(super) copy_button: gtk4::Button,
    /// Back button shown when the adaptive split view collapses.
    back_button: gtk4::Button,
    /// Snapshot metadata backing the current list rows.
    snapshots: Vec<LocalHistorySnapshotMeta>,
    /// Last fully loaded snapshot preview.
    pub(super) loaded_snapshot: RefCell<Option<GuardedLocalHistorySnapshot>>,
    /// One-active/one-latest ownership for snapshot body reads.
    preview_loads: RefCell<local_history_service::LocalHistoryPreviewCoordinator>,
    /// Active compact selection waiting for disposal admission before disk I/O.
    preview_admission: RefCell<Option<local_history_service::LocalHistoryPreviewStart>>,
    /// One paced capacity wakeup for the selected snapshot.
    preview_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    /// Current browser-local sliced GTK installation, when required.
    preview_install: RefCell<Option<LocalHistoryPreviewInstallSession>>,
    /// Whether dialog teardown invalidated all preview work.
    disposed: Cell<bool>,
    /// Active safety capture for a pending restore, disposed with the dialog.
    pub(super) restore_snapshot: RefCell<Option<buffer_snapshot::BufferSnapshotHandle>>,
    /// Compact current restore intent retained while progress capacity is full.
    pub(super) restore_pending: Cell<bool>,
    /// One paced progress-capacity wakeup for the pending restore intent.
    pub(super) restore_capacity_wakeup: crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
}

struct LocalHistoryPreviewInstallSession {
    generation: u64,
    snapshot: Option<GuardedLocalHistorySnapshot>,
    offset: usize,
    source_id: Option<glib::SourceId>,
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

        let (dialog_width, dialog_height) =
            policy::viewer_dialog_size(current_window_width(self), current_window_height(self));
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
        split_view.set_min_sidebar_width(policy::VIEWER_MIN_SIDEBAR_WIDTH_SP);
        split_view.set_max_sidebar_width(policy::VIEWER_MAX_SIDEBAR_WIDTH_SP);
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
            preview_admission: RefCell::default(),
            preview_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup::default(),
            preview_install: RefCell::new(None),
            disposed: Cell::new(false),
            restore_snapshot: RefCell::new(None),
            restore_pending: Cell::new(false),
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

    pub(super) fn build_empty_local_history_dialog(path: &Path) -> libadwaita::Dialog {
        let dialog = libadwaita::Dialog::builder()
            .title("Local History")
            .content_width(policy::EMPTY_WIDTH_SP)
            .content_height(policy::EMPTY_HEIGHT_SP)
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
}

impl LocalHistoryBrowserState {
    fn load_preview_for_index(self: &Rc<Self>, index: usize, user_selected: bool) {
        let Some(meta) = self.snapshots.get(index).cloned() else {
            return;
        };

        self.restore_pending.set(false);
        self.restore_capacity_wakeup.cancel();
        self.cancel_preview_install();
        if let Some(snapshot) = self.loaded_snapshot.take() {
            retire_local_history_snapshot(snapshot);
        }
        self.preview_title.set_label("Loading snapshot…");
        self.preview_meta
            .set_label(&policy::format_snapshot_meta(meta.origin, meta.byte_len));
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
            debug_assert!(self.preview_admission.borrow().is_none());
            self.preview_admission.replace(Some(start));
            let state_weak = Rc::downgrade(self);
            self.preview_capacity_wakeup.arm(observed_epoch, move || {
                if let Some(state) = state_weak.upgrade() {
                    state.retry_preview_admission();
                }
            });
            self.preview_title.set_label("Preview deferred");
            self.preview_meta
                .set_label("Waiting for memory pressure to clear");
            set_local_history_preview_accessibility(
                &self.preview_stack,
                "Snapshot preview deferred by memory pressure",
                true,
                false,
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
                    retire_local_history_preview_result(result);
                    return;
                };
                state.finish_preview_load(generation, result);
            },
        );
    }

    fn retry_preview_admission(self: &Rc<Self>) {
        let Some(start) = self.preview_admission.borrow_mut().take() else {
            return;
        };
        self.start_preview_load(start);
    }

    fn finish_cancelled_preview_admission(self: &Rc<Self>) {
        let cancelled = self
            .preview_admission
            .borrow()
            .as_ref()
            .is_some_and(|start| start.cancellation.is_cancelled());
        if !cancelled {
            return;
        }
        self.preview_capacity_wakeup.cancel();
        let start = self.preview_admission.borrow_mut().take();
        if let Some(start) = start {
            self.finish_preview_load(
                start.generation,
                Ok(GuardedLocalHistoryPreviewLoadOutcome::Cancelled),
            );
        }
    }

    fn finish_preview_load(
        self: &Rc<Self>,
        generation: u64,
        result: anyhow::Result<GuardedLocalHistoryPreviewLoadOutcome>,
    ) {
        let (accepted, next) = {
            let mut loads = self.preview_loads.borrow_mut();
            let accepted = loads.is_current(generation) && !self.disposed.get();
            let next = loads.finish(generation);
            (accepted, next)
        };
        if accepted {
            match result {
                Ok(GuardedLocalHistoryPreviewLoadOutcome::Loaded(snapshot)) => {
                    self.begin_preview_install(generation, snapshot);
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
            retire_local_history_preview_result(result);
        }
        if let Some(next) = next {
            self.start_preview_load(next);
        }
    }

    /// Show the preview pane's error state under one title.
    ///
    /// The two failing outcomes — a snapshot whose body is gone, and a read that
    /// errored — present identically apart from that title. The title is also the
    /// accessible name, so one helper is what keeps the visible label and the
    /// announced name from drifting apart.
    fn show_preview_error(&self, title: &str) {
        self.preview_title.set_label(title);
        self.preview_meta.set_label("");
        self.preview_stack.set_visible_child_name("error");
        set_local_history_preview_accessibility(&self.preview_stack, title, false, true);
    }

    fn begin_preview_install(
        self: &Rc<Self>,
        generation: u64,
        snapshot: GuardedLocalHistorySnapshot,
    ) {
        self.preview_title
            .set_label(&format_history_time(snapshot.meta.captured_at_millis));
        self.preview_meta.set_label(&policy::format_snapshot_meta(
            snapshot.meta.origin,
            snapshot.meta.byte_len,
        ));
        match policy::preview_install_plan(snapshot.text.len()) {
            policy::PreviewInstallPlan::Empty => {
                self.preview_buffer.set_text("");
                self.preview_stack.set_visible_child_name("empty");
                set_local_history_preview_accessibility(
                    &self.preview_stack,
                    "This snapshot was empty",
                    false,
                    false,
                );
                // Copy stays disabled because there is nothing to copy, while
                // Restore stays enabled: restoring to an empty document is a
                // legitimate thing the user may want.
                set_local_history_action_enabled(&self.copy_button, false);
                set_local_history_action_enabled(&self.restore_button, true);
                self.loaded_snapshot.replace(Some(snapshot));
            }
            policy::PreviewInstallPlan::Direct => {
                self.preview_buffer.set_text(&snapshot.text);
                self.finish_preview_install(snapshot);
            }
            policy::PreviewInstallPlan::Sliced => {
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
            record_preview_install_cancellation();
            return;
        }
        let Some(snapshot) = session.snapshot.as_ref() else {
            return;
        };
        let end = next_replacement_boundary(&snapshot.text, session.offset);
        debug_assert!(end.saturating_sub(session.offset) <= policy::PREVIEW_INSTALL_SLICE_BYTES);
        let mut end_iter = self.preview_buffer.end_iter();
        self.preview_buffer
            .insert(&mut end_iter, &snapshot.text[session.offset..end]);
        session.offset = end;
        record_preview_install_slice();

        if policy::preview_install_is_complete(session.offset, snapshot.text.len()) {
            let snapshot = session.snapshot.take().expect("install snapshot exists");
            self.finish_preview_install(snapshot);
        } else {
            self.preview_install.replace(Some(session));
            self.schedule_preview_install_slice();
        }
    }

    fn finish_preview_install(&self, snapshot: GuardedLocalHistorySnapshot) {
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
        record_preview_install_cancellation();
    }

    fn dispose_preview_runtime(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.preview_loads.borrow_mut().invalidate();
        self.preview_admission.borrow_mut().take();
        self.preview_capacity_wakeup.cancel();
        self.restore_pending.set(false);
        self.restore_capacity_wakeup.cancel();
        self.cancel_preview_install();
        if let Some(snapshot) = self.loaded_snapshot.take() {
            retire_local_history_snapshot(snapshot);
        }
        set_local_history_action_enabled(&self.restore_button, false);
        set_local_history_action_enabled(&self.copy_button, false);
    }
}

fn retire_local_history_preview_result(
    result: anyhow::Result<GuardedLocalHistoryPreviewLoadOutcome>,
) {
    if let Ok(GuardedLocalHistoryPreviewLoadOutcome::Loaded(snapshot)) = result {
        drop(snapshot);
    }
}

fn retire_local_history_snapshot(snapshot: GuardedLocalHistorySnapshot) {
    drop(snapshot);
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
    let subtitle = policy::format_snapshot_meta(meta.origin, meta.byte_len);
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
pub(super) fn set_local_history_action_enabled(button: &gtk4::Button, enabled: bool) {
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
