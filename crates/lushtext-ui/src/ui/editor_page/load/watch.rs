// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-load workflow's settle watch
//! (`bound-post-open-settle`, design D1, review R25).
//!
//! GtkSourceView and GtkTextView expose no completion signal for highlighting
//! or line validation, so the workflow watches what they do expose: every
//! `GtkSourceBuffer::highlight-updated` of the editor's buffer and every change
//! of its vertical adjustment's `upper` is a **mark**. A mark arms one
//! `G_PRIORITY_LOW` **sentinel** idle; GTK's validation idles and the context
//! engine's first-update idle run at higher priorities, so the sentinel only
//! dispatches in an iteration where none of them was ready. The background
//! analysis worker shares `G_PRIORITY_LOW` (ledger A69) and emits a mark per
//! slice, which `SETTLE_QUIET_US` covers. The pure predicate over these facts
//! is `policy::settle_stage`; the evidence surface projects it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::signal_connect;
use lushtext_model::document::formatting_overrides::FormattingOverrides;
use lushtext_model::values::LoadGeneration;

use super::admission::PendingFileLoad;
use super::execution::ChunkedLoadInstall;
use super::policy::LoadOutcome;

pub(in crate::ui::editor_page) type EditorCallback = Box<dyn Fn()>;
pub(super) type LoadCompletedCallback = Box<dyn FnOnce()>;
pub(super) type LoadFailedCallback = Box<dyn FnOnce(String)>;
pub(super) type LoadPlanningTerminalCallback = Box<dyn FnOnce()>;

/// Whether the settle sentinel idle is queued.
#[derive(Clone, Copy, Default)]
enum SettleSentinelPhase {
    #[default]
    Idle,
    Armed,
}

/// Whether the view vertical adjustment's `upper` notify watch is attached.
#[derive(Clone, Copy, Default)]
enum UpperAdjustmentWatch {
    #[default]
    Detached,
    Attached,
}

/// The settle watch's state on one editor.
#[derive(Default)]
pub struct SettleWatchState {
    /// When the last mark was seen, monotonic µs.
    pub(super) last_mark_us: Cell<Option<i64>>,
    /// When the sentinel last dispatched, monotonic µs.
    pub(super) last_sentinel_us: Cell<Option<i64>>,
    sentinel: Cell<SettleSentinelPhase>,
    upper_watch: Cell<UpperAdjustmentWatch>,
}

/// Background file-load freshness primitives that advance together per load.
#[derive(Default)]
pub struct LoadTrackingState {
    /// Monotonic identity for file loads so stale background completions cannot
    /// apply after a newer open or reopen starts for the same editor tab.
    pub generation: Cell<LoadGeneration>,
    /// Cooperative cancellation token for the current background file load.
    pub cancel_token: RefCell<Arc<AtomicBool>>,
}

/// File-load lifecycle callbacks attached to one editor tab.
#[derive(Default)]
pub(crate) struct LoadCallbacks {
    pub(crate) planning_terminal: RefCell<Option<LoadPlanningTerminalCallback>>,
    pub(crate) completed: RefCell<Option<LoadCompletedCallback>>,
    pub(crate) failed: RefCell<Option<LoadFailedCallback>>,
    pub(crate) file_loaded: RefCell<Vec<EditorCallback>>,
    pub(crate) content_republished: RefCell<Vec<EditorCallback>>,
}

/// Exclusive phase of the editor's active installation and finalization.
#[derive(Default)]
pub(crate) enum LoadInstallActivity {
    #[default]
    Idle,
    Installing(Rc<RefCell<ChunkedLoadInstall>>),
    Finalizing {
        installation: Option<Rc<RefCell<ChunkedLoadInstall>>>,
        dispose_during_finalization: bool,
    },
}

/// Integrity and ownership of buffer content relative to sliced installations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LoadBufferStatus {
    #[default]
    Whole,
    Incomplete,
    InstallOwnsChanges {
        previous_incomplete: bool,
    },
}

impl LoadBufferStatus {
    pub(crate) fn is_incomplete(self) -> bool {
        match self {
            Self::Whole => false,
            Self::Incomplete => true,
            Self::InstallOwnsChanges {
                previous_incomplete,
            } => previous_incomplete,
        }
    }
}

