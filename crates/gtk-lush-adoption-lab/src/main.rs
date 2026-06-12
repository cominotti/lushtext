// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gio::prelude::*;
use gtk_lush_proof_harness::{HarnessConfig, RegisteredTest, recommended_pre_gtk_environment};
use gtk_lush_proof_spine::{
    ArtifactEnvelope, ArtifactSummaryProvider, BlockerSummary, PrivacyScope, ProofStatus,
    ReadinessPredicate, ReadinessProvider, ReadinessResult, Rect, SnapshotEnvelope,
    SnapshotProvider, SurfaceSummary, VersionInfo, WorkflowEvent, WorkflowEventProvider,
    WorkflowPhase,
};
use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer};
use gtk_lush_signals::{BindingBag, RegistrationBag, SignalBag};
use gtk_lush_tasks::{FreshnessToken, active_worker_count, spawn_blocking_then};
use gtk_lush_viewport::{RestState, ViewportAxis, ViewportObserver, rests_at_lower};
use gtk_lush_widgets::{ClipBin, RenderHoldCapture, RenderHoldNotReady, RenderHoldOverlay};
use gtk4::glib;
use gtk4::prelude::*;

fn main() -> glib::ExitCode {
    let app = libadwaita::Application::builder()
        .application_id("dev.cominotti.GtkLushAdoptionLab")
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &libadwaita::Application) {
    let stack = gtk4::Stack::builder()
        .hexpand(true)
        .vexpand(true)
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .build();
    let sidebar = gtk4::StackSidebar::new();
    sidebar.set_stack(&stack);
    sidebar.set_size_request(210, -1);

    let (signals_page, signal_owners) = build_signals_page();
    let (settle_page, settle_owners) = build_settle_page();
    let (tasks_page, task_owners) = build_tasks_page();
    let (viewport_page, viewport_owners) = build_viewport_page();
    let (widgets_page, widget_owners) = build_widgets_page();
    let (proof_harness_page, proof_harness_owners) = build_proof_harness_page();
    let (proof_spine_page, proof_spine_owners) = build_proof_spine_page();

    let _signals_page = stack.add_titled(&signals_page, Some("signals"), "Signals");
    let _settle_page = stack.add_titled(&settle_page, Some("settle"), "Settle");
    let _tasks_page = stack.add_titled(&tasks_page, Some("tasks"), "Tasks");
    let _viewport_page = stack.add_titled(&viewport_page, Some("viewport"), "Viewport");
    let _widgets_page = stack.add_titled(&widgets_page, Some("widgets"), "Widgets");
    let _proof_harness_page =
        stack.add_titled(&proof_harness_page, Some("proof-harness"), "Proof Harness");
    let _proof_spine_page = stack.add_titled(&proof_spine_page, Some("proof-spine"), "Proof Spine");

    let shell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    shell.append(&sidebar);
    shell.append(&stack);

    let header = libadwaita::HeaderBar::new();
    header.set_title_widget(Some(&gtk4::Label::new(Some("GTK Lush Adoption Lab"))));

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&shell));

    let owners = Rc::new(LabOwners {
        signal_owners,
        settle_owners,
        task_owners,
        viewport_owners,
        widget_owners,
        proof_harness_owners,
        proof_spine_owners,
    });

    let window = libadwaita::ApplicationWindow::builder()
        .application(app)
        .title("GTK Lush Adoption Lab")
        .default_width(1180)
        .default_height(760)
        .content(&toolbar)
        .build();
    window.connect_close_request(move |_| {
        let _summary = owners.lifecycle_summary();
        glib::Propagation::Proceed
    });
    window.present();
}

struct LabOwners {
    signal_owners: SignalOwners,
    settle_owners: SettleOwners,
    task_owners: TaskOwners,
    viewport_owners: ViewportOwners,
    widget_owners: WidgetOwners,
    proof_harness_owners: ProofHarnessOwners,
    proof_spine_owners: ProofSpineOwners,
}

impl LabOwners {
    fn lifecycle_summary(&self) -> String {
        let signal_registrations = self.signal_owners.registration_count();
        let settle_pending = self.settle_owners.pending();
        let task_generation = self.task_owners.generation();
        let viewport_registrations = self.viewport_owners.registration_count();
        let render_hold_active = self.widget_owners.render_hold_active();
        let harness_attempts = self.proof_harness_owners.attempt_summary();
        let proof_sequence = self.proof_spine_owners.snapshot_sequence();
        format!(
            "signals={signal_registrations}; settle_pending={settle_pending}; \
             task_generation={task_generation}; viewport_registrations={viewport_registrations}; \
             render_hold_active={render_hold_active}; harness={harness_attempts}; \
             proof_sequence={proof_sequence}"
        )
    }
}

