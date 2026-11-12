// SPDX-License-Identifier: MIT OR Apache-2.0

//! Real pointer and keyboard input for the probes that need GTK's own event
//! path (A59–A61, A64), through the private headless compositor's
//! `org.gnome.Mutter.RemoteDesktop` interface.
//!
//! GTK 4 has no public way to synthesize a `GdkEvent`, and a gesture's
//! handlers read the event that started it, so emitting a gesture signal by
//! hand is not the toolkit's path. The responsiveness lane drives the
//! compositor the same way. Every probe that uses this runs only under the
//! private `mutter --headless` session the runner and `--check` start, and
//! the samples never inject input in their interactive mode: a person on a
//! live desktop clicks and types themselves.
//!
//! Pointer positions are screen coordinates. The probes present their fixture
//! in a **fullscreen** window, so a widget's position in its window is its
//! position on the virtual monitor; each probe also checks where a press
//! actually landed before it trusts it.

use std::time::Duration;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::session::settle;

/// The variable an SDK sandbox run sets (`scripts/run-gtk-axioms.sh
/// runtimes`): the sandbox has no session bus, so no compositor
/// RemoteDesktop interface either. With it set to `1`, a sample whose probe
/// needs this module prints [`remote_desktop_skip_line`] and exits `0`
/// before connecting. The host lane and CI never set it, so a host
/// compositor without RemoteDesktop still fails those samples as fixture
/// invalid.
pub const SANDBOX_ENV: &str = "GTK_LUSH_AXIOMS_SANDBOX";

/// The axioms whose probe drives real input through [`RemoteInput`].
pub const REMOTE_INPUT_AXIOMS: &[crate::AxiomId] = &[
    crate::AxiomId::new(59),
    crate::AxiomId::new(60),
    crate::AxiomId::new(61),
    crate::AxiomId::new(64),
    crate::AxiomId::new(97),
];

/// Whether [`SANDBOX_ENV`] says this run is inside an SDK sandbox.
#[must_use]
pub fn in_sdk_sandbox() -> bool {
    std::env::var(SANDBOX_ENV).as_deref() == Ok("1")
}

/// The line a sample prints instead of its observation when its probe needs
/// RemoteDesktop and [`in_sdk_sandbox`] holds:
/// `skipped (needs RemoteDesktop): <id>`; `None` for every other axiom.
#[must_use]
pub fn remote_desktop_skip_line(axiom: crate::AxiomId, sandbox: bool) -> Option<String> {
    (sandbox && REMOTE_INPUT_AXIOMS.contains(&axiom))
        .then(|| format!("skipped (needs RemoteDesktop): {axiom}"))
}

const BUS: &str = "org.gnome.Mutter.RemoteDesktop";
const PATH: &str = "/org/gnome/Mutter/RemoteDesktop";
const SESSION_INTERFACE: &str = "org.gnome.Mutter.RemoteDesktop.Session";
const CALL_TIMEOUT_MS: i32 = 5_000;
/// Settle after each injected event, so GTK has dispatched it before the
/// next one: press and release in one main-context turn would merge.
const EVENT_SETTLE: Duration = Duration::from_millis(60);

/// `linux/input-event-codes.h` `BTN_LEFT`.
pub const BUTTON_PRIMARY: i32 = 0x110;
/// `BTN_RIGHT`.
pub const BUTTON_SECONDARY: i32 = 0x111;
/// `BTN_MIDDLE`.
pub const BUTTON_MIDDLE: i32 = 0x112;

/// X keysyms the probes press.
pub mod keysym {
    /// `Control_L`.
    pub const CONTROL_L: u32 = 0xffe3;
    /// `Shift_L`.
    pub const SHIFT_L: u32 = 0xffe1;
    /// `F7`.
    pub const F7: u32 = 0xffc4;
    /// `F10`.
    pub const F10: u32 = 0xffc7;
    /// `Page_Down`.
    pub const PAGE_DOWN: u32 = 0xff56;
    /// `Right`.
    pub const RIGHT: u32 = 0xff53;
    /// `a`.
    pub const A: u32 = 0x61;
    /// `X` (what Shift+x produces; the compositor picks the shifted level).
    pub const UPPER_X: u32 = 0x58;
}

/// One RemoteDesktop session, stopped when dropped.
pub struct RemoteInput {
    connection: gio::DBusConnection,
    session: String,
}

