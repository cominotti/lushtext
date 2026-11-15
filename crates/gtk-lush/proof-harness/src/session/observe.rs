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
