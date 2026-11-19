// SPDX-License-Identifier: GPL-3.0-or-later

//! The keys held while a collapse runs (design D22, task 10.14), part of the
//! collapse's `execution` role.
//!
//! While a sliced collapse is pending, the collapsing subtree's rows are
//! still in the list, hidden. A list-navigation key would move the focus
//! onto one of them: Down from the directory row, Up from the row after the
//! subtree, a page key from anywhere. So the section's key route holds every
//! list-navigation key pressed on a row of its list while a collapse of the
//! section is pending, and every key whose effect depends on the focused row
//! (activation, peek, the context menu, an expander's disclosure bindings)
//! pressed behind a held key, so it acts on the row the keys before it
//! reach. Every other key (Escape, Tab, accelerators) and every key typed in
//! an editable act at once. The decisions are the pure
//! `policy::classify_held_key` and `policy::held_key_action`.
//!
//! When the section's last collapse has ended, by any end, and the next
//! frame has laid out the final tree, the held events are **put back on
//! GDK's queue** in the order pressed (`gdk_display_put_event`, ledger
//! A126), and GTK routes each as it routes any press: the list's own binding
//! moves the focus from the directory row, Page Up and Page Down by the
//! list's page on the final tree, and a row key takes its own route on the
//! row reached. That is how `main` behaves: its frozen collapse leaves the
//! keys in GDK's queue, and GTK delivers them afterwards.
//!
//! # Resumption points
//!
//! - **The replay wait.** A one-shot `after-paint` handler on the list's frame
//!   clock, connected when the last collapse ends with keys held and
//!   disconnected when it runs (or on dispose); a `COLLAPSE_FRAME_STARVATION_MS`
//!   timeout replays without it when no frame comes, and an unmapped list
//!   replays from an idle.
//! - **The delivery check.** A `G_PRIORITY_LOW` idle added with the put runs
//!   after GDK has delivered every event put back (A126). It forgets those that
//!   did not come back through the route (the focus left the list, or a
//!   window-level controller took them) and arms the wait again if a key
//!   was held again because a replayed key started another collapse.
//!
//! A put-back event that comes back through the route is checked again: it is
//! held again, at its own place in the order, only while another collapse is
//! pending or a key pressed before it is still held. If the focus has left
//! the list when the keys would be put back, they are dropped, never sent to
//! the widget that now has the focus.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use super::super::LushtextWorkspaceSection;
use crate::ui::sidebar::policy::{
    COLLAPSE_FRAME_STARVATION_MS, HeldKeyAction, HeldKeyState, SidebarKey, SidebarKeyModifiers,
    classify_held_key, held_key_action,
};

/// The held keys and their replay, part of `CollapseRuntimeState`.
#[derive(Default)]
pub(in crate::ui::sidebar::workspace_section) struct HeldKeys {
    /// Keys held, with their order number, in that order.
    held: std::cell::RefCell<Vec<(u64, gdk::Event)>>,
    /// Keys put back and not yet seen again by the route.
    replaying: std::cell::RefCell<Vec<(u64, gdk::Event)>>,
    /// The next order number.
    next_order: std::cell::Cell<u64>,
    /// The replay wait's `after-paint` handler, while armed.
    wait_paint: std::cell::RefCell<Option<(glib::WeakRef<gdk::FrameClock>, glib::SignalHandlerId)>>,
    /// The replay wait's starvation timeout or unmapped idle, while armed.
    wait_source: std::cell::RefCell<Option<glib::SourceId>>,
    /// The delivery check, while armed.
    delivery_check: std::cell::RefCell<Option<glib::SourceId>>,
    /// Keys held, ever (a key held again counts once).
    held_total: std::cell::Cell<u64>,
    /// Keys put back, ever.
    replayed_total: std::cell::Cell<u64>,
    /// Keys dropped because the focus had left the list, ever.
    dropped_total: std::cell::Cell<u64>,
}

/// The held keys as the evidence reports them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct HeldKeyTotals {
    /// Keys waiting now: held, or put back and not yet delivered.
    pub(super) waiting: u32,
    /// Keys held, ever.
    pub(super) held_total: u64,
    /// Keys put back, ever.
    pub(super) replayed_total: u64,
    /// Keys dropped because the focus had left the list, ever.
    pub(super) dropped_total: u64,
}

