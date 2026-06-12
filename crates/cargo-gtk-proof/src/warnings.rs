// SPDX-License-Identifier: GPL-3.0-or-later

//! Runtime warning scan for live visual proof logs.
//!
//! Visual proof treats unexpected GTK, GDK, GSK, Adwaita, Libadwaita, AT-SPI,
//! accessibility, warning, critical, and error lines as proof failures. The
//! scanner mirrors the Python runner's allowlist while keeping each retained
//! line bounded for uploadable artifacts.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{artifacts, model};

const WARNING_TEXT_LIMIT: usize = 1200;

/// Write `warning-scan.json` for a case directory and return the report path.
pub(crate) fn scan_case_logs(case_dir: &Path) -> Result<PathBuf, String> {
    let report = scan_logs(case_dir)?;
    artifacts::write_artifact(
        &case_dir.join("warning-scan.json"),
        artifacts::ProofArtifactKind::WarningScan,
        &report,
    )
}

fn scan_logs(case_dir: &Path) -> Result<WarningScanReport, String> {
    let mut matches = Vec::new();
    for name in [
        "session.log",
        "mutter-child.log",
        "lushtext.stdout",
        "lushtext.stderr",
        "pipewire.log",
        "wireplumber.log",
    ] {
        let path = case_dir.join(name);
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read warning log {}: {error}", path.display()))?;
        for line in text.lines() {
            if warning_line_matches(line) {
                matches.push(WarningMatch {
                    artifact: name.to_string(),
                    line: bounded(line),
                });
            }
        }
    }
    Ok(WarningScanReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status: if matches.is_empty() {
            "passed".to_string()
        } else {
            "failed".to_string()
        },
        warning_count: u64::try_from(matches.len()).unwrap_or(u64::MAX),
        matches,
    })
}

fn warning_line_matches(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error reading events from display: broken pipe") {
        return false;
    }
    if known_headless_warning(&lower) {
        return false;
    }
    let has_source = [
        "gtk",
        "gdk",
        "gsk",
        "adwaita",
        "libadwaita",
        "at-spi",
        "accessibility",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let has_severity = ["warning", "critical", "error"]
        .iter()
        .any(|needle| lower.contains(needle));
    has_source && has_severity
}

fn known_headless_warning(line: &str) -> bool {
    if line.contains("unable to register the application:")
        && line.contains("org.a11y.atspi.registry")
    {
        return true;
    }
    [
        "unable to acquire session bus",
        "failed to connect to session bus",
        "portal",
        "accessibility bus",
        "at-spi: couldn't connect",
        "at-spi: could not obtain desktop path or name",
        "atk-bridge: getregisteredevents returned message with unknown signature",
        "atk-bridge: get_device_events_reply: unknown signature",
        "pipewire remote error",
    ]
    .iter()
    .any(|needle| line.contains(needle))
}

fn bounded(line: &str) -> String {
    if line.len() <= WARNING_TEXT_LIMIT {
        return line.to_string();
    }
    let suffix = " [truncated]";
    let target_len = WARNING_TEXT_LIMIT.saturating_sub(suffix.len());
    let end = line
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= target_len)
        .last()
        .unwrap_or(0);
    format!("{}{}", &line[..end], suffix)
}

#[derive(Debug, Serialize)]
struct WarningScanReport {
    schema_version: u64,
    status: String,
    warning_count: u64,
    matches: Vec<WarningMatch>,
}

#[derive(Debug, Serialize)]
struct WarningMatch {
    artifact: String,
    line: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_scan_reports_unexpected_toolkit_warning() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("lushtext.stderr"),
            "Gtk-WARNING **: allocation failed\n",
        )
        .expect("write log");

        let report = scan_logs(tempdir.path()).expect("scan");

        assert_eq!(report.status, "failed");
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.matches[0].artifact, "lushtext.stderr");
        assert!(report.matches[0].line.contains("Gtk-WARNING"));
    }

    #[test]
    fn warning_scan_ignores_known_headless_noise_and_broken_pipe() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("mutter-child.log"),
            [
                "Gdk-Message: Error reading events from display: Broken pipe",
                "Gtk-WARNING **: portal helper unavailable in headless session",
                "(lushtext:123): Gtk-CRITICAL **: Unable to register the application: GDBus.Error:org.freedesktop.DBus.Error.NameHasNoOwner: Could not activate remote peer 'org.a11y.atspi.Registry': unit failed",
            ]
            .join("\n"),
        )
        .expect("write log");

        let report = scan_logs(tempdir.path()).expect("scan");

        assert_eq!(report.status, "passed");
        assert_eq!(report.warning_count, 0);
    }

    #[test]
    fn warning_scan_artifact_is_schema_valid() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        fs::write(
            tempdir.path().join("session.log"),
            "Libadwaita-CRITICAL **: broken layout\n",
        )
        .expect("write log");

        let path = scan_case_logs(tempdir.path()).expect("write scan");
        let payload = crate::read_json_value(&path, "warning scan").expect("read scan");
        let outcome = model::validate_document(&payload).expect("valid warning scan");

        assert_eq!(outcome.kind, model::DocumentKind::WarningScan);
        assert_eq!(payload["status"], "failed");
    }
}
