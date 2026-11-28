// SPDX-License-Identifier: GPL-3.0-or-later

//! The language attach stage of one editor (`bound-post-open-settle`, design
//! D2, review R1, R2, R6, R8, R9).
//!
//! **Cross-cutting, no role**, beside `document_identity.rs`: the language is
//! a projection of the path that the document-load workflow
//! (`WFR-DOCUMENT-LOAD`, publish), the whole-buffer replacement terminal
//! (`WFR-BUFFER-REPLACEMENT`), the document scale (`WFR-DOCUMENT-SCALE`) and
//! the identity operations (Save As, rename) all ask for. Its pure decisions
//! (`deferred_attach_may_run`, `INPUT_QUIET_US`, `ATTACH_DEADLINE_US`) are in
//! the load workflow's `policy.rs`.
//!
//! Attaching a language with embedded definitions parses them: about 37 ms
//! for HTML, once per language per process, and that parse cannot leave the
//! GTK thread (ledger A67, a toolkit-floor exception with an upstream
//! report). So every site that sets a language calls
//! `LushtextEditorPage::request_language_attach`, which records the wanted
//! id and schedules **one** attach stage:
//!
//! 1. after the editor's first frame, or, for an editor that is not mapped (a
//!    restored tab that is not selected), after its first frame once mapped;
//! 2. from a `G_PRIORITY_LOW` idle, so no input event source is ready when it
//!    runs, and only after `INPUT_QUIET_US` without a key, button, scroll or
//!    touch (`ui::input_quiet`), or once `ATTACH_DEADLINE_US` has passed;
//! 3. once the application's language catalog is ready
//!    (`ui::language_catalog`), whose language the stage takes, re-guessing
//!    from the path there: GtkSourceView's own guess is authoritative.
//!
//! A newer request (another load, reload, rename) replaces a pending one
//! through the generation, so a stale language is never attached. While the
//! stage is pending, [`LushtextEditorPage::document_language_id`] reports the
//! requested id, so the Markdown preview and the status bar already know the
//! document's kind. Clearing a language for the scale gate stays immediate,
//! because removing a language costs nothing (A27).
//!
//! ## The chunked highlighted install (`chunked-highlight-attach`, C6b)
//!
//! A document above its language's attach-after-install threshold is
//! installed with its language attached (`model::highlight_chunking`): the
//! load or the whole-buffer replacement calls `begin_chunked_highlight`
//! before clearing, inserts the first chunk, and then asks
//! `decide_highlight_chunk` before every later one. That requests the attach
//! once through `request_language_attach_for_install` (the same stage; it
//! ignores the session scale, which the incoming body replaces only at
//! publish and does not wait for an unmapped editor to be mapped; in the
//! editor the user types into, shown and focused, it does not wait for
//! input-quiet either, since the install holds those keys), parks the
//! installer on `when_language_attach_resolves`, and afterwards lets a chunk
//! in only once the engine's largest `highlight-updated` end has reached the
//! end of the buffer's text (ledger A84; `reset_highlight_coverage`,
//! `highlight_analysed_to_end`), or after one wait at `analysis_wait_priority`.

use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use lushtext_model::values::LanguageAttachGeneration;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::load::policy::{ATTACH_DEADLINE_US, INPUT_QUIET_US, deferred_attach_may_run};
use crate::ui::responsiveness_probe::PostOpenSpan;
use lushtext_model::document::buffer_replacement::SliceBudget;
use lushtext_model::document::highlight_chunking::{ChunkFacts, ChunkGate, chunk_gate};

/// What the editor knows about the language its document wants.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum RequestedLanguage {
    /// Nothing guessed yet: the catalog was not ready when the path was seen.
    #[default]
    Unknown,
    /// The path names no language.
    Plain,
    /// The path names this language id.
    Id(String),
}

impl RequestedLanguage {
    fn from_guess(id: Option<&str>) -> Self {
        id.map_or(Self::Plain, |id| Self::Id(id.to_owned()))
    }

    /// The requested id, `None` when plain or unknown.
    pub(crate) fn id(&self) -> Option<&str> {
        match self {
            Self::Id(id) => Some(id),
            Self::Plain | Self::Unknown => None,
        }
    }
}

#[derive(Default)]
enum AttachWaitStep {
    #[default]
    Idle,
    AwaitingMap,
    AwaitingFirstFrame(gtk_lush_signals::SignalSlot),
    AwaitingQuiet {
        armed_since_us: i64,
        source: Option<glib::SourceId>,
    },
}

