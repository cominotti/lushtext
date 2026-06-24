// SPDX-License-Identifier: GPL-3.0-or-later

//! Adoption-lab page for GTK Lush widgets.

use std::rc::Rc;

use gtk_lush_widgets::{ClipBin, RenderHoldCapture, RenderHoldNotReady, RenderHoldOverlay};
use gtk4::prelude::*;

use crate::shared_ui::{append_body, append_control_row, scroll_page, status_label, workflow_box};

/// Keeps the render-hold overlay alive for the widget demo page.
pub(crate) struct WidgetOwners {
    render_hold: Rc<RenderHoldOverlay>,
}

impl WidgetOwners {
    /// Return whether the render hold currently has a captured cover active.
    pub(crate) fn render_hold_active(&self) -> bool {
        self.render_hold.is_active()
    }
}

/// Build the page that demonstrates ClipBin and RenderHoldOverlay behavior.
pub(crate) fn build_widgets_page() -> (gtk4::Widget, WidgetOwners) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
