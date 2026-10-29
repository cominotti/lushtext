// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`) of the window-composition workflow.
//!
//! Owns everything that is not a decision: the application's pending request,
//! the idle dispatches of stages W, C, R and P, the frame-clock handler that
//! gates B1–B3 on painted frames, the reveals themselves, forced reveals, the
//! bounded activation-open turns, the concealment of revealer children, and
//! teardown when the window leaves its application. Every *which* and *when*
//! comes from `super::policy`.
//!
//! **Borrow discipline.** Revealing a surface flips a widget's visibility, which
//! emits notifications whose handlers may call [`LushtextWindow::ensure_composed`]
//! again. So every function here decides under a short borrow of the runtime,
//! drops it, and only then touches a widget; a stage is marked done *before* its
//! reveal runs, so a re-entrant call finds nothing left to do.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use super::policy::{
    CompositionFacts, CompositionPlan, CompositionStage, FramePaintedDecision, FramePaintedFacts,
    StagedSurface, activation_opens_this_turn, frame_painted_decision, stage_for_surface,
};
use super::seams::PendingWindowRequest;
use super::test_policy;
use crate::app::LushtextApplication;
use crate::ui::construction_spans;
use crate::ui::window::LushtextWindow;

/// Icon names the first frame draws: the header bar's and the status bar's.
/// Stage W looks them up so the first frame's measure finds them loaded.
const FIRST_FRAME_ICON_NAMES: [&str; 6] = [
    "pan-down-symbolic",
    "folder-symbolic",
    "tab-new-symbolic",
    "info-outline-symbolic",
    "open-menu-symbolic",
    "sidebar-show-symbolic",
];
/// The size stage W looks the first-frame icons up at: GTK's symbolic size.
const FIRST_FRAME_ICON_SIZE: i32 = 16;
/// Text stage W lays out once, so the first text layout of the process is not
/// paid in the first frame's measure.
const WARM_TEXT: &str = "LushText Open";

thread_local! {
    /// Whether stage W already ran in this process: its work is one-time.
    static PROCESS_WARMED: Cell<bool> = const { Cell::new(false) };
}

/// The window-composition state of one window, kept in its `imp`.
#[derive(Default)]
pub(crate) struct CompositionState {
    runtime: RefCell<CompositionRuntime>,
}

/// Work deferred until a window's queued activation opens have run.
type AfterActivation = Box<dyn FnOnce(&LushtextWindow)>;

/// What one window's composition has done and what it holds.
#[derive(Default)]
struct CompositionRuntime {
    /// `None` until the window begins composing (end of stage C).
    plan: Option<CompositionPlan>,
    /// The queued stage dispatch (R, P, or one reveal), if any.
    dispatch: Option<glib::SourceId>,
    /// The window's frame clock and this workflow's `after-paint` handler.
    frame_handler: Option<(gtk4::gdk::FrameClock, glib::SignalHandlerId)>,
    /// The application and this window's `window-removed` handler.
    removed_handler: Option<(glib::WeakRef<gtk4::Application>, glib::SignalHandlerId)>,
    /// Whether the frame after the last scheduled stage painted.
    complete: bool,
    /// Whether the window left its application; nothing runs afterwards.
    torn_down: bool,
    /// The origin of the composition spans: the request, or the construction.
    origin_us: i64,
    /// When the window's first frame painted.
    first_frame_us: Option<i64>,
    /// When composition completed.
    complete_us: Option<i64>,
    /// The largest main-loop dispatch until completion, when a watchdog ran.
    largest_dispatch_us: Option<u64>,
    /// Activation files waiting for their bounded turn.
    activation: VecDeque<gio::File>,
    /// The queued activation turn, if any.
    activation_turn: Option<glib::SourceId>,
    /// Work that waits until the queued activation opens have run: the
    /// session restore, so a CLI tab keeps its place and selection (D5).
    after_activation: Vec<AfterActivation>,
    /// The stage a test policy holds composition before, if any.
    hold_before: Option<CompositionStage>,
    /// How long the `composition-hold` build holds after the first frame.
    timed_hold: Option<std::time::Duration>,
}