impl AttachWaitStep {
    fn retire(self) {
        match self {
            Self::AwaitingFirstFrame(slot) => slot.clear(),
            Self::AwaitingQuiet {
                source: Some(source),
                ..
            } => source.remove(),
            Self::Idle | Self::AwaitingMap | Self::AwaitingQuiet { source: None, .. } => {}
        }
    }
}

#[derive(Default)]
enum AttachStagePhase {
    #[default]
    Idle,
    Pending {
        for_install: bool,
        step: AttachWaitStep,
        waiters: Vec<Box<dyn FnOnce()>>,
    },
}

#[derive(Clone, Copy, Default)]
enum HighlightCoverage {
    #[default]
    Unwatched,
    Watched {
        covered_to: i32,
    },
}

#[derive(Clone, Copy, Default)]
struct ChunkedHighlightOwners {
    load: bool,
    replacement: bool,
}

/// One editor's attach-stage state.
#[derive(Default)]
pub struct LanguageAttachState {
    requested: RefCell<RequestedLanguage>,
    /// Advanced by every request and cancellation; a stage of an older one
    /// does nothing.
    generation: Cell<LanguageAttachGeneration>,
    /// When the editor's first frame after it was mapped was painted.
    first_frame_us: Cell<Option<i64>>,
    phase: RefCell<AttachStagePhase>,
    map_handler: gtk_lush_signals::SignalSlot,
    /// Evidence: attach stages that ran, and of those how many ran past the
    /// deadline rather than after input-quiet.
    stages_run: Cell<u64>,
    deadline_runs: Cell<u64>,
    /// The largest `highlight-updated` end since the last reset (ledger A84).
    coverage: Cell<HighlightCoverage>,
    /// Evidence: attach stages a chunked install requested.
    install_requests: Cell<u64>,
    /// Evidence: chunks let in because the engine reported the buffer
    /// analysed (A84), and because one `G_PRIORITY_LOW + 1` wait had passed.
    chunks_on_coverage: Cell<u64>,
    chunks_after_wait: Cell<u64>,
    /// Whether a chunked highlighted install owns the buffer: set by the
    /// load or the replacement when it starts one, cleared at its terminal or
    /// when it drops the highlight (review S-c: an explicit fact, never
    /// inferred from a borrowed session).
    owners: Cell<ChunkedHighlightOwners>,
}

/// Observable attach-stage state, projected into the load evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LanguageAttachEvidence {
    /// Whether an attach stage is scheduled and has not run.
    pub pending: bool,
    /// Whether the editor's first frame after mapping has been painted.
    pub first_frame_seen: bool,
    /// Attach stages that ran.
    pub stages_run: u64,
    /// Attach stages that ran because the deadline passed under input.
    pub deadline_runs: u64,
    /// Attach stages a chunked highlighted install requested.
    pub install_requests: u64,
    /// Highlighted chunks inserted once the engine reported the previous one
    /// analysed (A84).
    pub chunks_on_coverage: u64,
    /// Highlighted chunks inserted after one `G_PRIORITY_LOW + 1` wait
    /// without that report.
    pub chunks_after_wait: u64,
}

/// One chunked highlighted install's progress through
/// `model::highlight_chunking::chunk_gate`, owned by the installing session
/// (the load's or the whole-buffer replacement's).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HighlightChunks {
    /// The budget to fall back to when the attach leaves no language.
    base_budget: SliceBudget,
    attach_requested: bool,
    waited: bool,
}

impl HighlightChunks {
    pub(crate) const fn new(base_budget: SliceBudget) -> Self {
        Self {
            base_budget,
            attach_requested: false,
            waited: false,
        }
    }
}

/// What the installer does with its next chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChunkDecision {
    /// Insert it now.
    Insert,
    /// Insert it and the rest without the highlight cut, under this budget.
    Unhighlighted(SliceBudget),
    /// Park until the attach stage resolves (`when_language_attach_resolves`).
    AwaitAttach,
    /// Queue the step once at [`analysis_wait_priority`].
    AwaitAnalysis,
}

/// `G_PRIORITY_LOW + 1`: an idle there runs only after the context engine's
/// worker has finished (ledger A69).
pub(crate) fn analysis_wait_priority() -> glib::Priority {
    glib::Priority::from(glib::ffi::G_PRIORITY_LOW + 1)
}