struct SignalOwners {
    signal_bag: Rc<SignalBag>,
    binding_bag: Rc<BindingBag>,
    registration_bag: Rc<RegistrationBag>,
}

impl SignalOwners {
    fn registration_count(&self) -> usize {
        self.signal_bag.len() + self.binding_bag.len() + self.registration_bag.len()
    }
}

fn build_signals_page() -> (gtk4::Widget, SignalOwners) {
    let content = workflow_box("Signal, Binding, And Registration Lifetimes");
    append_body(
        &content,
        "Recycle the row, clear tracked handlers, and rebind a switch without \
         retaining stale callbacks.",
    );

    let hit_count = Rc::new(Cell::new(0u32));
    let signal_bag = Rc::new(SignalBag::new());
    let binding_bag = Rc::new(BindingBag::new());
    let registration_bag = Rc::new(RegistrationBag::new());

    let signal_button = gtk4::Button::with_label("Tracked signal");
    let signal_status = status_label("No tracked signal has fired.");
    install_signal_handler(&signal_bag, &signal_button, &signal_status, &hit_count);
    append_control_row(&content, "SignalBag", &signal_button, &signal_status);

    let source_switch = gtk4::Switch::new();
    let target_switch = gtk4::Switch::new();
    target_switch.set_sensitive(false);
    let binding_status = status_label("Target switch follows source until bindings clear.");
    install_switch_binding(
        &binding_bag,
        &source_switch,
        &target_switch,
        &binding_status,
    );

    let switch_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    switch_row.append(&source_switch);
    switch_row.append(&target_switch);
    append_control_row(&content, "BindingBag", &switch_row, &binding_status);

    let registration_status = status_label("Registration cleanup is pending.");
    install_registration_cleanup(&registration_bag, &registration_status, 1);
    append_control_row(
        &content,
        "RegistrationBag",
        &gtk4::Label::new(Some("row controller cleanup")),
        &registration_status,
    );

    let command_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let recycle_button = gtk4::Button::with_label("Recycle/Rebind");
    recycle_button.connect_clicked({
        let signal_bag = Rc::clone(&signal_bag);
        let binding_bag = Rc::clone(&binding_bag);
        let registration_bag = Rc::clone(&registration_bag);
        let signal_button = signal_button;
        let signal_status = signal_status.clone();
        let binding_status = binding_status.clone();
        let source_switch = source_switch;
        let target_switch = target_switch;
        let registration_status = registration_status;
        let hit_count = Rc::clone(&hit_count);
        move |_| {
            signal_bag.clear();
            binding_bag.clear();
            registration_bag.clear();
            install_signal_handler(&signal_bag, &signal_button, &signal_status, &hit_count);
            install_switch_binding(
                &binding_bag,
                &source_switch,
                &target_switch,
                &binding_status,
            );
            install_registration_cleanup(&registration_bag, &registration_status, hit_count.get());
            signal_status.set_text("Row recycled and signal rebound.");
        }
    });

    let clear_button = gtk4::Button::with_label("Clear All");
    clear_button.connect_clicked({
        let signal_bag = Rc::clone(&signal_bag);
        let binding_bag = Rc::clone(&binding_bag);
        let registration_bag = Rc::clone(&registration_bag);
        let signal_status = signal_status;
        let binding_status = binding_status;
        move |_| {
            signal_bag.clear();
            binding_bag.clear();
            registration_bag.clear();
            signal_status.set_text("Tracked signal disconnected.");
            binding_status.set_text("Binding cleared; target no longer follows source.");
        }
    });
    command_row.append(&recycle_button);
    command_row.append(&clear_button);
    content.append(&command_row);

    (
        scroll_page(&content),
        SignalOwners {
            signal_bag,
            binding_bag,
            registration_bag,
        },
    )
}

fn install_signal_handler(
    bag: &SignalBag,
    button: &gtk4::Button,
    status: &gtk4::Label,
    hit_count: &Rc<Cell<u32>>,
) {
    let status = status.clone();
    let hit_count = Rc::clone(hit_count);
    bag.track(
        button,
        button.connect_clicked(move |_| {
            let hits = hit_count.get().saturating_add(1);
            hit_count.set(hits);
            status.set_text(&format!("Tracked handler fired {hits} time(s)."));
        }),
    );
}

fn install_switch_binding(
    bag: &BindingBag,
    source: &gtk4::Switch,
    target: &gtk4::Switch,
    status: &gtk4::Label,
) {
    bag.track(
        source
            .bind_property("active", target, "active")
            .sync_create()
            .build(),
    );
    status.set_text("Binding active; target switch mirrors source.");
}

