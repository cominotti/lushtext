// SPDX-License-Identifier: GPL-3.0-or-later

//! GStreamer capture helpers for Mutter PipeWire monitor streams.
//!
//! Mutter owns the D-Bus ScreenCast session and exposes a PipeWire node. This
//! module keeps the GStreamer side small: given a node id, it captures either a
//! single PNG or a bounded frame stream with supervised logs.

#![allow(
    dead_code,
    reason = "capture primitives land before ScreenCast D-Bus wiring calls them from the live runner"
)]

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{artifacts, model, process};

/// Timeout for one still-frame GStreamer capture; longer values hide broken PipeWire sessions.
const GST_CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
/// Timeout for bounded frame streams so animation proof artifacts cannot run forever.
const GST_FRAME_STREAM_TIMEOUT: Duration = Duration::from_secs(30);
/// Mutter ScreenCast bus name used by GNOME's private screenshot pipeline.
const SCREENCAST_BUS_NAME: &str = "org.gnome.Mutter.ScreenCast";
/// Root object path for creating Mutter ScreenCast sessions.
const SCREENCAST_ROOT_PATH: &str = "/org/gnome/Mutter/ScreenCast";
/// Interface that starts ScreenCast sessions.
const SCREENCAST_INTERFACE: &str = "org.gnome.Mutter.ScreenCast";
/// Interface that starts, records, and stops one ScreenCast session.
const SCREENCAST_SESSION_INTERFACE: &str = "org.gnome.Mutter.ScreenCast.Session";
/// Interface used to discover the PipeWire node for a ScreenCast stream.
const SCREENCAST_STREAM_INTERFACE: &str = "org.gnome.Mutter.ScreenCast.Stream";
/// Monitor name used by headless Mutter in the proof environment.
const SCREENCAST_MONITOR_NAME: &str = "Meta-0";
/// Wait budget for Mutter to publish the PipeWire stream signal after capture starts.
const PIPEWIRE_SIGNAL_TIMEOUT: Duration = Duration::from_secs(5);

/// Result of running one GStreamer capture helper.
#[derive(Debug, Serialize)]
pub(crate) struct CaptureHelperReport {
    /// Schema version used by proof artifacts.
    pub(crate) schema_version: u64,
    /// Stable capture status.
    pub(crate) status: &'static str,
    /// Capture mode: one PNG or a bounded frame stream.
    pub(crate) mode: &'static str,
    /// PipeWire node id supplied by Mutter ScreenCast.
    pub(crate) node_id: u32,
    /// Relative output path or frame pattern.
    pub(crate) output: String,
    /// Relative helper log path.
    pub(crate) log: String,
    /// Whether the helper exceeded its timeout.
    pub(crate) timed_out: bool,
    /// Helper exit code when available.
    pub(crate) exit_code: Option<i32>,
}

/// Active Mutter ScreenCast session for one monitor stream.
#[derive(Debug)]
pub(crate) struct MonitorScreenCast {
    connection: gio::DBusConnection,
    session_path: String,
    stream_path: String,
    node_id: u32,
}

/// Active GStreamer frame recording tied to a Mutter ScreenCast session.
#[derive(Debug)]
pub(crate) struct FrameRecording {
    screencast: MonitorScreenCast,
    child: process::LoggedChild,
}

impl FrameRecording {
    /// Return whether GStreamer has finished writing the bounded frame stream.
    pub(crate) fn has_exited(&mut self) -> Result<bool, String> {
        self.child.has_exited()
    }

    /// Wait for the frame stream to finish, then stop the ScreenCast session.
    pub(crate) fn stop(mut self, timeout: Duration) -> Result<(), String> {
        if self.child.wait_for_exit(timeout)?.is_none() {
            self.child.terminate(timeout)?;
        }
        self.screencast.stop()
    }
}

impl MonitorScreenCast {
    /// PipeWire node id emitted by Mutter for this monitor stream.
    pub(crate) const fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Stop the Mutter ScreenCast session.
    pub(crate) fn stop(&self) -> Result<(), String> {
        self.connection
            .call_sync(
                Some(SCREENCAST_BUS_NAME),
                &self.session_path,
                SCREENCAST_SESSION_INTERFACE,
                "Stop",
                None,
                None,
                gio::DBusCallFlags::NONE,
                10_000,
                gio::Cancellable::NONE,
            )
            .map(|_| ())
            .map_err(|error| format!("cannot stop Mutter ScreenCast session: {error}"))
    }
}

/// Capture the current Mutter monitor to one PNG and write a helper report.
pub(crate) fn capture_monitor_png(
    output: &Path,
    log_path: &Path,
    report_path: &Path,
) -> Result<PathBuf, String> {
    let screencast = start_monitor_screencast(PIPEWIRE_SIGNAL_TIMEOUT)?;
    let report = capture_node_png(screencast.node_id(), output, log_path);
    let stop_result = screencast.stop();
    let report = report?;
    let status = report.status;
    let exit_code = report.exit_code;
    let timed_out = report.timed_out;
    artifacts::write_json(report_path, &report)?;
    stop_result?;
    if status != "captured" {
        return Err(format!(
            "GStreamer monitor capture failed: exit_code={exit_code:?} timed_out={timed_out}"
        ));
    }
    Ok(report_path.to_path_buf())
}

