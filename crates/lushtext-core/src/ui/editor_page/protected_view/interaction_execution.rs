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
    PressInput, ProtectedAction, ViewportAxis, ViewportMotion, press_disposition, viewport_step,
};
use crate::ui::editor_page::LushtextEditorPage;

/// Keybinding signals whose default handler moves a mark, changes the buffer
/// or the view state (design D3 step 5). `move-cursor` has its own handler.
const STOPPED_SIGNALS: [&str; 18] = [
    "select-all",
    "toggle-cursor-visible",
    "cut-clipboard",
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

/// The drop targets taken off the view while protected or while the edit lock
/// is held, to restore when neither holds them, and whether the buffer's
/// primary selection clipboard is off.
#[derive(Debug, Default)]
pub(crate) struct DetachedDropTargets {
    controllers: RefCell<Vec<gtk4::EventController>>,
    /// The holders, a bit per [`DropTargetHold`].
    holds: std::cell::Cell<u8>,
    primary_removed: std::cell::Cell<bool>,
}

/// Who keeps the view's drop targets off. Each holds them independently, so
/// the targets come back only when the last one releases them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DropTargetHold {
    /// A Protected View session (ledger A63).
    ProtectedView = 1,
    /// A workflow holding the edit lock (`edit_lock.rs`).
    EditLock = 2,
    /// Held typing's window: a load in flight or typed text held
    /// (`held_input.rs`), which outlasts the lock on both sides.
    HeldInput = 4,
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
                    if let Some(action) = ProtectedAction::from_toolkit_name(name) {
                        page.notify_protected_preemption(action);
                    }
                }
                None
            });
        }
        let page = self.downgrade();
        view.connect_local("copy-clipboard", false, move |args| {
            if let Some(page) = page.upgrade()
                && page.is_protected_view()
                && let Some(Ok(view)) = args.first().map(glib::Value::get::<glib::Object>)
            {
                // GTK's copy reads visible text only and would drop every
                // hidden tail (C6 review finding): serve it from the source.
                glib::signal::signal_stop_emission_by_name(&view, "copy-clipboard");
                page.copy_protected_selection();
            }
            None
        });
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
        // GtkTextView adds the primary clipboard to its buffer on realize and
        // removes it on unrealize (removing an absent one is a CRITICAL): put
        // it back before an unrealize, take it off again after a realize.
        let page = self.downgrade();
        view.connect_unrealize(move |_| {
            if let Some(page) = page.upgrade() {
                page.set_protected_primary_selection_off(false);
            }
        });
        let page = self.downgrade();
        view.connect_realize(move |_| {
            if let Some(page) = page.upgrade()
                && page.is_protected_view()
            {
                page.set_protected_primary_selection_off(true);
            }
        });
    }

    /// Take the buffer's primary selection clipboard off while protected
    /// (`off`), so the primary selection never offers GTK's visible-only text
    /// of a folded range (C6 review finding), or put it back.
    ///
    /// While the view shows its placeholder (`view_attachment`), GTK's
    /// registration is on the placeholder, not the document, and this records
    /// nothing: the state stays "on the document" and the attach applies
    /// Protected View's choice (review M1: entering Protected View while
    /// detached removed an absent registration, a `Gtk-CRITICAL`, and the
    /// attach then left the provider on PRIMARY). Entering it while detached
    /// does give back the reference the detach held, so the protected
    /// document offers nothing meanwhile.
    pub(in crate::ui::editor_page) fn set_protected_primary_selection_off(&self, off: bool) {
        let view = self.source_view();
        let state = &self.imp_detached_drop_targets().primary_removed;
        if !self.view_shows_document() {
            debug_assert!(
                !state.get(),
                "a detached view's document keeps no protected state"
            );
            if off {
                self.release_primary_hold();
            }
            return;
        }
        if off == state.get() || (off && !view.is_realized()) {
            return;
        }
        let primary = view.primary_clipboard();
        if off {
            self.buffer().remove_selection_clipboard(&primary);
        } else {
            self.buffer().add_selection_clipboard(&primary);
        }
        state.set(off);
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
                if !page.undo_protected_entry() {
                    page.notify_protected_preemption(ProtectedAction::TextUndo);
                }
                return glib::Propagation::Stop;
            }
            let redo =
                control && (lower == gtk4::gdk::Key::y || (shift && lower == gtk4::gdk::Key::z));
            if redo {
                page.notify_protected_preemption(ProtectedAction::TextRedo);
                return glib::Propagation::Stop;
            }
            if key == gtk4::gdk::Key::Clear {
                page.notify_protected_preemption(ProtectedAction::ClearText);
                return glib::Propagation::Stop;
            }
            let menu = (shift && key == gtk4::gdk::Key::F10) || key == gtk4::gdk::Key::Menu;
            if menu {
                page.open_protected_menu_from_keyboard();
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
        click.connect_pressed(move |gesture, n_press, x, y| {
            let Some(page) = page.upgrade().filter(LushtextEditorPage::is_protected_view) else {
                return;
            };
            let button = press_button(gesture.current_button());
            let input = PressInput {
                gesture: CaptureGesture::Click,
                button,
                n_press,
                hit: page.press_hit(x, y),
                device: pointer_device(gesture.upcast_ref()),
            };
            let disposition = press_disposition(input);
            if disposition != PressDisposition::Unclaimed {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                // A claimed press never reaches GtkTextView's own gesture,
                // which is what focuses the view: focus it here, so the
                // keyboard (Page Down, Shift+F10, Ctrl+C) acts on the document
                // the user clicked.
                if button != PressButton::Middle
                    && let Some(view) = gesture.widget()
                {
                    view.grab_focus();
                }
            }
            match disposition {
                PressDisposition::Unclaimed => {}
                PressDisposition::Claim => {
                    if button == PressButton::Middle && n_press == 1 {
                        page.notify_protected_preemption(ProtectedAction::MiddleClickPaste);
                    }
                }
                PressDisposition::OpenMenu => page.open_protected_menu_at(x, y),
                PressDisposition::OpenInspector { paragraph } => {
                    page.open_line_inspector(paragraph, 0);
                }
            }
        });
        view.add_controller(click);

        let drag = gtk4::GestureDrag::new();
        drag.set_button(0);
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let page = self.downgrade();
        drag.connect_drag_begin(move |gesture, _, _| {
            let Some(page) = page.upgrade().filter(LushtextEditorPage::is_protected_view) else {
                return;
            };
            let hit = gesture
                .start_point()
                .map_or(PressHit::Text, |(x, y)| page.press_hit(x, y));
            let input = PressInput {
                gesture: CaptureGesture::Drag,
                button: press_button(gesture.current_button()),
                n_press: 1,
                hit,
                device: pointer_device(gesture.upcast_ref()),
            };
            if press_disposition(input) != PressDisposition::Unclaimed {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
            }
        });
        view.add_controller(drag);
    }

    /// Copy the current selection of a protected document from its text
    /// source (design D3): the selection's character offsets are the source's,
    /// a worker cuts that range into `glib::Bytes`, and GTK installs them as
    /// the clipboard content without copying them. With no selection, or
    /// before an edit-growth entry has its source, nothing is copied.
    fn copy_protected_selection(&self) {
        let buffer = self.buffer();
        let Some((start, end)) = buffer.selection_bounds() else {
            return;
        };
        let Some(source) = self.protected_source() else {
            return;
        };
        let (Ok(start), Ok(end)) = (
            usize::try_from(start.offset()),
            usize::try_from(end.offset()),
        ) else {
            return;
        };
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let text = source.text();
                let byte_at = |chars: usize| {
                    text.char_indices()
                        .nth(chars)
                        .map_or(text.len(), |(index, _)| index)
                };
                let from = byte_at(start);
                let to = from
                    + text[from..]
                        .char_indices()
                        .nth(end - start)
                        .map_or(text.len() - from, |(index, _)| index);
                glib::Bytes::from(&text.as_bytes()[from..to])
            },
            |page, bytes| {
                let providers = [
                    gtk4::gdk::ContentProvider::for_bytes("text/plain;charset=utf-8", &bytes),
                    gtk4::gdk::ContentProvider::for_bytes("text/plain", &bytes),
                ];
                page.source_view()
                    .clipboard()
                    .set_content(Some(&gtk4::gdk::ContentProvider::new_union(&providers)))
                    .ok();
            },
        );
    }

    /// What a press at `(x, y)` in the view's coordinates landed on: a fold
    /// marker when it is over the left gutter beside a folded paragraph.
    fn press_hit(&self, x: f64, y: f64) -> PressHit {
        if !self.x_is_over_left_gutter(x) {
            return PressHit::Text;
        }
        let view = self.source_view();
        match self.paragraph_at_widget_y(view.upcast_ref(), y) {
            Some(paragraph)
                if self
                    .protected_fold_plan()
                    .is_some_and(|plan| plan.range_of(paragraph).is_some()) =>
            {
                PressHit::FoldMarker { paragraph }
            }
            _ => PressHit::Gutter,
        }
    }

    /// Take the view's drop targets off while protected (ledger A63).
    pub(in crate::ui::editor_page) fn detach_protected_drop_targets(&self) {
        self.hold_drop_targets(DropTargetHold::ProtectedView);
    }

    /// Put the drop targets back when the session leaves Protected View
    /// (still off while the edit lock holds them).
    pub(in crate::ui::editor_page) fn restore_protected_drop_targets(&self) {
        self.release_drop_targets(DropTargetHold::ProtectedView);
    }

    /// Whether the view's drop targets are off now.
    #[must_use]
    pub fn drop_targets_detached(&self) -> bool {
        self.imp_detached_drop_targets().holds.get() != 0
    }

    /// Take the drop targets off for `hold`; the first holder detaches them.
    pub(in crate::ui::editor_page) fn hold_drop_targets(&self, hold: DropTargetHold) {
        let state = self.imp_detached_drop_targets();
        let holds = state.holds.get();
        state.holds.set(holds | hold as u8);
        if holds != 0 {
            return;
        }
        let view = self.source_view();
        let detached = &state.controllers;
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

    /// Release `hold`; the last holder puts the drop targets back.
    pub(in crate::ui::editor_page) fn release_drop_targets(&self, hold: DropTargetHold) {
        let state = self.imp_detached_drop_targets();
        let holds = state.holds.get() & !(hold as u8);
        state.holds.set(holds);
        if holds != 0 {
            return;
        }
        let view = self.source_view();
        for controller in state.controllers.take() {
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
