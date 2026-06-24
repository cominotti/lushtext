// SPDX-License-Identifier: GPL-3.0-or-later

//! Adoption-lab page for GTK Lush debounce and settle helpers.

use std::time::Duration;

use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer};
use gtk4::prelude::*;

use crate::shared_ui::{append_body, append_control_row, scroll_page, status_label, workflow_box};

#[derive(Clone)]
/// Keeps the settle burst alive so the shell can report pending timer state.
pub(crate) struct SettleOwners {
    burst: SettleBurst,
}

impl SettleOwners {
    /// Return whether the page currently has a pending settle burst.
    pub(crate) fn pending(&self) -> bool {
        self.burst.pending()
    }
}

/// Build the page that demonstrates debounce, settle-burst, and superseding timers.
pub(crate) fn build_settle_page() -> (gtk4::Widget, SettleOwners) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_tokens_keep_latest_generation() {
        let debounce = Debounce::new();
        let first = debounce.advance();
        let second = debounce.advance();

        assert!(!debounce.is_current(first));
        assert!(debounce.is_current(second));
    }
}