/// Start a Mutter ScreenCast session and wait for its PipeWire node id.
pub(crate) fn start_monitor_screencast(timeout: Duration) -> Result<MonitorScreenCast, String> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|error| format!("cannot connect to session bus for Mutter ScreenCast: {error}"))?;
    let session_path = create_session(&connection)?;
    let stream_path = record_monitor(&connection, &session_path)?;
    let node_id = wait_for_pipewire_node(&connection, &session_path, &stream_path, timeout)?;
    Ok(MonitorScreenCast {
        connection,
        session_path,
        stream_path,
        node_id,
    })
}

/// Capture one PipeWire monitor frame to a PNG file.
pub(crate) fn capture_node_png(
    node_id: u32,
    output: &Path,
    log_path: &Path,
) -> Result<CaptureHelperReport, String> {
    let output_arg = output.to_string_lossy().into_owned();
    let args = gst_single_png_args(node_id, &output_arg);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = process::run_logged_command(
        "gst-launch-1.0",
        &arg_refs,
        &[],
        log_path,
        GST_CAPTURE_TIMEOUT,
    )?;
    let status = if !result.timed_out && result.exit_code == Some(0) && output.is_file() {
        "captured"
    } else {
        "failed"
    };
    Ok(CaptureHelperReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status,
        mode: "single-png",
        node_id,
        output: artifacts::safe_display_path(output),
        log: artifacts::safe_display_path(log_path),
        timed_out: result.timed_out,
        exit_code: result.exit_code,
    })
}

/// Capture a bounded PipeWire monitor frame stream to numbered PNG files.
pub(crate) fn capture_node_frames(
    node_id: u32,
    frame_count: u32,
    frame_pattern: &Path,
    log_path: &Path,
) -> Result<CaptureHelperReport, String> {
    let pattern_arg = frame_pattern.to_string_lossy().into_owned();
    let args = gst_frame_stream_args(node_id, frame_count, &pattern_arg);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = process::run_logged_command(
        "gst-launch-1.0",
        &arg_refs,
        &[],
        log_path,
        GST_FRAME_STREAM_TIMEOUT,
    )?;
    let status = if !result.timed_out && result.exit_code == Some(0) {
        "captured"
    } else {
        "failed"
    };
    Ok(CaptureHelperReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status,
        mode: "frame-stream",
        node_id,
        output: artifacts::safe_display_path(frame_pattern),
        log: artifacts::safe_display_path(log_path),
        timed_out: result.timed_out,
        exit_code: result.exit_code,
    })
}

/// Start recording a bounded frame stream from the current Mutter monitor.
pub(crate) fn start_monitor_frame_recording(
    frame_count: u32,
    frame_pattern: &Path,
    log_path: &Path,
) -> Result<FrameRecording, String> {
    let screencast = start_monitor_screencast(PIPEWIRE_SIGNAL_TIMEOUT)?;
    let pattern_arg = frame_pattern.to_string_lossy().into_owned();
    let args = gst_frame_stream_args(screencast.node_id(), frame_count, &pattern_arg);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let child = process::start_logged_child("gst-launch-1.0", &arg_refs, &[], log_path)?;
    Ok(FrameRecording { screencast, child })
}

fn create_session(connection: &gio::DBusConnection) -> Result<String, String> {
    let result = connection
        .call_sync(
            Some(SCREENCAST_BUS_NAME),
            SCREENCAST_ROOT_PATH,
            SCREENCAST_INTERFACE,
            "CreateSession",
            Some(&empty_options_variant()?),
            Some(reply_type("(o)")?),
            gio::DBusCallFlags::NONE,
            10_000,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("Mutter ScreenCast CreateSession failed: {error}"))?;
    object_path_child(&result, 0, "CreateSession")
}

fn record_monitor(connection: &gio::DBusConnection, session_path: &str) -> Result<String, String> {
    let result = connection
        .call_sync(
            Some(SCREENCAST_BUS_NAME),
            session_path,
            SCREENCAST_SESSION_INTERFACE,
            "RecordMonitor",
            Some(&record_monitor_variant()?),
            Some(reply_type("(o)")?),
            gio::DBusCallFlags::NONE,
            10_000,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("Mutter ScreenCast RecordMonitor failed: {error}"))?;
    object_path_child(&result, 0, "RecordMonitor")
}