impl LushtextEditorPage {
    /// Prepare the buffer for a chunked highlighted install, before any old
    /// text is cleared: no pending attach, no language (removing one costs
    /// nothing, A27, and an attached one would re-analyse the shrinking old
    /// text after every deleted slice), highlighting on for the attach.
    ///
    /// A language already attached to an **empty** buffer stays: there is
    /// nothing to clear or re-analyse, and the first chunk then goes into an
    /// attached buffer, which A84 pins the same way; keeping it saves a
    /// second definition parse (A67) after a path-time attach.
    pub(crate) fn begin_chunked_highlight(&self) {
        self.cancel_language_attach();
        let buffer = self.buffer();
        if buffer.language().is_some() && buffer.char_count() > 0 {
            buffer.set_language(None::<&sourceview5::Language>);
        }
        buffer.set_highlight_syntax(true);
    }

    /// Decide the next chunk of a chunked highlighted install that has
    /// `chunks_inserted` chunks in (`chunk_gate`), requesting the attach
    /// once after the first.
    pub(crate) fn decide_highlight_chunk(
        &self,
        chunks: &mut HighlightChunks,
        chunks_inserted: u64,
    ) -> ChunkDecision {
        loop {
            let facts = ChunkFacts {
                chunks_inserted,
                attach_requested: chunks.attach_requested,
                attach_pending: self.language_attach_pending(),
                language_attached: self.buffer().language().is_some(),
                analysed_to_end: self.highlight_analysed_to_end(),
                waited: chunks.waited,
            };
            match chunk_gate(facts) {
                ChunkGate::Insert => {
                    if chunks_inserted > 0 {
                        let state = &self.imp().language_attach;
                        let counter = if facts.analysed_to_end {
                            &state.chunks_on_coverage
                        } else {
                            &state.chunks_after_wait
                        };
                        counter.set(counter.get().saturating_add(1));
                    }
                    chunks.waited = false;
                    return ChunkDecision::Insert;
                }
                ChunkGate::RequestAttach => {
                    chunks.attach_requested = true;
                    self.request_language_attach_for_install();
                }
                ChunkGate::AwaitAttach => return ChunkDecision::AwaitAttach,
                ChunkGate::AwaitAnalysis => {
                    chunks.waited = true;
                    return ChunkDecision::AwaitAnalysis;
                }
                ChunkGate::InsertUnhighlighted => {
                    return ChunkDecision::Unhighlighted(chunks.base_budget);
                }
            }
        }
    }

    /// Record the language id a load-planning worker guessed from the path
    /// (the catalog's glob table): known from now on, before any attach.
    pub(crate) fn set_requested_language_guess(&self, id: Option<&str>) {
        self.imp()
            .language_attach
            .requested
            .replace(RequestedLanguage::from_guess(id));
    }

    /// The language the document wants, as far as it is known.
    pub(crate) fn requested_language(&self) -> RequestedLanguage {
        self.imp().language_attach.requested.borrow().clone()
    }

    /// Ask for the document's language to be attached in one deferred stage
    /// (module docs). Replaces the former `reapply_language` at all four of its
    /// sites (review R1).
    ///
    /// A language the document scale gated off is never requested: a rename
    /// or Save As changes the path, not the paragraphs the gate measured. An
    /// untitled tab has no path and keeps whatever language it has.
    pub(crate) fn request_language_attach(&self) {
        // A chunked install that owns the buffer keeps its stage armed the
        // install's way (review S1): an identity change (rename, Save As)
        // must neither send a background install back to waiting for a map
        // nor apply the outgoing session's scale to the incoming body.
        let install = self.chunked_highlight_install_active();
        if !install && !self.document_scale().syntax_enabled {
            self.cancel_language_attach();
            return;
        }
        self.request_language_attach_now(install);
    }

    /// Whether the pending stage is a chunked install's in the editor the
    /// user types into: requested by the install, and its view mapped and
    /// holding the keyboard focus. `has_focus`, not `is_focus`: the input
    /// clock is process-wide, and a focus child of an inactive window is
    /// not where the keys go (re-review of the re-review, fix 2).
    fn install_attach_skips_input_quiet(&self, for_install: bool) -> bool {
        let view = self.imp().source_view.get();
        for_install && view.is_mapped() && view.has_focus()
    }

