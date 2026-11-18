// SPDX-License-Identifier: MIT OR Apache-2.0

//! Named session profiles: what a lane is, as one option bundle.

use std::fmt;

/// WirePlumber profile of every quiet media session.
///
/// `policy` inherits `base` plus the session-manager metadata and
/// `policy.standard`, which is what links mutter's ScreenCast stream to a
/// `pipewiresrc` consumer, and no `hardware.*` feature: no ALSA, bluez, v4l2
/// or libcamera monitor touches the host's devices. `main` (WirePlumber's
/// default) and `video-only` (keeps `hardware.video-capture`) were rejected.
/// `make check-session-hygiene` fails if this drifts.
pub const WIREPLUMBER_PROFILE: &str = "policy";

/// Whether the session's applications reach an AT-SPI accessibility bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum A11y {
    /// `GTK_A11Y=test` (or a caller's `none`) and `NO_AT_BRIDGE=1`, set before
    /// the session bus starts so activated services inherit them; no
    /// `org.a11y.Bus` may be acquired.
    Off,
    /// The AT-SPI bus is available, for accessibility lanes.
    Atspi,
}

/// Whether PipeWire and WirePlumber run in the session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Media {
    /// No media daemons.
    Off,
    /// PipeWire and WirePlumber (`policy` profile) with module-rt off and
    /// helper-private homes.
    Quiet,
}

/// Whether desktop portals may be activated on the session bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Portals {
    /// `ADW_DISABLE_PORTAL=1`, `GDK_DEBUG=no-portals`, `GTK_USE_PORTAL=0`; no
    /// `org.freedesktop.portal.Desktop` or `.Documents` may be acquired.
    Off,
    /// Portals may start (screenshot, accessibility and sandbox lanes).
    On,
}

/// Which system bus the session's processes see.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemBus {
    /// A private, monitored `dbus-daemon`, exported as
    /// `DBUS_SYSTEM_BUS_ADDRESS`: rtkit and bluetoothd cannot be reached, and
    /// every attempt is recorded for the runtime guard.
    Private,
    /// The host system bus, for a lane that provably needs it. Media are
    /// refused with it, and the reason is printed with the session's facts.
    Host {
        /// Why the lane cannot use the private bus.
        reason: &'static str,
    },
}

/// A named option bundle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Profile {
    /// Stable name used on the command line and in reports.
    pub name: &'static str,
    /// Accessibility bus.
    pub a11y: A11y,
    /// Media daemons.
    pub media: Media,
    /// Portals.
    pub portals: Portals,
    /// System bus.
    pub system_bus: SystemBus,
}

/// Widget and axiom tests: nothing but the bus and the compositor.
pub const WIDGET: Profile = Profile {
    name: "widget",
    a11y: A11y::Off,
    media: Media::Off,
    portals: Portals::Off,
    system_bus: SystemBus::Private,
};

/// An application lane without screenshots.
pub const PLAIN: Profile = Profile {
    name: "plain",
    a11y: A11y::Off,
    media: Media::Off,
    portals: Portals::On,
    system_bus: SystemBus::Private,
};

/// A screenshot lane: quiet media for mutter's ScreenCast.
pub const SCREENCAST: Profile = Profile {
    name: "screencast",
    a11y: A11y::Off,
    media: Media::Quiet,
    portals: Portals::On,
    system_bus: SystemBus::Private,
};

/// An accessibility lane: AT-SPI plus screenshots.
pub const ACCESSIBILITY: Profile = Profile {
    name: "accessibility",
    a11y: A11y::Atspi,
    media: Media::Quiet,
    portals: Portals::On,
    system_bus: SystemBus::Private,
};

/// Every profile, in documentation order.
pub const PROFILES: [Profile; 4] = [WIDGET, PLAIN, SCREENCAST, ACCESSIBILITY];

/// Look a profile up by name.
#[must_use]
pub fn profile_named(name: &str) -> Option<Profile> {
    PROFILES.into_iter().find(|profile| profile.name == name)
}

impl fmt::Display for A11y {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Off => "off",
            Self::Atspi => "atspi",
        })
    }
}

impl fmt::Display for Media {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Off => "off",
            Self::Quiet => "quiet",
        })
    }
}

impl fmt::Display for Portals {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Off => "off",
            Self::On => "on",
        })
    }
}

impl fmt::Display for SystemBus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Private => formatter.write_str("private"),
            Self::Host { .. } => formatter.write_str("host"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_match_the_design_table() {
        assert_eq!(WIREPLUMBER_PROFILE, "policy");
        let rows: Vec<_> = PROFILES
            .iter()
            .map(|p| format!("{} {} {} {}", p.name, p.a11y, p.media, p.portals))
            .collect();
        assert_eq!(
            rows,
            [
                "widget off off off",
                "plain off off on",
                "screencast off quiet on",
                "accessibility atspi quiet on",
            ]
        );
        assert!(PROFILES.iter().all(|p| p.system_bus == SystemBus::Private));
        assert_eq!(profile_named("screencast"), Some(SCREENCAST));
        assert_eq!(profile_named("main"), None);
    }
}
