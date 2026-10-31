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
//! GTK thread (ledger A52, a toolkit-floor exception with an upstream
//! report). So every site that sets a language calls
//! [`LushtextEditorPage::request_language_attach`], which records the wanted
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

use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::load::policy::{ATTACH_DEADLINE_US, INPUT_QUIET_US, deferred_attach_may_run};
use crate::ui::responsiveness_probe::PostOpenSpan;

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

/// One editor's attach-stage state.
#[derive(Default)]
pub struct LanguageAttachState {
    requested: RefCell<RequestedLanguage>,
    /// Whether an attach stage is scheduled and has not run.
    pending: Cell<bool>,
    /// Advanced by every request and cancellation; a stage of an older one
    /// does nothing.
    generation: Cell<u64>,
    /// When the editor's first frame after it was mapped was painted.
    first_frame_us: Cell<Option<i64>>,
    /// When the pending stage became eligible to wait for input-quiet: the
    /// later of the first frame and the request.
    armed_since_us: Cell<Option<i64>>,
    source: RefCell<Option<glib::SourceId>>,
    frame_handler: RefCell<Option<(gtk4::gdk::FrameClock, glib::SignalHandlerId)>>,
    map_handler: RefCell<Option<glib::SignalHandlerId>>,
    /// Evidence: attach stages that ran, and of those how many ran past the
    /// deadline rather than after input-quiet.
    stages_run: Cell<u64>,
    deadline_runs: Cell<u64>,
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
}

impl LushtextEditorPage {
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
        if !self.document_scale().syntax_enabled {
            self.cancel_language_attach();
            return;
        }
        let Some(path) = self.imp().file_path.borrow().clone() else {
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
            state.pending.set(false);
            return;
        }
        if settled {
            state.pending.set(false);
            return;
        }
        state.pending.set(true);
        state.armed_since_us.set(None);
        self.schedule_attach_after_first_frame();
    }

    /// Drop any pending attach stage.
    pub(crate) fn cancel_language_attach(&self) {
        self.cancel_scheduled_attach();
        self.imp().language_attach.pending.set(false);
    }

    /// Whether an attach stage is pending.
    pub(crate) fn language_attach_pending(&self) -> bool {
        self.imp().language_attach.pending.get()
    }

    /// Observable attach-stage state.
    pub(crate) fn language_attach_evidence(&self) -> LanguageAttachEvidence {
        let state = &self.imp().language_attach;
        LanguageAttachEvidence {
            pending: state.pending.get(),
            first_frame_seen: state.first_frame_us.get().is_some(),
            stages_run: state.stages_run.get(),
            deadline_runs: state.deadline_runs.get(),
        }
    }

    /// Retire every source and handler (from `dispose`).
    pub(crate) fn dispose_language_attach(&self) {
        self.cancel_language_attach();
        if let Some(handler) = self.imp().language_attach.map_handler.take() {
            self.imp().source_view.disconnect(handler);
        }
    }

    fn cancel_scheduled_attach(&self) {
        let state = &self.imp().language_attach;
        state.generation.set(state.generation.get().wrapping_add(1));
        if let Some(source) = state.source.take() {
            source.remove();
        }
        if let Some((clock, handler)) = state.frame_handler.take() {
            clock.disconnect(handler);
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
        let view = self.imp().source_view.get();
        if !view.is_mapped() {
            if state.map_handler.borrow().is_none() {
                let editor = self.downgrade();
                let handler = view.connect_map(move |_| {
                    if let Some(editor) = editor.upgrade()
                        && editor.language_attach_pending()
                    {
                        editor.cancel_scheduled_attach();
                        editor.schedule_attach_after_first_frame();
                    }
                });
                state.map_handler.replace(Some(handler));
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
            state.frame_handler.replace(Some((clock, handler)));
        }
    }

    fn first_editor_frame_painted(&self, generation: u64) {
        let state = &self.imp().language_attach;
        if let Some((clock, handler)) = state.frame_handler.take() {
            clock.disconnect(handler);
        }
        if state.first_frame_us.get().is_none() {
            state.first_frame_us.set(Some(glib::monotonic_time()));
        }
        if state.generation.get() == generation && state.pending.get() {
            self.arm_attach_idle();
        }
    }

    /// Stage 2: a low-priority idle that runs once input has been quiet, or
    /// once the deadline has passed.
    fn arm_attach_idle(&self) {
        let state = &self.imp().language_attach;
        let now = glib::monotonic_time();
        let armed_since = match (state.armed_since_us.get(), state.first_frame_us.get()) {
            (Some(armed), _) => armed,
            (None, first) => first.map_or(now, |first| first.max(now)),
        };
        state.armed_since_us.set(Some(armed_since));
        let generation = state.generation.get();
        let editor = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(editor) = editor.upgrade() {
                editor.imp().language_attach.source.borrow_mut().take();
                editor.attach_idle_dispatched(generation);
            }
            glib::ControlFlow::Break
        });
        state.source.replace(Some(source));
    }

    fn attach_idle_dispatched(&self, generation: u64) {
        let state = &self.imp().language_attach;
        if state.generation.get() != generation || !state.pending.get() {
            return;
        }
        let now = glib::monotonic_time();
        let armed_since = state.armed_since_us.get();
        let last_input = crate::ui::input_quiet::last_input_us();
        if !deferred_attach_may_run(now, armed_since, last_input) {
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
                        editor.imp().language_attach.source.borrow_mut().take();
                        editor.attach_idle_dispatched(generation);
                    }
                    glib::ControlFlow::Break
                },
            );
            state.source.replace(Some(source));
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
    fn run_language_attach(&self, generation: u64, past_deadline: bool) {
        let state = &self.imp().language_attach;
        if state.generation.get() != generation || !state.pending.get() {
            return;
        }
        state.pending.set(false);
        state
            .stages_run
            .set(state.stages_run.get().saturating_add(1));
        if past_deadline {
            state
                .deadline_runs
                .set(state.deadline_runs.get().saturating_add(1));
        }
        if !self.document_scale().syntax_enabled {
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