fn wait_for_pipewire_node(
    connection: &gio::DBusConnection,
    session_path: &str,
    stream_path: &str,
    timeout: Duration,
) -> Result<u32, String> {
    let node_id = Rc::new(Cell::new(None::<u32>));
    let observed = Rc::clone(&node_id);
    let _subscription = connection.subscribe_to_signal(
        Some(SCREENCAST_BUS_NAME),
        Some(SCREENCAST_STREAM_INTERFACE),
        Some("PipeWireStreamAdded"),
        Some(stream_path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if let Some(value) = signal.parameters.child_value(0).get::<u32>() {
                observed.set(Some(value));
            }
        },
    );
    connection
        .call_sync(
            Some(SCREENCAST_BUS_NAME),
            session_path,
            SCREENCAST_SESSION_INTERFACE,
            "Start",
            None,
            None,
            gio::DBusCallFlags::NONE,
            10_000,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("Mutter ScreenCast Start failed: {error}"))?;

    let deadline = Instant::now() + timeout;
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        if let Some(value) = node_id.get() {
            return Ok(value);
        }
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("Mutter did not emit PipeWireStreamAdded for Meta-0".to_string())
}

fn empty_options_variant() -> Result<glib::Variant, String> {
    parse_variant("(a{sv})", "(@a{sv} {},)")
}

fn record_monitor_variant() -> Result<glib::Variant, String> {
    parse_variant("(sa{sv})", "('Meta-0', {'is-recording': <true>})")
}

fn parse_variant(variant_type: &str, text: &str) -> Result<glib::Variant, String> {
    let variant_type = glib::VariantTy::new(variant_type)
        .map_err(|error| format!("invalid GVariant type {variant_type}: {error}"))?;
    glib::Variant::parse(Some(variant_type), text)
        .map_err(|error| format!("cannot parse GVariant {variant_type}: {error}"))
}

fn reply_type(variant_type: &str) -> Result<&glib::VariantTy, String> {
    glib::VariantTy::new(variant_type)
        .map_err(|error| format!("invalid D-Bus reply type {variant_type}: {error}"))
}

fn object_path_child(result: &glib::Variant, index: usize, method: &str) -> Result<String, String> {
    result
        .child_value(index)
        .str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Mutter ScreenCast {method} returned unexpected reply"))
}

fn gst_single_png_args(node_id: u32, output: &str) -> Vec<String> {
    [
        "-q".to_string(),
        "pipewiresrc".to_string(),
        format!("path={node_id}"),
        "num-buffers=1".to_string(),
        "!".to_string(),
        "videoconvert".to_string(),
        "!".to_string(),
        "pngenc".to_string(),
        "!".to_string(),
        "filesink".to_string(),
        format!("location={output}"),
    ]
    .to_vec()
}

fn gst_frame_stream_args(node_id: u32, frame_count: u32, frame_pattern: &str) -> Vec<String> {
    [
        "-q".to_string(),
        "pipewiresrc".to_string(),
        format!("path={node_id}"),
        format!("num-buffers={frame_count}"),
        "!".to_string(),
        "videoconvert".to_string(),
        "!".to_string(),
        "pngenc".to_string(),
        "!".to_string(),
        "multifilesink".to_string(),
        format!("location={frame_pattern}"),
    ]
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_png_command_matches_mutter_pipewire_contract() {
        let args = gst_single_png_args(37, "/tmp/out.png");

        assert_eq!(
            args,
            [
                "-q",
                "pipewiresrc",
                "path=37",
                "num-buffers=1",
                "!",
                "videoconvert",
                "!",
                "pngenc",
                "!",
                "filesink",
                "location=/tmp/out.png"
            ]
        );
    }

    #[test]
    fn frame_stream_command_matches_animation_contract() {
        let args = gst_frame_stream_args(42, 12, "/tmp/frame-%03d.png");

        assert_eq!(
            args,
            [
                "-q",
                "pipewiresrc",
                "path=42",
                "num-buffers=12",
                "!",
                "videoconvert",
                "!",
                "pngenc",
                "!",
                "multifilesink",
                "location=/tmp/frame-%03d.png"
            ]
        );
    }

    #[test]
    fn capture_report_is_versioned_and_bounded() {
        let report = CaptureHelperReport {
            schema_version: model::SUPPORTED_SCHEMA_VERSION,
            status: "captured",
            mode: "single-png",
            node_id: 1,
            output: "before.png".to_string(),
            log: "gst.log".to_string(),
            timed_out: false,
            exit_code: Some(0),
        };
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("capture-report.json");

        artifacts::write_json(&path, &report).expect("write report");
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("report text"))
                .expect("json");

        assert_eq!(value["schema_version"], model::SUPPORTED_SCHEMA_VERSION);
        assert_eq!(value["node_id"], 1);
        assert_eq!(value["log"], "gst.log");
    }

    #[test]
    fn screencast_variants_match_mutter_signatures() {
        let create = empty_options_variant().expect("create session variant");
        let record = record_monitor_variant().expect("record monitor variant");

        assert_eq!(create.type_().as_str(), "(a{sv})");
        assert_eq!(record.type_().as_str(), "(sa{sv})");
        assert!(format!("{record}").contains(SCREENCAST_MONITOR_NAME));
    }
}
