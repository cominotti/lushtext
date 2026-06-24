// SPDX-License-Identifier: GPL-3.0-or-later

//! Adoption-lab page for GTK Lush signal, binding, and registration lifetimes.

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_signals::{BindingBag, RegistrationBag, SignalBag};
use gtk4::prelude::*;

use crate::shared_ui::{append_body, append_control_row, scroll_page, status_label, workflow_box};

/// Keeps signal, binding, and cleanup registrations alive for the Signals page.
pub(crate) struct SignalOwners {
    signal_bag: Rc<SignalBag>,
    binding_bag: Rc<BindingBag>,
    registration_bag: Rc<RegistrationBag>,
}

impl SignalOwners {
    /// Return the number of live registrations tracked by the page.
    pub(crate) fn registration_count(&self) -> usize {
        self.signal_bag.len() + self.binding_bag.len() + self.registration_bag.len()
    }
}

/// Build the page that demonstrates GTK Lush lifetime bags.
pub(crate) fn build_signals_page() -> (gtk4::Widget, SignalOwners) {
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
}