impl HeldKeys {
    pub(super) fn totals(&self) -> HeldKeyTotals {
        let waiting = self.held.borrow().len() + self.replaying.borrow().len();
        HeldKeyTotals {
            waiting: u32::try_from(waiting).unwrap_or(u32::MAX),
            held_total: self.held_total.get(),
            replayed_total: self.replayed_total.get(),
            dropped_total: self.dropped_total.get(),
        }
    }

    pub(super) fn any_waiting(&self) -> bool {
        !self.held.borrow().is_empty() || !self.replaying.borrow().is_empty()
    }
}

/// The key the route tells apart, from a key event's keyval.
fn sidebar_key(keyval: gdk::Key) -> Option<SidebarKey> {
    use gdk::Key;
    Some(match keyval {
        Key::Up => SidebarKey::Up,
        Key::Down => SidebarKey::Down,
        Key::Page_Up => SidebarKey::PageUp,
        Key::Page_Down => SidebarKey::PageDown,
        Key::Home => SidebarKey::Home,
        Key::End => SidebarKey::End,
        Key::KP_Up
        | Key::KP_Down
        | Key::KP_Page_Up
        | Key::KP_Page_Down
        | Key::KP_Home
        | Key::KP_End => SidebarKey::KeypadNavigation,
        Key::Return | Key::ISO_Enter | Key::KP_Enter => SidebarKey::Enter,
        Key::space | Key::KP_Space => SidebarKey::Space,
        Key::Menu => SidebarKey::Menu,
        Key::F10 => SidebarKey::F10,
        _ => return None,
    })
}

fn sidebar_modifiers(state: gdk::ModifierType) -> SidebarKeyModifiers {
    use gdk::ModifierType;
    SidebarKeyModifiers {
        shift: state.contains(ModifierType::SHIFT_MASK),
        control: state.contains(ModifierType::CONTROL_MASK),
        other: state.intersects(
            ModifierType::ALT_MASK
                | ModifierType::SUPER_MASK
                | ModifierType::HYPER_MASK
                | ModifierType::META_MASK,
        ),
    }
}

impl LushtextWorkspaceSection {
    /// The key route's first question (design D22): hold `event`, and return
    /// true, when the pure policy says so. `disclosure` says that the key
    /// matches one of the focused expander's class bindings.
    pub(super) fn hold_key(
        &self,
        event: &gdk::Event,
        key: &gdk::KeyEvent,
        disclosure: bool,
    ) -> bool {
        let runtime = &self.imp().collapse_runtime;
        let keys = &runtime.held_keys;
        let returning = {
            let mut replaying = keys.replaying.borrow_mut();
            replaying
                .iter()
                .position(|(_, put)| put.as_ptr() == event.as_ptr())
                .map(|index| replaying.remove(index).0)
        };
        let class = classify_held_key(
            sidebar_key(key.keyval()),
            sidebar_modifiers(key.modifier_state()),
            disclosure,
        );
        let earlier_keys_waiting = match returning {
            Some(order) => keys.held.borrow().iter().any(|(held, _)| *held < order),
            None => keys.any_waiting(),
        };
        let state = HeldKeyState {
            collapse_pending: !runtime.queue.borrow().is_empty(),
            earlier_keys_waiting,
            focus_on_row: self.focus_on_list_row(),
        };
        if held_key_action(class, state) == HeldKeyAction::Propagate {
            return false;
        }
        let order = returning.unwrap_or_else(|| {
            let order = keys.next_order.get();
            keys.next_order.set(order.saturating_add(1));
            keys.held_total.set(keys.held_total.get().saturating_add(1));
            order
        });
        let mut held = keys.held.borrow_mut();
        let at = held.partition_point(|(other, _)| *other < order);
        held.insert(at, (order, event.clone()));
        drop(held);
        if !state.collapse_pending {
            // Held behind a key that is itself held again: it goes back
            // with it once the delivery check has run.
            self.arm_held_key_replay();
        }
        true
    }

    /// Whether the window's focus is on a row of this section's list: the
    /// row widget or a widget inside it, never an editable (the inline
    /// rename entry keeps its keys, design R6.1).
    fn focus_on_list_row(&self) -> bool {
        let Some(window) = self.root().and_downcast::<gtk4::Window>() else {
            return false;
        };
        let Some(focus) = gtk4::prelude::GtkWindowExt::focus(&window) else {
            return false;
        };
        let list = self.imp().file_tree_view.upcast_ref::<gtk4::Widget>();
        &focus != list && focus.is_ancestor(list) && !focus.is::<gtk4::Editable>()
    }