/// Scalar metrics recorded for the newest load installation.
#[derive(Clone, Copy, Default)]
pub(crate) struct LoadInstallMetrics {
    pub(crate) slice_count: u64,
    pub(crate) chunked: bool,
    pub(crate) highlight_chunked: bool,
    pub(crate) formatting_applied_before_insert: bool,
    pub(crate) protected_install: crate::ui::protected_fold::FoldedInstallMetrics,
    pub(crate) protected_clear_turns: u64,
}

/// Latest replacement request waiting for bounded cancellation cleanup.
#[derive(Default)]
pub(crate) enum PendingReloadSlot {
    #[default]
    Empty,
    Parked(Box<PendingFileLoad>),
}

/// EditorConfig overrides resolved by planning, awaiting apply before insert.
#[derive(Clone, Copy, Default)]
pub(crate) enum PendingEditorConfig {
    #[default]
    None,
    Overrides(FormattingOverrides),
}

/// Whether bounded cleanup must publish the user-cancelled terminal state.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum UserCancelPending {
    #[default]
    No,
    PublishTerminal,
}

/// Whether the token carried by the previous request was cancelled at rotation.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum RetiredRequestCancellation {
    #[default]
    Unrecorded,
    Uncancelled,
    Cancelled,
}

impl RetiredRequestCancellation {
    pub(crate) fn into_option(self) -> Option<bool> {
        match self {
            Self::Unrecorded => None,
            Self::Uncancelled => Some(false),
            Self::Cancelled => Some(true),
        }
    }
}

/// Document-load workflow state owned by one editor tab.
#[derive(Default)]
pub struct LoadState {
    pub(crate) callbacks: LoadCallbacks,
    pub(crate) projection_suspended: Cell<bool>,
    pub(crate) install_activity: RefCell<LoadInstallActivity>,
    pub(crate) pending_load: RefCell<PendingReloadSlot>,
    pub(crate) buffer_status: Cell<LoadBufferStatus>,
    pub(crate) user_cancel_pending: Cell<UserCancelPending>,
    pub(crate) metrics: Cell<LoadInstallMetrics>,
    pub(crate) pending_formatting: Cell<PendingEditorConfig>,
    pub settle: SettleWatchState,
    pub(crate) outcome: Cell<LoadOutcome>,
    pub(crate) previous_request_cancelled: Cell<RetiredRequestCancellation>,
}

impl LoadState {
    pub(crate) fn installation(&self) -> Option<Rc<RefCell<ChunkedLoadInstall>>> {
        match &*self.install_activity.borrow() {
            LoadInstallActivity::Idle => None,
            LoadInstallActivity::Installing(session) => Some(Rc::clone(session)),
            LoadInstallActivity::Finalizing { installation, .. } => {
                installation.as_ref().map(Rc::clone)
            }
        }
    }

    pub(crate) fn is_finalizing(&self) -> bool {
        matches!(
            *self.install_activity.borrow(),
            LoadInstallActivity::Finalizing { .. }
        )
    }

    pub(crate) fn dispose_during_finalization(&self) -> bool {
        matches!(
            *self.install_activity.borrow(),
            LoadInstallActivity::Finalizing {
                dispose_during_finalization: true,
                ..
            }
        )
    }

    pub(crate) fn begin_finalization(&self) {
        let mut activity = self.install_activity.borrow_mut();
        let installation = match std::mem::take(&mut *activity) {
            LoadInstallActivity::Idle => None,
            LoadInstallActivity::Installing(session) => Some(session),
            LoadInstallActivity::Finalizing { installation, .. } => installation,
        };
        *activity = LoadInstallActivity::Finalizing {
            installation,
            dispose_during_finalization: false,
        };
    }

    pub(crate) fn mark_dispose_during_finalization(&self) {
        if let LoadInstallActivity::Finalizing {
            dispose_during_finalization,
            ..
        } = &mut *self.install_activity.borrow_mut()
        {
            *dispose_during_finalization = true;
        }
    }

    pub(crate) fn finish_finalization(&self) -> bool {
        let mut activity = self.install_activity.borrow_mut();
        let (installation, disposed) = match std::mem::take(&mut *activity) {
            LoadInstallActivity::Idle => (None, false),
            LoadInstallActivity::Installing(session) => (Some(session), false),
            LoadInstallActivity::Finalizing {
                installation,
                dispose_during_finalization,
            } => (installation, dispose_during_finalization),
        };
        if let Some(session) = installation {
            *activity = LoadInstallActivity::Installing(session);
        }
        disposed
    }

