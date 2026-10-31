// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`interaction_execution`, the caret-free interaction
//! stage of the Protected View workflow; design D3, task 4.3).
//!
//! Installed once per editor and gated at run time on the session's
//! `protected` flag, so entering and leaving Protected View connects nothing:
//!
//! - every GtkTextView and GtkSourceView keybinding **signal** that can move a
//!   mark or change the buffer gets a handler that stops its emission (ledger
//!   A64: a stopped signal keeps its default handler from running; per-instance
//!   action enablement is not a guard, GtkTextView recomputes it);
//! - `move-cursor` becomes a viewport movement (`policy::viewport_step`, A61);
//! - a capture-phase key controller claims the binding **actions**
//!   (`text.undo` routes to the entry undo while it is available, `text.redo`
//!   and `menu.popup` are pre-empted);
//! - capture-phase click and drag gestures claim presses and drags before
//!   GtkTextView's own gestures see them (A59; `policy::press_disposition`,
//!   touchscreen pans stay unclaimed so the scrolled window pans);
//! - the view's drop target is removed while protected and restored on exit
//!   (A63).

use std::cell::RefCell;

use gtk4::prelude::*;

use super::policy::{
    CaptureGesture, MovementStep, PointerDevice, PressButton, PressDisposition, PressHit,
    PressInput, ViewportAxis, ViewportMotion, press_disposition, viewport_step,
};
use crate::ui::editor_page::LushtextEditorPage;

/// Keybinding signals whose default handler moves a mark, changes the buffer
/// or the view state (design D3 step 5). `move-cursor` has its own handler.
const STOPPED_SIGNALS: [&str; 19] = [
    "select-all",
    "toggle-cursor-visible",
    "cut-clipboard",
    "copy-clipboard",
    "paste-clipboard",
    "backspace",
    "delete-from-cursor",
    "insert-at-cursor",
    "toggle-overwrite",
    "set-anchor",
    "insert-emoji",
    "change-number",
    "change-case",
    "join-lines",
    "move-to-matching-bracket",
    "move-lines",
    "move-words",
    "show-completion",
    "push-snippet",
];

/// The drop targets taken off the view while protected, to restore on exit.
#[derive(Debug, Default)]
pub(crate) struct DetachedDropTargets {
    controllers: RefCell<Vec<gtk4::EventController>>,
}

impl LushtextEditorPage {
    /// Install the caret-free interaction handlers once, at construction.
    pub(in crate::ui::editor_page) fn setup_protected_view_interaction(&self) {
        let view = self.source_view();
        for name in STOPPED_SIGNALS {
            let page = self.downgrade();
            view.connect_local(name, false, move |args| {
                if let Some(page) = page.upgrade()
                    && page.is_protected_view()
                    && let Some(Ok(view)) = args.first().map(glib::Value::get::<glib::Object>)
                {
                    glib::signal::signal_stop_emission_by_name(&view, name);
                }
                None
            });
        }
        let page = self.downgrade();
        view.connect_local("extend-selection", false, move |_| {
            // Returns whether the handler extended the selection; claiming it
            // keeps GtkTextView's default from selecting.
            let protected = page.upgrade().is_some_and(|page| page.is_protected_view());
            protected.then(|| true.to_value())
        });
        let page = self.downgrade();
        view.connect_move_cursor(move |view, step, count, extend| {
            let Some(page) = page.upgrade() else { return };
            if !page.is_protected_view() {
                return;
            }
            glib::signal::signal_stop_emission_by_name(view, "move-cursor");
            if let Some(motion) = viewport_step(movement_step(step), count, extend) {
                scroll_viewport(view.upcast_ref(), motion.axis, motion.motion);
            }
        });
        self.install_protected_key_controller(view);
        self.install_protected_gestures(view);
    }

