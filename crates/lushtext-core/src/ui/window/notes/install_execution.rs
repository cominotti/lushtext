// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination role `execution`, stage-qualified for the note editors' body
//! install (`protected-view-inspector-and-surfaces` review M3).
//!
//! A note body can be up to 16 MiB. One that could hold a paragraph over
//! `L_PROTECT_EDIT` (it has more bytes than that) is never profiled or
//! installed in the dispatch that builds the editor:
//!
//! 1. **Profile on a worker.** The editor opens empty, read-only and caret-free
//!    with a "Loading note…" line; the body is profiled
//!    (`DocumentProfile::of_text`) on a worker. *Resumes* in that worker's
//!    completion.
//! 2. **Install sliced.** The body goes into the buffer in bounded turns
//!    through the history preview's slice step (`folded_text::install_slice`),
//!    folded when the profile carries a fold plan, one hidden tail per
//!    main-loop iteration process-wide. *Resumes* in one idle per turn.
//! 3. **Open.** A folded note stays read-only, says why, and routes its copies
//!    through its text (`folded_text::FoldedTextSurface`); any other note
//!    becomes editable. Work queued with [`NoteBody::when_installed`] (the
//!    Save sensitivity, the pre-rendered Render page) runs now.
//!
//! A small body installs in the building dispatch, exactly as before. Reads of
//! the note's text ([`NoteBody::snapshot_text`]) go to the source for a folded
//! note: the buffer snapshot is quadratic inside a hidden tail (ledger A66).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gtk4::prelude::*;

use crate::model::document_scale::L_PROTECT_EDIT;
use crate::model::paragraph_boundary::DocumentProfile;
use crate::model::protected_fold::FoldPlan;
use crate::ui::buffer_snapshot;
use crate::ui::folded_text::{FoldedTextSurface, InstallSlice, SurfaceText, install_slice};
use crate::ui::protected_fold::{BoundaryLedger, FoldedInstall};

/// Note-body install turns run, process-wide (evidence).
pub(super) static NOTE_BODY_INSTALL_SLICES: AtomicU64 = AtomicU64::new(0);
/// The longest note-body install turn, in microseconds, process-wide.
pub(super) static NOTE_BODY_LARGEST_SLICE_US: AtomicU64 = AtomicU64::new(0);

/// Accessible and visible copy of a folded note.
const FOLDED_NOTE_NOTICE: &str =
    "This note has very long lines, so it opens read-only with long lines shortened.";
const LOADING_NOTE_NOTICE: &str = "Loading note…";

/// Work waiting for the body to be installed; it receives the installed text
/// when the install was deferred (`None` for a body installed at once).
type InstalledCallback = Box<dyn FnOnce(&Rc<NoteBody>, Option<&Arc<String>>)>;

/// One note editor's body: its view, its install, and where its text is read.
pub(super) struct NoteBody {
    view: gtk4::TextView,
    notice: gtk4::Label,
    copy: Rc<FoldedTextSurface>,
    /// The text of a folded note, which is read-only: the source its copies,
    /// renders and saves read.
    folded: RefCell<Option<Arc<String>>>,
    /// `Some` while a deferred install runs: the work waiting for it.
    waiting: RefCell<Option<Vec<InstalledCallback>>>,
    installing: Cell<bool>,
}

impl NoteBody {
    /// Build the body of a note editor over `view`, installing `text` now when
    /// it is small, or starting the deferred install otherwise. `notice` is the
    /// label the editor shows above its body.
    pub(super) fn install(view: &gtk4::TextView, notice: &gtk4::Label, text: &str) -> Rc<Self> {
        let body = Rc::new(Self {
            view: view.clone(),
            notice: notice.clone(),
            copy: FoldedTextSurface::attach(view),
            folded: RefCell::new(None),
            waiting: RefCell::new(None),
            installing: Cell::new(false),
        });
        // A body of at most `L_PROTECT_EDIT` bytes cannot hold a paragraph of
        // more than `L_PROTECT_EDIT` characters: no profile is needed.
        let threshold = usize::try_from(L_PROTECT_EDIT).unwrap_or(usize::MAX);
        if text.len() <= threshold {
            view.buffer().set_text(text);
            notice.set_visible(false);
            return body;
        }
        body.installing.set(true);
        body.waiting.replace(Some(Vec::new()));
        view.set_editable(false);
        view.set_cursor_visible(false);
        notice.set_label(LOADING_NOTE_NOTICE);
        notice.set_visible(true);
        // One copy of the body leaves the GTK thread here (a memcpy); the
        // profile, and so the fold plan, is computed on the worker.
        let owned = Arc::new(text.to_owned());
        let weak = Rc::downgrade(&body);
        gtk_lush_tasks::spawn_blocking_then(
            view.clone(),
            move || {
                let plan = DocumentProfile::of_text(&owned).fold_plan;
                (owned, plan)
            },
            move |_view, (text, plan)| {
                if let Some(body) = weak.upgrade() {
                    body.start_sliced_install(text, plan);
                }
            },
        );
        body
    }