/// What the evidence surface reads, in one borrow.
pub(super) struct CompositionRuntimeFacts {
    pub(super) plan: Option<CompositionPlan>,
    pub(super) complete: bool,
    pub(super) torn_down: bool,
    pub(super) hold_before: Option<CompositionStage>,
    pub(super) origin_us: i64,
    pub(super) first_frame_us: Option<i64>,
    pub(super) complete_us: Option<i64>,
    pub(super) largest_dispatch_us: Option<u64>,
    pub(super) activation_queued: usize,
    pub(super) dispatch_queued: bool,
}

impl CompositionState {
    pub(super) fn facts(&self) -> CompositionRuntimeFacts {
        let runtime = self.runtime.borrow();
        CompositionRuntimeFacts {
            plan: runtime.plan,
            complete: runtime.complete,
            torn_down: runtime.torn_down,
            hold_before: runtime.hold_before,
            origin_us: runtime.origin_us,
            first_frame_us: runtime.first_frame_us,
            complete_us: runtime.complete_us,
            largest_dispatch_us: runtime.largest_dispatch_us,
            activation_queued: runtime.activation.len(),
            dispatch_queued: runtime.dispatch.is_some(),
        }
    }
}

// --- The application side: the request, stage W and stage C ---------------

/// Record a window request from `open` (with files) or `activate` (without),
/// and return without window work (design D1).
pub(crate) fn request_window(app: &LushtextApplication, files: Vec<gio::File>) {
    if let Some(window) = app
        .active_window()
        .and_then(|window| window.downcast::<LushtextWindow>().ok())
    {
        // A window between stage C and stage P presents itself in its own
        // stage; presenting it here would realize and map it inside this
        // handler. Its last activation turn raises it, as below.
        if window.composition_presented() {
            window.present();
        }
        window.admit_activation_files(files);
        return;
    }
    let pending = app.pending_window_request();
    if let Some(request) = pending.borrow_mut().as_mut() {
        request.merge(files);
        return;
    }
    let request = PendingWindowRequest::new(files, app.hold(), construction_spans::now_us());
    pending.replace(Some(request));
    let app_weak = app.downgrade();
    if PROCESS_WARMED.with(Cell::get) {
        schedule_construct(app_weak);
    } else {
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            run_warm(app_weak.clone());
            glib::ControlFlow::Break
        });
    }
}

/// Drop a request the process quit before constructing, releasing its hold.
pub(crate) fn drop_pending_window_request(app: &LushtextApplication) {
    drop(app.pending_window_request().take());
}

/// Stage W: the process's one-time text and icon initialisation, alone in a
/// dispatch, so neither lands in the first frame's measure.
fn run_warm(app_weak: glib::WeakRef<LushtextApplication>) {
    let Some(app) = app_weak.upgrade() else {
        return;
    };
    if app.pending_window_request().borrow().is_none() {
        return;
    }
    let start_us = construction_spans::now_us();
    if !PROCESS_WARMED.with(|warmed| warmed.replace(true)) {
        warm_first_frame_resources();
    }
    construction_spans::emit(
        CompositionStage::Warm.span_name(),
        start_us,
        construction_spans::now_us(),
    );
    schedule_construct(app_weak);
}

/// Lay out text once and look up the first frame's icons.
fn warm_first_frame_resources() {
    let label = gtk4::Label::new(Some(WARM_TEXT));
    let _ = label.measure(gtk4::Orientation::Horizontal, -1);
    if let Some(display) = gtk4::gdk::Display::default() {
        let theme = gtk4::IconTheme::for_display(&display);
        for name in FIRST_FRAME_ICON_NAMES {
            let _ = theme.lookup_icon(
                name,
                &[],
                FIRST_FRAME_ICON_SIZE,
                1,
                gtk4::TextDirection::Ltr,
                gtk4::IconLookupFlags::empty(),
            );
        }
    }
}

fn schedule_construct(app_weak: glib::WeakRef<LushtextApplication>) {
    glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
        run_construct(&app_weak);
        glib::ControlFlow::Break
    });
}