    /// Whether a chunked highlighted install (the load's or a whole-buffer
    /// replacement's) owns the buffer now.
    fn chunked_highlight_install_active(&self) -> bool {
        let owners = self.imp().language_attach.owners.get();
        owners.load || owners.replacement
    }

    /// Record whether the load's installation is a chunked highlighted one.
    pub(crate) fn set_load_chunked_owner(&self, owns: bool) {
        let owners_cell = &self.imp().language_attach.owners;
        let mut owners = owners_cell.get();
        owners.load = owns;
        owners_cell.set(owners);
    }

    /// Record whether the replacement's session is a chunked highlighted one.
    pub(crate) fn set_replacement_chunked_owner(&self, owns: bool) {
        let owners_cell = &self.imp().language_attach.owners;
        let mut owners = owners_cell.get();
        owners.replacement = owns;
        owners_cell.set(owners);
    }

    /// Ask for the attach on behalf of a chunked highlighted install
    /// (`chunked-highlight-attach`, C6b): the install has decided from the
    /// incoming body's scale, which the session adopts only at publish, so
    /// the session scale is not consulted; and a background tab's install
    /// must not wait for the tab to be shown, so an unmapped editor arms the
    /// stage at once (it still waits for input-quiet or the deadline).
    pub(crate) fn request_language_attach_for_install(&self) {
        let state = &self.imp().language_attach;
        state
            .install_requests
            .set(state.install_requests.get().saturating_add(1));
        self.request_language_attach_now(true);
    }

    fn request_language_attach_now(&self, for_install: bool) {
        let Some(path) = self.imp().file_path.borrow().clone() else {
            self.cancel_language_attach();
            return;
        };
        // The glob table answers in microseconds once the catalog is ready;
        // until then a planning worker's guess, if any, stands.
        if let Some(table) = crate::ui::language_catalog::glob_table() {
            let guess = path.to_str().and_then(|name| table.guess(name));
            self.set_requested_language_guess(guess);
        }
        let state = &self.imp().language_attach;
        let current = self.buffer().language().map(|language| language.id());
        let settled = match &*state.requested.borrow() {
            RequestedLanguage::Unknown => false,
            RequestedLanguage::Plain => current.is_none(),
            RequestedLanguage::Id(id) => current.as_deref() == Some(id.as_str()),
        };
        self.cancel_scheduled_attach();
        // Removing a language costs nothing (A27), so a path that names none
        // clears it at once: renaming `notes.md` to `notes.txt` must stop the
        // tab claiming to be Markdown.
        if !settled && *state.requested.borrow() == RequestedLanguage::Plain {
            self.buffer().set_language(None::<&sourceview5::Language>);
            self.resolve_pending_attach_to_idle();
            return;
        }
        if settled {
            self.resolve_pending_attach_to_idle();
            return;
        }
        {
            let mut phase = state.phase.borrow_mut();
            let waiters = match std::mem::take(&mut *phase) {
                AttachStagePhase::Idle => Vec::new(),
                AttachStagePhase::Pending { step, waiters, .. } => {
                    drop(step);
                    waiters
                }
            };
            *phase = AttachStagePhase::Pending {
                for_install,
                step: AttachWaitStep::Idle,
                waiters,
            };
        }
        self.schedule_attach_after_first_frame();
    }

    /// Drop any pending attach stage.
    pub(crate) fn cancel_language_attach(&self) {
        self.cancel_scheduled_attach();
        self.resolve_pending_attach_to_idle();
    }

    fn resolve_pending_attach_to_idle(&self) {
        let old = std::mem::take(&mut *self.imp().language_attach.phase.borrow_mut());
        if let AttachStagePhase::Pending { step, waiters, .. } = old {
            step.retire();
            for waiter in waiters {
                waiter();
            }
        }
    }

    /// Run `resume` once the pending attach stage resolves (it ran, or was
    /// withdrawn or settled without running), or now when none is pending.
    pub(crate) fn when_language_attach_resolves(&self, resume: impl FnOnce() + 'static) {
        let enqueued = {
            let mut phase = self.imp().language_attach.phase.borrow_mut();
            match &mut *phase {
                AttachStagePhase::Pending { waiters, .. } => {
                    waiters.push(Box::new(resume));
                    return;
                }
                AttachStagePhase::Idle => false,
            }
        };
        if !enqueued {
            resume();
        }
    }

