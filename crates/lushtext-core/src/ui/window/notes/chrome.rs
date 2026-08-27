// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: dialog chrome shared by the notes surfaces.
//!
//! This module carries **no role**. The notes browser and both note editors need
//! the same compact close affordance, the same Escape-closes-content controller,
//! the same post-`present()` focus deferral, and the same empty-state label, so
//! they live here rather than being duplicated per dialog. It owns no pure policy
//! and no evidence surface, and it is named in the `WFR-NOTES-BOOKMARKS` matrix
//! row.

use gtk4::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use crate::ui::accessibility;

/// Build one compact close affordance for browser-style dialogs.
pub(super) fn build_dialog_close_button(dialog: &libadwaita::Dialog) -> gtk4::Button {
    let close_button = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close")
        .build();
    accessibility::set_labelled_description(
        &close_button,
        "Close",
        "Close this dialog and return to the editor",
    );
    let dialog_weak = dialog.downgrade();
    close_button.connect_clicked(move |_| {
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
    });
    close_button
}

/// Close dialog content on Escape even when the focused child owns key handling.
pub(super) fn install_dialog_escape_close(
    dialog: &libadwaita::Dialog,
    widget: &impl IsA<gtk4::Widget>,
) {
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let dialog_weak = dialog.downgrade();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key != gtk4::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
        glib::Propagation::Stop
    });
    widget.as_ref().add_controller(controller);
}

/// Defer focus until after `AdwDialog::present()` realizes its child tree.
pub(super) fn focus_after_present(widget: &impl IsA<gtk4::Widget>) {
    let widget_weak = widget.as_ref().downgrade();
    glib::idle_add_local_once(move || {
        if let Some(widget) = widget_weak.upgrade() {
            widget.grab_focus();
        }
    });
}

/// Build the empty-state label shown when a browser search has no matches.
#[must_use]
pub(super) fn empty_browser_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_halign(gtk4::Align::Center);
    label.add_css_class("dim-label");
    accessibility::set_role(&label, gtk4::AccessibleRole::Status);
    accessibility::set_label(&label, text);
    label
}
