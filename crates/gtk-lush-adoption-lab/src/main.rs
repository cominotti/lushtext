// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK Lush adoption lab application shell.

use std::rc::Rc;

use gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;

mod proof_pages;
mod settle_page;
mod shared_ui;
mod signals_page;
mod tasks_page;
mod viewport_page;
mod widgets_page;

use proof_pages::{
    ProofHarnessOwners, ProofSpineOwners, build_proof_harness_page, build_proof_spine_page,
};
use settle_page::{SettleOwners, build_settle_page};
use signals_page::{SignalOwners, build_signals_page};
use tasks_page::{TaskOwners, build_tasks_page};
use viewport_page::{ViewportOwners, build_viewport_page};
use widgets_page::{WidgetOwners, build_widgets_page};

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
