// SPDX-License-Identifier: MIT OR Apache-2.0

//! The runtime guard's verdict: pure set membership over bounded observations.
//!
//! Nothing here starts a process or reads a file. The session stage collects
//! [`Observations`] while its services are still alive and hands them to
//! [`verdict`], so every class, the clean case and the one exemption are unit
//! tested without a session.

use std::collections::BTreeSet;
use std::fmt;

use super::profiles::{A11y, Media, Portals, WIREPLUMBER_PROFILE};

/// The RealtimeKit1 bus name and interface.
pub const RTKIT: &str = "org.freedesktop.RealtimeKit1";
/// The BlueZ bus name prefix.
pub const BLUEZ: &str = "org.bluez";
/// The AT-SPI accessibility bus launcher's name (exact: mutter 51 owns the
/// unrelated `org.freedesktop.a11y.Manager`).
pub const A11Y_BUS: &str = "org.a11y.Bus";
/// Portal names that must not be acquired in a portals-off session.
pub const PORTAL_NAMES: [&str; 2] = [
    "org.freedesktop.portal.Desktop",
    "org.freedesktop.portal.Documents",
];
/// `device.api` values of host devices a quiet session must never create.
pub const HOST_DEVICE_APIS: [&str; 4] = ["alsa", "v4l2", "libcamera", "bluez5"];
/// The one sender exempt from the non-`MakeThread*` rtkit rule.
pub const PORTAL_FRONTEND_EXE: &str = "xdg-desktop-portal";
/// The rtkit properties the portal frontend reads once at startup.
pub const RTKIT_STARTUP_PROPERTIES: [&str; 3] =
    ["MaxRealtimePriority", "MinNiceLevel", "RTTimeUSecMax"];
/// The bus-daemon lookups the portal frontend makes for rtkit.
const FRONTEND_LOOKUPS: [&str; 3] = ["AddMatch", "GetNameOwner", "StartServiceByName"];
const BUS_DAEMON: &str = "org.freedesktop.DBus";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
/// Upper bound of a violation detail in bytes, so a pathological record stays
/// readable.
///
/// Kind: presentation cap; 240 rounded up to a power of two (the detail a
/// reader of `SESSION-HYGIENE:` sees).
const DETAIL_LIMIT: usize = 256;

/// Kind of one recorded bus message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordKind {
    /// A method call.
    MethodCall,
    /// A signal.
    Signal,
    /// A method return.
    Return,
    /// An error reply.
    Error,
}

/// One message recorded by the private system bus's monitor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BusRecord {
    /// Message kind (`None` when the header did not parse).
    pub kind: Option<RecordKind>,
    /// Unique or well-known sender name.
    pub sender: String,
    /// Sender PID, resolved while it was connected.
    pub sender_pid: Option<u32>,
    /// Basename of the sender's executable, resolved with the PID.
    pub sender_exe: Option<String>,
    /// Destination name.
    pub destination: Option<String>,
    /// Interface.
    pub interface: Option<String>,
    /// Member.
    pub member: Option<String>,
    /// String arguments, in order.
    pub args: Vec<String>,
}

impl BusRecord {
    fn names(&self, needle: &str) -> bool {
        self.destination
            .as_deref()
            .is_some_and(|d| d.contains(needle))
            || self
                .interface
                .as_deref()
                .is_some_and(|i| i.contains(needle))
            || self.args.iter().any(|arg| arg.contains(needle))
    }

    fn sent_by_peer(&self) -> bool {
        self.sender != BUS_DAEMON
            && matches!(self.kind, Some(RecordKind::MethodCall | RecordKind::Signal))
    }

    fn summary(&self) -> String {
        format!(
            "{} {} -> {} {}.{} {:?}",
            self.sender_exe.as_deref().unwrap_or("<unresolved>"),
            self.sender,
            self.destination.as_deref().unwrap_or("-"),
            self.interface.as_deref().unwrap_or("-"),
            self.member.as_deref().unwrap_or("-"),
            self.args
        )
    }
}

/// A session-bus name acquired at some point of the session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcquiredName {
    /// The well-known name.
    pub name: String,
    /// Basename of its owner's executable, when it was resolved.
    pub owner_exe: Option<String>,
}