    fn install_protected_key_controller(&self, view: &sourceview5::View) {
        let controller = gtk4::EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let page = self.downgrade();
        controller.connect_key_pressed(move |_, key, _, state| {
            let Some(page) = page.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !page.is_protected_view() {
                return glib::Propagation::Proceed;
            }
            let control = state.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            let lower = key.to_lower();
            if control && !shift && lower == gtk4::gdk::Key::z {
                // `text.undo`: the entry undo while it is available; otherwise
                // pre-empted like every other mutating action.
                let _ = page.undo_protected_entry();
                return glib::Propagation::Stop;
            }
            let redo =
                control && (lower == gtk4::gdk::Key::y || (shift && lower == gtk4::gdk::Key::z));
            let menu = (shift && key == gtk4::gdk::Key::F10) || key == gtk4::gdk::Key::Menu;
            if redo || menu {
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        view.add_controller(controller);
    }

    fn install_protected_gestures(&self, view: &sourceview5::View) {
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let page = self.downgrade();
        click.connect_pressed(move |gesture, n_press, _, _| {
            if !page.upgrade().is_some_and(|page| page.is_protected_view()) {
                return;
            }
            let input = PressInput {
                gesture: CaptureGesture::Click,
                button: press_button(gesture.current_button()),
                n_press,
                hit: PressHit::Text,
                device: pointer_device(gesture.upcast_ref()),
            };
            match press_disposition(input) {
                PressDisposition::Unclaimed => {}
                // The inspector and the Protected View menu join with tasks
                // 4.4 and 5.x; until then every press is only claimed.
                PressDisposition::Claim
                | PressDisposition::OpenMenu
                | PressDisposition::OpenInspector { .. } => {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                }
            }
        });
        view.add_controller(click);

        let drag = gtk4::GestureDrag::new();
        drag.set_button(0);
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let page = self.downgrade();
        drag.connect_drag_begin(move |gesture, _, _| {
            if !page.upgrade().is_some_and(|page| page.is_protected_view()) {
                return;
            }
            let input = PressInput {
                gesture: CaptureGesture::Drag,
                button: press_button(gesture.current_button()),
                n_press: 1,
                hit: PressHit::Text,
                device: pointer_device(gesture.upcast_ref()),
            };
            if press_disposition(input) != PressDisposition::Unclaimed {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
            }
        });
        view.add_controller(drag);
    }

    /// Take the view's drop targets off while protected (ledger A63).
    pub(in crate::ui::editor_page) fn detach_protected_drop_targets(&self) {
        let view = self.source_view();
        let detached = &self.imp_detached_drop_targets().controllers;
        if !detached.borrow().is_empty() {
            return;
        }
        let controllers = view.observe_controllers();
        let found: Vec<gtk4::EventController> = (0..controllers.n_items())
            .filter_map(|index| controllers.item(index))
            .filter_map(|item| item.downcast::<gtk4::EventController>().ok())
            .filter(|controller| {
                controller.is::<gtk4::DropTargetAsync>() || controller.is::<gtk4::DropTarget>()
            })
            .collect();
        for controller in &found {
            view.remove_controller(controller);
        }
        detached.replace(found);
    }

    /// Put the drop targets back when the session leaves Protected View.
    pub(in crate::ui::editor_page) fn restore_protected_drop_targets(&self) {
        let view = self.source_view();
        for controller in self.imp_detached_drop_targets().controllers.take() {
            view.add_controller(controller);
        }
    }

    fn imp_detached_drop_targets(&self) -> &DetachedDropTargets {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        &self.imp().protected_drop_targets
    }
}

fn movement_step(step: gtk4::MovementStep) -> MovementStep {
    match step {
        gtk4::MovementStep::VisualPositions => MovementStep::VisualPositions,
        gtk4::MovementStep::Words => MovementStep::Words,
        gtk4::MovementStep::DisplayLines => MovementStep::DisplayLines,
        gtk4::MovementStep::DisplayLineEnds => MovementStep::DisplayLineEnds,
        gtk4::MovementStep::Paragraphs => MovementStep::Paragraphs,
        gtk4::MovementStep::ParagraphEnds => MovementStep::ParagraphEnds,
        gtk4::MovementStep::Pages => MovementStep::Pages,
        gtk4::MovementStep::BufferEnds => MovementStep::BufferEnds,
        gtk4::MovementStep::HorizontalPages => MovementStep::HorizontalPages,
        _ => MovementStep::LogicalPositions,
    }
}

fn press_button(button: u32) -> PressButton {
    match button {
        gtk4::gdk::BUTTON_PRIMARY => PressButton::Primary,
        gtk4::gdk::BUTTON_MIDDLE => PressButton::Middle,
        gtk4::gdk::BUTTON_SECONDARY => PressButton::Secondary,
        _ => PressButton::Other,
    }
}

fn pointer_device(gesture: &gtk4::Gesture) -> PointerDevice {
    let Some(device) = gesture.device() else {
        return PointerDevice::Other;
    };
    match device.source() {
        gtk4::gdk::InputSource::Mouse => PointerDevice::Mouse,
        gtk4::gdk::InputSource::Touchpad => PointerDevice::Touchpad,
        gtk4::gdk::InputSource::Touchscreen => PointerDevice::Touchscreen,
        gtk4::gdk::InputSource::Pen => PointerDevice::Pen,
        _ => PointerDevice::Other,
    }
}

/// Apply one viewport movement to the text view's adjustments.
fn scroll_viewport(view: &gtk4::TextView, axis: ViewportAxis, motion: ViewportMotion) {
    let adjustment = match axis {
        ViewportAxis::Horizontal => view.hadjustment(),
        ViewportAxis::Vertical => view.vadjustment(),
    };
    let Some(adjustment) = adjustment else { return };
    let lower = adjustment.lower();
    let top = (adjustment.upper() - adjustment.page_size()).max(lower);
    let target = match motion {
        ViewportMotion::Steps(count) => {
            adjustment.value() + f64::from(count) * adjustment.step_increment()
        }
        ViewportMotion::Pages(count) => {
            adjustment.value() + f64::from(count) * adjustment.page_increment()
        }
        ViewportMotion::Start => lower,
        ViewportMotion::End => top,
    };
    adjustment.set_value(target.clamp(lower, top));
}