/// Stage C: construct the window whole, release the request's hold (the
/// window holds the application from now on), and queue stage R.
fn run_construct(app_weak: &glib::WeakRef<LushtextApplication>) {
    let Some(app) = app_weak.upgrade() else {
        return;
    };
    let Some(request) = app.pending_window_request().take() else {
        return;
    };
    let start_us = construction_spans::now_us();
    let window = LushtextWindow::new_composed(app.upcast_ref(), true, request.requested_us);
    let PendingWindowRequest { files, hold, .. } = request;
    drop(hold);
    window.admit_activation_files(files);
    construction_spans::emit(
        CompositionStage::Construct.span_name(),
        start_us,
        construction_spans::now_us(),
    );
    window.queue_lifecycle_dispatch(CompositionStage::Realize);
}

// --- The window side --------------------------------------------------------

impl LushtextWindow {
    /// Begin this window's composition at the end of stage C (design D2).
    ///
    /// `warmed` is whether its request ran stage W; `origin_us` is when the
    /// request (or, for a window constructed directly, the construction) began.
    pub(in crate::ui::window) fn begin_composition(&self, warmed: bool, origin_us: i64) {
        let imp = self.imp();
        let facts = CompositionFacts { warmed };
        {
            let mut runtime = imp.composition.runtime.borrow_mut();
            runtime.plan = Some(CompositionPlan::new(facts));
            runtime.origin_us = origin_us;
            runtime.hold_before = test_policy::hold_before();
            if let Some(hold) = test_policy::timed_hold() {
                runtime.hold_before = Some(CompositionStage::Content);
                runtime.timed_hold = Some(hold);
            }
        }
        conceal_revealed_child_while_hidden(&imp.search_panel_revealer);
        conceal_revealed_child_while_hidden(&imp.palette_revealer);
        conceal_revealed_child_while_hidden(&imp.focus_mode_revealer);

        // R and P are observed rather than assumed, so a window presented by
        // its caller (a widget test) composes exactly as one the request
        // path realized and presented.
        let window_weak = self.downgrade();
        self.connect_realize(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.composition_window_realized();
            }
        });
        let window_weak = self.downgrade();
        self.connect_map(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.composition_window_mapped();
            }
        });
        if let Some(app) = self.application() {
            let window_weak = self.downgrade();
            let id = app.connect_window_removed(move |_, removed| {
                if let Some(window) = window_weak.upgrade()
                    && removed == window.upcast_ref::<gtk4::Window>()
                {
                    window.tear_down_composition();
                }
            });
            imp.composition.runtime.borrow_mut().removed_handler = Some((app.downgrade(), id));
        }
        if test_policy::collapse_to_one_dispatch() {
            self.reveal_every_scheduled_stage_now();
        }
    }

    /// Complete the missing stages `surface` needs, in order, synchronously in
    /// the current dispatch (design D4). A reveal is a visibility flip, so the
    /// dispatch stays small; its measure and paint land in the next frame.
    pub(crate) fn ensure_composed(&self, surface: StagedSurface) {
        let stage = stage_for_surface(surface);
        let chain = {
            let runtime = self.imp().composition.runtime.borrow();
            // A completed composition revealed every stage it schedules.
            if runtime.torn_down || runtime.complete {
                return;
            }
            let Some(plan) = runtime.plan.as_ref() else {
                return;
            };
            plan.forced_chain(stage)
        };
        for stage in chain.iter() {
            let newly = {
                let mut runtime = self.imp().composition.runtime.borrow_mut();
                runtime
                    .plan
                    .as_mut()
                    .is_some_and(|plan| plan.mark_forced(stage))
            };
            if newly {
                self.reveal_stage(stage);
            }
        }
    }

    /// Whether this window's composition still blocks readiness
    /// (`window-composition`): begun, not complete, and still in its
    /// application. A cheap accessor over the runtime; the evidence surface
    /// applies the same policy rule.
    #[must_use]
    pub(crate) fn composition_blocks_readiness(&self) -> bool {
        let runtime = self.imp().composition.runtime.borrow();
        super::policy::composition_blocks_readiness(
            runtime.plan.is_some(),
            runtime.complete,
            runtime.torn_down,
        )
    }

    /// Queue activation files for their bounded turns, which start once the
    /// content stage has run (design D5).
    pub fn admit_activation_files(&self, files: Vec<gio::File>) {
        if files.is_empty() {
            return;
        }
        let content_done = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            if runtime.torn_down {
                // The window left its application: it opens nothing more.
                return;
            }
            runtime.activation.extend(files);
            runtime
                .plan
                .as_ref()
                .is_none_or(|plan| plan.is_done(CompositionStage::Content))
        };
        self.sync_tab_bar_visibility();
        if content_done {
            self.queue_activation_turn();
        }
    }

    /// Whether queued activation opens will add a tab: every local file opens
    /// a page, a failed one as its placeholder.
    pub(in crate::ui::window) fn composition_activation_adds_pages(&self) -> bool {
        self.imp()
            .composition
            .runtime
            .borrow()
            .activation
            .iter()
            .any(|file| file.path().is_some())
    }

    /// Whether this window has reached stage P (or never staged at all).
    pub(in crate::ui::window) fn composition_presented(&self) -> bool {
        self.imp()
            .composition
            .runtime
            .borrow()
            .plan
            .is_none_or(|plan| plan.is_done(CompositionStage::Present))
    }

    /// Run `work` once the queued activation opens have run, or now when
    /// none are queued.
    pub(in crate::ui::window) fn after_activation_opens(
        &self,
        work: impl FnOnce(&LushtextWindow) + 'static,
    ) {
        let deferred = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            let waiting = !runtime.torn_down && !runtime.activation.is_empty();
            if waiting {
                runtime.after_activation.push(Box::new(work));
                None
            } else {
                Some(work)
            }
        };
        if let Some(work) = deferred {
            work(self);
        }
    }

    /// Leave composition when the window leaves its application (ledger A21):
    /// remove every source and handler, so no stage runs afterwards.
    pub(in crate::ui::window) fn tear_down_composition(&self) {
        let (dispatch, turn, frame, removed) = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            if runtime.torn_down {
                return;
            }
            runtime.torn_down = true;
            runtime.activation.clear();
            runtime.after_activation.clear();
            (
                runtime.dispatch.take(),
                runtime.activation_turn.take(),
                runtime.frame_handler.take(),
                runtime.removed_handler.take(),
            )
        };
        if let Some(source) = dispatch {
            source.remove();
        }
        if let Some(source) = turn {
            source.remove();
        }
        if let Some((clock, id)) = frame {
            clock.disconnect(id);
        }
        if let Some((app, id)) = removed
            && let Some(app) = app.upgrade()
        {
            app.disconnect(id);
        }
    }

    /// Release a test policy's hold, so the held stage follows the next frame.
    #[cfg(feature = "test-utils")]
    pub fn release_composition_hold_for_test(&self) {
        self.release_composition_hold();
    }

    /// Release a hold, so the held stage follows the next painted frame.
    fn release_composition_hold(&self) {
        self.imp().composition.runtime.borrow_mut().hold_before = None;
        self.queue_draw();
    }

    // --- Lifecycle stages R and P ------------------------------------------

    /// Queue stage R or P in its own `G_PRIORITY_DEFAULT_IDLE` dispatch.
    fn queue_lifecycle_dispatch(&self, stage: CompositionStage) {
        let window_weak = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            if let Some(window) = window_weak.upgrade() {
                window.imp().composition.runtime.borrow_mut().dispatch = None;
                window.run_lifecycle_stage(stage);
            }
            glib::ControlFlow::Break
        });
        self.imp().composition.runtime.borrow_mut().dispatch = Some(source);
    }

    fn run_lifecycle_stage(&self, stage: CompositionStage) {
        if self.imp().composition.runtime.borrow().torn_down {
            return;
        }
        let start_us = construction_spans::now_us();
        match stage {
            CompositionStage::Realize => {
                if !self.is_realized() {
                    WidgetExt::realize(self);
                }
                construction_spans::emit(stage.span_name(), start_us, construction_spans::now_us());
                self.queue_lifecycle_dispatch(CompositionStage::Present);
            }
            CompositionStage::Present => {
                self.present();
                // A caller may have mapped the window already, so `present`
                // queues nothing; the frame after P is what resumes the reveals.
                self.queue_draw();
                construction_spans::emit(stage.span_name(), start_us, construction_spans::now_us());
            }
            _ => {}
        }
    }

    /// The window was realized (by stage R or by its caller): record R and
    /// watch its frame clock.
    fn composition_window_realized(&self) {
        let Some(clock) = self.frame_clock() else {
            return;
        };
        let already = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            if runtime.torn_down || runtime.frame_handler.is_some() {
                true
            } else {
                if let Some(plan) = runtime.plan.as_mut() {
                    plan.mark_done(CompositionStage::Realize);
                }
                false
            }
        };
        if already {
            return;
        }
        let window_weak = self.downgrade();
        let id = clock.connect_after_paint(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.composition_frame_painted();
            }
        });
        self.imp().composition.runtime.borrow_mut().frame_handler = Some((clock, id));
    }

    /// The window was mapped (by stage P or by its caller): record P.
    fn composition_window_mapped(&self) {
        let mut runtime = self.imp().composition.runtime.borrow_mut();
        if let Some(plan) = runtime.plan.as_mut() {
            plan.mark_done(CompositionStage::Realize);
            plan.mark_done(CompositionStage::Present);
        }
    }

    // --- Reveal stages B1–B3, gated on painted frames ----------------------

    /// One frame of this window painted: ask the policy what it decides.
    fn composition_frame_painted(&self) {
        let decision = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            if runtime.torn_down {
                return;
            }
            let Some(plan) = runtime.plan else {
                return;
            };
            if runtime.first_frame_us.is_none() && plan.is_done(CompositionStage::Present) {
                runtime.first_frame_us = Some(construction_spans::now_us());
                if let Some(hold) = runtime.timed_hold.take() {
                    let window_weak = self.downgrade();
                    glib::timeout_add_local_once(hold, move || {
                        if let Some(window) = window_weak.upgrade() {
                            window.release_composition_hold();
                        }
                    });
                }
            }
            frame_painted_decision(
                plan,
                FramePaintedFacts {
                    dispatch_queued: runtime.dispatch.is_some(),
                    hold_before: runtime.hold_before,
                    complete: runtime.complete,
                },
            )
        };
        match decision {
            FramePaintedDecision::Wait => {}
            FramePaintedDecision::Queue(stage) => self.queue_reveal_dispatch(stage),
            FramePaintedDecision::Complete => self.complete_composition(),
        }
    }

    /// Queue `stage`'s reveal in its own dispatch, after the painted frame.
    fn queue_reveal_dispatch(&self, stage: CompositionStage) {
        let window_weak = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            if let Some(window) = window_weak.upgrade() {
                window.run_reveal_dispatch(stage);
            }
            glib::ControlFlow::Break
        });
        self.imp().composition.runtime.borrow_mut().dispatch = Some(source);
    }

    fn run_reveal_dispatch(&self, stage: CompositionStage) {
        let newly = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            runtime.dispatch = None;
            if runtime.torn_down {
                return;
            }
            runtime
                .plan
                .as_mut()
                .is_some_and(|plan| plan.mark_done(stage))
        };
        if newly {
            let start_us = construction_spans::now_us();
            self.reveal_stage(stage);
            construction_spans::emit(stage.span_name(), start_us, construction_spans::now_us());
        }
        // The frame showing this stage (or, when a forced reveal ran it first,
        // the next frame) decides what follows.
        self.queue_draw();
    }

    /// Make one stage's staged surface visible.
    fn reveal_stage(&self, stage: CompositionStage) {
        let imp = self.imp();
        match stage {
            CompositionStage::Content => {
                // The content box was opaque only to cover the staged stack;
                // the editor's background opacity needs it transparent.
                imp.content_box.remove_css_class("shell-chrome-opaque");
                imp.content_stack.set_visible(true);
                self.queue_activation_turn();
            }
            CompositionStage::Workspace => imp.sidebar.set_visible(true),
            CompositionStage::Properties => imp.properties_panel.set_visible(true),
            _ => {}
        }
    }

    /// The `collapse_to_one_dispatch` test policy: every scheduled reveal in
    /// stage C itself, for bisecting.
    fn reveal_every_scheduled_stage_now(&self) {
        let stages = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            let Some(plan) = runtime.plan.as_mut() else {
                return;
            };
            let stages: Vec<_> = plan.pending().iter().filter(|s| s.is_reveal()).collect();
            for stage in &stages {
                plan.mark_done(*stage);
            }
            stages
        };
        for stage in stages {
            self.reveal_stage(stage);
        }
    }

    fn complete_composition(&self) {
        let (frame, origin_us) = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            if runtime.complete {
                return;
            }
            runtime.complete = true;
            runtime.complete_us = Some(construction_spans::now_us());
            runtime.largest_dispatch_us =
                construction_spans::largest_dispatch_since_us(runtime.origin_us);
            (runtime.frame_handler.take(), runtime.origin_us)
        };
        if let Some((clock, id)) = frame {
            clock.disconnect(id);
        }
        construction_spans::emit(
            "composition_complete",
            origin_us,
            construction_spans::now_us(),
        );
    }

    // --- Activation opens in bounded turns ---------------------------------

    fn queue_activation_turn(&self) {
        let queue = {
            let runtime = self.imp().composition.runtime.borrow();
            !runtime.torn_down
                && runtime.activation_turn.is_none()
                && !runtime.activation.is_empty()
        };
        if !queue {
            return;
        }
        let window_weak = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            if let Some(window) = window_weak.upgrade() {
                window.run_activation_turn();
            }
            glib::ControlFlow::Break
        });
        self.imp().composition.runtime.borrow_mut().activation_turn = Some(source);
    }

    /// Open at most one session-restore page turn of activation files.
    fn run_activation_turn(&self) {
        let (files, more) = {
            let mut runtime = self.imp().composition.runtime.borrow_mut();
            runtime.activation_turn = None;
            if runtime.torn_down {
                return;
            }
            let take = activation_opens_this_turn(runtime.activation.len());
            let files: Vec<_> = runtime.activation.drain(..take).collect();
            (files, !runtime.activation.is_empty())
        };
        for file in &files {
            if let Some(path) = file.path() {
                self.open_document_from_activation(&path);
            } else {
                self.report_unsupported_open_file(file);
            }
        }
        // The strip followed the queue; a turn whose files added no page
        // (a remote file, a path another window owns) must not leave it shown.
        self.sync_tab_bar_visibility();
        if more {
            self.queue_activation_turn();
            return;
        }
        let deferred =
            std::mem::take(&mut self.imp().composition.runtime.borrow_mut().after_activation);
        for work in deferred {
            work(self);
        }
        if !files.is_empty() {
            // Raise the window once its activation finished, as the
            // pre-composition `open` handler did after opening its files.
            self.present();
        }
    }
}