    /// Forget what the engine reported, before a chunk goes in (ledger A84).
    pub(crate) fn reset_highlight_coverage(&self) {
        let state = &self.imp().language_attach;
        let watched = matches!(state.coverage.get(), HighlightCoverage::Watched { .. });
        state
            .coverage
            .set(HighlightCoverage::Watched { covered_to: 0 });
        if !watched {
            let editor = self.downgrade();
            self.buffer().connect_highlight_updated(move |_, _, end| {
                if let Some(editor) = editor.upgrade() {
                    let coverage = &editor.imp().language_attach.coverage;
                    if let HighlightCoverage::Watched { covered_to } = coverage.get() {
                        coverage.set(HighlightCoverage::Watched {
                            covered_to: covered_to.max(end.offset()),
                        });
                    }
                }
            });
        }
    }

    /// Whether the engine has reported the buffer analysed to the end of its
    /// last line's text since the last reset (ledger A84: the empty line after
    /// a final delimiter has no text, so the report ends where it starts).
    pub(crate) fn highlight_analysed_to_end(&self) -> bool {
        let buffer = self.buffer();
        let end = buffer.end_iter();
        let target = if end.is_start() || !end.starts_line() {
            end.offset()
        } else {
            let mut iter = end;
            iter.backward_line();
            if !iter.ends_line() {
                iter.forward_to_line_end();
            }
            iter.offset()
        };
        match self.imp().language_attach.coverage.get() {
            HighlightCoverage::Unwatched => 0 >= target,
            HighlightCoverage::Watched { covered_to } => covered_to >= target,
        }
    }

    /// Whether an attach stage is pending.
    pub(crate) fn language_attach_pending(&self) -> bool {
        matches!(
            *self.imp().language_attach.phase.borrow(),
            AttachStagePhase::Pending { .. }
        )
    }

    /// Observable attach-stage state.
    pub(crate) fn language_attach_evidence(&self) -> LanguageAttachEvidence {
        let state = &self.imp().language_attach;
        LanguageAttachEvidence {
            pending: self.language_attach_pending(),
            first_frame_seen: state.first_frame_us.get().is_some(),
            stages_run: state.stages_run.get(),
            deadline_runs: state.deadline_runs.get(),
            install_requests: state.install_requests.get(),
            chunks_on_coverage: state.chunks_on_coverage.get(),
            chunks_after_wait: state.chunks_after_wait.get(),
        }
    }

    /// Retire every source and handler (from `dispose`).
    pub(crate) fn dispose_language_attach(&self) {
        self.cancel_language_attach();
        self.imp().language_attach.map_handler.clear();
    }

    fn cancel_scheduled_attach(&self) {
        let state = &self.imp().language_attach;
        state.generation.set(state.generation.get().next());
        let retired = {
            let mut phase = state.phase.borrow_mut();
            match &mut *phase {
                AttachStagePhase::Pending { step, .. } => Some(std::mem::take(step)),
                AttachStagePhase::Idle => None,
            }
        };
        if let Some(step) = retired {
            step.retire();
        }
    }