impl std::fmt::Debug for RemoteInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteInput")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl RemoteInput {
    /// Create and start a session, or say why none is available.
    ///
    /// # Errors
    ///
    /// The D-Bus error text when the session bus or the compositor's
    /// RemoteDesktop interface is unavailable.
    pub fn start() -> Result<Self, String> {
        let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>)
            .map_err(|error| error.to_string())?;
        let reply = connection
            .call_sync(
                Some(BUS),
                PATH,
                BUS,
                "CreateSession",
                None,
                Some(glib::VariantTy::new("(o)").map_err(|error| error.to_string())?),
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .map_err(|error| error.to_string())?;
        let session = reply
            .child_value(0)
            .str()
            .ok_or_else(|| "CreateSession returned no object path".to_owned())?
            .to_owned();
        let input = Self {
            connection,
            session,
        };
        input.call("Start", None)?;
        // The compositor drops the first notification of a new session's
        // devices; spend it on a harmless modifier and a one-pixel motion.
        input.call(
            "NotifyKeyboardKeysym",
            Some(&(keysym::SHIFT_L, true).to_variant()),
        )?;
        input.call(
            "NotifyKeyboardKeysym",
            Some(&(keysym::SHIFT_L, false).to_variant()),
        )?;
        input.call(
            "NotifyPointerMotionRelative",
            Some(&(1.0f64, 1.0f64).to_variant()),
        )?;
        settle(Duration::from_millis(300));
        Ok(input)
    }

    fn call(&self, method: &str, parameters: Option<&glib::Variant>) -> Result<(), String> {
        self.connection
            .call_sync(
                Some(BUS),
                &self.session,
                SESSION_INTERFACE,
                method,
                parameters,
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                None::<&gio::Cancellable>,
            )
            .map(|_| ())
            .map_err(|error| format!("{method}: {error}"))
    }

    /// Move the pointer to screen position (`x`, `y`): to the top-left
    /// corner first (the motion is relative and clamps at the edge), then by
    /// the position.
    pub fn move_to(&self, x: f64, y: f64) -> Result<(), String> {
        self.call(
            "NotifyPointerMotionRelative",
            Some(&(-10_000.0f64, -10_000.0f64).to_variant()),
        )?;
        self.call("NotifyPointerMotionRelative", Some(&(x, y).to_variant()))?;
        settle(EVENT_SETTLE);
        Ok(())
    }

    /// Move the pointer by (`dx`, `dy`).
    pub fn move_by(&self, dx: f64, dy: f64) -> Result<(), String> {
        self.call("NotifyPointerMotionRelative", Some(&(dx, dy).to_variant()))?;
        settle(EVENT_SETTLE);
        Ok(())
    }

    /// Press or release `button` (an evdev code such as [`BUTTON_PRIMARY`]).
    pub fn button(&self, button: i32, pressed: bool) -> Result<(), String> {
        self.call("NotifyPointerButton", Some(&(button, pressed).to_variant()))?;
        settle(EVENT_SETTLE);
        Ok(())
    }

    /// Press and release `button` where the pointer is.
    pub fn click(&self, button: i32) -> Result<(), String> {
        self.button(button, true)?;
        self.button(button, false)
    }

    /// Press or release the key carrying `keysym`.
    pub fn key(&self, keysym: u32, pressed: bool) -> Result<(), String> {
        self.call(
            "NotifyKeyboardKeysym",
            Some(&(keysym, pressed).to_variant()),
        )?;
        settle(EVENT_SETTLE);
        Ok(())
    }

    /// Press `keysym` with `modifiers` held (pressed in order, released in
    /// reverse).
    pub fn chord(&self, modifiers: &[u32], keysym: u32) -> Result<(), String> {
        for modifier in modifiers {
            self.key(*modifier, true)?;
        }
        self.key(keysym, true)?;
        self.key(keysym, false)?;
        for modifier in modifiers.iter().rev() {
            self.key(*modifier, false)?;
        }
        Ok(())
    }
}

impl Drop for RemoteInput {
    fn drop(&mut self) {
        let _ = self.call("Stop", None);
    }
}

/// The centre of `widget` in its root's coordinates, which are screen
/// coordinates for a fullscreen window.
#[must_use]
pub fn centre_on_screen(widget: &impl IsA<gtk4::Widget>) -> Option<(f64, f64)> {
    point_on_screen(
        widget,
        f64::from(widget.width()) / 2.0,
        f64::from(widget.height()) / 2.0,
    )
}

/// The point (`x`, `y`) of `widget` in its root's coordinates.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "graphene points are f32; widget coordinates on the virtual monitor fit it exactly"
)]
pub fn point_on_screen(widget: &impl IsA<gtk4::Widget>, x: f64, y: f64) -> Option<(f64, f64)> {
    let root = widget.root()?;
    let point = widget.compute_point(&root, &gtk4::graphene::Point::new(x as f32, y as f32))?;
    Some((f64::from(point.x()), f64::from(point.y())))
}

/// A window that fills the virtual monitor, so its coordinates are screen
/// coordinates, holding `content`.
#[must_use]
pub fn fullscreen_window(content: &impl IsA<gtk4::Widget>) -> libadwaita::Window {
    let window = libadwaita::Window::builder().content(content).build();
    window.fullscreen();
    window
}

#[cfg(test)]
mod tests {
    use super::{REMOTE_INPUT_AXIOMS, remote_desktop_skip_line};
    use crate::AxiomId;

    /// The list names exactly the probes that start a RemoteInput session, so
    /// a new probe driving real input cannot be left out of the sandbox skip.
    #[test]
    fn remote_input_axioms_are_exactly_the_probes_that_start_a_session() {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/src/probes");
        let mut users: Vec<AxiomId> = std::fs::read_dir(directory)
            .expect("the probes directory is readable")
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let stem = path.file_stem()?.to_str()?.strip_prefix('a')?.to_owned();
                let number: u16 = stem.parse().ok()?;
                let source = std::fs::read_to_string(&path).ok()?;
                source
                    .contains("RemoteInput::start(")
                    .then_some(AxiomId::new(number))
            })
            .collect();
        users.sort();
        assert_eq!(users, REMOTE_INPUT_AXIOMS);
    }

    #[test]
    fn only_a_remote_input_axiom_inside_the_sandbox_is_skipped() {
        assert_eq!(
            remote_desktop_skip_line(AxiomId::new(59), true).as_deref(),
            Some("skipped (needs RemoteDesktop): A59")
        );
        assert_eq!(remote_desktop_skip_line(AxiomId::new(59), false), None);
        assert_eq!(remote_desktop_skip_line(AxiomId::new(29), true), None);
    }
}
