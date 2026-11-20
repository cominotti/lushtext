// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure parsers for what the session stage observes: `dbus-monitor` records,
//! `pw-dump` output, module-rt evidence and `gdbus` replies.

use super::verdict::{BusRecord, RecordKind};

/// Incremental parser of `dbus-monitor`'s default output.
///
/// A record is a header line (`method call time=… sender=… -> destination=…
/// …; interface=…; member=…`) followed by indented argument lines. A record
/// is complete when the next header arrives or the stream ends.
#[derive(Debug, Default)]
pub struct MonitorParser {
    current: Option<BusRecord>,
}

impl MonitorParser {
    /// Feed one line; returns the record the line completed, if any.
    pub fn push_line(&mut self, line: &str) -> Option<BusRecord> {
        if let Some(header) = parse_header(line) {
            return self.current.replace(header);
        }
        if let Some(record) = self.current.as_mut()
            && let Some(value) = string_argument(line)
        {
            record.args.push(value);
        }
        None
    }

    /// End of stream: the last record, if any.
    pub fn finish(&mut self) -> Option<BusRecord> {
        self.current.take()
    }
}

fn parse_header(line: &str) -> Option<BusRecord> {
    let (kind, rest) = [
        ("method call ", RecordKind::MethodCall),
        ("method return ", RecordKind::Return),
        ("signal ", RecordKind::Signal),
        ("error ", RecordKind::Error),
    ]
    .into_iter()
    .find_map(|(prefix, kind)| line.strip_prefix(prefix).map(|rest| (kind, rest)))?;
    let field = |key: &str| -> Option<String> {
        rest.split([' ', ';'])
            .find_map(|token| token.strip_prefix(key))
            .map(str::to_string)
            .filter(|value| !value.is_empty() && value != "(null")
    };
    Some(BusRecord {
        kind: Some(kind),
        sender: field("sender=").unwrap_or_default(),
        destination: field("destination="),
        interface: field("interface="),
        member: field("member="),
        ..BusRecord::default()
    })
}

fn string_argument(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let quoted = trimmed.strip_prefix("string \"")?;
    let value = quoted.strip_suffix('"').unwrap_or(quoted);
    Some(value.replace("\\\"", "\""))
}

/// The `NameOwnerChanged` acquisition a record reports: `(name, new owner)`.
#[must_use]
pub fn name_acquisition(record: &BusRecord) -> Option<(String, String)> {
    if record.member.as_deref() != Some("NameOwnerChanged") || record.args.len() < 3 {
        return None;
    }
    let name = &record.args[0];
    let new_owner = &record.args[2];
    (!name.starts_with(':') && !new_owner.is_empty()).then(|| (name.clone(), new_owner.clone()))
}

/// `device.api` values of every object in `pw-dump` JSON.
#[must_use]
pub fn device_apis(pw_dump: &str) -> Vec<String> {
    const KEY: &str = "\"device.api\"";
    let mut apis = Vec::new();
    let mut rest = pw_dump;
    while let Some(index) = rest.find(KEY) {
        rest = &rest[index + KEY.len()..];
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(value) = value.trim_start().strip_prefix('"') else {
            continue;
        };
        if let Some(end) = value.find('"') {
            apis.push(value[..end].to_string());
        }
    }
    apis
}

/// Whether WirePlumber has loaded its whole profile, linking policy
/// included: its client object's `session.services` property, a bracketed
/// list (`[a, b, c]`), has `policy.linking.standard` as a whole member.
///
/// WirePlumber's `session-services.lua` sets that property from the features
/// that loaded, and its component rules order it after every other component
/// ("session-services.lua must execute at the very end"). It lists
/// `policy.linking.standard` only when that virtual feature holds, which
/// requires the linking hooks. So a member here means a `pipewiresrc` link
/// to a named target can be made at once.
///
/// The property sits on a Client object, which `pw-dump` always prints with
/// its properties. Readiness must not be read from a metadata object:
/// `pw-dump` omits every metadata object without entries, and in a session
/// without audio devices the `default` metadata has none.
///
/// The dump is scanned by key, as [`device_apis`] does; the order of the
/// members does not matter, and a member that only starts with the name
/// does not count.
#[must_use]
pub fn wireplumber_policy_ready(pw_dump: &str) -> bool {
    const KEY: &str = "\"session.services\"";
    const MEMBER: &str = "policy.linking.standard";
    let mut rest = pw_dump;
    while let Some(index) = rest.find(KEY) {
        rest = &rest[index + KEY.len()..];
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(value) = value.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = value.find('"') else {
            continue;
        };
        let Some(list) = value[..end]
            .trim()
            .strip_prefix('[')
            .and_then(|list| list.strip_suffix(']'))
        else {
            continue;
        };
        if list.split(',').any(|member| member.trim() == MEMBER) {
            return true;
        }
    }
    false
}