fn install_registration_cleanup(bag: &RegistrationBag, status: &gtk4::Label, generation: u32) {
    let status = status.clone();
    bag.track(move || {
        status.set_text(&format!(
            "Registration cleanup ran for generation {generation}."
        ));
    });
}

#[derive(Clone)]
struct SettleOwners {
    burst: SettleBurst,
}

impl SettleOwners {
    fn pending(&self) -> bool {
        self.burst.pending()
    }
}

fn build_settle_page() -> (gtk4::Widget, SettleOwners) {
    let content = workflow_box("Settle And Timer Generations");
    append_body(
        &content,
        "Type rapidly, extend the settle burst, re-arm cleanup, and schedule a \
         weak target that is dropped before it can run.",
    );

    let debounce = Debounce::new();
    let burst = SettleBurst::new();
    let superseding_timer = SupersedingTimer::new();

    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some("Debounced text"));
    let debounce_status = status_label("Latest generation has not fired yet.");
    entry.connect_changed({
        let debounce = debounce.clone();
        let debounce_status = debounce_status.clone();
        move |entry| {
            let text = entry.text().to_string();
            let token = debounce.schedule(
                &debounce_status,
                Duration::from_millis(180),
                move |label, token| {
                    label.set_text(&format!(
                        "Debounced latest generation {}: {text}",
                        token.value()
                    ));
                },
            );
            debounce_status.set_text(&format!("Scheduled debounce generation {}.", token.value()));
        }
    });
    append_control_row(&content, "Debounce", &entry, &debounce_status);

    let settle_button = gtk4::Button::with_label("Extend Burst");
    let settle_status = status_label("No settle burst is pending.");
    settle_button.connect_clicked({
        let burst = burst.clone();
        let settle_status = settle_status.clone();
        move |_| {
            let handle = burst.schedule(
                &settle_status,
                Duration::from_millis(220),
                move |label, handle| {
                    let token = handle.token().value();
                    label.set_text(&format!("Settle burst repaired generation {token}."));
                    handle.finish_if_current();
                },
            );
            settle_status.set_text(&format!(
                "Settle pending for generation {}.",
                handle.token().value()
            ));
        }
    });
    append_control_row(&content, "SettleBurst", &settle_button, &settle_status);

    let timer_button = gtk4::Button::with_label("Re-arm Cleanup");
    let timer_status = status_label("No cleanup timer armed.");
    timer_button.connect_clicked({
        let timer_status = timer_status.clone();
        move |_| {
            let token = superseding_timer.arm(
                &timer_status,
                Duration::from_millis(240),
                move |label, token| {
                    label.set_text(&format!(
                        "Only latest cleanup generation {} ran.",
                        token.value()
                    ));
                },
            );
            timer_status.set_text(&format!("Cleanup generation {} armed.", token.value()));
        }
    });
    append_control_row(&content, "SupersedingTimer", &timer_button, &timer_status);

    let weak_button = gtk4::Button::with_label("Drop Weak Target");
    let weak_status = status_label("Weak target cancellation not exercised yet.");
    weak_button.connect_clicked({
        let weak_status = weak_status.clone();
        move |_| {
            let temporary = gtk4::Label::new(Some("temporary target"));
            let token = debounce.schedule(
                &temporary,
                Duration::from_millis(120),
                move |label, token| {
                    label.set_text(&format!("Unexpected weak callback {}", token.value()));
                },
            );
            weak_status.set_text(&format!(
                "Scheduled generation {} against a dropped weak target.",
                token.value()
            ));
        }
    });
    append_control_row(&content, "Weak target", &weak_button, &weak_status);

    (scroll_page(&content), SettleOwners { burst })
}

#[derive(Clone)]
struct TaskOwners {
    generation: Rc<Cell<u64>>,
    fresh_results: Rc<Cell<u32>>,
    stale_results: Rc<Cell<u32>>,
    pending_work: Rc<Cell<bool>>,
}

impl TaskOwners {
    fn generation(&self) -> u64 {
        let _fresh = self.fresh_results.get();
        let _stale = self.stale_results.get();
        let _pending = self.pending_work.get();
        self.generation.get()
    }
}