    /// Arm the replay wait if keys are held and no collapse of the section
    /// is pending; called by every end of a collapse.
    pub(super) fn arm_held_key_replay(&self) {
        let runtime = &self.imp().collapse_runtime;
        let keys = &runtime.held_keys;
        if keys.held.borrow().is_empty()
            || !runtime.queue.borrow().is_empty()
            || keys.wait_paint.borrow().is_some()
            || keys.wait_source.borrow().is_some()
            || keys.delivery_check.borrow().is_some()
        {
            return;
        }
        let list = &*self.imp().file_tree_view;
        let section = self.downgrade();
        let Some(clock) = list.is_mapped().then(|| list.frame_clock()).flatten() else {
            let id = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
                if let Some(section) = section.upgrade() {
                    section.imp().collapse_runtime.held_keys.wait_source.take();
                    section.replay_held_keys();
                }
                glib::ControlFlow::Break
            });
            keys.wait_source.replace(Some(id));
            return;
        };
        let handler = {
            let section = section.clone();
            clock.connect_after_paint(move |_| {
                if let Some(section) = section.upgrade() {
                    section.replay_held_keys();
                }
            })
        };
        keys.wait_paint.replace(Some((clock.downgrade(), handler)));
        clock.request_phase(gdk::FrameClockPhase::AFTER_PAINT);
        let id = glib::timeout_add_local_full(
            std::time::Duration::from_millis(COLLAPSE_FRAME_STARVATION_MS),
            glib::Priority::DEFAULT_IDLE,
            move || {
                if let Some(section) = section.upgrade() {
                    section.imp().collapse_runtime.held_keys.wait_source.take();
                    section.replay_held_keys();
                }
                glib::ControlFlow::Break
            },
        );
        keys.wait_source.replace(Some(id));
    }

    /// Stop the replay wait, if armed.
    fn disarm_held_key_wait(&self) {
        let keys = &self.imp().collapse_runtime.held_keys;
        if let Some((clock, handler)) = keys.wait_paint.borrow_mut().take()
            && let Some(clock) = clock.upgrade()
        {
            clock.disconnect(handler);
        }
        if let Some(source) = keys.wait_source.borrow_mut().take() {
            source.remove();
        }
    }

    /// Put every held key back on GDK's queue, in order, if the focus is
    /// still on a row of the list; else drop them.
    fn replay_held_keys(&self) {
        self.disarm_held_key_wait();
        let runtime = &self.imp().collapse_runtime;
        if !runtime.queue.borrow().is_empty() {
            // Another collapse started: its end arms the wait again.
            return;
        }
        let keys = &runtime.held_keys;
        let held: Vec<(u64, gdk::Event)> = keys.held.borrow_mut().drain(..).collect();
        if held.is_empty() {
            return;
        }
        let count = u64::try_from(held.len()).unwrap_or(u64::MAX);
        if !self.focus_on_list_row() {
            keys.dropped_total
                .set(keys.dropped_total.get().saturating_add(count));
            return;
        }
        let display = self.imp().file_tree_view.display();
        for (_, event) in &held {
            #[expect(
                deprecated,
                reason = "ledger A126: gdk_display_put_event, deprecated since GTK 4.10 with no \
                          replacement, re-delivers a held press through GTK's own route"
            )]
            display.put_event(event);
        }
        keys.replayed_total
            .set(keys.replayed_total.get().saturating_add(count));
        keys.replaying.borrow_mut().extend(held);
        let section = self.downgrade();
        let id = glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(section) = section.upgrade() {
                let keys = &section.imp().collapse_runtime.held_keys;
                keys.delivery_check.take();
                keys.replaying.borrow_mut().clear();
                section.arm_held_key_replay();
            }
            glib::ControlFlow::Break
        });
        keys.delivery_check.replace(Some(id));
    }

    /// Forget every held key and stop their sources: the section is being
    /// disposed.
    pub(in crate::ui::sidebar::workspace_section) fn drop_held_keys(&self) {
        self.disarm_held_key_wait();
        let keys = &self.imp().collapse_runtime.held_keys;
        if let Some(source) = keys.delivery_check.borrow_mut().take() {
            source.remove();
        }
        keys.held.borrow_mut().clear();
        keys.replaying.borrow_mut().clear();
    }
}