/// Lines of a daemon's `/proc/<pid>/maps` or log that show module-rt loaded.
#[must_use]
pub fn module_rt_evidence(source: &str, text: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for line in text.lines() {
        let hit = line.contains("libpipewire-module-rt")
            || line.contains(" mod.rt ")
            || line.contains("mod.rt |");
        if hit {
            let evidence = format!("{source}: {}", line.trim());
            if !seen.contains(&evidence) {
                seen.push(evidence);
            }
            if seen.len() >= 4 {
                break;
            }
        }
    }
    seen
}

/// The PID in a `gdbus call … GetConnectionUnixProcessID` reply, `(uint32 N,)`.
#[must_use]
pub fn gdbus_uint32_reply(reply: &str) -> Option<u32> {
    let digits: String = reply
        .trim()
        .strip_prefix("(uint32 ")?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Names in a `gdbus call … ListNames` reply, `(['a', 'b'],)`.
#[must_use]
pub fn gdbus_string_list(reply: &str) -> Vec<String> {
    reply
        .split('\'')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, name)| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: &str = "\
signal time=1731612345.000001 sender=org.freedesktop.DBus -> destination=:1.2 serial=2 path=/org/freedesktop/DBus; interface=org.freedesktop.DBus; member=NameAcquired
   string \":1.2\"
method call time=1731612345.100 sender=:1.4 -> destination=org.freedesktop.DBus serial=5 path=/org/freedesktop/DBus; interface=org.freedesktop.DBus; member=AddMatch
   string \"type='signal',sender='org.freedesktop.RealtimeKit1'\"
method call time=1731612345.200 sender=:1.4 -> destination=org.freedesktop.RealtimeKit1 serial=6 path=/org/freedesktop/RealtimeKit1; interface=org.freedesktop.DBus.Properties; member=Get
   string \"org.freedesktop.RealtimeKit1\"
   string \"MinNiceLevel\"
error time=1731612345.201 sender=org.freedesktop.DBus -> destination=:1.4 error_name=org.freedesktop.DBus.Error.ServiceUnknown reply_serial=6
   string \"The name org.freedesktop.RealtimeKit1 was not provided by any .service files\"
";

    fn parse(text: &str) -> Vec<BusRecord> {
        let mut parser = MonitorParser::default();
        let mut records: Vec<_> = text
            .lines()
            .filter_map(|line| parser.push_line(line))
            .collect();
        records.extend(parser.finish());
        records
    }

    #[test]
    fn monitor_records_parse_headers_and_string_arguments() {
        let records = parse(MONITOR);
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].kind, Some(RecordKind::Signal));
        assert_eq!(records[0].destination.as_deref(), Some(":1.2"));
        assert_eq!(records[1].member.as_deref(), Some("AddMatch"));
        assert_eq!(records[1].sender, ":1.4");
        assert_eq!(
            records[2].destination.as_deref(),
            Some("org.freedesktop.RealtimeKit1")
        );
        assert_eq!(
            records[2].interface.as_deref(),
            Some("org.freedesktop.DBus.Properties")
        );
        assert_eq!(
            records[2].args,
            ["org.freedesktop.RealtimeKit1", "MinNiceLevel"]
        );
        assert_eq!(records[3].kind, Some(RecordKind::Error));
        assert_eq!(records[3].sender, "org.freedesktop.DBus");
    }

    #[test]
    fn name_owner_changed_reports_acquisitions_only() {
        let text = "\
signal time=1 sender=org.freedesktop.DBus -> destination=(null destination) serial=9 path=/org/freedesktop/DBus; interface=org.freedesktop.DBus; member=NameOwnerChanged
   string \"org.a11y.Bus\"
   string \"\"
   string \":1.7\"
signal time=2 sender=org.freedesktop.DBus -> destination=(null destination) serial=10 path=/org/freedesktop/DBus; interface=org.freedesktop.DBus; member=NameOwnerChanged
   string \"org.a11y.Bus\"
   string \":1.7\"
   string \"\"
signal time=3 sender=org.freedesktop.DBus -> destination=(null destination) serial=11 path=/org/freedesktop/DBus; interface=org.freedesktop.DBus; member=NameOwnerChanged
   string \":1.8\"
   string \"\"
   string \":1.8\"
";
        let records = parse(text);
        assert_eq!(records[0].destination, None);
        let acquired: Vec<_> = records.iter().filter_map(name_acquisition).collect();
        assert_eq!(acquired, [("org.a11y.Bus".to_string(), ":1.7".to_string())]);
    }

    #[test]
    fn pw_dump_device_apis_are_read_by_key() {
        let dump = r#"[
  { "id": 40, "info": { "props": {
        "device.api": "alsa",
        "device.description": "bluez5 headset (restored)"
  } } },
  { "id": 41, "info": { "props": { "device.api": "v4l2" } } }
]"#;
        assert_eq!(device_apis(dump), ["alsa", "v4l2"]);
        assert!(device_apis("[]").is_empty());
    }

    /// The WirePlumber `policy` client as a real saved dump prints it
    /// (`baseline/logs-quiet-*/pw-dump.json`), cut to the keys that matter,
    /// with `session.services` replaced by `services` (`None` drops it).
    fn wireplumber_client(services: Option<&str>) -> String {
        let services = services.map_or_else(String::new, |list| {
            format!("        \"session.services\": \"{list}\",\n")
        });
        format!(
            r#"[
  {{
    "id": 31,
    "type": "PipeWire:Interface:Metadata",
    "props": {{ "metadata.name": "settings" }},
    "metadata": [
      {{ "subject": 0, "key": "clock.rate", "type": "", "value": 48000 }}
    ]
  }},
  {{
    "id": 32,
    "type": "PipeWire:Interface:Client",
    "version": 3,
    "permissions": [ "r", "w", "x", "m" ],
    "info": {{
      "change-mask": [ "props" ],
      "props": {{
        "application.name": "WirePlumber (policy)",
        "application.process.binary": "wireplumber",
        "application.version": "0.5.19",
        "remote.name": "[pipewire-0-manager,pipewire-0]",
{services}        "wireplumber.daemon": true,
        "wireplumber.profile": "policy"
      }}
    }}
  }}
]"#
        )
    }

    #[test]
    fn wireplumber_policy_readiness_is_a_whole_session_services_member() {
        let last = wireplumber_client(Some(
            "[policy.default-nodes, policy.device.profile, policy.device.routes, policy.linking.role-based, policy.linking.standard]",
        ));
        let first = wireplumber_client(Some(
            "[policy.linking.standard, policy.default-nodes, policy.linking.role-based, policy.device.routes, policy.device.profile]",
        ));
        let absent = wireplumber_client(None);
        let without = wireplumber_client(Some(
            "[policy.default-nodes, policy.device.profile, policy.device.routes, policy.linking.role-based]",
        ));
        let empty_list = wireplumber_client(Some("[]"));
        let prefix_only = wireplumber_client(Some(
            "[policy.default-nodes, policy.linking.standard-extra]",
        ));
        assert!(wireplumber_policy_ready(&last), "member last");
        assert!(wireplumber_policy_ready(&first), "member first");
        assert!(!wireplumber_policy_ready(&absent), "no session.services");
        assert!(!wireplumber_policy_ready(&without), "member missing");
        assert!(!wireplumber_policy_ready(&empty_list), "empty list");
        assert!(!wireplumber_policy_ready(&prefix_only), "prefix only");
        for malformed in [
            "",
            "not json",
            "\"session.services\"",
            "\"session.services\": ",
            "\"session.services\": \"policy.linking.standard\"",
            "\"session.services\": \"[policy.linking.standard",
            "\"session.services\": 7, \"x\": \"[policy.linking.standard]\"",
        ] {
            assert!(!wireplumber_policy_ready(malformed), "{malformed:?}");
        }
    }

    #[test]
    fn the_old_default_metadata_predicate_never_holds_on_a_ready_dump() {
        // `pw-dump` prints no metadata object without entries, and in a
        // session without audio devices `default` has none, so the helper's
        // old wait for this text always ran to its deadline.
        for services in [
            "[policy.default-nodes, policy.linking.standard]",
            "[policy.linking.standard, policy.default-nodes]",
        ] {
            let dump = wireplumber_client(Some(services));
            assert!(!dump.contains("\"metadata.name\": \"default\""));
        }
    }

    #[test]
    fn module_rt_evidence_reads_maps_and_log_topics() {
        let maps = "7f00-7f10 r-xp 0 00:00 1 /usr/lib64/pipewire-0.3/libpipewire-module-rt.so\n7f10-7f20 r-xp 0 00:00 1 /usr/lib64/libc.so.6\n";
        assert_eq!(module_rt_evidence("pipewire", maps).len(), 1);
        let log =
            "[I][00:00:00.1] mod.rt | [module-rt.c:1] RTKit\n[I][00:00:00.2] pw.context | ok\n";
        assert_eq!(module_rt_evidence("wireplumber", log).len(), 1);
        assert!(module_rt_evidence("pipewire", "pipewiresrc ok\n").is_empty());
    }

    #[test]
    fn gdbus_replies_parse() {
        assert_eq!(gdbus_uint32_reply("(uint32 4242,)\n"), Some(4242));
        assert_eq!(gdbus_uint32_reply("Error: x"), None);
        assert_eq!(
            gdbus_string_list("(['org.freedesktop.DBus', ':1.0', 'org.a11y.Bus'],)"),
            ["org.freedesktop.DBus", ":1.0", "org.a11y.Bus"]
        );
    }
}
