// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** of `WFR-TAB-OVERFLOW-NAVIGATION` — not a
//! role. It builds the strip's trailing control group (design D1): the Scroll
//! Tabs Back and Forward buttons, their icons, tooltips and accessible names,
//! each with its capture-phase hold gesture, and moves keyboard focus between
//! them when a focused one can no longer hold it. Every decision it projects
//! comes from `policy.rs`; the stages are in `scroll_execution.rs` and
//! `watch.rs`.

use gtk4::prelude::*;

use super::policy::{
    FocusHandoff, OverflowControl, SCROLL_BACK_LABEL, SCROLL_FORWARD_LABEL, ScrollDirection,
};
use crate::ui::accessibility;

/// The window action each scroll button activates.
pub(super) const SCROLL_BACK_ACTION: &str = "scroll-tabs-back";
/// The window action each scroll button activates.
pub(super) const SCROLL_FORWARD_ACTION: &str = "scroll-tabs-forward";

/// The end group and its controls, built once per window.
#[derive(Clone, Debug)]
pub(crate) struct OverflowControls {
    /// The `end-action-widget` of the bar.
    pub(crate) group: gtk4::Box,
    /// Scroll Tabs Back.
    pub(crate) back: gtk4::Button,
    /// Scroll Tabs Forward.
    pub(crate) forward: gtk4::Button,
}

impl OverflowControls {
    /// The scroll control that holds keyboard focus inside `window`, if any.
    pub(super) fn focused_control(
        &self,
        window: &impl IsA<gtk4::Window>,
    ) -> Option<OverflowControl> {
        let focus = gtk4::prelude::GtkWindowExt::focus(window.as_ref())?;
        if focus.is_ancestor(&self.back) || focus == *self.back.upcast_ref::<gtk4::Widget>() {
            Some(OverflowControl::Back)
        } else if focus.is_ancestor(&self.forward)
            || focus == *self.forward.upcast_ref::<gtk4::Widget>()
        {
            Some(OverflowControl::Forward)
        } else {
            None
        }
    }

    /// Move focus to `handoff`'s control; the editor case is the caller's.
    pub(super) fn focus_control(&self, handoff: FocusHandoff) -> bool {
        match handoff {
            FocusHandoff::Control(OverflowControl::Back) => self.back.grab_focus(),
            FocusHandoff::Control(OverflowControl::Forward) => self.forward.grab_focus(),
            // The All Tabs button joins the group with the popover half.
            FocusHandoff::Control(OverflowControl::AllTabs) | FocusHandoff::Editor => false,
        }
    }
}

/// Build the end group: two flat scroll buttons that never take focus on a
/// pointer press, whose keyboard activation runs the window action, and whose
/// pointer press is owned by a capture-phase hold gesture (`hold` runs on
/// press, `release` on release or cancel).
pub(super) fn build_controls(
    hold: impl Fn(ScrollDirection) + Clone + 'static,
    release: impl Fn() + Clone + 'static,
) -> OverflowControls {
    let group = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    group.add_css_class("tab-overflow-controls");
    let back = scroll_button(
        "go-previous-symbolic",
        SCROLL_BACK_LABEL,
        SCROLL_BACK_ACTION,
        ScrollDirection::Back,
        hold.clone(),
        release.clone(),
    );
    let forward = scroll_button(
        "go-next-symbolic",
        SCROLL_FORWARD_LABEL,
        SCROLL_FORWARD_ACTION,
        ScrollDirection::Forward,
        hold,
        release,
    );
    group.append(&back);
    group.append(&forward);
    OverflowControls {
        group,
        back,
        forward,
    }
}

fn scroll_button(
    icon: &str,
    label: &str,
    action: &str,
    direction: ScrollDirection,
    hold: impl Fn(ScrollDirection) + 'static,
    release: impl Fn() + Clone + 'static,
) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.set_focus_on_click(false);
    button.set_tooltip_text(Some(label));
    button.set_action_name(Some(&format!("win.{action}")));
    accessibility::set_label(&button, label);

    // The capture-phase press claims the sequence, so GtkButton's own
    // bubble-phase click gesture is denied: no `clicked`, and no extra step
    // on release. Space and Enter still emit `clicked`, which runs the action
    // once, without repeat (design D3).
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    gesture.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        hold(direction);
    });
    let on_release = release.clone();
    gesture.connect_released(move |_, _, _, _| on_release());
    gesture.connect_cancel(move |_, _| release());
    button.add_controller(gesture);
    button
}