/// Keep a revealer's child invisible whenever the revealer conceals it
/// (design D2): a concealed vertical-slide revealer measures a visible child
/// horizontally at full scale (ledger A49), so the hidden surfaces cost the
/// first frame nothing only while their child is invisible.
///
/// The child becomes visible as `reveal-child` turns true (inside the setter,
/// before the transition's first frame), and invisible again once the
/// revealer reports the child concealed. Accessibility is unchanged: each
/// surface keeps the accessible name its template and `ui::accessibility`
/// gave it in stage C, and a concealed child is absent from the accessibility
/// tree, as it was when it was only zero-height (design D6). Dismissal and
/// focus restoration stay the surfaces' own (`transient_dismissal`).
fn conceal_revealed_child_while_hidden(revealer: &gtk4::Revealer) {
    if let Some(child) = revealer.child()
        && !revealer.reveals_child()
    {
        child.set_visible(false);
    }
    revealer.connect_reveal_child_notify(|revealer| {
        if revealer.reveals_child()
            && let Some(child) = revealer.child()
        {
            child.set_visible(true);
        }
    });
    revealer.connect_child_revealed_notify(|revealer| {
        if !revealer.reveals_child()
            && !revealer.is_child_revealed()
            && let Some(child) = revealer.child()
        {
            child.set_visible(false);
        }
    });
}
