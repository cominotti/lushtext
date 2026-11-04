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
//! (`(ssas)`) and `EventListenerDeregistered` (`(ss)`; review round 2: reading
//! both as `(ssas)` never counted a listener leaving) signals; and
//! `GetRegisteredEvents` for the listeners that were there first.
//!
//! **Fail-safe** (review round 2): the mirror reads as listening until
//! `GetRegisteredEvents` has answered, and for good when the bus address,
//! the connection or that call fails. Only a backend other than AT-SPI
//! (`GTK_A11Y` naming another one), which sends nothing, reads as not
//! listening without an answer. The answer *replaces* the table rather than
//! adding to it: the registry sends its signals and its reply in order, so a
//! signal that arrived first is already in the reply, and counting both left
//! a listener that registered in between counted twice. The table holds
//! (sender, event) pairs, as the registry does, so a repeated signal changes
//! nothing.
//!
//! One known divergence: GTK inside a Flatpak sandbox whose **host's**
//! Flatpak portal is older than version 7 (Flatpak before about 1.15) assumes
//! listeners are present (`can_use_event_listeners`), and that host's
//! accessibility proxy may not forward the registry's call and signals; the
//! call then fails, the mirror stays unknown, and nothing is detached (safe).
//! The connection closing also returns the mirror to unknown.

use std::cell::RefCell;
use std::collections::BTreeSet;

use gtk4::prelude::*;
use gtk4::{gio, glib};

const REGISTRY_NAME: &str = "org.a11y.atspi.Registry";
const REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
const REGISTRY_INTERFACE: &str = "org.a11y.atspi.Registry";
const REGISTERED: &str = "EventListenerRegistered";
const DEREGISTERED: &str = "EventListenerDeregistered";

/// What the mirror knows about GTK's listener table.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Knowledge {
    /// GTK uses another accessibility backend: nothing is ever sent.
    Disabled,
    /// No answer yet, or no way to get one: read as listening.
    Unknown,
    /// The registry answered: these (sender, event) pairs listen.
    Known(BTreeSet<(String, String)>),
}

impl Knowledge {
    fn listening(&self) -> bool {
        match self {
            Self::Disabled => false,
            Self::Unknown => true,
            Self::Known(pairs) => !pairs.is_empty(),
        }
    }
}

/// One registry signal: a listener for `event` registered or left.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListenerChange {
    sender: String,
    event: String,
    registered: bool,
}

/// Parse one registry signal by its member: `EventListenerRegistered` is
/// `(ssas)` and `EventListenerDeregistered` is `(ss)` (at-spi2-registryd's
/// introspection, and GTK's own parsing); both shapes are accepted for
/// either, since only the first two strings matter. Anything else is `None`.
fn listener_change(member: &str, parameters: &glib::Variant) -> Option<ListenerChange> {
    let registered = match member {
        REGISTERED => true,
        DEREGISTERED => false,
        _ => return None,
    };
    if !parameters.is_container() || parameters.n_children() < 2 {
        return None;
    }
    let sender = parameters.child_value(0).get::<String>()?;
    let event = parameters.child_value(1).get::<String>()?;
    Some(ListenerChange {
        sender,
        event,
        registered,
    })
}

/// Apply one signal; signals before the first answer change nothing (the
/// answer will hold them).
fn apply_change(knowledge: &mut Knowledge, change: ListenerChange) {
    if let Knowledge::Known(pairs) = knowledge {
        let pair = (change.sender, change.event);
        if change.registered {
            pairs.insert(pair);
        } else {
            pairs.remove(&pair);
        }
    }
}

/// Apply the registry's answer, which replaces the table.
fn apply_reply(knowledge: &mut Knowledge, pairs: Vec<(String, String)>) {
    if *knowledge != Knowledge::Disabled {
        *knowledge = Knowledge::Known(pairs.into_iter().collect());
    }
}

/// A subscriber's callback, given its owner and whether a listener listens.
type SubscriberCallback = Box<dyn Fn(&glib::Object, bool)>;

struct Subscriber {
    owner: glib::WeakRef<glib::Object>,
    callback: SubscriberCallback,
}