/// Configuration facts checked at start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigFacts {
    /// Accessibility mode of the session.
    pub a11y: A11y,
    /// Media mode of the session.
    pub media: Media,
    /// Portal mode of the session.
    pub portals: Portals,
    /// `GTK_A11Y` as the session's processes see it.
    pub gtk_a11y: Option<String>,
    /// WirePlumber profile the helper starts (media sessions).
    pub wireplumber_profile: String,
    /// The quiet PipeWire and WirePlumber fragments are in place.
    pub fragments_present: bool,
    /// The daemons' config, state and data homes are helper-private.
    pub homes_private: bool,
}

/// Everything the guard looks at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observations {
    /// Configuration facts.
    pub config: ConfigFacts,
    /// Private system-bus records naming rtkit or bluez.
    pub system_bus: Vec<BusRecord>,
    /// `device.api` values of PipeWire objects (media sessions).
    pub device_apis: Vec<String>,
    /// Evidence lines of a loaded `libpipewire-module-rt`.
    pub module_rt: Vec<String>,
    /// Session-bus names acquired during the session.
    pub names_acquired: Vec<AcquiredName>,
}

/// A violation class.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ViolationClass {
    /// A message that can reach, or a call that consumes, RealtimeKit.
    Rtkit,
    /// A message naming BlueZ.
    Bluez,
    /// A host ALSA, v4l2, libcamera or bluez5 device.
    HardwareDevice,
    /// `libpipewire-module-rt` was loaded.
    ModuleRt,
    /// `org.a11y.Bus` in a session with a11y off.
    AccessibilityBus,
    /// A portal in a session with portals off.
    Portal,
    /// The session's configuration was not the quiet one.
    Config,
}

impl ViolationClass {
    /// Stable kebab-case name printed after `SESSION-HYGIENE:`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rtkit => "rtkit",
            Self::Bluez => "bluez",
            Self::HardwareDevice => "hardware-device",
            Self::ModuleRt => "module-rt",
            Self::AccessibilityBus => "accessibility-bus",
            Self::Portal => "portal",
            Self::Config => "config",
        }
    }
}

impl fmt::Display for ViolationClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One violation with a bounded detail.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Violation {
    /// Class.
    pub class: ViolationClass,
    /// Bounded, human-readable detail.
    pub detail: String,
}

impl Violation {
    fn new(class: ViolationClass, detail: impl Into<String>) -> Self {
        let mut detail: String = detail.into();
        if detail.len() > DETAIL_LIMIT {
            let mut cut = DETAIL_LIMIT;
            while !detail.is_char_boundary(cut) {
                cut -= 1;
            }
            detail.truncate(cut);
            detail.push('…');
        }
        Self { class, detail }
    }

    /// The line the helper prints.
    #[must_use]
    pub fn line(&self) -> String {
        format!("SESSION-HYGIENE: {}: {}", self.class, self.detail)
    }
}

/// Accepted `GTK_A11Y` values in an a11y-off session.
#[must_use]
pub fn gtk_a11y_value_is_accepted(value: Option<&str>) -> bool {
    matches!(value, Some("test" | "none"))
}

/// Evaluate a session. An empty result is a clean session.
#[must_use]
pub fn verdict(observations: &Observations) -> Vec<Violation> {
    let mut violations = Vec::new();
    config_violations(&observations.config, &mut violations);

    let mut frontend_reads: BTreeSet<(u32, String)> = BTreeSet::new();
    for record in &observations.system_bus {
        if !record.sent_by_peer() {
            continue;
        }
        let member = record.member.as_deref().unwrap_or_default();
        let names_rtkit = record.names(RTKIT);
        if names_rtkit && member.starts_with("MakeThread") {
            violations.push(Violation::new(
                ViolationClass::Rtkit,
                format!("MakeThread call: {}", record.summary()),
            ));
        } else if names_rtkit
            && !frontend_exemption(record, observations.config.portals, &mut frontend_reads)
        {
            violations.push(Violation::new(ViolationClass::Rtkit, record.summary()));
        }
        if record.names(BLUEZ) {
            violations.push(Violation::new(ViolationClass::Bluez, record.summary()));
        }
    }

    for api in &observations.device_apis {
        if HOST_DEVICE_APIS.contains(&api.as_str()) {
            violations.push(Violation::new(
                ViolationClass::HardwareDevice,
                format!("PipeWire object with device.api = {api}"),
            ));
        }
    }
    for line in &observations.module_rt {
        violations.push(Violation::new(ViolationClass::ModuleRt, line.clone()));
    }
    for acquired in &observations.names_acquired {
        let owner = acquired
            .owner_exe
            .as_deref()
            .unwrap_or("<unresolved owner>");
        if observations.config.a11y == A11y::Off && acquired.name == A11Y_BUS {
            violations.push(Violation::new(
                ViolationClass::AccessibilityBus,
                format!("{A11Y_BUS} acquired by {owner}"),
            ));
        }
        if observations.config.portals == Portals::Off
            && PORTAL_NAMES.contains(&acquired.name.as_str())
        {
            violations.push(Violation::new(
                ViolationClass::Portal,
                format!("{} acquired by {owner}", acquired.name),
            ));
        }
    }
    violations.sort();
    violations.dedup();
    violations
}