fn build_tasks_page() -> (gtk4::Widget, TaskOwners) {
    let content = workflow_box("Bounded Background Work And Freshness");
    append_body(
        &content,
        "Start a worker, supersede its generation before completion, and run a \
         panic-safe job whose error is reported on the GTK main loop.",
    );

    let generation = Rc::new(Cell::new(0u64));
    let fresh_results = Rc::new(Cell::new(0u32));
    let stale_results = Rc::new(Cell::new(0u32));
    let pending_work = Rc::new(Cell::new(false));
    let task_status = status_label("No background work has completed.");
    let counter_status = status_label("fresh=0 stale=0 active-workers=0");

    let start_button = gtk4::Button::with_label("Start Worker");
    start_button.connect_clicked({
        let generation = Rc::clone(&generation);
        let fresh_results = Rc::clone(&fresh_results);
        let stale_results = Rc::clone(&stale_results);
        let pending_work = Rc::clone(&pending_work);
        let task_status = task_status.clone();
        let counter_status = counter_status.clone();
        move |_| {
            if pending_work.get() {
                task_status.set_text("Worker already pending; request collapsed.");
                return;
            }
            let next_generation = generation.get().saturating_add(1);
            generation.set(next_generation);
            pending_work.set(true);
            let token = FreshnessToken::new(next_generation);
            task_status.set_text(&format!(
                "Worker requested for generation {next_generation}."
            ));
            let state = TaskCompletionState {
                generation: Rc::clone(&generation),
                fresh_results: Rc::clone(&fresh_results),
                stale_results: Rc::clone(&stale_results),
                pending_work: Rc::clone(&pending_work),
                task_status: task_status.clone(),
                counter_status: counter_status.clone(),
            };
            spawn_blocking_then(
                token,
                move || {
                    std::thread::sleep(Duration::from_millis(90));
                    Ok(format!("loaded payload for generation {next_generation}"))
                },
                move |token, result| apply_task_result(&state, token, result),
            );
        }
    });

    let supersede_button = gtk4::Button::with_label("Supersede Generation");
    supersede_button.connect_clicked({
        let generation = Rc::clone(&generation);
        let task_status = task_status.clone();
        move |_| {
            let next_generation = generation.get().saturating_add(1);
            generation.set(next_generation);
            task_status.set_text(&format!("Generation advanced to {next_generation}."));
        }
    });

    let panic_button = gtk4::Button::with_label("Panic-Safe Worker");
    panic_button.connect_clicked({
        let generation = Rc::clone(&generation);
        let fresh_results = Rc::clone(&fresh_results);
        let stale_results = Rc::clone(&stale_results);
        let pending_work = Rc::clone(&pending_work);
        let task_status = task_status.clone();
        let counter_status = counter_status.clone();
        move |_| {
            if pending_work.get() {
                task_status.set_text("Worker already pending; panic-safe request collapsed.");
                return;
            }
            let next_generation = generation.get().saturating_add(1);
            generation.set(next_generation);
            pending_work.set(true);
            let token = FreshnessToken::new(next_generation);
            let state = TaskCompletionState {
                generation: Rc::clone(&generation),
                fresh_results: Rc::clone(&fresh_results),
                stale_results: Rc::clone(&stale_results),
                pending_work: Rc::clone(&pending_work),
                task_status: task_status.clone(),
                counter_status: counter_status.clone(),
            };
            spawn_blocking_then(
                token,
                move || {
                    let result = std::panic::catch_unwind(|| {
                        panic!("simulated adoption-lab worker panic");
                    });
                    match result {
                        Ok(()) => Ok("unexpected clean worker".to_string()),
                        Err(_) => Err("caught simulated worker panic".to_string()),
                    }
                },
                move |token, result| apply_task_result(&state, token, result),
            );
        }
    });

    let command_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    command_row.append(&start_button);
    command_row.append(&supersede_button);
    command_row.append(&panic_button);
    append_control_row(&content, "gtk-lush-tasks", &command_row, &task_status);
    content.append(&counter_status);

    (
        scroll_page(&content),
        TaskOwners {
            generation,
            fresh_results,
            stale_results,
            pending_work,
        },
    )
}

struct TaskCompletionState {
    generation: Rc<Cell<u64>>,
    fresh_results: Rc<Cell<u32>>,
    stale_results: Rc<Cell<u32>>,
    pending_work: Rc<Cell<bool>>,
    task_status: gtk4::Label,
    counter_status: gtk4::Label,
}

fn apply_task_result(
    state: &TaskCompletionState,
    token: FreshnessToken,
    result: Result<String, String>,
) {
    let current = FreshnessToken::new(state.generation.get());
    state.pending_work.set(false);
    match token.accept(current, result) {
        Ok(fresh) => {
            let accepted = fresh.into_inner();
            state
                .fresh_results
                .set(state.fresh_results.get().saturating_add(1));
            match accepted {
                Ok(text) => state.task_status.set_text(&format!("Fresh result: {text}")),
                Err(error) => state
                    .task_status
                    .set_text(&format!("Fresh worker error: {error}")),
            }
        }
        Err(stale) => {
            state
                .stale_results
                .set(state.stale_results.get().saturating_add(1));
            state.task_status.set_text(&format!(
                "Stale result rejected: requested {} current {}",
                stale.requested().generation(),
                stale.current().generation()
            ));
        }
    }
    let fresh = state.fresh_results.get();
    let stale = state.stale_results.get();
    let active = active_worker_count();
    state.counter_status.set_text(&format!(
        "fresh={fresh} stale={stale} active-workers={active}"
    ));
}