    /// Stage 1: wait until the editor is mapped and has painted a frame.
    ///
    /// An editor that is not mapped (a tab that is not selected) waits for
    /// its mapping (review R8); a mapped one that never painted waits for its
    /// first frame. After that, the low-priority idle runs after the frame the
    /// mapping queues, because frames dispatch at a higher priority.
    fn schedule_attach_after_first_frame(&self) {
        let state = &self.imp().language_attach;
        let for_install = match *state.phase.borrow() {
            AttachStagePhase::Pending { for_install, .. } => for_install,
            AttachStagePhase::Idle => return,
        };
        let view = self.imp().source_view.get();
        if !view.is_mapped() && for_install {
            // A background install waits for the attach; the tab may never
            // be shown (`chunked-highlight-attach`).
            self.arm_attach_idle();
            return;
        }
        if !view.is_mapped() {
            let old = {
                let mut phase = state.phase.borrow_mut();
                match &mut *phase {
                    AttachStagePhase::Pending { step, .. } => {
                        Some(std::mem::replace(step, AttachWaitStep::AwaitingMap))
                    }
                    AttachStagePhase::Idle => None,
                }
            };
            if let Some(step) = old {
                step.retire();
            }
            if !state.map_handler.is_connected() {
                let editor = self.downgrade();
                let handler = view.connect_map(move |_| {
                    if let Some(editor) = editor.upgrade()
                        && editor.language_attach_pending()
                    {
                        editor.cancel_scheduled_attach();
                        editor.schedule_attach_after_first_frame();
                    }
                });
                state.map_handler.replace(&view, handler);
            }
            return;
        }
        if state.first_frame_us.get().is_some() {
            self.arm_attach_idle();
            return;
        }
        if let Some(clock) = view.frame_clock() {
            let generation = state.generation.get();
            let editor = self.downgrade();
            let handler = clock.connect_after_paint(move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.first_editor_frame_painted(generation);
                }
            });
            let slot = gtk_lush_signals::SignalSlot::new();
            slot.replace(&clock, handler);
            let new_step = AttachWaitStep::AwaitingFirstFrame(slot);
            let old = {
                let mut phase = state.phase.borrow_mut();
                match &mut *phase {
                    AttachStagePhase::Pending { step, .. }
                        if state.generation.get() == generation =>
                    {
                        Ok(std::mem::replace(step, new_step))
                    }
                    _ => Err(new_step),
                }
            };
            match old {
                Ok(step) => step.retire(),
                Err(unused) => unused.retire(),
            }
        }
    }

    fn first_editor_frame_painted(&self, generation: LanguageAttachGeneration) {
        let state = &self.imp().language_attach;
        let frame_step = {
            let mut phase = state.phase.borrow_mut();
            match &mut *phase {
                AttachStagePhase::Pending { step, .. }
                    if matches!(step, AttachWaitStep::AwaitingFirstFrame(_)) =>
                {
                    Some(std::mem::take(step))
                }
                _ => None,
            }
        };
        if let Some(step) = frame_step {
            step.retire();
        }
        if state.first_frame_us.get().is_none() {
            state.first_frame_us.set(Some(glib::monotonic_time()));
        }
        if state.generation.get() == generation && self.language_attach_pending() {
            self.arm_attach_idle();
        }
    }

    /// Stage 2: a low-priority idle that runs once input has been quiet, or
    /// once the deadline has passed.
    fn arm_attach_idle(&self) {
        let state = &self.imp().language_attach;
        let now = glib::monotonic_time();
        let (generation, armed_since_us) = {
            let phase = state.phase.borrow();
            let AttachStagePhase::Pending { step, .. } = &*phase else {
                return;
            };
            let armed = match step {
                AttachWaitStep::AwaitingQuiet { armed_since_us, .. } => *armed_since_us,
                AttachWaitStep::Idle
                | AttachWaitStep::AwaitingMap
                | AttachWaitStep::AwaitingFirstFrame(_) => state
                    .first_frame_us
                    .get()
                    .map_or(now, |first| first.max(now)),
            };
            (state.generation.get(), armed)
        };
        let editor = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(editor) = editor.upgrade() {
                editor.clear_attach_quiet_source(generation);
                editor.attach_idle_dispatched(generation);
            }
            glib::ControlFlow::Break
        });
        self.store_attach_quiet_source(generation, armed_since_us, source);
    }

    fn clear_attach_quiet_source(&self, generation: LanguageAttachGeneration) {
        let state = &self.imp().language_attach;
        if state.generation.get() != generation {
            return;
        }
        if let AttachStagePhase::Pending {
            step: AttachWaitStep::AwaitingQuiet { source, .. },
            ..
        } = &mut *state.phase.borrow_mut()
        {
            *source = None;
        }
    }

    fn store_attach_quiet_source(
        &self,
        generation: LanguageAttachGeneration,
        armed_since_us: i64,
        source_id: glib::SourceId,
    ) {
        let state = &self.imp().language_attach;
        let new_step = AttachWaitStep::AwaitingQuiet {
            armed_since_us,
            source: Some(source_id),
        };
        let replaced = {
            let mut phase = state.phase.borrow_mut();
            match &mut *phase {
                AttachStagePhase::Pending { step, .. } if state.generation.get() == generation => {
                    Ok(std::mem::replace(step, new_step))
                }
                _ => Err(new_step),
            }
        };
        match replaced {
            Ok(old) => old.retire(),
            Err(unused) => unused.retire(),
        }
    }

    fn attach_idle_dispatched(&self, generation: LanguageAttachGeneration) {
        let state = &self.imp().language_attach;
        if state.generation.get() != generation {
            return;
        }
        let (for_install, armed_since) = match &*state.phase.borrow() {
            AttachStagePhase::Pending {
                for_install,
                step: AttachWaitStep::AwaitingQuiet { armed_since_us, .. },
                ..
            } => (*for_install, Some(*armed_since_us)),
            AttachStagePhase::Pending { for_install, .. } => (*for_install, None),
            AttachStagePhase::Idle => return,
        };
        let now = glib::monotonic_time();
        let last_input = crate::ui::input_quiet::last_input_us();
        // A chunked install holds the keys typed into its own editor until
        // its terminal, so when that editor is the one receiving keys (shown
        // and focused) its attach does not wait for input-quiet (review S6,
        // narrowed by the re-review F2): a key waiting behind the definition
        // parse (A67) costs less than every key held up to
        // `ATTACH_DEADLINE_US`. Any other install, a background tab's above
        // all, keeps waiting for input-quiet or the deadline, so the parse
        // never lands while the user types elsewhere.
        if !self.install_attach_skips_input_quiet(for_install)
            && !deferred_attach_may_run(now, armed_since, last_input)
        {
            // Come back when the quiet interval or the deadline can have
            // passed, whichever is sooner.
            let since_input = last_input.map_or(INPUT_QUIET_US, |input| now - input);
            let since_armed = armed_since.map_or(0, |armed| now - armed);
            let wait = (INPUT_QUIET_US - since_input)
                .min(ATTACH_DEADLINE_US - since_armed)
                .max(1_000);
            let editor = self.downgrade();
            let source = glib::timeout_add_local_full(
                std::time::Duration::from_micros(u64::try_from(wait).unwrap_or(1_000)),
                glib::Priority::LOW,
                move || {
                    if let Some(editor) = editor.upgrade() {
                        editor.clear_attach_quiet_source(generation);
                        editor.attach_idle_dispatched(generation);
                    }
                    glib::ControlFlow::Break
                },
            );
            let armed = armed_since.unwrap_or(now);
            self.store_attach_quiet_source(generation, armed, source);
            return;
        }
        let past_deadline =
            armed_since.is_some_and(|armed| now.saturating_sub(armed) >= ATTACH_DEADLINE_US);
        let editor = self.downgrade();
        crate::ui::language_catalog::when_ready(move || {
            if let Some(editor) = editor.upgrade() {
                editor.run_language_attach(generation, past_deadline);
            }
        });
    }

    /// Stage 3: attach, with GtkSourceView's own guess authoritative.
    fn run_language_attach(&self, generation: LanguageAttachGeneration, past_deadline: bool) {
        let state = &self.imp().language_attach;
        let (for_install, waiters) = {
            if state.generation.get() != generation {
                return;
            }
            let old = std::mem::take(&mut *state.phase.borrow_mut());
            match old {
                AttachStagePhase::Pending {
                    for_install,
                    step,
                    waiters,
                } => {
                    step.retire();
                    (for_install, waiters)
                }
                AttachStagePhase::Idle => return,
            }
        };
        state
            .stages_run
            .set(state.stages_run.get().saturating_add(1));
        if past_deadline {
            state
                .deadline_runs
                .set(state.deadline_runs.get().saturating_add(1));
        }
        self.attach_language_now(for_install);
        for waiter in waiters {
            waiter();
        }
    }

    fn attach_language_now(&self, for_install: bool) {
        let state = &self.imp().language_attach;
        if !for_install && !self.document_scale().syntax_enabled {
            return;
        }
        let Some(path) = self.imp().file_path.borrow().clone() else {
            return;
        };
        let language = path.to_str().and_then(|name| {
            crate::ui::language_catalog::manager().guess_language(Some(name), None::<&str>)
        });
        let guessed = language.as_ref().map(|language| language.id().to_string());
        let previous = state
            .requested
            .replace(RequestedLanguage::from_guess(guessed.as_deref()));
        let buffer = self.buffer();
        if buffer.language() != language {
            let _span = PostOpenSpan::begin("post_open_language_attach");
            buffer.set_language(language.as_ref());
        }
        self.record_settle_mark();
        if previous.id() != guessed.as_deref() {
            // The worker's table guessed differently (review R4): projections
            // that read the kind must see the authoritative one.
            self.fire_content_republished();
        }
    }
}