fn config_violations(config: &ConfigFacts, violations: &mut Vec<Violation>) {
    if config.a11y == A11y::Off && !gtk_a11y_value_is_accepted(config.gtk_a11y.as_deref()) {
        violations.push(Violation::new(
            ViolationClass::Config,
            format!(
                "GTK_A11Y must be `test` or `none` with a11y off, found {:?}",
                config.gtk_a11y
            ),
        ));
    }
    if config.media == Media::Quiet {
        if config.wireplumber_profile != WIREPLUMBER_PROFILE {
            violations.push(Violation::new(
                ViolationClass::Config,
                format!(
                    "WirePlumber profile is {:?}, not {WIREPLUMBER_PROFILE:?}",
                    config.wireplumber_profile
                ),
            ));
        }
        if !config.fragments_present {
            violations.push(Violation::new(
                ViolationClass::Config,
                "the quiet PipeWire/WirePlumber fragments are missing",
            ));
        }
        if !config.homes_private {
            violations.push(Violation::new(
                ViolationClass::Config,
                "the media daemons' homes are not helper-private",
            ));
        }
    }
}

/// Whether a non-`MakeThread*` rtkit record is the portal frontend's startup.
///
/// Bounded by sender *and* shape: the frontend executable, a portals-on
/// session, a lookup on the bus daemon or the first read of each rtkit
/// property per frontend process. An unresolved sender gets no exemption.
fn frontend_exemption(
    record: &BusRecord,
    portals: Portals,
    reads: &mut BTreeSet<(u32, String)>,
) -> bool {
    if portals != Portals::On || record.sender_exe.as_deref() != Some(PORTAL_FRONTEND_EXE) {
        return false;
    }
    let Some(pid) = record.sender_pid else {
        return false;
    };
    let member = record.member.as_deref().unwrap_or_default();
    if record.destination.as_deref() == Some(BUS_DAEMON) && FRONTEND_LOOKUPS.contains(&member) {
        return true;
    }
    if record.destination.as_deref() == Some(RTKIT)
        && record.interface.as_deref() == Some(PROPERTIES_INTERFACE)
        && member == "Get"
        && record.args.first().map(String::as_str) == Some(RTKIT)
        && let Some(property) = record.args.get(1)
        && RTKIT_STARTUP_PROPERTIES.contains(&property.as_str())
    {
        return reads.insert((pid, property.clone()));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean_config(portals: Portals) -> ConfigFacts {
        ConfigFacts {
            a11y: A11y::Off,
            media: Media::Quiet,
            portals,
            gtk_a11y: Some("test".to_string()),
            wireplumber_profile: WIREPLUMBER_PROFILE.to_string(),
            fragments_present: true,
            homes_private: true,
        }
    }

    fn observations(portals: Portals) -> Observations {
        Observations {
            config: clean_config(portals),
            system_bus: Vec::new(),
            device_apis: Vec::new(),
            module_rt: Vec::new(),
            names_acquired: Vec::new(),
        }
    }

    fn call(
        exe: Option<&str>,
        destination: &str,
        interface: &str,
        member: &str,
        args: &[&str],
    ) -> BusRecord {
        BusRecord {
            kind: Some(RecordKind::MethodCall),
            sender: ":1.4".to_string(),
            sender_pid: exe.map(|_| 4242),
            sender_exe: exe.map(str::to_string),
            destination: Some(destination.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        }
    }

    /// The review-stage spike's frontend records: lookups plus one read of
    /// each startup property (`evidence/baseline.md`).
    fn frontend_startup(exe: Option<&str>) -> Vec<BusRecord> {
        let mut records = vec![
            call(
                exe,
                BUS_DAEMON,
                BUS_DAEMON,
                "AddMatch",
                &["type='signal',sender='org.freedesktop.RealtimeKit1'"],
            ),
            call(
                exe,
                BUS_DAEMON,
                BUS_DAEMON,
                "AddMatch",
                &["type='signal',arg0='org.freedesktop.RealtimeKit1'"],
            ),
            call(exe, BUS_DAEMON, BUS_DAEMON, "GetNameOwner", &[RTKIT]),
            call(exe, BUS_DAEMON, BUS_DAEMON, "StartServiceByName", &[RTKIT]),
        ];
        for property in RTKIT_STARTUP_PROPERTIES {
            records.push(call(
                exe,
                RTKIT,
                PROPERTIES_INTERFACE,
                "Get",
                &[RTKIT, property],
            ));
        }
        records
    }

    fn classes(observations: &Observations) -> Vec<ViolationClass> {
        verdict(observations).into_iter().map(|v| v.class).collect()
    }

    #[test]
    fn clean_session_has_no_violation() {
        assert!(verdict(&observations(Portals::On)).is_empty());
        assert!(verdict(&observations(Portals::Off)).is_empty());
    }

    #[test]
    fn portal_frontend_startup_reads_pass_with_portals_on() {
        let mut obs = observations(Portals::On);
        obs.system_bus = frontend_startup(Some(PORTAL_FRONTEND_EXE));
        assert!(verdict(&obs).is_empty(), "{:?}", verdict(&obs));
    }

    #[test]
    fn the_same_records_from_another_executable_fire() {
        let mut obs = observations(Portals::On);
        obs.system_bus = frontend_startup(Some("mutter"));
        assert_eq!(classes(&obs).first(), Some(&ViolationClass::Rtkit));
    }

    #[test]
    fn frontend_records_fire_in_a_portals_off_profile() {
        let mut obs = observations(Portals::Off);
        obs.system_bus = frontend_startup(Some(PORTAL_FRONTEND_EXE));
        assert!(classes(&obs).iter().all(|c| *c == ViolationClass::Rtkit));
        assert!(!classes(&obs).is_empty());
    }

    #[test]
    fn an_unresolved_sender_gets_no_exemption() {
        let mut obs = observations(Portals::On);
        obs.system_bus = frontend_startup(None);
        assert!(!classes(&obs).is_empty());
    }

    #[test]
    fn a_second_property_read_by_the_frontend_fires() {
        let mut obs = observations(Portals::On);
        obs.system_bus = frontend_startup(Some(PORTAL_FRONTEND_EXE));
        obs.system_bus.push(call(
            Some(PORTAL_FRONTEND_EXE),
            RTKIT,
            PROPERTIES_INTERFACE,
            "Get",
            &[RTKIT, "MinNiceLevel"],
        ));
        assert_eq!(classes(&obs), [ViolationClass::Rtkit]);
    }

    #[test]
    fn make_thread_fires_from_every_sender_including_the_frontend() {
        for exe in [Some(PORTAL_FRONTEND_EXE), Some("pipewire"), None] {
            for member in [
                "MakeThreadRealtime",
                "MakeThreadRealtimeWithPID",
                "MakeThreadHighPriority",
                "MakeThreadHighPriorityWithPID",
            ] {
                let mut obs = observations(Portals::On);
                obs.system_bus = vec![call(exe, RTKIT, RTKIT, member, &[])];
                let found = verdict(&obs);
                assert_eq!(found.len(), 1, "{exe:?} {member}");
                assert_eq!(found[0].class, ViolationClass::Rtkit);
                assert!(found[0].detail.starts_with("MakeThread call"));
            }
        }
    }

    #[test]
    fn bus_daemon_replies_naming_rtkit_are_not_violations() {
        let mut obs = observations(Portals::On);
        obs.system_bus = vec![BusRecord {
            kind: Some(RecordKind::Error),
            sender: BUS_DAEMON.to_string(),
            args: vec![format!(
                "The name {RTKIT} was not provided by any .service files"
            )],
            ..BusRecord::default()
        }];
        assert!(verdict(&obs).is_empty());
    }

    #[test]
    fn bluez_fires_from_any_sender() {
        for exe in [Some(PORTAL_FRONTEND_EXE), Some("wireplumber"), None] {
            let mut obs = observations(Portals::On);
            obs.system_bus = vec![call(
                exe,
                "org.bluez",
                "org.bluez.Media1",
                "RegisterEndpoint",
                &[],
            )];
            assert_eq!(classes(&obs), [ViolationClass::Bluez]);
        }
    }

    #[test]
    fn host_devices_fire_by_device_api_only() {
        let mut obs = observations(Portals::On);
        obs.device_apis = vec![
            "alsa".into(),
            "v4l2".into(),
            "libcamera".into(),
            "bluez5".into(),
        ];
        assert_eq!(classes(&obs), [ViolationClass::HardwareDevice; 4]);
        // Restored metadata naming a bluez device is not a device.
        let mut obs = observations(Portals::On);
        obs.device_apis = vec!["bluez5-restored-metadata".into(), "videotestsrc".into()];
        assert!(verdict(&obs).is_empty());
    }

    #[test]
    fn module_rt_evidence_fires() {
        let mut obs = observations(Portals::On);
        obs.module_rt = vec!["pipewire maps libpipewire-module-rt.so".into()];
        assert_eq!(classes(&obs), [ViolationClass::ModuleRt]);
    }

    #[test]
    fn accessibility_bus_fires_on_the_exact_name_even_after_release() {
        let mut obs = observations(Portals::Off);
        // Acquired and released before teardown: still in the record.
        obs.names_acquired = vec![
            AcquiredName {
                name: A11Y_BUS.into(),
                owner_exe: Some("at-spi-bus-launcher".into()),
            },
            AcquiredName {
                name: "org.freedesktop.a11y.Manager".into(),
                owner_exe: Some("mutter".into()),
            },
        ];
        let found = verdict(&obs);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].class, ViolationClass::AccessibilityBus);
        assert!(found[0].detail.contains("at-spi-bus-launcher"));
        obs.config.a11y = A11y::Atspi;
        assert!(verdict(&obs).is_empty());
    }

    #[test]
    fn portal_fires_only_with_portals_off() {
        let mut obs = observations(Portals::Off);
        obs.names_acquired = PORTAL_NAMES
            .iter()
            .map(|name| AcquiredName {
                name: (*name).into(),
                owner_exe: None,
            })
            .collect();
        assert_eq!(classes(&obs), [ViolationClass::Portal; 2]);
        obs.config.portals = Portals::On;
        assert!(verdict(&obs).is_empty());
    }

    #[test]
    fn config_accepts_test_and_none_and_nothing_else() {
        for (value, ok) in [
            (Some("test"), true),
            (Some("none"), true),
            (Some("atspi"), false),
            (Some(""), false),
            (None, false),
        ] {
            let mut obs = observations(Portals::Off);
            obs.config.gtk_a11y = value.map(str::to_string);
            assert_eq!(verdict(&obs).is_empty(), ok, "{value:?}");
        }
        let mut obs = observations(Portals::Off);
        obs.config.a11y = A11y::Atspi;
        obs.config.gtk_a11y = None;
        assert!(verdict(&obs).is_empty());
    }

    #[test]
    fn config_fires_on_media_drift() {
        let mut obs = observations(Portals::On);
        obs.config.wireplumber_profile = "main".into();
        obs.config.fragments_present = false;
        obs.config.homes_private = false;
        assert_eq!(classes(&obs), [ViolationClass::Config; 3]);
        obs.config.media = Media::Off;
        assert!(verdict(&obs).is_empty());
    }

    #[test]
    fn details_are_bounded() {
        let mut obs = observations(Portals::On);
        obs.module_rt = vec!["x".repeat(10_000)];
        let found = verdict(&obs);
        assert!(found[0].detail.len() <= DETAIL_LIMIT + '…'.len_utf8());
        assert!(found[0].line().starts_with("SESSION-HYGIENE: module-rt: "));
    }
}