struct ViewportOwners {
    observer: Option<ViewportObserver>,
    rest_state: RestState,
}

impl ViewportOwners {
    fn registration_count(&self) -> usize {
        let _vertical_rest = self.rest_state.at_lower(ViewportAxis::Vertical);
        self.observer.as_ref().map_or(0, ViewportObserver::len)
    }
}

fn build_viewport_page() -> (gtk4::Widget, ViewportOwners) {
    let content = workflow_box("Viewport Observation And Rest State");
    append_body(
        &content,
        "The observer watches public scroll adjustments, while the app owns how \
         it reacts to bounds changes and lower-edge rest state.",
    );

    let text_view = gtk4::TextView::new();
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk4::WrapMode::None);
    text_view.buffer().set_text(&awkward_rows(
        "viewport row with a deliberately long visible line",
        72,
    ));

    let scroller = gtk4::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_height(260)
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&text_view)
        .build();

    let bounds_status = status_label("Bounds changes will appear here.");
    let value_status = status_label("Adjustment values will appear here.");
    let rest_status = status_label("Rest state has not been recorded.");
    let rest_state = RestState::new();
    let observer = ViewportObserver::for_scrollable(
        &text_view,
        {
            let bounds_status = bounds_status.clone();
            move |change| {
                let axis = axis_name(change.axis());
                let page_size = change.page_size();
                let previous = change.previous_page_size();
                bounds_status.set_text(&format!(
                    "{axis} page size changed from {previous:.1} to {page_size:.1}."
                ));
            }
        },
        {
            let value_status = value_status.clone();
            let rest_status = rest_status.clone();
            let rest_state = rest_state.clone();
            move |change| {
                let axis = change.axis();
                let updated = rest_state.record_value(axis, change.value(), change.lower());
                let axis_text = axis_name(axis);
                let at_lower = rests_at_lower(change.value(), change.lower());
                value_status.set_text(&format!(
                    "{axis_text} value {:.1}, lower {:.1}, at_lower={at_lower}",
                    change.value(),
                    change.lower()
                ));
                rest_status.set_text(&format!(
                    "recorded={updated}; horizontal_lower={}; vertical_lower={}",
                    rest_state.at_lower(ViewportAxis::Horizontal),
                    rest_state.at_lower(ViewportAxis::Vertical)
                ));
            }
        },
    );

    let pause_button = gtk4::Button::with_label("Pause Rest Tracking");
    pause_button.connect_clicked({
        let rest_state = rest_state.clone();
        let rest_status = rest_status.clone();
        move |_| {
            let pause = rest_state.pause();
            let updated = rest_state.record_value(ViewportAxis::Vertical, 30.0, 0.0);
            pause.finish();
            rest_status.set_text(&format!(
                "Transient value ignored while paused: recorded={updated}."
            ));
        }
    });

    content.append(&scroller);
    append_control_row(&content, "RestPause", &pause_button, &rest_status);
    content.append(&bounds_status);
    content.append(&value_status);

    (
        scroll_page(&content),
        ViewportOwners {
            observer,
            rest_state,
        },
    )
}

fn axis_name(axis: ViewportAxis) -> &'static str {
    match axis {
        ViewportAxis::Horizontal => "horizontal",
        ViewportAxis::Vertical => "vertical",
    }
}

struct WidgetOwners {
    render_hold: Rc<RenderHoldOverlay>,
}

impl WidgetOwners {
    fn render_hold_active(&self) -> bool {
        self.render_hold.is_active()
    }
}