    /// Run `then` once the body is installed (now, when it already is).
    pub(super) fn when_installed(
        self: &Rc<Self>,
        then: impl FnOnce(&Rc<Self>, Option<&Arc<String>>) + 'static,
    ) {
        if let Some(waiting) = self.waiting.borrow_mut().as_mut() {
            waiting.push(Box::new(then));
            return;
        }
        then(self, None);
    }

    /// The note's buffer.
    pub(super) fn buffer(&self) -> gtk4::TextBuffer {
        self.view.buffer()
    }

    /// Whether the deferred install is still running.
    pub(super) fn is_installing(&self) -> bool {
        self.installing.get()
    }

    /// Read the note's text: from the folded note's source on a worker, else
    /// from the buffer (chunked when large, `snapshot_note_buffer_text`).
    pub(super) fn snapshot_text<F: FnOnce(buffer_snapshot::BufferSnapshotOutcome) + 'static>(
        &self,
        callback: F,
    ) -> Option<buffer_snapshot::BufferSnapshotHandle> {
        if let Some(text) = self.folded.borrow().clone() {
            gtk_lush_tasks::spawn_blocking_then(
                self.view.clone(),
                move || String::clone(&text),
                move |_view, text| {
                    callback(buffer_snapshot::BufferSnapshotOutcome::Captured(
                        buffer_snapshot::BufferSnapshotPayload::direct(text),
                    ));
                },
            );
            return None;
        }
        super::editor_execution::snapshot_note_buffer_text(self.view.buffer(), callback)
    }

    fn start_sliced_install(self: &Rc<Self>, text: Arc<String>, plan: Option<Arc<FoldPlan>>) {
        if let Some(plan) = plan.as_ref() {
            *self.folded.borrow_mut() = Some(Arc::clone(&text));
            let weak = Arc::downgrade(&text);
            let source: std::sync::Weak<dyn SurfaceText> = weak;
            self.copy
                .set_folded_source(Some((source, Arc::clone(plan))));
        }
        let session = InstallSession {
            body: Rc::downgrade(self),
            text,
            offset: 0,
            fold: plan.map(|plan| FoldedInstall::new(plan, &self.view.buffer(), false)),
            boundaries: BoundaryLedger::default(),
        };
        session.schedule();
    }

    fn finish_install(self: &Rc<Self>, text: &Arc<String>) {
        self.installing.set(false);
        if self.folded.borrow().is_some() {
            self.notice.set_label(FOLDED_NOTE_NOTICE);
            crate::ui::accessibility::set_labelled_description(
                &self.view,
                "Note body, read-only",
                "This note has very long lines, so it opens read-only with long lines shortened",
            );
        } else {
            self.notice.set_visible(false);
            self.view.set_editable(true);
            self.view.set_cursor_visible(true);
        }
        let waiting = self.waiting.take().unwrap_or_default();
        for then in waiting {
            then(self, Some(text));
        }
    }
}

impl std::fmt::Debug for NoteBody {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NoteBody")
            .field("installing", &self.installing.get())
            .field("folded", &self.folded.borrow().is_some())
            .finish()
    }
}

/// One deferred install in flight.
struct InstallSession {
    body: Weak<NoteBody>,
    text: Arc<String>,
    offset: usize,
    fold: Option<FoldedInstall>,
    /// The boundary marks of the sliced protected install (C3). A note body
    /// is installed once and never cleared by an install, so they stay in the
    /// buffer until it is freed, as inert marks.
    boundaries: BoundaryLedger,
}

impl InstallSession {
    fn schedule(self) {
        glib::idle_add_local_once(move || self.run_turn());
    }

    fn run_turn(mut self) {
        // The editor closed: drop the text (its last owner may be this turn).
        let Some(body) = self.body.upgrade() else {
            return;
        };
        let started_us = glib::monotonic_time();
        let step = install_slice(
            &body.view.buffer(),
            &self.text,
            &mut self.offset,
            &mut self.fold,
            &self.boundaries,
        );
        if step == InstallSlice::Inserted {
            let ended_us = glib::monotonic_time();
            crate::ui::responsiveness_probe::record_span(
                "note_body_install_slice",
                started_us,
                ended_us,
            );
            NOTE_BODY_INSTALL_SLICES.fetch_add(1, Ordering::Release);
            let elapsed = u64::try_from(ended_us.saturating_sub(started_us)).unwrap_or(0);
            NOTE_BODY_LARGEST_SLICE_US.fetch_max(elapsed, Ordering::AcqRel);
        }
        if self.offset >= self.text.len() {
            let text = Arc::clone(&self.text);
            drop(self);
            body.finish_install(&text);
            return;
        }
        self.schedule();
    }
}
