// SPDX-License-Identifier: GPL-3.0-or-later

//! Whether an assistive technology listens to this application's text
//! events, as GTK's AT-SPI backend decides it (`defer-background-tab-layout`,
//! review M2).
//!
//! **Cross-cutting, no role.** Its one consumer is
//! `ui/editor_page/view_attachment.rs`: `gtk_text_view_set_buffer` reports the
//! whole old buffer removed and the whole new one inserted to the accessible
//! (`gtk_accessible_text_update_contents`), and GTK's AT-SPI context copies
//! that text into a D-Bus signal whenever an event listener is registered
//! (`gtk_at_spi_root_has_event_listeners`). Swapping an 8 MiB document then
//! takes 9–19 ms in one dispatch, and the cost grows with the document; with
//! no listener the same call returns before reading any text.
//!
//! GTK keeps that table private, so this module mirrors it on its own
//! connection to the accessibility bus, the way `gtkatspiroot.c` builds it:
//! the bus address from `AT_SPI_BUS_ADDRESS`, else `org.a11y.Bus.GetAddress`
//! on the session bus; `org.a11y.atspi.Registry`'s `EventListenerRegistered`
//! and `EventListenerDeregistered` signals, counted per sender; and
//! `GetRegisteredEvents` for the listeners that were there first. A backend
//! other than AT-SPI (`GTK_A11Y` set to anything but `atspi`) or an
//! unreachable accessibility bus emits nothing, so it reads as inactive.
//!
//! Everything is asynchronous and starts at application startup; until the
//! first answer arrives the state is inactive, and the subscribers hear the
//! change when it comes. One known divergence: GTK inside a Flatpak sandbox
//! whose portal is older than version 7 assumes listeners are present
//! (`can_use_event_listeners`), and this mirror does not check the portal
//! version; every supported runtime has a newer portal.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk4::{gio, glib};

const REGISTRY_NAME: &str = "org.a11y.atspi.Registry";
const REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
const REGISTRY_INTERFACE: &str = "org.a11y.atspi.Registry";

type Subscriber = Box<dyn Fn(bool) -> bool>;

#[derive(Default)]
struct State {
    started: bool,
    /// Registered listeners per sender, as GTK counts them.
    listeners: HashMap<String, u32>,
    /// Callbacks told when the state changes; one returning false is dropped
    /// (its owner is gone).
    subscribers: Vec<Subscriber>,
    /// The accessibility-bus connection and its signal subscriptions, kept for
    /// the life of the process.
    connection: Option<gio::DBusConnection>,
    subscriptions: Vec<gio::SignalSubscription>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Whether GTK would send text-change payloads to an assistive technology now.
#[must_use]
pub fn listeners_active() -> bool {
    STATE.with_borrow(|state| !state.listeners.is_empty())
}

/// Be told, on the GTK thread, whenever [`listeners_active`] changes; the
/// callback returns false once it no longer wants to hear (its owner is gone).
pub fn subscribe(callback: impl Fn(bool) -> bool + 'static) {
    STATE.with_borrow_mut(|state| state.subscribers.push(Box::new(callback)));
}

/// Start mirroring GTK's listener table. Idempotent; on the GTK thread.
pub fn start() {
    let first = STATE.with_borrow_mut(|state| !std::mem::replace(&mut state.started, true));
    if !first || !gtk_uses_atspi() {
        return;
    }
    glib::MainContext::default().spawn_local(async {
        let Some(address) = bus_address().await else {
            return;
        };
        let flags = gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION;
        let Ok(connection) = gio::DBusConnection::for_address_future(&address, flags, None).await
        else {
            return;
        };
        let subscriptions = ["EventListenerRegistered", "EventListenerDeregistered"]
            .into_iter()
            .map(|member| {
                connection.subscribe_to_signal(
                    Some(REGISTRY_NAME),
                    Some(REGISTRY_INTERFACE),
                    Some(member),
                    Some(REGISTRY_PATH),
                    None,
                    gio::DBusSignalFlags::NONE,
                    move |signal| {
                        if let Some((sender, _, _)) =
                            signal.parameters.get::<(String, String, Vec<String>)>()
                        {
                            update(&sender, member == "EventListenerRegistered");
                        }
                    },
                )
            })
            .collect();
        STATE.with_borrow_mut(|state| {
            state.connection = Some(connection.clone());
            state.subscriptions = subscriptions;
        });
        let reply = connection
            .call_future(
                Some(REGISTRY_NAME),
                REGISTRY_PATH,
                REGISTRY_INTERFACE,
                "GetRegisteredEvents",
                None,
                // The reply is read with a typed `get`, which rejects any other shape.
                None,
                gio::DBusCallFlags::NONE,
                -1,
            )
            .await;
        if let Ok(reply) = reply
            && let Some((listeners,)) = reply.get::<(Vec<(String, String)>,)>()
        {
            for (sender, _) in listeners {
                update(&sender, true);
            }
        }
    });
}

/// GTK uses its AT-SPI backend unless `GTK_A11Y` names another one.
fn gtk_uses_atspi() -> bool {
    std::env::var("GTK_A11Y").map_or(true, |value| value == "atspi")
}

async fn bus_address() -> Option<String> {
    if let Ok(address) = std::env::var("AT_SPI_BUS_ADDRESS")
        && !address.is_empty()
    {
        return Some(address);
    }
    let session = gio::bus_get_future(gio::BusType::Session).await.ok()?;
    let reply = session
        .call_future(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
            None,
            None,
            gio::DBusCallFlags::NONE,
            -1,
        )
        .await
        .ok()?;
    reply.get::<(String,)>().map(|(address,)| address)
}

/// Count one registration or deregistration and tell the subscribers when the
/// state changed.
fn update(sender: &str, registered: bool) {
    let changed = STATE.with_borrow_mut(|state| {
        let before = !state.listeners.is_empty();
        if registered {
            let count = state.listeners.entry(sender.to_owned()).or_insert(0);
            *count = count.saturating_add(1);
        } else if let Some(count) = state.listeners.get_mut(sender) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.listeners.remove(sender);
            }
        }
        let after = !state.listeners.is_empty();
        (before != after).then_some(after)
    });
    if let Some(active) = changed {
        notify(active);
    }
}

fn notify(active: bool) {
    // Taken out for the calls, so a subscriber may subscribe or read the state.
    let subscribers = STATE.with_borrow_mut(|state| std::mem::take(&mut state.subscribers));
    let kept: Vec<Subscriber> = subscribers
        .into_iter()
        .filter(|subscriber| subscriber(active))
        .collect();
    STATE.with_borrow_mut(|state| {
        let added = std::mem::take(&mut state.subscribers);
        state.subscribers = kept;
        state.subscribers.extend(added);
    });
}

/// Pretend a listener registered or left, as the registry's signals would
/// (widget tests drive the attachment's AT rule with it).
#[cfg(feature = "test-utils")]
pub fn simulate_listener(registered: bool) {
    update(":test.listener", registered);
}