fn build_widgets_page() -> (gtk4::Widget, WidgetOwners) {
    let content = workflow_box("Widget Geometry And Render Hold");
    append_body(
        &content,
        "ClipBin keeps flexible content from pushing fixed chrome away. \
         RenderHoldOverlay owns a non-targetable cover and caller-directed \
         capture, warm, reveal, and clear phases.",
    );

    let clipped_label = gtk4::Label::new(Some(
        "A very long ClipBin child that should clip inside constrained geometry \
         instead of growing the page root horizontally.",
    ));
    clipped_label.set_hexpand(true);
    clipped_label.set_xalign(0.0);
    let clip_bin = ClipBin::with_child(&clipped_label);
    clip_bin.set_size_request(260, 54);
    clip_bin.add_css_class("view");
    append_control_row(
        &content,
        "ClipBin",
        &clip_bin,
        &status_label("Constrained width; flexible child remains clipped."),
    );

    let overlay = gtk4::Overlay::new();
    overlay.set_size_request(360, 180);
    let live_child = gtk4::Label::new(Some("render-hold live child"));
    live_child.set_hexpand(true);
    live_child.set_vexpand(true);
    live_child.add_css_class("title-2");
    overlay.set_child(Some(&live_child));
    let render_hold = Rc::new(RenderHoldOverlay::new(&overlay, &live_child));

    let hold_status = status_label(&format!(
        "cover_targetable={}",
        render_hold.cover_can_target()
    ));
    let capture_button = gtk4::Button::with_label("Capture");
    capture_button.connect_clicked({
        let render_hold = Rc::clone(&render_hold);
        let hold_status = hold_status.clone();
        move |_| {
            let result = render_hold.capture();
            hold_status.set_text(render_hold_capture_label(result));
        }
    });

    let not_ready_button = gtk4::Button::with_label("Check Not Ready");
    not_ready_button.connect_clicked({
        let hold_status = hold_status.clone();
        move |_| {
            let unmapped_overlay = gtk4::Overlay::new();
            let unmapped_child = gtk4::Label::new(Some("unmapped"));
            unmapped_overlay.set_child(Some(&unmapped_child));
            let unmapped_hold = RenderHoldOverlay::new(&unmapped_overlay, &unmapped_child);
            hold_status.set_text(render_hold_capture_label(unmapped_hold.capture()));
        }
    });

    let warm_button = gtk4::Button::with_label("Warm");
    warm_button.connect_clicked({
        let render_hold = Rc::clone(&render_hold);
        let hold_status = hold_status.clone();
        move |_| {
            render_hold.warm_live_child();
            hold_status.set_text(&format!("warmed={}", render_hold.is_warmed()));
        }
    });

    let reveal_button = gtk4::Button::with_label("Reveal");
    reveal_button.connect_clicked({
        let render_hold = Rc::clone(&render_hold);
        let hold_status = hold_status.clone();
        move |_| {
            render_hold.reveal();
            hold_status.set_text("Live child revealed.");
        }
    });

    let clear_button = gtk4::Button::with_label("Early Clear");
    clear_button.connect_clicked({
        let render_hold = Rc::clone(&render_hold);
        let hold_status = hold_status.clone();
        move |_| {
            render_hold.clear();
            hold_status.set_text("Hold cleared early.");
        }
    });

    let button_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    button_row.append(&capture_button);
    button_row.append(&not_ready_button);
    button_row.append(&warm_button);
    button_row.append(&reveal_button);
    button_row.append(&clear_button);
    content.append(&overlay);
    append_control_row(&content, "RenderHoldOverlay", &button_row, &hold_status);

    (scroll_page(&content), WidgetOwners { render_hold })
}

fn render_hold_capture_label(result: RenderHoldCapture) -> &'static str {
    match result {
        RenderHoldCapture::Captured => "Captured: cover visible, live child hidden.",
        RenderHoldCapture::AlreadyHolding => "Already holding: original pixels preserved.",
        RenderHoldCapture::NotReady(RenderHoldNotReady::NotMapped) => {
            "Not ready: overlay or child is not mapped."
        }
        RenderHoldCapture::NotReady(RenderHoldNotReady::EmptyAllocation) => {
            "Not ready: allocation is empty."
        }
        RenderHoldCapture::NotReady(RenderHoldNotReady::MissingRenderer) => {
            "Not ready: renderer is missing."
        }
        RenderHoldCapture::NotReady(RenderHoldNotReady::EmptySnapshot) => {
            "Not ready: snapshot was empty."
        }
    }
}

struct ProofHarnessOwners {
    config: HarnessConfig,
}

impl ProofHarnessOwners {
    fn attempt_summary(&self) -> String {
        format!(
            "{}:{}",
            self.config.child_test_env(),
            self.config.headless_runner_env()
        )
    }
}

