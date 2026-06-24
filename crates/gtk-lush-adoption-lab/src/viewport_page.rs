// SPDX-License-Identifier: GPL-3.0-or-later

//! Adoption-lab page for viewport observation and rest-state tracking.

use gtk_lush_viewport::{RestState, ViewportAxis, ViewportObserver, rests_at_lower};
use gtk4::prelude::*;

use crate::shared_ui::{
    append_body, append_control_row, awkward_rows, scroll_page, status_label, workflow_box,
};

/// Keeps viewport observers and rest state alive for the viewport demo page.
pub(crate) struct ViewportOwners {
    observer: Option<ViewportObserver>,
    rest_state: RestState,
}

impl ViewportOwners {
    /// Return the number of adjustment signal registrations owned by the observer.
    pub(crate) fn registration_count(&self) -> usize {
        let _vertical_rest = self.rest_state.at_lower(ViewportAxis::Vertical);
        self.observer.as_ref().map_or(0, ViewportObserver::len)
    }
}

/// Build the page that demonstrates viewport observation and rest-state repair.
pub(crate) fn build_viewport_page() -> (gtk4::Widget, ViewportOwners) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