    pub(crate) fn set_installation(&self, session: Rc<RefCell<ChunkedLoadInstall>>) {
        let mut activity = self.install_activity.borrow_mut();
        match &mut *activity {
            LoadInstallActivity::Finalizing { installation, .. } => {
                *installation = Some(session);
            }
            LoadInstallActivity::Idle | LoadInstallActivity::Installing(_) => {
                *activity = LoadInstallActivity::Installing(session);
            }
        }
    }

    pub(crate) fn clear_installation_if_current(
        &self,
        session: &Rc<RefCell<ChunkedLoadInstall>>,
    ) -> bool {
        let mut activity = self.install_activity.borrow_mut();
        match &mut *activity {
            LoadInstallActivity::Installing(current) if Rc::ptr_eq(current, session) => {
                *activity = LoadInstallActivity::Idle;
                true
            }
            LoadInstallActivity::Finalizing { installation, .. }
                if installation
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, session)) =>
            {
                *installation = None;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn mark_install_owns_changes(&self) {
        let previous_incomplete = self.buffer_status.get().is_incomplete();
        self.buffer_status
            .set(LoadBufferStatus::InstallOwnsChanges {
                previous_incomplete,
            });
    }

    pub(crate) fn mark_installation_incomplete(&self) {
        let next = match self.buffer_status.get() {
            LoadBufferStatus::Whole | LoadBufferStatus::Incomplete => LoadBufferStatus::Incomplete,
            LoadBufferStatus::InstallOwnsChanges { .. } => LoadBufferStatus::InstallOwnsChanges {
                previous_incomplete: true,
            },
        };
        self.buffer_status.set(next);
    }

    pub(crate) fn clear_install_owns_changes_keeping_incomplete(&self) {
        let next = if self.buffer_status.get().is_incomplete() {
            LoadBufferStatus::Incomplete
        } else {
            LoadBufferStatus::Whole
        };
        self.buffer_status.set(next);
    }

    pub(crate) fn update_metrics(&self, f: impl FnOnce(&mut LoadInstallMetrics)) {
        let mut m = self.metrics.get();
        f(&mut m);
        self.metrics.set(m);
    }
}

impl LushtextEditorPage {
    /// Install the settle watch's handlers (from `constructed`). The
    /// adjustment is watched once the view is first mapped, when its
    /// scrolled window has given it one.
    pub(crate) fn setup_settle_watch(&self) {
        let editor = self.downgrade();
        signal_connect::connect_void(&self.buffer(), "highlight-updated", false, move |_| {
            if let Some(editor) = editor.upgrade() {
                editor.record_settle_mark();
            }
        });
        let editor = self.downgrade();
        self.imp().source_view.connect_map(move |view| {
            let Some(editor) = editor.upgrade() else {
                return;
            };
            let watch = &editor.imp().load.settle;
            if matches!(watch.upper_watch.get(), UpperAdjustmentWatch::Attached) {
                return;
            }
            watch.upper_watch.set(UpperAdjustmentWatch::Attached);
            if let Some(adjustment) = view.vadjustment() {
                let editor = editor.downgrade();
                adjustment.connect_upper_notify(move |_| {
                    if let Some(editor) = editor.upgrade() {
                        editor.record_settle_mark();
                    }
                });
            }
        });
    }

    /// Record one mark and arm the sentinel; post-open stages call it too,
    /// so a stage with no toolkit follow-up still needs a later sentinel.
    pub(crate) fn record_settle_mark(&self) {
        let watch = &self.imp().load.settle;
        watch.last_mark_us.set(Some(glib::monotonic_time()));
        if matches!(watch.sentinel.get(), SettleSentinelPhase::Armed) {
            return;
        }
        watch.sentinel.set(SettleSentinelPhase::Armed);
        let editor = self.downgrade();
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(editor) = editor.upgrade() {
                let watch = &editor.imp().load.settle;
                watch.sentinel.set(SettleSentinelPhase::Idle);
                watch.last_sentinel_us.set(Some(glib::monotonic_time()));
            }
            glib::ControlFlow::Break
        });
    }
}