struct State {
    started: bool,
    knowledge: Knowledge,
    /// Set by the test-utils simulation: the registry's own reply and
    /// signals then change nothing, so a real answer arriving later cannot
    /// replace what a widget test simulated.
    simulated: bool,
    subscribers: Vec<Subscriber>,
    /// The accessibility-bus connection and its signal subscriptions, kept for
    /// the life of the process.
    connection: Option<gio::DBusConnection>,
    subscriptions: Vec<gio::SignalSubscription>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        started: false,
        knowledge: if gtk_uses_atspi() { Knowledge::Unknown } else { Knowledge::Disabled },
        simulated: false,
        subscribers: Vec::new(),
        connection: None,
        subscriptions: Vec::new(),
    });
}

/// Whether GTK may send text-change payloads to an assistive technology now:
/// true while a listener is registered, and while the registry has not
/// answered (module docs).
#[must_use]
pub fn listeners_active() -> bool {
    STATE.with_borrow(|state| state.knowledge.listening())
}

/// Whether the mirror has its answer (or needs none): `listeners_active` then
/// reads the registry rather than the fail-safe default.
#[must_use]
pub fn answered() -> bool {
    STATE.with_borrow(|state| state.knowledge != Knowledge::Unknown)
}

/// Call `callback` with `owner`, on the GTK thread, whenever
/// [`listeners_active`] changes, for as long as `owner` lives. Subscribers
/// whose owner is gone are dropped here and on every change.
pub fn subscribe<O: IsA<glib::Object>>(owner: &O, callback: impl Fn(&O, bool) + 'static) {
    let weak = glib::WeakRef::new();
    weak.set(Some(owner.upcast_ref::<glib::Object>()));
    STATE.with_borrow_mut(|state| {
        state.subscribers.retain(|s| s.owner.upgrade().is_some());
        state.subscribers.push(Subscriber {
            owner: weak,
            callback: Box::new(move |object, active| {
                if let Some(owner) = object.downcast_ref::<O>() {
                    callback(owner, active);
                }
            }),
        });
    });
}

/// Start mirroring GTK's listener table. Idempotent; on the GTK thread.
pub fn start() {
    let first = STATE.with_borrow_mut(|state| !std::mem::replace(&mut state.started, true));
    if !first || !gtk_uses_atspi() {
        return;
    }
    // Every early return below leaves the knowledge `Unknown`: listening.
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
        let subscriptions = [REGISTERED, DEREGISTERED]
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
                        if let Some(change) = listener_change(member, signal.parameters) {
                            update_from_registry(|knowledge| apply_change(knowledge, change));
                        }
                    },
                )
            })
            .collect();
        // A closed connection receives no more signals: unknown again.
        connection.connect_closed(|_, _, _| {
            update_from_registry(|knowledge| {
                if *knowledge != Knowledge::Disabled {
                    *knowledge = Knowledge::Unknown;
                }
            });
        });
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
            && let Some((pairs,)) = reply.get::<(Vec<(String, String)>,)>()
        {
            update_from_registry(|knowledge| apply_reply(knowledge, pairs));
        }
    });
}

/// [`update`] for what the registry itself sends; a no-op once a widget test
/// simulated the registry.
fn update_from_registry(change: impl FnOnce(&mut Knowledge)) {
    if !STATE.with_borrow(|state| state.simulated) {
        update(change);
    }
}