fn build_proof_harness_page() -> (gtk4::Widget, ProofHarnessOwners) {
    let content = workflow_box("Headless Proof Harness Contract");
    append_body(
        &content,
        "The lab keeps test registration, waits, and child-process environment \
         settings outside LushText resources.",
    );

    let config = adoption_harness_config();
    let registered = RegisteredTest::new("adoption-lab-smoke", harness_smoke_test);
    let env_summary = recommended_pre_gtk_environment()
        .into_iter()
        .map(|entry| format!("{}={}", entry.key, entry.value))
        .collect::<Vec<_>>()
        .join(", ");

    append_fact(&content, "child env", config.child_test_env());
    append_fact(&content, "runner env", config.headless_runner_env());
    append_fact(&content, "monitor env", config.headless_monitor_env());
    append_fact(&content, "registered test", registered.name());
    append_fact(&content, "recommended env", &env_summary);

    (scroll_page(&content), ProofHarnessOwners { config })
}

fn adoption_harness_config() -> HarnessConfig {
    HarnessConfig::new(
        "GTK_LUSH_ADOPTION_LAB_CHILD_TEST",
        "GTK_LUSH_ADOPTION_LAB_HEADLESS",
        "GTK_LUSH_ADOPTION_LAB_MONITOR",
    )
    .with_default_headless_monitor("1280x900")
    .with_test_attempts(1)
    .with_runner_label("GTK Lush adoption lab")
}

fn harness_smoke_test() {}

#[derive(Clone)]
struct DemoProofProvider {
    sequence: Rc<Cell<u64>>,
}

impl DemoProofProvider {
    fn new() -> Self {
        Self {
            sequence: Rc::new(Cell::new(1)),
        }
    }
}

impl ReadinessProvider for DemoProofProvider {
    fn readiness(&self, predicate: &ReadinessPredicate) -> ReadinessResult {
        match predicate.as_str() {
            "lab-idle" => ReadinessResult::ready(predicate.clone()),
            "render-hold-settled" => ReadinessResult::blocked(
                predicate.clone(),
                BlockerSummary::new("render-hold").with_detail("cover is warming"),
            ),
            _ => ReadinessResult::unknown(predicate.clone()),
        }
    }
}

impl SnapshotProvider for DemoProofProvider {
    fn snapshot(&self) -> SnapshotEnvelope {
        let sequence = self.sequence.get();
        SnapshotEnvelope {
            version: VersionInfo::current().with_interface_version("adoption-lab"),
            surfaces: vec![
                SurfaceSummary::new("signals", true).with_rect(Rect::new(0, 0, 280, 180)),
                SurfaceSummary::new("render-hold", true).with_rect(Rect::new(300, 0, 360, 180)),
            ],
            workflows: self.workflow_events(),
            privacy: PrivacyScope::PublicDiagnostic,
            ..SnapshotEnvelope::new(sequence)
        }
    }
}

impl WorkflowEventProvider for DemoProofProvider {
    fn workflow_events(&self) -> Vec<WorkflowEvent> {
        vec![
            WorkflowEvent {
                workflow_id: "adoption-lab".to_string(),
                phase: WorkflowPhase::Start,
                status: ProofStatus::Ready,
                sequence: self.sequence.get(),
                detail: Some("bounded workflow metadata only".to_string()),
                blocker: None,
            },
            WorkflowEvent {
                workflow_id: "render-hold".to_string(),
                phase: WorkflowPhase::Progress,
                status: ProofStatus::Blocked,
                sequence: self.sequence.get().saturating_add(1),
                detail: Some("cover warming".to_string()),
                blocker: Some(BlockerSummary::new("render-hold")),
            },
        ]
    }
}

impl ArtifactSummaryProvider for DemoProofProvider {
    fn summarize_artifact(&self, command: &str) -> ArtifactEnvelope {
        ArtifactEnvelope::success(command, "adoption lab summary").with_data(serde_json::json!({
            "workflow_count": 7,
            "privacy": "public-diagnostic"
        }))
    }
}

struct ProofSpineOwners {
    provider: DemoProofProvider,
}

impl ProofSpineOwners {
    fn snapshot_sequence(&self) -> u64 {
        self.provider.snapshot().sequence
    }
}

fn build_proof_spine_page() -> (gtk4::Widget, ProofSpineOwners) {
    let content = workflow_box("GTK-Free Proof Spine Values");
    append_body(
        &content,
        "The provider owns application state and only emits bounded readiness, \
         workflow, snapshot, and artifact values.",
    );

    let provider = DemoProofProvider::new();
    let idle = provider.readiness(&ReadinessPredicate::new("lab-idle"));
    let blocked = provider.readiness(&ReadinessPredicate::new("render-hold-settled"));
    let snapshot = provider.snapshot();
    let artifact = provider.summarize_artifact("lab-summary");

    append_fact(&content, "idle ready", &idle.ready.to_string());
    append_fact(&content, "blocked status", &format!("{:?}", blocked.status));
    append_fact(
        &content,
        "snapshot surfaces",
        &snapshot.surfaces.len().to_string(),
    );
    append_fact(
        &content,
        "workflow events",
        &snapshot.workflows.len().to_string(),
    );
    append_fact(&content, "artifact ok", &artifact.ok.to_string());

    (scroll_page(&content), ProofSpineOwners { provider })
}

