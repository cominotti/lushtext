// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** of `WFR-TAB-OVERFLOW-NAVIGATION` — not a
//! role. It builds the strip's trailing control group (design D1): the Scroll
//! Tabs Back and Forward buttons, their icons, tooltips and accessible names,
//! each with its capture-phase hold gesture, then the All Tabs menu button
//! with its count and popover (D5), and moves keyboard focus between them
//! when a focused one can no longer hold it. Every decision it projects
//! comes from `policy.rs`; the stages are in `scroll_execution.rs` and
//! `watch.rs`.

use gtk4::prelude::*;

use super::all_tabs_popover::LushtextAllTabsPopover;
use super::policy::{
    FocusHandoff, OverflowControl, SCROLL_BACK_LABEL, SCROLL_FORWARD_LABEL, ScrollDirection,
    all_tabs_button_name,
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
    /// The All Tabs menu button.
    pub(crate) all_tabs: gtk4::MenuButton,
    /// The All Tabs button's count.
    pub(crate) all_tabs_count: gtk4::Label,
    /// The All Tabs popover.
    pub(crate) popover: LushtextAllTabsPopover,
}

impl OverflowControls {
    /// The scroll control that holds keyboard focus inside `window`, if any;
    /// none while the focus is inside the All Tabs popover.
    pub(super) fn focused_control(
        &self,
        window: &impl IsA<gtk4::Window>,
    ) -> Option<OverflowControl> {
        let focus = gtk4::prelude::GtkWindowExt::focus(window.as_ref())?;
        // The popover is parented to the All Tabs button, but focus inside it
        // is the popover's: its close gives the saved focus back
        // (`restore_focus_after_all_tabs`), and a hand-off here would compete.
        if focus == *self.popover.upcast_ref::<gtk4::Widget>() || focus.is_ancestor(&self.popover) {
            return None;
        }
        [
            (
                self.back.upcast_ref::<gtk4::Widget>(),
                OverflowControl::Back,
            ),
            (self.forward.upcast_ref(), OverflowControl::Forward),
            (self.all_tabs.upcast_ref(), OverflowControl::AllTabs),
        ]
        .into_iter()
        .find_map(|(widget, control)| {
            (focus == *widget || focus.is_ancestor(widget)).then_some(control)
        })
    }

    /// Show `count` on the All Tabs button, its tooltip and accessible name.
    pub(super) fn show_all_tabs_count(&self, count: u32) {
        let text = count.to_string();
        if self.all_tabs_count.label() != text {
            self.all_tabs_count.set_label(&text);
            let name = all_tabs_button_name(count);
            self.all_tabs.set_tooltip_text(Some(&name));
            accessibility::set_label(&self.all_tabs, &name);
        }
    }

    /// Move focus to `handoff`'s control; the editor case is the caller's.
    pub(super) fn focus_control(&self, handoff: FocusHandoff) -> bool {
        match handoff {
            FocusHandoff::Control(OverflowControl::Back) => self.back.grab_focus(),
            FocusHandoff::Control(OverflowControl::Forward) => self.forward.grab_focus(),
            FocusHandoff::Control(OverflowControl::AllTabs) => self.all_tabs.grab_focus(),
            FocusHandoff::Editor => false,
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
    // Hidden until the reconcile shows them; the group can attach for the
    // All Tabs button alone.
    back.set_visible(false);
    forward.set_visible(false);
    group.append(&back);
    group.append(&forward);
    let (all_tabs, all_tabs_count, popover) = all_tabs_button();
    group.append(&all_tabs);
    OverflowControls {
        group,
        back,
        forward,
        all_tabs,
        all_tabs_count,
        popover,
    }
}

/// The All Tabs button: a flat menu button that never takes focus on a
/// pointer press, showing the open-tab count in tabular figures, and owning
/// the popover (design D5; `AdwTabButton` cannot own a popover).
fn all_tabs_button() -> (gtk4::MenuButton, gtk4::Label, LushtextAllTabsPopover) {
    let count = gtk4::Label::new(Some("0"));
    count.add_css_class("numeric");
    let button = gtk4::MenuButton::new();
    button.add_css_class("flat");
    button.add_css_class("tab-overflow-all-tabs");
    button.set_focus_on_click(false);
    button.set_child(Some(&count));
    button.set_always_show_arrow(false);
    button.set_visible(false);
    let popover = LushtextAllTabsPopover::new();
    button.set_popover(Some(&popover));
    let name = all_tabs_button_name(0);
    button.set_tooltip_text(Some(&name));
    accessibility::set_label(&button, &name);
    accessibility::set_has_popup(&button, true);
    accessibility::set_key_shortcuts(&button, "Control+Shift+O");
    (button, count, popover)
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

    // The pointer belongs to the capture-phase hold gesture alone. GtkButton's
    // own bubble-phase click gesture is disabled at the source
    // (`widget-wiring.md`, "GTK4 Signal Delivery Pitfalls"): a claim only
    // denies it while this gesture holds the sequence, and the live check
    // found a release after a selection during a hold reaching it, so the
    // release emitted `clicked` and added a step. Space and Enter still emit
    // `clicked` through the button's keybinding, which runs the action once,
    // without repeat (design D3).
    disable_own_click_gesture(&button);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    gesture.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        if let Some(widget) = gesture.widget() {
            widget.set_state_flags(gtk4::StateFlags::ACTIVE, false);
        }
        hold(direction);
    });
    let on_release = release.clone();
    gesture.connect_released(move |gesture, _, _, _| {
        if let Some(widget) = gesture.widget() {
            widget.unset_state_flags(gtk4::StateFlags::ACTIVE);
        }
        on_release();
    });
    gesture.connect_cancel(move |gesture, _| {
        if let Some(widget) = gesture.widget() {
            widget.unset_state_flags(gtk4::StateFlags::ACTIVE);
        }
        release();
    });
    button.add_controller(gesture);
    button
}

/// Turn off `button`'s own click gesture, so no pointer release can emit
/// `clicked`; its pressed look comes from the hold gesture instead.
fn disable_own_click_gesture(button: &gtk4::Button) {
    let controllers = button.observe_controllers();
    for index in 0..controllers.n_items() {
        if let Some(click) = controllers.item(index).and_downcast::<gtk4::GestureClick>() {
            click.set_propagation_phase(gtk4::PropagationPhase::None);
        }
    }
}