/// Whether GTK's accessibility backend may send to the AT-SPI registry
/// (`gtk_at_context_create`): unset or any value starting with `0` selects the
/// first available backend; `atspi` and `accesskit` (which reaches AT-SPI on
/// Linux) select one that does; `help` falls back to the default. Only `none`,
/// `test` and an unrecognized value, which GTK ignores with a warning and
/// sends nothing for, read as disabled.
fn gtk_uses_atspi() -> bool {
    std::env::var("GTK_A11Y").map_or(true, |value| {
        value.starts_with('0')
            || ["atspi", "accesskit", "help"]
                .iter()
                .any(|backend| value.eq_ignore_ascii_case(backend))
    })
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

/// Change the knowledge and tell the subscribers when `listeners_active`
/// changed.
fn update(change: impl FnOnce(&mut Knowledge)) {
    let changed = STATE.with_borrow_mut(|state| {
        let before = state.knowledge.listening();
        change(&mut state.knowledge);
        let after = state.knowledge.listening();
        (before != after).then_some(after)
    });
    if let Some(active) = changed {
        notify(active);
    }
}

fn notify(active: bool) {
    // Taken out for the calls, so a subscriber may subscribe or read the state.
    let mut subscribers = STATE.with_borrow_mut(|state| std::mem::take(&mut state.subscribers));
    subscribers.retain(|subscriber| {
        let Some(owner) = subscriber.owner.upgrade() else {
            return false;
        };
        (subscriber.callback)(&owner, active);
        true
    });
    STATE.with_borrow_mut(|state| {
        let added = std::mem::take(&mut state.subscribers);
        state.subscribers = subscribers;
        state.subscribers.extend(added);
    });
}

/// Pretend the registry answered `GetRegisteredEvents` with no listener, if
/// nothing was simulated yet, and ignore the real registry from now on
/// (widget tests: an unanswered mirror reads as listening, and a real answer,
/// which a host whose session can activate at-spi2-registryd gives, would
/// otherwise replace a simulated listener).
#[cfg(feature = "test-utils")]
pub fn simulate_registry_answer() {
    let first = STATE.with_borrow_mut(|state| !std::mem::replace(&mut state.simulated, true));
    if first {
        update(|knowledge| apply_reply(knowledge, Vec::new()));
    }
}

/// Pretend the registry sent `EventListenerRegistered` or
/// `EventListenerDeregistered`, through the same parsing (widget tests drive
/// the attachment's AT rule with it). Answers the mirror first if needed, as
/// an empty `GetRegisteredEvents` reply would.
#[cfg(feature = "test-utils")]
pub fn simulate_listener(registered: bool) {
    use gtk4::glib::prelude::ToVariant;
    simulate_registry_answer();
    let event = "Object:TextChanged:";
    let parameters = if registered {
        (":test.listener", event, Vec::<String>::new()).to_variant()
    } else {
        (":test.listener", event).to_variant()
    };
    let member = if registered { REGISTERED } else { DEREGISTERED };
    if let Some(change) = listener_change(member, &parameters) {
        update(|knowledge| apply_change(knowledge, change));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::glib::prelude::ToVariant;

    fn change(sender: &str, event: &str, registered: bool) -> ListenerChange {
        ListenerChange {
            sender: sender.to_owned(),
            event: event.to_owned(),
            registered,
        }
    }

    #[test]
    fn registry_signals_of_both_shapes_are_parsed() {
        let registered = (":1.5", "Object:TextChanged:", Vec::<String>::new()).to_variant();
        assert_eq!(
            listener_change(REGISTERED, &registered),
            Some(change(":1.5", "Object:TextChanged:", true))
        );
        // at-spi2-registryd and GTK: `EventListenerDeregistered` is `(ss)`.
        let deregistered = (":1.5", "Object:TextChanged:").to_variant();
        assert_eq!(
            listener_change(DEREGISTERED, &deregistered),
            Some(change(":1.5", "Object:TextChanged:", false))
        );
        assert_eq!(
            listener_change(DEREGISTERED, &registered),
            Some(change(":1.5", "Object:TextChanged:", false)),
            "either shape for either member"
        );
        assert_eq!(listener_change(DEREGISTERED, &7u32.to_variant()), None);
        assert_eq!(listener_change(REGISTERED, &(":1.5",).to_variant()), None);
        assert_eq!(
            listener_change(REGISTERED, &(1u32, 2u32).to_variant()),
            None
        );
        assert_eq!(listener_change("Other", &deregistered), None);
    }

    #[test]
    fn the_mirror_listens_until_answered_and_follows_the_answer() {
        let mut knowledge = Knowledge::Unknown;
        assert!(knowledge.listening(), "unknown reads as listening");
        apply_change(&mut knowledge, change(":1.5", "a", false));
        assert_eq!(
            knowledge,
            Knowledge::Unknown,
            "a signal before the answer changes nothing"
        );
        apply_reply(&mut knowledge, vec![(":1.5".into(), "a".into())]);
        assert!(knowledge.listening());
        // The listener that registered between subscription and reply is in
        // the reply; its signal again changes nothing (review N2).
        apply_change(&mut knowledge, change(":1.5", "a", true));
        apply_change(&mut knowledge, change(":1.5", "a", false));
        assert!(!knowledge.listening(), "one deregistration clears one pair");
        apply_change(&mut knowledge, change(":1.6", "a", true));
        apply_change(&mut knowledge, change(":1.6", "b", true));
        apply_change(&mut knowledge, change(":1.6", "a", false));
        assert!(knowledge.listening(), "one event of two left");
        apply_change(&mut knowledge, change(":1.6", "b", false));
        assert!(!knowledge.listening());

        let mut disabled = Knowledge::Disabled;
        apply_reply(&mut disabled, vec![(":1.5".into(), "a".into())]);
        assert!(!disabled.listening(), "another backend never listens");
    }
}