fn workflow_box(title: &str) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_hexpand(true);
    content.set_vexpand(true);

    let label = gtk4::Label::new(Some(title));
    label.add_css_class("title-2");
    label.set_xalign(0.0);
    content.append(&label);
    content
}

fn append_body(container: &gtk4::Box, text: &str) {
    let label = gtk4::Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    container.append(&label);
}

fn append_control_row(
    container: &gtk4::Box,
    title: &str,
    control: &impl IsA<gtk4::Widget>,
    status: &gtk4::Label,
) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.set_hexpand(true);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_width_chars(20);
    title_label.set_xalign(0.0);
    row.append(&title_label);
    row.append(control);
    row.append(status);
    container.append(&row);
}

fn append_fact(container: &gtk4::Box, key: &str, value: &str) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let key_label = gtk4::Label::new(Some(key));
    key_label.set_width_chars(20);
    key_label.set_xalign(0.0);
    let value_label = status_label(value);
    row.append(&key_label);
    row.append(&value_label);
    container.append(&row);
}

fn status_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_hexpand(true);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

fn scroll_page(content: &gtk4::Box) -> gtk4::Widget {
    gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_width(false)
        .child(content)
        .build()
        .upcast()
}

fn awkward_rows(prefix: &str, count: usize) -> String {
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        rows.push(format!(
            "{prefix} {index:02} -- alpha beta gamma delta epsilon zeta eta theta iota kappa"
        ));
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_cleanup_runs_once() {
        let calls = Rc::new(Cell::new(0u32));
        let bag = RegistrationBag::new();
        bag.track({
            let calls = Rc::clone(&calls);
            move || calls.set(calls.get().saturating_add(1))
        });

        bag.clear();
        bag.clear();

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn settle_tokens_keep_latest_generation() {
        let debounce = Debounce::new();
        let first = debounce.advance();
        let second = debounce.advance();

        assert!(!debounce.is_current(first));
        assert!(debounce.is_current(second));
    }

    #[test]
    fn task_freshness_rejects_stale_result() {
        let requested = FreshnessToken::new(4);
        let current = FreshnessToken::new(5);
        let result = requested.accept(current, "payload");

        assert!(result.is_err());
    }

    #[test]
    fn rest_pause_excludes_transient_values() {
        let rest = RestState::new();
        rest.set_at_lower(ViewportAxis::Vertical, true);

        let pause = rest.pause();
        let recorded = rest.record_value(ViewportAxis::Vertical, 42.0, 0.0);
        pause.finish();

        assert!(!recorded);
        assert!(rest.at_lower(ViewportAxis::Vertical));
    }

    #[test]
    fn render_hold_capture_labels_all_not_ready_reasons() {
        let reasons = [
            RenderHoldCapture::NotReady(RenderHoldNotReady::NotMapped),
            RenderHoldCapture::NotReady(RenderHoldNotReady::EmptyAllocation),
            RenderHoldCapture::NotReady(RenderHoldNotReady::MissingRenderer),
            RenderHoldCapture::NotReady(RenderHoldNotReady::EmptySnapshot),
        ];

        for reason in reasons {
            assert!(render_hold_capture_label(reason).starts_with("Not ready:"));
        }
    }

    #[test]
    fn proof_harness_config_uses_lab_environment_names() {
        let config = adoption_harness_config();

        assert_eq!(config.child_test_env(), "GTK_LUSH_ADOPTION_LAB_CHILD_TEST");
        assert_eq!(
            config.headless_runner_env(),
            "GTK_LUSH_ADOPTION_LAB_HEADLESS"
        );
        assert_eq!(recommended_pre_gtk_environment().len(), 4);
    }

    #[test]
    fn proof_provider_emits_bounded_snapshot_and_artifact() {
        let provider = DemoProofProvider::new();
        let ready = provider.readiness(&ReadinessPredicate::new("lab-idle"));
        let blocked = provider.readiness(&ReadinessPredicate::new("render-hold-settled"));
        let snapshot = provider.snapshot();
        let artifact = provider.summarize_artifact("lab-summary");

        assert!(ready.ready);
        assert_eq!(blocked.status, ProofStatus::Blocked);
        assert_eq!(snapshot.privacy, PrivacyScope::PublicDiagnostic);
        assert_eq!(snapshot.surfaces.len(), 2);
        assert!(artifact.ok);
    }
}
