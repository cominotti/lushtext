// SPDX-License-Identifier: GPL-3.0-or-later

//! Live process orchestration for the Rust visual proof runner.
//!
//! This module owns the process tree shape that used to live in the Python
//! visual runner: a private D-Bus session, PipeWire/WirePlumber, headless
//! Mutter, and a LushText child. It writes same-session screenshots, geometry
//! snapshots, and animation-stream reports, while the outer runner remains
//! responsible for deciding when those artifacts can be counted as proof.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use serde_json::Value;

use crate::{
    artifacts, automation, capture,
    geometry::{
        Insets, VisualBox, app_pixel_anchor_geometry, pixel_anchor_box, png_rect_with_message,
        safe_name as safe_anchor_name, selected_surface_rows as select_surface_rows, surface_box,
        visual_geometry,
    },
    host, model, png, process, read_json_value,
};

/// Application id used for isolated settings and process discovery.
const APP_ID: &str = "dev.cominotti.lushtext";
/// Time allowed for PipeWire to publish its socket before the run is considered environmental.
const PIPEWIRE_READY_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound for the hidden Mutter child so broken launches cannot hang CI indefinitely.
const MUTTER_CHILD_TIMEOUT: Duration = Duration::from_secs(120);
/// Initial app settle period before first readiness checks start.
const APP_LAUNCH_SETTLE: Duration = Duration::from_millis(750);
/// Grace period for auxiliary services to flush logs after each case.
const PROCESS_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
/// Number of matching final geometry samples required before accepting a settled state.
const FINAL_GEOMETRY_SAMPLE_COUNT: usize = 3;
/// Delay between final geometry samples; short enough for fast tests, long enough to catch animation tails.
const FINAL_GEOMETRY_SAMPLE_INTERVAL: Duration = Duration::from_millis(50);
/// Total wait budget for final geometry convergence before a case fails.
const FINAL_GEOMETRY_TIMEOUT: Duration = Duration::from_secs(5);
/// Default invariant that proves native minimap anchors during sidebar animation.
const DEFAULT_ANIMATION_INVARIANT_ID: &str = "native-minimap-animation-highlight-anchors";
/// Frame count high enough to catch intermediate animation geometry at 60 Hz.
const DEFAULT_ANIMATION_STREAM_FRAME_COUNT: u32 = 48;
/// Capture timeout sized for one sidebar animation plus recorder startup overhead.
const DEFAULT_ANIMATION_STREAM_TIMEOUT: Duration = Duration::from_millis(1_400);
/// Geometry sample cadence near one display frame for animation proof matching.
const DEFAULT_ANIMATION_SAMPLE_INTERVAL: Duration = Duration::from_millis(16);
/// Maximum tolerated wall-clock mismatch between a PNG frame and its geometry sample.
const DEFAULT_ANIMATION_MAX_SAMPLE_SKEW_MS: u64 = 80;
/// Delay before stopping capture so Mutter has time to attach the stream.
const ANIMATION_RECORDING_ATTACH_DELAY: Duration = Duration::from_millis(30);
/// Stop timeout for the recorder process so stalled capture cleanup cannot hang the proof run.
const ANIMATION_RECORDING_STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// Result of a top-level per-case Rust session launch.
#[derive(Debug)]
pub(crate) struct CaseSessionResult {
    /// Exit status of the internal session child when it exited normally.
    pub(crate) exit_code: Option<i32>,
    /// Whether the internal session exceeded its timeout and was killed.
    pub(crate) timed_out: bool,
    /// Process report artifact for the case.
    pub(crate) report_path: PathBuf,
}

/// Paths to runtime programs used by the live process tree.
#[derive(Clone, Debug)]
struct LivePrograms {
    proof_tool: String,
    dbus_run_session: String,
    pipewire: String,
    wireplumber: String,
    pw_dump: String,
    gsettings: String,
    mutter: String,
}

impl LivePrograms {
    fn current() -> Result<Self, String> {
        let proof_tool = std::env::current_exe()
            .map_err(|error| format!("cannot locate cargo-gtk-proof executable: {error}"))?
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            proof_tool,
            dbus_run_session: "dbus-run-session".to_string(),
            pipewire: "pipewire".to_string(),
            wireplumber: "wireplumber".to_string(),
            pw_dump: "pw-dump".to_string(),
            gsettings: "gsettings".to_string(),
            mutter: "mutter".to_string(),
        })
    }
}

/// Launch one case through a private session child and write a process report.
pub(crate) fn run_case_session(
    case_json: &Path,
    timeout: Duration,
) -> Result<CaseSessionResult, String> {
    let programs = LivePrograms::current()?;
    run_case_session_with_programs(case_json, timeout, &programs)
}

fn run_case_session_with_programs(
    case_json: &Path,
    timeout: Duration,
    programs: &LivePrograms,
) -> Result<CaseSessionResult, String> {
    let case_dir = case_dir_for(case_json)?;
    let runtime = host::RuntimeLayout::prepare(&case_dir)?;
    fs::write(
        case_dir.join("runtime-dir.txt"),
        format!("{}\n", runtime.runtime_dir().display()),
    )
    .map_err(|error| format!("cannot write runtime-dir.txt: {error}"))?;
    let mut env = runtime.process_environment();
    env.push((
        "LUSHTEXT_MUTTER_ARTIFACT_DIR".to_string(),
        case_dir.to_string_lossy().into_owned(),
    ));
    let args = [
        "--".to_string(),
        programs.proof_tool.clone(),
        "run".to_string(),
        "--internal-session".to_string(),
        "--case-json".to_string(),
        case_json.to_string_lossy().into_owned(),
    ];
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let session_log = case_dir.join("session.log");
    let result = process::run_logged_command(
        &programs.dbus_run_session,
        &arg_refs,
        &env,
        &session_log,
        timeout,
    )?;
    let cleanup = runtime.cleanup_runtime_dir();
    let status = if result.timed_out {
        "timed-out"
    } else if result.exit_code == Some(0) {
        "launched"
    } else {
        "failed"
    };
    let report = ProcessReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status,
        stage: "session",
        detail: process_detail(status),
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        logs: vec![artifacts::safe_display_path(&session_log)],
        runtime_cleanup: Some(cleanup),
    };
    let report_path = case_dir.join("process-report.json");
    artifacts::write_json(&report_path, &report)?;
    Ok(CaseSessionResult {
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        report_path,
    })
}

/// Run the D-Bus-session child that supervises PipeWire, WirePlumber, and Mutter.
pub(crate) fn run_internal_session(case_json: &Path) -> Result<(), String> {
    let programs = LivePrograms::current()?;
    run_internal_session_with_programs(case_json, &programs)
}

fn run_internal_session_with_programs(
    case_json: &Path,
    programs: &LivePrograms,
) -> Result<(), String> {
    let case = read_json_value(case_json, "expanded visual case")?;
    let case_dir = case_dir_for(case_json)?;
    let runtime_dir = runtime_dir_from_env()?;
    let mut children = Vec::new();
    let mut logs = Vec::new();
    let result: Result<(), String> = (|| {
        let pipewire_log = case_dir.join("pipewire.log");
        children.push(process::start_logged_child(
            &programs.pipewire,
            &[],
            &[],
            &pipewire_log,
        )?);
        logs.push(artifacts::safe_display_path(&pipewire_log));
        wait_for_pipewire(&runtime_dir, programs, &case_dir)?;

        let wireplumber_log = case_dir.join("wireplumber.log");
        children.push(process::start_logged_child(
            &programs.wireplumber,
            &[],
            &[],
            &wireplumber_log,
        )?);
        logs.push(artifacts::safe_display_path(&wireplumber_log));

        apply_gsettings(&case, programs, &case_dir)?;
        run_mutter_for_case(&case, case_json, programs, &case_dir, &runtime_dir)?;
        Ok(())
    })();

    // Tear children down in reverse launch order so session services outlive
    // the clients that may still be flushing diagnostics through them.
    for child in children.iter_mut().rev() {
        let _ = child.terminate(PROCESS_CLEANUP_TIMEOUT);
    }

    let status = if result.is_ok() { "launched" } else { "failed" };
    let mut report_logs = logs;
    report_logs.push(artifacts::safe_display_path(
        &case_dir.join("mutter-child.log"),
    ));
    let report = ProcessReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status,
        stage: "internal-session",
        detail: match &result {
            Ok(()) => "PipeWire, WirePlumber, and Mutter child launched".to_string(),
            Err(error) => error.clone(),
        },
        exit_code: None,
        timed_out: false,
        logs: report_logs,
        runtime_cleanup: None,
    };
    artifacts::write_json(
        &case_dir.join("internal-session-process-report.json"),
        &report,
    )?;
    result
}

/// Run inside Mutter and launch LushText long enough to prove process ownership.
pub(crate) fn run_mutter_child(case_json: &Path) -> Result<(), String> {
    let programs = LivePrograms::current()?;
    run_mutter_child_with_programs(case_json, &programs)
}

fn run_mutter_child_with_programs(
    case_json: &Path,
    _programs: &LivePrograms,
) -> Result<(), String> {
    let case = read_json_value(case_json, "expanded visual case")?;
    let case_dir = case_dir_for(case_json)?;
    let binary = required_string(&case, "binary")?;
    let fixture = required_string(&case, "fixture")?;
    let app_data_dir = case_dir.join("app-data");
    fs::create_dir_all(&app_data_dir)
        .map_err(|error| format!("cannot create {}: {error}", app_data_dir.display()))?;
    prepare_open_popover_recents(&case, &case_dir, &app_data_dir)?;
    fs::write(case_dir.join("lushtext.stdout"), b"")
        .map_err(|error| format!("cannot create lushtext stdout log: {error}"))?;
    let app_env = lushtext_process_environment(&app_data_dir);
    let log_path = case_dir.join("lushtext.stderr");
    let mut app = process::start_logged_child(binary, &[fixture], &app_env, &log_path)?;
    fs::write(case_dir.join("app.pid"), format!("{}\n", app.id()))
        .map_err(|error| format!("cannot write app pid: {error}"))?;

    let result = if let Some(code) = app.wait_for_exit(APP_LAUNCH_SETTLE)? {
        Err(format!("LushText exited during launch with status {code}"))
    } else {
        let client = wait_for_initial_automation(&case_dir)?;
        capture_case_steps(&client, &case, &case_dir)
    };
    let _ = app.terminate(PROCESS_CLEANUP_TIMEOUT);
    let status = if result.is_ok() { "launched" } else { "failed" };
    let report = ProcessReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status,
        stage: "mutter-child",
        detail: result.as_ref().err().map_or_else(
            || "LushText launched under headless Mutter".to_string(),
            Clone::clone,
        ),
        exit_code: None,
        timed_out: false,
        logs: vec![
            artifacts::safe_display_path(&case_dir.join("lushtext.stdout")),
            artifacts::safe_display_path(&log_path),
            artifacts::safe_display_path(&case_dir.join("before-gst.log")),
            artifacts::safe_display_path(&case_dir.join("after-gst.log")),
        ],
        runtime_cleanup: None,
    };
    artifacts::write_json(&case_dir.join("mutter-child-process-report.json"), &report)?;
    result
}

fn wait_for_initial_automation(case_dir: &Path) -> Result<automation::AutomationClient, String> {
    let client = automation::AutomationClient::connect_with_retry(Duration::from_secs(15))?;
    for predicate in ["file-open-complete", "visual-geometry-settled"] {
        let wait = client.wait_for_ready(predicate, 5_000)?;
        append_automation_wait(case_dir, &wait)?;
        if !wait.ok {
            return Err(format!(
                "Automation1 WaitForReady({predicate}) failed: {}: {}",
                wait.status, wait.detail
            ));
        }
    }
    Ok(client)
}

fn append_automation_wait(case_dir: &Path, wait: &automation::ReadinessWait) -> Result<(), String> {
    append_automation_wait_line(
        case_dir,
        &format!(
            "predicate={} ok={} status={} detail={}",
            wait.predicate, wait.ok, wait.status, wait.detail
        ),
    )
}

fn append_automation_wait_line(case_dir: &Path, line: &str) -> Result<(), String> {
    use std::io::Write;

    let path = case_dir.join("automation-waits.txt");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    writeln!(file, "{line}").map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn capture_case_steps(
    client: &automation::AutomationClient,
    case: &Value,
    case_dir: &Path,
) -> Result<(), String> {
    prepare_sidebar_target_state(client, case, case_dir, "before")?;
    let prepared_actions = prepare_case_before_primary_action(client, case, case_dir)?;
    wait_for_case_final_geometry(client, case, case_dir, "before")?;
    let before = capture_step(client, case_dir, "before")?;
    let action =
        run_case_action_with_optional_animation(client, case, case_dir, &before, prepared_actions)?;
    let wait = client.wait_for_ready("visual-geometry-settled", 5_000)?;
    append_automation_wait(case_dir, &wait)?;
    if !wait.ok {
        return Err(format!(
            "Automation1 visual-geometry-settled after action failed: {}: {}",
            wait.status, wait.detail
        ));
    }
    wait_for_case_final_geometry(client, case, case_dir, "after")?;
    let after = capture_step(client, case_dir, "after")?;
    let workflow_events_path = case_dir.join("workflow-events.json");
    artifacts::write_json(&workflow_events_path, &client.workflow_events()?)?;
    artifacts::write_json(
        &case_dir.join("same-session-captures.json"),
        &serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "status": "captured",
            "same_session": same_session_metadata(case, case_dir),
            "before": before.artifact,
            "action": action,
            "after": after.artifact,
            "workflow_events": artifacts::safe_display_path(&workflow_events_path),
        }),
    )
}

fn prepare_sidebar_target_state(
    client: &automation::AutomationClient,
    case: &Value,
    case_dir: &Path,
    step: &str,
) -> Result<(), String> {
    if scenario_type(case)? != "minimap-sidebar" {
        return Ok(());
    }

    let target_visible = sidebar_target_visible(case, step)?;
    let action = client.activate_window_action(
        "set-sidebar-visible",
        automation::ActionParameter::Bool(target_visible),
    )?;
    append_automation_wait_line(
        case_dir,
        &format!(
            "predicate=prepare-sidebar-target:{step} ok=true status=action detail={}",
            action.detail
        ),
    )?;
    let wait = client.wait_for_ready("visual-geometry-settled", 5_000)?;
    append_automation_wait(case_dir, &wait)?;
    if !wait.ok {
        return Err(format!(
            "Automation1 visual-geometry-settled preparing sidebar {step} failed: {}: {}",
            wait.status, wait.detail
        ));
    }
    Ok(())
}

fn wait_for_case_final_geometry(
    client: &automation::AutomationClient,
    case: &Value,
    case_dir: &Path,
    step: &str,
) -> Result<(), String> {
    if scenario_type(case)? != "minimap-sidebar" {
        return Ok(());
    }
    let target_visible = sidebar_target_visible(case, step)?;
    let deadline = Instant::now() + FINAL_GEOMETRY_TIMEOUT;
    let mut samples = Vec::new();
    let mut stable_signatures = Vec::new();
    let mut last_detail = "no samples collected".to_string();

    while Instant::now() < deadline {
        let snapshot = client.snapshot()?;
        let (matches, detail) = sidebar_final_geometry_matches(
            &snapshot,
            target_visible,
            compact_overlay_allowed(case),
        );
        last_detail.clone_from(&detail);
        samples.push(final_geometry_sample(
            &snapshot,
            target_visible,
            matches,
            &detail,
        ));
        if samples.len() > 64 {
            samples.remove(0);
        }
        if matches {
            let signature = final_geometry_signature(&snapshot);
            if stable_signatures
                .last()
                .is_some_and(|previous| previous != &signature)
            {
                stable_signatures.clear();
            }
            stable_signatures.push(signature);
            if stable_signatures.len() >= FINAL_GEOMETRY_SAMPLE_COUNT {
                write_final_geometry_samples(case_dir, step, target_visible, &samples, "passed")?;
                return Ok(());
            }
        } else {
            stable_signatures.clear();
        }
        thread::sleep(FINAL_GEOMETRY_SAMPLE_INTERVAL);
    }

    write_final_geometry_samples(case_dir, step, target_visible, &samples, "failed")?;
    Err(format!(
        "sidebar final geometry did not settle for {step}: {last_detail}"
    ))
}

/// One screenshot capture plus the in-memory snapshot used by follow-up proof steps.
struct StepCapture {
    artifact: serde_json::Value,
    snapshot: serde_json::Value,
    snapshot_path: PathBuf,
    screenshot_path: PathBuf,
}

fn capture_step(
    client: &automation::AutomationClient,
    case_dir: &Path,
    step: &str,
) -> Result<StepCapture, String> {
    let snapshot = client.snapshot()?;
    assert_snapshot_ready_for_capture(&snapshot)?;
    let snapshot_path = case_dir.join(format!("{step}-geometry-snapshot.json"));
    artifacts::write_json(&snapshot_path, &snapshot)?;
    let screenshot_path = case_dir.join(format!("{step}.png"));
    let capture_report_path = case_dir.join(format!("{step}-capture-report.json"));
    capture::capture_monitor_png(
        &screenshot_path,
        &case_dir.join(format!("{step}-gst.log")),
        &capture_report_path,
    )?;
    let artifact = serde_json::json!({
        "step": step,
        "snapshot": artifacts::safe_display_path(&snapshot_path),
        "screenshot": artifacts::safe_display_path(&screenshot_path),
        "capture_report": artifacts::safe_display_path(&capture_report_path),
    });
    Ok(StepCapture {
        artifact,
        snapshot,
        snapshot_path,
        screenshot_path,
    })
}

fn same_session_metadata(case: &Value, case_dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "required": true,
        "process": "single-mutter-child",
        "app_pid": fs::read_to_string(case_dir.join("app.pid"))
            .ok()
            .map(|pid| pid.trim().to_string()),
        "compositor_session": "single-headless-mutter",
        "renderer": "cairo",
        "theme": case.get("color_scheme").and_then(Value::as_str).unwrap_or("default"),
        "scale_factor": 1,
        "font_configuration": "isolated-gsettings-keyfile",
        "fixture": case.get("fixture").and_then(Value::as_str).unwrap_or(""),
        "window_size": case.get("size").cloned().unwrap_or_default(),
    })
}

fn run_case_action_with_optional_animation(
    client: &automation::AutomationClient,
    case: &Value,
    case_dir: &Path,
    before: &StepCapture,
    prepared_actions: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let mut actions = prepared_actions;
    let mut action_error = None;
    let animation_report = if let Some(config) = animation_config(case)? {
        let result = capture_animation_stream(client, case, case_dir, before, &config)?;
        actions.push(action_row(&result.action));
        if result.status != "passed" {
            action_error = Some(format!(
                "animation-frame proof failed: {}",
                result
                    .failure_reason
                    .unwrap_or_else(|| "animation-report status was not passed".to_string())
            ));
        }
        Some(artifacts::safe_display_path(
            &case_dir.join("animation/animation-report.json"),
        ))
    } else {
        let primary = activate_primary_case_action(client, case)?;
        actions.push(action_row(&primary));
        None
    };

    let action_name = primary_action_name(case)?;
    artifacts::write_json(
        &case_dir.join("automation-actions.json"),
        &serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "actions": actions,
        }),
    )?;
    if let Some(error) = action_error {
        return Err(error);
    }
    Ok(serde_json::json!({
        "action": action_name,
        "actions": actions,
        "artifact": artifacts::safe_display_path(&case_dir.join("automation-actions.json")),
        "animation_report": animation_report,
    }))
}

fn prepare_case_before_primary_action(
    client: &automation::AutomationClient,
    case: &Value,
    case_dir: &Path,
) -> Result<Vec<serde_json::Value>, String> {
    let mut actions = Vec::new();
    if scenario_type(case)? != "minimap-sidebar"
        || case.get("viewport_position").and_then(Value::as_str) != Some("mid")
    {
        return Ok(actions);
    }

    actions.push(action_row(&client.activate_window_action(
        "set-search-query",
        automation::ActionParameter::String("line 0180"),
    )?));
    let search_wait = client.wait_for_ready("search-complete", 5_000)?;
    append_automation_wait(case_dir, &search_wait)?;
    if !search_wait.ok {
        return Err(format!(
            "Automation1 search-complete failed: {}: {}",
            search_wait.status, search_wait.detail
        ));
    }
    wait_for_snapshot_predicate(
        client,
        case_dir,
        "editor search query 'line 0180' with one match",
        Duration::from_millis(5_000),
        |snapshot| editor_search_has_one_match(snapshot, "line 0180"),
    )?;
    actions.push(action_row(&client.activate_window_action(
        "next-match",
        automation::ActionParameter::None,
    )?));
    let settle_wait = client.wait_for_ready("visual-geometry-settled", 5_000)?;
    append_automation_wait(case_dir, &settle_wait)?;
    if !settle_wait.ok {
        return Err(format!(
            "Automation1 visual-geometry-settled after search failed: {}: {}",
            settle_wait.status, settle_wait.detail
        ));
    }
    wait_for_snapshot_predicate(
        client,
        case_dir,
        "source-view scrolled to middle fixture line",
        Duration::from_millis(5_000),
        source_view_scrolled,
    )?;
    Ok(actions)
}

fn wait_for_snapshot_predicate<F>(
    client: &automation::AutomationClient,
    case_dir: &Path,
    description: &str,
    timeout: Duration,
    predicate: F,
) -> Result<(), String>
where
    F: Fn(&Value) -> bool,
{
    let deadline = Instant::now() + timeout;
    let mut last_snapshot_state = "not sampled".to_string();
    while Instant::now() < deadline {
        let snapshot = client.snapshot()?;
        if predicate(&snapshot) {
            append_snapshot_predicate_wait(case_dir, description, true, "ready")?;
            return Ok(());
        }
        last_snapshot_state = snapshot_predicate_state(&snapshot);
        thread::sleep(Duration::from_millis(100));
    }
    append_snapshot_predicate_wait(case_dir, description, false, &last_snapshot_state)?;
    Err(format!(
        "Automation1 snapshot predicate timed out: {description}: {last_snapshot_state}"
    ))
}

fn editor_search_has_one_match(snapshot: &Value, query: &str) -> bool {
    snapshot.pointer("/window/search/editor_search_visible") == Some(&Value::Bool(true))
        && snapshot
            .pointer("/window/search/editor_query")
            .and_then(Value::as_str)
            == Some(query)
        && snapshot
            .pointer("/window/search/editor_match_count")
            .and_then(Value::as_u64)
            == Some(1)
}

fn source_view_scrolled(snapshot: &Value) -> bool {
    visual_geometry(snapshot)
        .and_then(|geometry| geometry.get("scroll_anchors"))
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter().any(|row| {
                row.get("name").and_then(Value::as_str) == Some("source-view")
                    && row
                        .get("y_value_milli")
                        .and_then(Value::as_i64)
                        .is_some_and(|value| value > 0)
            })
        })
}

fn snapshot_predicate_state(snapshot: &Value) -> String {
    let search = snapshot
        .pointer("/window/search")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let scroll = visual_geometry(snapshot)
        .and_then(|geometry| geometry.get("scroll_anchors"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::json!({
        "search": search,
        "scroll_anchors": scroll,
    })
    .to_string()
}

fn append_snapshot_predicate_wait(
    case_dir: &Path,
    description: &str,
    ok: bool,
    detail: &str,
) -> Result<(), String> {
    append_automation_wait_line(
        case_dir,
        &format!(
            "predicate=snapshot:{description} ok={ok} status={} detail={detail}",
            if ok { "ready" } else { "timeout" }
        ),
    )
}

fn activate_primary_case_action(
    client: &automation::AutomationClient,
    case: &Value,
) -> Result<automation::AutomationArtifactRow, String> {
    client.activate_window_action(
        primary_action_name(case)?,
        automation::ActionParameter::None,
    )
}

struct AnimationActionResult {
    action: automation::AutomationArtifactRow,
    status: String,
    failure_reason: Option<String>,
}

fn capture_animation_stream(
    client: &automation::AutomationClient,
    case: &Value,
    case_dir: &Path,
    before: &StepCapture,
    config: &AnimationConfig,
) -> Result<AnimationActionResult, String> {
    let animation_dir = case_dir.join("animation");
    let frames_dir = animation_dir.join("frames");
    let crops_dir = animation_dir.join("crops");
    fs::create_dir_all(&frames_dir)
        .map_err(|error| format!("cannot create {}: {error}", frames_dir.display()))?;
    fs::create_dir_all(&crops_dir)
        .map_err(|error| format!("cannot create {}: {error}", crops_dir.display()))?;

    let baseline = detect_animation_baseline(before, &config.anchor_specs, &crops_dir)?;
    let wall_started = SystemTime::now();
    let started = Instant::now();
    let frame_pattern = frames_dir.join("stream-frame-%03d.png");
    let mut recording = capture::start_monitor_frame_recording(
        config.stream_frame_count,
        &frame_pattern,
        &animation_dir.join("stream-gst.log"),
    )?;

    thread::sleep(ANIMATION_RECORDING_ATTACH_DELAY);
    let action_started_ms = duration_millis(started.elapsed());
    let action = activate_primary_case_action(client, case)?;

    let mut samples = Vec::new();
    let deadline = started + config.stream_timeout;
    while Instant::now() < deadline {
        let snapshot = client.snapshot()?;
        samples.push(animation_geometry_sample(
            &snapshot,
            duration_millis(started.elapsed()),
        ));
        if recording.has_exited()? {
            break;
        }
        thread::sleep(config.sample_interval);
    }
    recording.stop(ANIMATION_RECORDING_STOP_TIMEOUT)?;

    let frames = frame_paths(&frames_dir)?;
    let report = build_animation_report(AnimationReportInput {
        case,
        config,
        baseline: &baseline,
        samples: &samples,
        frame_paths: &frames,
        wall_started,
        action_started_ms,
        crops_dir: &crops_dir,
    })?;
    let status = report
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("failed")
        .to_string();
    let failure_reason = report
        .get("failure_reason")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    artifacts::write_artifact(
        &animation_dir.join("animation-report.json"),
        artifacts::ProofArtifactKind::AnimationReport,
        &report,
    )?;
    Ok(AnimationActionResult {
        action,
        status,
        failure_reason,
    })
}

#[derive(Clone, Copy)]
struct AnimationReportInput<'a> {
    case: &'a Value,
    config: &'a AnimationConfig,
    baseline: &'a AnimationBaseline,
    samples: &'a [GeometrySample],
    frame_paths: &'a [PathBuf],
    wall_started: SystemTime,
    action_started_ms: u64,
    crops_dir: &'a Path,
}

fn build_animation_report(input: AnimationReportInput<'_>) -> Result<Value, String> {
    let AnimationReportInput {
        case,
        config,
        baseline,
        samples,
        frame_paths,
        wall_started,
        action_started_ms,
        crops_dir,
    } = input;
    let mut frames = Vec::new();
    let mut failures = Vec::new();
    let mut max_row_drift = 0i64;
    let mut max_sample_skew_observed_ms = None;
    let mut status = "passed";
    let mut failure_reason = None;

    if frame_paths.is_empty() {
        status = "failed";
        failure_reason = Some("stream animation capture produced no PNG frames".to_string());
    }

    for (frame_index, frame_path) in frame_paths.iter().enumerate() {
        let elapsed_ms = animation_frame_elapsed_ms(
            frame_path,
            wall_started,
            frame_index,
            config.sample_interval,
        );
        let (sample, sample_skew_ms) =
            animation_sample_for_frame(samples, elapsed_ms, config.max_sample_skew_ms);
        if let Some(sample_skew_ms) = sample_skew_ms {
            max_sample_skew_observed_ms = Some(
                max_sample_skew_observed_ms
                    .map_or(sample_skew_ms, |value: u64| value.max(sample_skew_ms)),
            );
        }
        let snapshot = sample.map_or_else(
            || baseline.snapshot.clone(),
            |sample| sample.snapshot.clone(),
        );
        let snapshot_path =
            frame_path.with_file_name(format!("frame-{frame_index:03}-geometry-snapshot.json"));
        artifacts::write_json(&snapshot_path, &snapshot)?;
        let frame = if let Some(sample) = sample {
            evaluate_animation_frame(AnimationFrameInput {
                case,
                frame_index,
                elapsed_ms,
                snapshot: &snapshot,
                frame_path,
                snapshot_path: &snapshot_path,
                baseline_snapshot: &baseline.snapshot,
                baseline: &baseline.rows,
                anchor_specs: &config.anchor_specs,
                crops_dir,
                max_screen_y_delta: config.max_screen_y_delta,
                mapped_sample_elapsed_ms: Some(sample.elapsed_ms),
                sample_skew_ms,
                sidebar_phase: &sample.sidebar_phase,
            })?
        } else {
            stale_animation_frame_report(
                frame_index,
                elapsed_ms,
                frame_path,
                &snapshot_path,
                config.max_sample_skew_ms,
                sample_skew_ms,
            )
        };
        max_row_drift = max_row_drift.max(
            frame
                .get("max_row_drift")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        );
        if frame.get("status").and_then(Value::as_str) != Some("passed") {
            status = "failed";
            failures.push(frame.clone());
        }
        frames.push(frame);
    }

    let intermediate_geometry_sample_count = samples
        .iter()
        .filter(|sample| sample.sidebar_phase == "intermediate")
        .count();
    let mapped_intermediate_frame_count = frames
        .iter()
        .filter(|frame| frame.get("sidebar_phase").and_then(Value::as_str) == Some("intermediate"))
        .count();
    if config.require_intermediate_geometry && intermediate_geometry_sample_count == 0 {
        status = "failed";
        failure_reason =
            Some("animation sampling did not observe intermediate sidebar geometry".to_string());
    } else if config.require_intermediate_geometry && mapped_intermediate_frame_count == 0 {
        status = "failed";
        failure_reason = Some(
            "animation stream did not capture a PNG frame mapped to intermediate sidebar geometry"
                .to_string(),
        );
    }
    if failure_reason.is_none() {
        failure_reason = animation_failure_reason(&failures);
    }

    Ok(serde_json::json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "status": status,
        "capture_mode": "stream",
        "invariant_id": config.invariant_id,
        "stream_frame_count": config.stream_frame_count,
        "stream_timeout_ms": duration_millis(config.stream_timeout),
        "sample_interval_ms": duration_millis(config.sample_interval),
        "max_sample_skew_ms": config.max_sample_skew_ms,
        "max_sample_skew_observed_ms": max_sample_skew_observed_ms,
        "action_started_ms": action_started_ms,
        "sampled_frame_count": frames.len(),
        "geometry_sample_count": samples.len(),
        "intermediate_geometry_sample_count": intermediate_geometry_sample_count,
        "mapped_intermediate_frame_count": mapped_intermediate_frame_count,
        "phase_sequence": animation_phase_sequence(samples),
        "max_screen_y_delta": config.max_screen_y_delta,
        "max_row_drift": max_row_drift,
        "baseline": baseline.report,
        "geometry_samples": samples.iter().map(bounded_animation_geometry_sample).collect::<Vec<_>>(),
        "frames": frames,
        "failures": summarize_animation_failures(&failures),
        "failure_reason": failure_reason,
        "animation_frame_evidence": animation_evidence_from_report(
            status,
            config,
            frames.len(),
            mapped_intermediate_frame_count,
            max_sample_skew_observed_ms,
            &frames,
        ),
    }))
}

fn animation_evidence_from_report(
    status: &str,
    config: &AnimationConfig,
    sampled_frame_count: usize,
    mapped_intermediate_frame_count: usize,
    max_sample_skew_observed_ms: Option<u64>,
    frames: &[Value],
) -> Value {
    serde_json::json!({
        "status": status,
        "capture_mode": "stream",
        "invariant_id": config.invariant_id,
        "sampled_frame_count": sampled_frame_count,
        "mapped_intermediate_frame_count": mapped_intermediate_frame_count,
        "max_sample_skew_ms": config.max_sample_skew_ms,
        "max_sample_skew_observed_ms": max_sample_skew_observed_ms,
        "frames": frames,
    })
}

#[derive(Clone, Debug)]
struct AnimationConfig {
    invariant_id: String,
    stream_frame_count: u32,
    stream_timeout: Duration,
    sample_interval: Duration,
    max_sample_skew_ms: u64,
    max_screen_y_delta: i64,
    require_intermediate_geometry: bool,
    anchor_specs: Vec<AnimationAnchorSpec>,
}

#[derive(Clone, Debug)]
struct AnimationAnchorSpec {
    name: String,
    detector: String,
    min_pixels: usize,
    crop_surface: Option<String>,
    crop_insets: Option<Insets>,
    source: Value,
}

#[derive(Clone, Debug)]
struct GeometrySample {
    elapsed_ms: u64,
    sidebar_phase: String,
    snapshot: Value,
}

#[derive(Clone, Debug)]
struct AnimationBaseline {
    report: Value,
    snapshot: Value,
    rows: HashMap<String, BaselineAnchor>,
}

#[derive(Clone, Copy, Debug)]
struct BaselineAnchor {
    row_y: Option<i32>,
}

fn animation_config(case: &Value) -> Result<Option<AnimationConfig>, String> {
    let Some(config) = case
        .pointer("/manifest/animation_sampling")
        .and_then(Value::as_object)
    else {
        return Ok(None);
    };
    if !config
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Ok(None);
    }
    let capture_mode = config
        .get("capture_mode")
        .and_then(Value::as_str)
        .unwrap_or("stream");
    if capture_mode != "stream" {
        return Err(format!(
            "unsupported animation capture_mode {capture_mode}; Rust live proof requires stream"
        ));
    }
    let required_anchors = config
        .get("required_anchors")
        .and_then(Value::as_array)
        .map(|anchors| {
            anchors
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let anchor_specs = animation_anchor_specs(case, &required_anchors)?;
    Ok(Some(AnimationConfig {
        invariant_id: config
            .get("invariant_id")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_ANIMATION_INVARIANT_ID)
            .to_string(),
        stream_frame_count: config
            .get("stream_frame_count")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(DEFAULT_ANIMATION_STREAM_FRAME_COUNT),
        stream_timeout: Duration::from_millis(
            config
                .get("stream_timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| duration_millis(DEFAULT_ANIMATION_STREAM_TIMEOUT)),
        ),
        sample_interval: Duration::from_millis(
            config
                .get("sample_interval_ms")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| duration_millis(DEFAULT_ANIMATION_SAMPLE_INTERVAL)),
        ),
        max_sample_skew_ms: config
            .get("max_sample_skew_ms")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_ANIMATION_MAX_SAMPLE_SKEW_MS),
        max_screen_y_delta: config
            .get("max_screen_y_delta")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        require_intermediate_geometry: config
            .get("require_intermediate_geometry")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        anchor_specs,
    }))
}

fn animation_anchor_specs(
    case: &Value,
    required_anchors: &[String],
) -> Result<Vec<AnimationAnchorSpec>, String> {
    let specs = case
        .pointer("/manifest/pixel_anchors")
        .and_then(Value::as_array)
        .ok_or_else(|| "animation_sampling requires manifest pixel_anchors".to_string())?;
    let selected = specs
        .iter()
        .filter(|spec| {
            required_anchors.is_empty()
                || spec
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| required_anchors.iter().any(|required| required == name))
        })
        .map(parse_animation_anchor_spec)
        .collect::<Result<Vec<_>, _>>()?;
    let selected_names = selected
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();
    let missing = required_anchors
        .iter()
        .filter(|required| !selected_names.iter().any(|selected| selected == required))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "animation_sampling references unknown pixel anchors: {}",
            missing.join(", ")
        ));
    }
    if selected.is_empty() {
        return Err("animation_sampling selected no pixel anchors".to_string());
    }
    Ok(selected)
}

fn parse_animation_anchor_spec(value: &Value) -> Result<AnimationAnchorSpec, String> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "pixel anchor is missing name".to_string())?
        .to_string();
    let detector = value
        .get("detector")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("pixel anchor {name} is missing detector"))?
        .to_string();
    let min_pixels = value
        .get("min_pixels")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1);
    let crop_surface = value
        .get("crop_surface")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let crop_insets = value.get("crop_insets").and_then(parse_insets);
    Ok(AnimationAnchorSpec {
        name,
        detector,
        min_pixels,
        crop_surface,
        crop_insets,
        source: value.clone(),
    })
}

fn parse_insets(value: &Value) -> Option<Insets> {
    Some(Insets::from_value(value))
}

fn detect_animation_baseline(
    before: &StepCapture,
    anchor_specs: &[AnimationAnchorSpec],
    crops_dir: &Path,
) -> Result<AnimationBaseline, String> {
    let mut report_rows = serde_json::Map::new();
    let mut rows = HashMap::new();
    for spec in anchor_specs {
        let crop_path = crops_dir.join(format!(
            "{}-baseline-anchor.png",
            safe_anchor_name(&spec.name)
        ));
        let detection = png::detect_pixel_anchor_in_file(
            &before.screenshot_path,
            &spec.name,
            rect_for_anchor_search(&before.snapshot, spec)?,
            &spec.detector,
            spec.min_pixels,
            Some(&crop_path),
        )?;
        rows.insert(
            spec.name.clone(),
            BaselineAnchor {
                row_y: detection.row_y,
            },
        );
        report_rows.insert(
            spec.name.clone(),
            serde_json::json!({
                "spec": spec.source,
                "status": detection.status,
                "row_y": detection.row_y,
                "crop": artifacts::safe_display_path(&crop_path),
                "detection": detection,
            }),
        );
    }
    Ok(AnimationBaseline {
        report: serde_json::json!({
            "screenshot": artifacts::safe_display_path(&before.screenshot_path),
            "snapshot": artifacts::safe_display_path(&before.snapshot_path),
            "anchors": report_rows,
            "relationships": [],
        }),
        snapshot: before.snapshot.clone(),
        rows,
    })
}

#[derive(Clone, Copy)]
struct AnimationFrameInput<'a> {
    case: &'a Value,
    frame_index: usize,
    elapsed_ms: u64,
    snapshot: &'a Value,
    frame_path: &'a Path,
    snapshot_path: &'a Path,
    baseline_snapshot: &'a Value,
    baseline: &'a HashMap<String, BaselineAnchor>,
    anchor_specs: &'a [AnimationAnchorSpec],
    crops_dir: &'a Path,
    max_screen_y_delta: i64,
    mapped_sample_elapsed_ms: Option<u64>,
    sample_skew_ms: Option<u64>,
    sidebar_phase: &'a str,
}

fn evaluate_animation_frame(input: AnimationFrameInput<'_>) -> Result<Value, String> {
    let AnimationFrameInput {
        case,
        frame_index,
        elapsed_ms,
        snapshot,
        frame_path,
        snapshot_path,
        baseline_snapshot,
        baseline,
        anchor_specs,
        crops_dir,
        max_screen_y_delta,
        mapped_sample_elapsed_ms,
        sample_skew_ms,
        sidebar_phase,
    } = input;
    let mut status = "passed";
    let mut anchors = Vec::new();
    let mut app_vs_rendered_disagreements = Vec::new();
    let mut max_row_drift = 0i64;
    for spec in anchor_specs {
        let crop_path = crops_dir.join(format!(
            "{}-frame-{frame_index:03}-anchor.png",
            safe_anchor_name(&spec.name)
        ));
        let detection = png::detect_pixel_anchor_in_file(
            frame_path,
            &spec.name,
            rect_for_anchor_search(snapshot, spec)?,
            &spec.detector,
            spec.min_pixels,
            Some(&crop_path),
        )?;
        let baseline_row_y = baseline.get(&spec.name).and_then(|row| row.row_y);
        let row_delta_from_baseline = detection
            .row_y
            .zip(baseline_row_y)
            .map(|(frame_row, baseline_row)| i64::from((frame_row - baseline_row).abs()));
        if let Some(delta) = row_delta_from_baseline {
            max_row_drift = max_row_drift.max(delta);
        }
        let app_geometry = app_pixel_anchor_geometry(baseline_snapshot, snapshot, &spec.name);
        let mut row_status = "passed";
        let mut diagnostics = Vec::new();
        if detection.status != "passed"
            || row_delta_from_baseline.is_none()
            || row_delta_from_baseline.is_some_and(|delta| delta > max_screen_y_delta)
        {
            row_status = "failed";
            status = "failed";
            if let Some(app_delta) = app_geometry
                .as_ref()
                .and_then(|geometry| geometry.get("screen_y_delta"))
                .and_then(Value::as_i64)
                && app_delta <= max_screen_y_delta
                && row_delta_from_baseline.is_some_and(|delta| delta > max_screen_y_delta)
            {
                let diagnostic = serde_json::json!({
                    "name": spec.name,
                    "status": "animation-app-vs-rendered-anchor-disagreement",
                    "app_screen_y_delta": app_delta,
                    "rendered_screen_y_delta": row_delta_from_baseline,
                    "max_screen_y_delta": max_screen_y_delta,
                });
                diagnostics.push(diagnostic.clone());
                app_vs_rendered_disagreements.push(diagnostic);
            }
        }
        anchors.push(serde_json::json!({
            "name": spec.name,
            "detector": spec.detector,
            "status": row_status,
            "baseline_row_y": baseline_row_y,
            "frame_row_y": detection.row_y,
            "row_delta_from_baseline": row_delta_from_baseline,
            "max_screen_y_delta": max_screen_y_delta,
            "detection": detection,
            "crop": artifacts::safe_display_path(&crop_path),
            "app_geometry": app_geometry,
            "diagnostics": diagnostics,
        }));
    }
    let relationships = evaluate_animation_relationships(case, baseline, &anchors);
    if relationships
        .iter()
        .any(|row| row.get("status").and_then(Value::as_str) != Some("passed"))
    {
        status = "failed";
    }
    Ok(serde_json::json!({
        "frame_index": frame_index,
        "elapsed_ms": elapsed_ms,
        "mapped_sample_elapsed_ms": mapped_sample_elapsed_ms,
        "sample_skew_ms": sample_skew_ms,
        "sidebar_phase": sidebar_phase,
        "status": status,
        "screenshot": artifacts::safe_display_path(frame_path),
        "snapshot": artifacts::safe_display_path(snapshot_path),
        "max_row_drift": max_row_drift,
        "anchors": anchors,
        "relationships": relationships,
        "app_vs_rendered_disagreements": app_vs_rendered_disagreements,
        "surfaces": selected_surface_rows(snapshot),
        "native_minimap": visual_geometry(snapshot).and_then(|geometry| geometry.get("native_minimap")).cloned(),
        "scroll_anchors": visual_geometry(snapshot).and_then(|geometry| geometry.get("scroll_anchors")).cloned().unwrap_or_default(),
    }))
}

fn stale_animation_frame_report(
    frame_index: usize,
    elapsed_ms: u64,
    frame_path: &Path,
    snapshot_path: &Path,
    max_sample_skew_ms: u64,
    sample_skew_ms: Option<u64>,
) -> Value {
    serde_json::json!({
        "frame_index": frame_index,
        "elapsed_ms": elapsed_ms,
        "mapped_sample_elapsed_ms": Value::Null,
        "sample_skew_ms": sample_skew_ms,
        "sidebar_phase": "unmapped",
        "status": "failed",
        "failure_reason": "stale-frame-geometry-pairing",
        "screenshot": artifacts::safe_display_path(frame_path),
        "snapshot": artifacts::safe_display_path(snapshot_path),
        "max_sample_skew_ms": max_sample_skew_ms,
        "max_row_drift": 0,
        "anchors": [],
        "relationships": [],
        "app_vs_rendered_disagreements": [],
        "surfaces": [],
        "native_minimap": Value::Null,
        "scroll_anchors": [],
    })
}

fn animation_sample_for_frame(
    samples: &[GeometrySample],
    frame_elapsed_ms: u64,
    max_sample_skew_ms: u64,
) -> (Option<&GeometrySample>, Option<u64>) {
    let Some(nearest) = samples
        .iter()
        .min_by_key(|sample| sample.elapsed_ms.abs_diff(frame_elapsed_ms))
    else {
        return (None, None);
    };
    let skew = nearest.elapsed_ms.abs_diff(frame_elapsed_ms);
    if skew > max_sample_skew_ms {
        (None, Some(skew))
    } else {
        (Some(nearest), Some(skew))
    }
}

fn animation_geometry_sample(snapshot: &Value, elapsed_ms: u64) -> GeometrySample {
    let sidebar = surface_box(snapshot, "workspace-sidebar-transition")
        .or_else(|_| surface_box(snapshot, "workspace-sidebar"));
    let sidebar_phase = match sidebar {
        Ok(sidebar) if sidebar.x == 0 => "shown",
        Ok(sidebar) if sidebar.x == -sidebar.width => "hidden",
        Ok(sidebar) if -sidebar.width < sidebar.x && sidebar.x < 0 => "intermediate",
        Ok(_) => "unknown",
        Err(_) => "unavailable",
    }
    .to_string();
    GeometrySample {
        elapsed_ms,
        sidebar_phase,
        snapshot: snapshot.clone(),
    }
}

fn bounded_animation_geometry_sample(sample: &GeometrySample) -> Value {
    serde_json::json!({
        "elapsed_ms": sample.elapsed_ms,
        "sidebar_phase": sample.sidebar_phase,
        "surfaces": selected_surface_rows(&sample.snapshot),
        "native_minimap": visual_geometry(&sample.snapshot).and_then(|geometry| geometry.get("native_minimap")).cloned(),
        "scroll_anchors": visual_geometry(&sample.snapshot).and_then(|geometry| geometry.get("scroll_anchors")).cloned().unwrap_or_default(),
    })
}

fn animation_phase_sequence(samples: &[GeometrySample]) -> Vec<String> {
    let mut phases = Vec::new();
    for sample in samples {
        if phases.last() != Some(&sample.sidebar_phase) {
            phases.push(sample.sidebar_phase.clone());
        }
    }
    phases
}

fn summarize_animation_failures(failures: &[Value]) -> Vec<Value> {
    failures
        .iter()
        .map(|failure| {
            let anchors = failure
                .get("anchors")
                .and_then(Value::as_array)
                .map(|anchors| {
                    anchors
                        .iter()
                        .filter(|anchor| anchor.get("status").and_then(Value::as_str) != Some("passed"))
                        .map(|anchor| {
                            serde_json::json!({
                                "name": anchor.get("name"),
                                "baseline_row_y": anchor.get("baseline_row_y"),
                                "frame_row_y": anchor.get("frame_row_y"),
                                "row_delta_from_baseline": anchor.get("row_delta_from_baseline"),
                                "crop": anchor.get("crop"),
                                "diagnostics": anchor.get("diagnostics").cloned().unwrap_or_default(),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let relationships = failure
                .get("relationships")
                .and_then(Value::as_array)
                .map(|relationships| {
                    relationships
                        .iter()
                        .filter(|row| row.get("status").and_then(Value::as_str) != Some("passed"))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            serde_json::json!({
                "frame_index": failure.get("frame_index"),
                "elapsed_ms": failure.get("elapsed_ms"),
                "mapped_sample_elapsed_ms": failure.get("mapped_sample_elapsed_ms"),
                "sample_skew_ms": failure.get("sample_skew_ms"),
                "sidebar_phase": failure.get("sidebar_phase"),
                "failure_reason": failure.get("failure_reason"),
                "screenshot": failure.get("screenshot"),
                "snapshot": failure.get("snapshot"),
                "max_row_drift": failure.get("max_row_drift"),
                "anchors": anchors,
                "relationships": relationships,
            })
        })
        .collect()
}

fn animation_failure_reason(failures: &[Value]) -> Option<String> {
    let first = failures.first()?;
    Some(format!(
        "frame {} at {}ms drifted {}px",
        first
            .get("frame_index")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        first
            .get("elapsed_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        first
            .get("max_row_drift")
            .and_then(Value::as_i64)
            .unwrap_or_default()
    ))
}

fn evaluate_animation_relationships(
    case: &Value,
    baseline: &HashMap<String, BaselineAnchor>,
    anchors: &[Value],
) -> Vec<Value> {
    let by_name = anchors
        .iter()
        .filter_map(|anchor| Some((anchor.get("name")?.as_str()?.to_string(), anchor.clone())))
        .collect::<HashMap<_, _>>();
    let Some(specs) = case
        .pointer("/manifest/relative_pixel_anchors")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    specs
        .iter()
        .map(|spec| {
            let first = spec.get("from").and_then(Value::as_str).unwrap_or_default();
            let second = spec.get("to").and_then(Value::as_str).unwrap_or_default();
            let frame_first = by_name
                .get(first)
                .and_then(|anchor| anchor.get("frame_row_y"))
                .and_then(Value::as_i64);
            let frame_second = by_name
                .get(second)
                .and_then(|anchor| anchor.get("frame_row_y"))
                .and_then(Value::as_i64);
            let base_first = baseline.get(first).and_then(|row| row.row_y).map(i64::from);
            let base_second = baseline
                .get(second)
                .and_then(|row| row.row_y)
                .map(i64::from);
            let mut row = serde_json::json!({
                "from": first,
                "to": second,
                "status": "passed",
            });
            match (frame_first, frame_second, base_first, base_second) {
                (Some(frame_first), Some(frame_second), Some(base_first), Some(base_second)) => {
                    let frame_delta = frame_first - frame_second;
                    let baseline_delta = base_first - base_second;
                    let delta_change = frame_delta - baseline_delta;
                    row["baseline_delta"] = serde_json::json!(baseline_delta);
                    row["frame_delta"] = serde_json::json!(frame_delta);
                    row["delta_change"] = serde_json::json!(delta_change);
                    if spec
                        .get("max_delta_change")
                        .and_then(Value::as_i64)
                        .is_some_and(|max| delta_change.abs() > max)
                        || spec
                            .get("min_delta")
                            .and_then(Value::as_i64)
                            .is_some_and(|min| frame_delta < min)
                        || spec
                            .get("max_delta")
                            .and_then(Value::as_i64)
                            .is_some_and(|max| frame_delta > max)
                    {
                        row["status"] = serde_json::json!("failed");
                    }
                }
                _ => row["status"] = serde_json::json!("failed"),
            }
            row
        })
        .collect()
}

fn frame_paths(frames_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = fs::read_dir(frames_dir)
        .map_err(|error| format!("cannot list {}: {error}", frames_dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("stream-frame-")
                        && Path::new(name)
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
                })
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn animation_frame_elapsed_ms(
    frame_path: &Path,
    wall_started: SystemTime,
    frame_index: usize,
    fallback_interval: Duration,
) -> u64 {
    let fallback = u64::try_from(frame_index)
        .unwrap_or(u64::MAX)
        .saturating_mul(duration_millis(fallback_interval));
    fs::metadata(frame_path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(wall_started).ok())
        .map_or(fallback, duration_millis)
}

fn rect_for_anchor_search(
    snapshot: &Value,
    spec: &AnimationAnchorSpec,
) -> Result<png::Rect, String> {
    let rect = if let Some(surface) = &spec.crop_surface {
        surface_box(snapshot, surface)?
    } else {
        pixel_anchor_box(snapshot, &spec.name)?
    };
    let rect = if let Some(insets) = spec.crop_insets {
        inset_box(rect, insets)?
    } else {
        rect
    };
    png_rect(rect)
}

fn selected_surface_rows(snapshot: &Value) -> Vec<Value> {
    select_surface_rows(
        snapshot,
        &[
            "workspace-sidebar",
            "editor-viewport",
            "minimap-shell",
            "minimap-source-map",
            "minimap-marker-strip",
        ],
    )
}

fn inset_box(rect: VisualBox, insets: Insets) -> Result<VisualBox, String> {
    crate::geometry::inset_box(
        rect,
        insets,
        "crop insets leave an empty animation anchor rectangle",
    )
}

fn png_rect(rect: VisualBox) -> Result<png::Rect, String> {
    png_rect_with_message(rect, "animation anchor crop rectangle is empty")
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn action_row(row: &automation::AutomationArtifactRow) -> serde_json::Value {
    serde_json::json!({
        "name": row.name,
        "status": row.status,
        "detail": row.detail,
        "artifact": row.artifact,
    })
}

fn assert_snapshot_ready_for_capture(snapshot: &Value) -> Result<(), String> {
    if snapshot.get("window").is_none() {
        return Err("Automation1 snapshot has no active window before screenshot".to_string());
    }
    if snapshot
        .pointer("/window/visual_geometry")
        .and_then(Value::as_object)
        .is_none()
    {
        return Err("Automation1 snapshot has no visual_geometry before screenshot".to_string());
    }
    Ok(())
}

fn scenario_type(case: &Value) -> Result<&str, String> {
    case.pointer("/manifest/scenario_type")
        .and_then(Value::as_str)
        .ok_or_else(|| "case manifest is missing scenario_type".to_string())
}

fn primary_action_name(case: &Value) -> Result<&'static str, String> {
    match scenario_type(case)? {
        "minimap-sidebar" => Ok("toggle-sidebar"),
        "command-palette-overlay" => Ok("toggle-command-palette"),
        "open-popover" => Ok("open-recent"),
        other => Err(format!(
            "unsupported visual geometry scenario type: {other}"
        )),
    }
}

fn sidebar_target_visible(case: &Value, step: &str) -> Result<bool, String> {
    let direction = case
        .get("direction")
        .and_then(Value::as_str)
        .ok_or_else(|| "minimap-sidebar case is missing direction".to_string())?;
    match (direction, step) {
        ("hide", "before") | ("show", "after") => Ok(true),
        ("show", "before") | ("hide", "after") => Ok(false),
        ("hide" | "show", other) => Err(format!("unsupported capture step: {other}")),
        (other, _) => Err(format!("unsupported sidebar direction: {other}")),
    }
}

fn sidebar_final_geometry_matches(
    snapshot: &Value,
    target_visible: bool,
    compact_overlay_allowed: bool,
) -> (bool, String) {
    let sidebar = match surface_rect(snapshot, "workspace-sidebar") {
        Ok(rect) => rect,
        Err(error) => return (false, error),
    };
    let editor = match surface_rect(snapshot, "editor-viewport") {
        Ok(rect) => rect,
        Err(error) => return (false, error),
    };
    for required in [
        "minimap-shell",
        "minimap-source-map",
        "minimap-marker-strip",
    ] {
        if let Err(error) = surface_rect(snapshot, required) {
            return (false, error);
        }
    }

    if target_visible {
        if sidebar.x != 0 {
            return (
                false,
                format!("workspace-sidebar x={}, expected 0", sidebar.x),
            );
        }
        let side_by_side_editor_x = sidebar.x + sidebar.width;
        if editor.x == side_by_side_editor_x {
            return (
                true,
                "workspace sidebar is fully visible beside editor".to_string(),
            );
        }
        if compact_overlay_allowed && editor.x == 0 {
            return (
                true,
                "workspace sidebar is fully visible as collapsed overlay".to_string(),
            );
        }
        let expected = if compact_overlay_allowed {
            format!("0 or {side_by_side_editor_x}")
        } else {
            side_by_side_editor_x.to_string()
        };
        return (
            false,
            format!("editor-viewport x={}, expected {expected}", editor.x),
        );
    }

    let expected_sidebar_x = -sidebar.width;
    if sidebar.x != expected_sidebar_x {
        return (
            false,
            format!(
                "workspace-sidebar x={}, expected {expected_sidebar_x}",
                sidebar.x
            ),
        );
    }
    if editor.x != 0 {
        return (false, format!("editor-viewport x={}, expected 0", editor.x));
    }
    (true, "workspace sidebar is fully hidden".to_string())
}

fn compact_overlay_allowed(case: &Value) -> bool {
    case.pointer("/size/width")
        .and_then(Value::as_i64)
        .is_some_and(|width| width <= 860)
}

fn final_geometry_signature(snapshot: &Value) -> String {
    let visual_geometry = snapshot.pointer("/window/visual_geometry");
    let surfaces = visual_geometry
        .and_then(|geometry| geometry.get("surfaces"))
        .cloned()
        .unwrap_or_default();
    let native_minimap = visual_geometry
        .and_then(|geometry| geometry.get("native_minimap"))
        .cloned()
        .unwrap_or_default();
    serde_json::json!({
        "surfaces": surfaces,
        "native_minimap": native_minimap,
    })
    .to_string()
}

fn final_geometry_sample(
    snapshot: &Value,
    target_visible: bool,
    matches: bool,
    detail: &str,
) -> serde_json::Value {
    serde_json::json!({
        "target": if target_visible { "visible" } else { "hidden" },
        "matches": matches,
        "detail": detail,
        "surfaces": snapshot.pointer("/window/visual_geometry/surfaces").cloned().unwrap_or_default(),
        "native_minimap": snapshot.pointer("/window/visual_geometry/native_minimap").cloned().unwrap_or_default(),
    })
}

fn write_final_geometry_samples(
    case_dir: &Path,
    step: &str,
    target_visible: bool,
    samples: &[serde_json::Value],
    status: &'static str,
) -> Result<(), String> {
    artifacts::write_json(
        &case_dir.join(format!("{step}-final-geometry-samples.json")),
        &serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "status": status,
            "step": step,
            "target": if target_visible { "visible" } else { "hidden" },
            "required_stable_samples": FINAL_GEOMETRY_SAMPLE_COUNT,
            "sample_interval_ms": FINAL_GEOMETRY_SAMPLE_INTERVAL.as_millis(),
            "samples": samples,
        }),
    )
}

fn surface_rect(snapshot: &Value, name: &str) -> Result<VisualRect, String> {
    let surfaces = snapshot
        .pointer("/window/visual_geometry/surfaces")
        .and_then(Value::as_array)
        .ok_or_else(|| "snapshot has no visual geometry surfaces".to_string())?;
    let surface = surfaces
        .iter()
        .find(|surface| surface.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("snapshot has no surface {name}"))?;
    let rect = surface
        .get("rect")
        .ok_or_else(|| format!("surface {name} has no rect"))?;
    VisualRect::from_value(rect).ok_or_else(|| format!("surface {name} has malformed rect"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VisualRect {
    x: i64,
    width: i64,
}

impl VisualRect {
    fn from_value(value: &Value) -> Option<Self> {
        Some(Self {
            x: value.get("x")?.as_i64()?,
            width: value.get("width")?.as_i64()?,
        })
    }
}

fn run_mutter_for_case(
    case: &Value,
    case_json: &Path,
    programs: &LivePrograms,
    case_dir: &Path,
    runtime_dir: &Path,
) -> Result<(), String> {
    let size = case_size(case)?;
    let monitor = format!("{}x{}", size.width, size.height);
    let args = [
        "--headless".to_string(),
        "--wayland".to_string(),
        "--no-x11".to_string(),
        "--virtual-monitor".to_string(),
        monitor,
        "--".to_string(),
        programs.proof_tool.clone(),
        "run".to_string(),
        "--mutter-child".to_string(),
        "--case-json".to_string(),
        case_json.to_string_lossy().into_owned(),
    ];
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let env = vec![
        ("NO_AT_BRIDGE".to_string(), "1".to_string()),
        (
            "XDG_RUNTIME_DIR".to_string(),
            runtime_dir.to_string_lossy().into_owned(),
        ),
    ];
    let result = process::run_logged_command(
        &programs.mutter,
        &arg_refs,
        &env,
        &case_dir.join("mutter-child.log"),
        MUTTER_CHILD_TIMEOUT,
    )?;
    if result.timed_out {
        return Err("headless Mutter child timed out".to_string());
    }
    if result.exit_code != Some(0) {
        return Err(format!(
            "headless Mutter child exited with status {:?}",
            result.exit_code
        ));
    }
    Ok(())
}

fn wait_for_pipewire(
    runtime_dir: &Path,
    programs: &LivePrograms,
    case_dir: &Path,
) -> Result<(), String> {
    let deadline = Instant::now() + PIPEWIRE_READY_TIMEOUT;
    let socket = runtime_dir.join("pipewire-0");
    let log_path = case_dir.join("pw-dump-ready.log");
    while Instant::now() < deadline {
        if socket.exists() {
            let result = process::run_logged_command(
                &programs.pw_dump,
                &[],
                &[],
                &log_path,
                Duration::from_secs(1),
            )?;
            if !result.timed_out && result.exit_code == Some(0) {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("PipeWire did not become ready in the isolated runtime directory".to_string())
}

fn apply_gsettings(case: &Value, programs: &LivePrograms, case_dir: &Path) -> Result<(), String> {
    let values = case
        .get("gsettings")
        .and_then(Value::as_array)
        .ok_or_else(|| "case is missing gsettings array".to_string())?;
    for value in values {
        let key = value
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| "gsettings entry is missing key".to_string())?;
        let setting = value
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| "gsettings entry is missing value".to_string())?;
        let args = ["set", APP_ID, key, setting];
        let log_path = case_dir.join(format!("gsettings-{key}.log"));
        let result = process::run_logged_command(
            &programs.gsettings,
            &args,
            &[],
            &log_path,
            Duration::from_secs(5),
        )?;
        if result.timed_out || result.exit_code != Some(0) {
            return Err(format!("gsettings set {key} failed"));
        }
    }
    Ok(())
}

fn prepare_open_popover_recents(
    case: &Value,
    case_dir: &Path,
    data_dir: &Path,
) -> Result<(), String> {
    if scenario_type(case)? != "open-popover" {
        return Ok(());
    }

    let fixture_kind = case
        .get("fixture_kind")
        .and_then(Value::as_str)
        .unwrap_or("dense");
    let count = match fixture_kind {
        "empty" => 0,
        "single" => 1,
        "representative" | "all-closed" | "all-open" => 2,
        "ten" => 10,
        _ => 12,
    };
    let recent_root = case_dir.join("open-popover-recents");
    fs::create_dir_all(&recent_root)
        .map_err(|error| format!("cannot create {}: {error}", recent_root.display()))?;
    let mut entries = Vec::new();
    let mut session_paths = Vec::new();
    let base_time = 2_000_000_000u64;

    for index in 0..count {
        let path = open_popover_recent_path(&recent_root, fixture_kind, index);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        fs::write(&path, format!("Open popover recent fixture {index}\n"))
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        let canonical = fs::canonicalize(&path)
            .map_err(|error| format!("cannot canonicalize {}: {error}", path.display()))?;
        entries.push(serde_json::json!({
            "path": path.to_string_lossy(),
            "canonical_path": canonical.to_string_lossy(),
            "last_opened_secs": base_time.saturating_sub(index as u64),
        }));
        session_paths.push(path);
    }

    fs::create_dir_all(data_dir)
        .map_err(|error| format!("cannot create {}: {error}", data_dir.display()))?;
    let recent_path = data_dir.join("recent-documents.json");
    let document = serde_json::json!({ "entries": entries });
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("cannot serialize recent documents: {error}"))?;
    fs::write(&recent_path, bytes)
        .map_err(|error| format!("cannot write {}: {error}", recent_path.display()))?;

    if fixture_kind == "all-open" {
        write_open_popover_session(data_dir, &session_paths)?;
    }

    Ok(())
}

fn open_popover_recent_path(root: &Path, fixture_kind: &str, index: usize) -> PathBuf {
    if fixture_kind == "awkward" {
        let folder = root
            .join("a very long folder name with spaces")
            .join("symbols []() and mixed width")
            .join(format!("deep-level-{index:02}"));
        return folder.join(format!(
            "this-is-a-ridiculously-long-file-name-that-must-ellipsize-{index:02}.rs"
        ));
    }

    root.join(format!("recent-document-{index:02}.txt"))
}

/// Seed restored tabs so the all-open proof case exercises true tab filtering.
fn write_open_popover_session(data_dir: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let tabs = paths
        .iter()
        .map(|path| {
            serde_json::json!({
                "path": path.to_string_lossy(),
                "cursor_line": 0,
                "cursor_col": 0,
                "scroll_line": 0,
                "pinned": false,
            })
        })
        .collect::<Vec<_>>();
    let active_tab_index = (!tabs.is_empty()).then_some(0);
    let session = serde_json::json!({
        "kind": "dev.cominotti.lushtext.session",
        "version": 1,
        "data": {
            "tabs": tabs,
            "active_tab_index": active_tab_index,
        }
    });
    let session_path = data_dir.join("session.json");
    let bytes = serde_json::to_vec_pretty(&session)
        .map_err(|error| format!("cannot serialize session fixture: {error}"))?;
    fs::write(&session_path, bytes)
        .map_err(|error| format!("cannot write {}: {error}", session_path.display()))
}

fn lushtext_process_environment(data_dir: &Path) -> Vec<(String, String)> {
    let mut env = vec![
        ("GDK_BACKEND".to_string(), "wayland".to_string()),
        ("GSETTINGS_BACKEND".to_string(), "keyfile".to_string()),
        (
            "GSETTINGS_SCHEMA_DIR".to_string(),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../data")
                .to_string_lossy()
                .into_owned(),
        ),
        ("GSK_RENDERER".to_string(), "cairo".to_string()),
        ("GTK_USE_PORTAL".to_string(), "0".to_string()),
        ("NO_AT_BRIDGE".to_string(), "1".to_string()),
        (
            "LUSHTEXT_DATA_DIR".to_string(),
            data_dir.to_string_lossy().into_owned(),
        ),
    ];
    if let Ok(value) = std::env::var("XDG_RUNTIME_DIR") {
        env.push(("XDG_RUNTIME_DIR".to_string(), value));
    }
    env
}

fn case_dir_for(case_json: &Path) -> Result<PathBuf, String> {
    case_json
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{} has no parent directory", case_json.display()))
}

fn runtime_dir_from_env() -> Result<PathBuf, String> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set for internal live session".to_string())
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("case is missing string field {key}"))
}

fn case_size(case: &Value) -> Result<CaseSize, String> {
    let size = case
        .get("size")
        .ok_or_else(|| "case is missing size".to_string())?;
    let width = size
        .get("width")
        .and_then(Value::as_u64)
        .ok_or_else(|| "case size is missing width".to_string())?;
    let height = size
        .get("height")
        .and_then(Value::as_u64)
        .ok_or_else(|| "case size is missing height".to_string())?;
    Ok(CaseSize { width, height })
}

fn process_detail(status: &str) -> String {
    match status {
        "launched" => {
            "Rust launched the live session process tree; visual proof layers are still pending"
                .to_string()
        }
        "timed-out" => "Rust live session process tree timed out".to_string(),
        _ => "Rust live session process tree failed".to_string(),
    }
}

#[derive(Debug)]
struct CaseSize {
    width: u64,
    height: u64,
}

#[derive(Debug, Serialize)]
struct ProcessReport {
    schema_version: u64,
    status: &'static str,
    stage: &'static str,
    detail: String,
    exit_code: Option<i32>,
    timed_out: bool,
    logs: Vec<String>,
    runtime_cleanup: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutter_command_uses_case_size_and_hidden_child_mode() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case_json = tempdir.path().join("case.json");
        let case = serde_json::json!({
            "schema_version": 1,
            "case_id": "case",
            "manifest": {},
            "size": { "width": 800, "height": 600 },
            "color_scheme": "default",
            "artifact_dir": "case",
            "binary": "/bin/true",
            "fixture": "/tmp/file.txt",
            "gsettings": [],
        });
        fs::write(&case_json, serde_json::to_vec(&case).expect("json")).expect("case write");
        let programs = LivePrograms {
            proof_tool: "/proof/tool".to_string(),
            dbus_run_session: "dbus-run-session".to_string(),
            pipewire: "pipewire".to_string(),
            wireplumber: "wireplumber".to_string(),
            pw_dump: "pw-dump".to_string(),
            gsettings: "gsettings".to_string(),
            mutter: "/bin/false".to_string(),
        };

        let error =
            run_mutter_for_case(&case, &case_json, &programs, tempdir.path(), tempdir.path())
                .expect_err("fake mutter fails after command construction");

        assert!(error.contains("headless Mutter child exited"));
        let log = fs::read_to_string(tempdir.path().join("mutter-child.log")).expect("log text");
        assert!(log.is_empty());
    }

    #[test]
    fn process_report_json_is_bounded_and_versioned() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let report = ProcessReport {
            schema_version: model::SUPPORTED_SCHEMA_VERSION,
            status: "launched",
            stage: "session",
            detail: "process tree launched".to_string(),
            exit_code: Some(0),
            timed_out: false,
            logs: vec!["session.log".to_string()],
            runtime_cleanup: Some("removed".to_string()),
        };
        let path = tempdir.path().join("process-report.json");

        artifacts::write_json(&path, &report).expect("write report");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("report text")).expect("json");

        assert_eq!(value["schema_version"], model::SUPPORTED_SCHEMA_VERSION);
        assert_eq!(value["status"], "launched");
        assert_eq!(value["logs"][0], "session.log");
    }

    #[test]
    fn unsupported_scenario_type_rejects_action_mapping() {
        let case = serde_json::json!({
            "manifest": {
                "scenario_type": "unknown-scenario"
            }
        });

        let error = primary_action_name(&case).expect_err("unsupported scenario rejected");

        assert!(error.contains("unsupported visual geometry scenario type"));
    }

    #[test]
    fn snapshot_state_mismatch_blocks_capture() {
        let missing_window = serde_json::json!({});
        let missing_geometry = serde_json::json!({
            "window": {}
        });

        assert!(
            assert_snapshot_ready_for_capture(&missing_window)
                .expect_err("missing window rejected")
                .contains("no active window")
        );
        assert!(
            assert_snapshot_ready_for_capture(&missing_geometry)
                .expect_err("missing visual geometry rejected")
                .contains("no visual_geometry")
        );
    }

    #[test]
    fn sidebar_final_geometry_matches_visible_and_hidden_targets() {
        let visible = geometry_snapshot(0, 320);
        let overlay_visible = geometry_snapshot(0, 0);
        let hidden = geometry_snapshot(-320, 0);

        assert_eq!(
            sidebar_final_geometry_matches(&visible, true, false),
            (
                true,
                "workspace sidebar is fully visible beside editor".to_string()
            )
        );
        assert_eq!(
            sidebar_final_geometry_matches(&overlay_visible, true, true),
            (
                true,
                "workspace sidebar is fully visible as collapsed overlay".to_string()
            )
        );
        assert!(!sidebar_final_geometry_matches(&overlay_visible, true, false).0);
        assert_eq!(
            sidebar_final_geometry_matches(&hidden, false, false),
            (true, "workspace sidebar is fully hidden".to_string())
        );
    }

    #[test]
    fn sidebar_final_geometry_reports_mismatch_detail() {
        let snapshot = geometry_snapshot(-100, 0);

        let (matches, detail) = sidebar_final_geometry_matches(&snapshot, false, false);

        assert!(!matches);
        assert!(detail.contains("workspace-sidebar x=-100, expected -320"));
    }

    #[test]
    fn animation_report_builder_accepts_stream_intermediate_frame() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case = animation_case(&["minimap-native-viewport-top-edge"]);
        let config = animation_config(&case)
            .expect("animation config")
            .expect("enabled animation");
        let snapshot = animation_snapshot(-160, 3);
        let frame = tempdir.path().join("stream-frame-000.png");
        crate::png::write_rgba_fixture(&frame, &anchor_rows(3)).expect("frame image");
        let baseline = animation_baseline(&snapshot, 3);
        let samples = vec![GeometrySample {
            elapsed_ms: 0,
            sidebar_phase: "intermediate".to_string(),
            snapshot,
        }];

        let report = build_animation_report(AnimationReportInput {
            case: &case,
            config: &config,
            baseline: &baseline,
            samples: &samples,
            frame_paths: &[frame],
            wall_started: SystemTime::now(),
            action_started_ms: 30,
            crops_dir: tempdir.path(),
        })
        .expect("animation report");

        assert_eq!(report["status"], "passed");
        assert_eq!(report["capture_mode"], "stream");
        assert_eq!(report["sampled_frame_count"], 1);
        assert_eq!(report["mapped_intermediate_frame_count"], 1);
        assert_eq!(report["frames"][0]["anchors"][0]["status"], "passed");
        assert_eq!(report["animation_frame_evidence"]["capture_mode"], "stream");
        model::validate_document(&report).expect("schema-valid animation report");
    }

    #[test]
    fn animation_report_builder_rejects_final_settle_only_capture() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case = animation_case(&["minimap-native-viewport-top-edge"]);
        let config = animation_config(&case)
            .expect("animation config")
            .expect("enabled animation");
        let snapshot = animation_snapshot(-320, 3);
        let baseline = animation_baseline(&snapshot, 3);
        let samples = vec![GeometrySample {
            elapsed_ms: 0,
            sidebar_phase: "hidden".to_string(),
            snapshot,
        }];

        let report = build_animation_report(AnimationReportInput {
            case: &case,
            config: &config,
            baseline: &baseline,
            samples: &samples,
            frame_paths: &[],
            wall_started: SystemTime::now(),
            action_started_ms: 30,
            crops_dir: tempdir.path(),
        })
        .expect("animation report");

        assert_eq!(report["status"], "failed");
        assert_eq!(
            report["failure_reason"],
            "animation sampling did not observe intermediate sidebar geometry"
        );
        assert_eq!(report["sampled_frame_count"], 0);
    }

    #[test]
    fn animation_report_builder_rejects_stale_frame_sample_pairing() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case = animation_case(&["minimap-native-viewport-top-edge"]);
        let config = AnimationConfig {
            require_intermediate_geometry: false,
            ..animation_config(&case)
                .expect("animation config")
                .expect("enabled animation")
        };
        let snapshot = animation_snapshot(-160, 3);
        let frame = tempdir.path().join("stream-frame-000.png");
        crate::png::write_rgba_fixture(&frame, &anchor_rows(3)).expect("frame image");
        let baseline = animation_baseline(&snapshot, 3);
        let samples = vec![GeometrySample {
            elapsed_ms: 200,
            sidebar_phase: "intermediate".to_string(),
            snapshot,
        }];

        let report = build_animation_report(AnimationReportInput {
            case: &case,
            config: &config,
            baseline: &baseline,
            samples: &samples,
            frame_paths: &[frame],
            wall_started: SystemTime::now(),
            action_started_ms: 30,
            crops_dir: tempdir.path(),
        })
        .expect("animation report");

        assert_eq!(report["status"], "failed");
        assert_eq!(
            report["frames"][0]["failure_reason"],
            "stale-frame-geometry-pairing"
        );
        assert_eq!(report["max_sample_skew_observed_ms"], 200);
    }

    #[test]
    fn animation_config_rejects_unknown_required_anchor() {
        let case = animation_case(&["missing-anchor"]);

        let error = animation_config(&case).expect_err("unknown anchor rejected");

        assert!(error.contains("unknown pixel anchors"));
    }

    #[test]
    fn animation_config_rejects_unsupported_capture_mode() {
        let mut case = animation_case(&["minimap-native-viewport-top-edge"]);
        case["manifest"]["animation_sampling"]["capture_mode"] = serde_json::json!("screenshot");

        let error = animation_config(&case).expect_err("unsupported mode rejected");

        assert!(error.contains("requires stream"));
    }

    #[test]
    fn animation_report_builder_rejects_no_mapped_intermediate_frame() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case = animation_case(&["minimap-native-viewport-top-edge"]);
        let config = animation_config(&case)
            .expect("animation config")
            .expect("enabled animation");
        let hidden_snapshot = animation_snapshot(-320, 3);
        let intermediate_snapshot = animation_snapshot(-160, 3);
        let frame = tempdir.path().join("stream-frame-000.png");
        crate::png::write_rgba_fixture(&frame, &anchor_rows(3)).expect("frame image");
        let baseline = animation_baseline(&hidden_snapshot, 3);
        let samples = vec![
            GeometrySample {
                elapsed_ms: 0,
                sidebar_phase: "hidden".to_string(),
                snapshot: hidden_snapshot,
            },
            GeometrySample {
                elapsed_ms: 100,
                sidebar_phase: "intermediate".to_string(),
                snapshot: intermediate_snapshot,
            },
        ];

        let report = build_animation_report(AnimationReportInput {
            case: &case,
            config: &config,
            baseline: &baseline,
            samples: &samples,
            frame_paths: &[frame],
            wall_started: SystemTime::now(),
            action_started_ms: 30,
            crops_dir: tempdir.path(),
        })
        .expect("animation report");

        assert_eq!(report["status"], "failed");
        assert_eq!(
            report["failure_reason"],
            "animation stream did not capture a PNG frame mapped to intermediate sidebar geometry"
        );
        assert_eq!(report["mapped_intermediate_frame_count"], 0);
    }

    #[test]
    fn animation_frame_reports_rendered_drift_against_stable_app_geometry() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case = animation_case(&["minimap-native-viewport-top-edge"]);
        let config = AnimationConfig {
            require_intermediate_geometry: false,
            ..animation_config(&case)
                .expect("animation config")
                .expect("enabled animation")
        };
        let baseline_snapshot = animation_snapshot(-160, 3);
        let frame_snapshot = animation_snapshot(-160, 3);
        let frame = tempdir.path().join("stream-frame-000.png");
        crate::png::write_rgba_fixture(&frame, &anchor_rows(4)).expect("frame image");
        let baseline = animation_baseline(&baseline_snapshot, 3);
        let samples = vec![GeometrySample {
            elapsed_ms: 0,
            sidebar_phase: "intermediate".to_string(),
            snapshot: frame_snapshot,
        }];

        let report = build_animation_report(AnimationReportInput {
            case: &case,
            config: &config,
            baseline: &baseline,
            samples: &samples,
            frame_paths: &[frame],
            wall_started: SystemTime::now(),
            action_started_ms: 30,
            crops_dir: tempdir.path(),
        })
        .expect("animation report");

        assert_eq!(report["status"], "failed");
        assert_eq!(report["frames"][0]["anchors"][0]["status"], "failed");
        assert_eq!(
            report["frames"][0]["anchors"][0]["diagnostics"][0]["status"],
            "animation-app-vs-rendered-anchor-disagreement"
        );
    }

    fn geometry_snapshot(sidebar_x: i64, editor_x: i64) -> Value {
        serde_json::json!({
            "window": {
                "visual_geometry": {
                    "surfaces": [
                        surface("workspace-sidebar", sidebar_x, 320),
                        surface("editor-viewport", editor_x, 900),
                        surface("minimap-shell", editor_x + 800, 80),
                        surface("minimap-source-map", editor_x + 800, 80),
                        surface("minimap-marker-strip", editor_x + 880, 20)
                    ],
                    "native_minimap": {
                        "visible": true
                    }
                }
            }
        })
    }

    fn surface(name: &str, x: i64, width: i64) -> Value {
        serde_json::json!({
            "name": name,
            "visible": true,
            "rect": {
                "x": x,
                "y": 0,
                "width": width,
                "height": 100
            }
        })
    }

    fn animation_case(required_anchors: &[&str]) -> Value {
        serde_json::json!({
            "manifest": {
                "scenario_type": "minimap-sidebar",
                "animation_sampling": {
                    "enabled": true,
                    "capture_mode": "stream",
                    "invariant_id": "native-minimap-animation-highlight-anchors",
                    "stream_frame_count": 1,
                    "stream_timeout_ms": 100,
                    "sample_interval_ms": 16,
                    "max_sample_skew_ms": 80,
                    "max_screen_y_delta": 0,
                    "require_intermediate_geometry": true,
                    "required_anchors": required_anchors,
                },
                "pixel_anchors": [{
                    "name": "minimap-native-viewport-top-edge",
                    "crop_surface": "minimap-shell",
                    "detector": "native-minimap-viewport-top-edge-row",
                    "min_pixels": 8,
                    "max_screen_y_delta": 0
                }],
                "relative_pixel_anchors": []
            }
        })
    }

    fn animation_snapshot(sidebar_x: i64, anchor_y: i64) -> Value {
        serde_json::json!({
            "window": {
                "visual_geometry": {
                    "surfaces": [
                        surface("workspace-sidebar", sidebar_x, 320),
                        surface("editor-viewport", 0, 900),
                        surface("minimap-shell", 0, 20),
                        surface("minimap-source-map", 0, 20),
                        surface("minimap-marker-strip", 20, 5)
                    ],
                    "pixel_anchors": [{
                        "name": "minimap-native-viewport-top-edge",
                        "surface": "minimap-shell",
                        "visible": true,
                        "rect": {"x": 0, "y": anchor_y, "width": 20, "height": 1}
                    }],
                    "native_minimap": {
                        "visible": true
                    },
                    "scroll_anchors": []
                }
            }
        })
    }

    fn animation_baseline(snapshot: &Value, row_y: i32) -> AnimationBaseline {
        AnimationBaseline {
            report: serde_json::json!({
                "screenshot": "before.png",
                "snapshot": "before-geometry-snapshot.json",
                "anchors": {
                    "minimap-native-viewport-top-edge": {
                        "status": "passed",
                        "row_y": row_y,
                    }
                },
                "relationships": []
            }),
            snapshot: snapshot.clone(),
            rows: HashMap::from([(
                "minimap-native-viewport-top-edge".to_string(),
                BaselineAnchor { row_y: Some(row_y) },
            )]),
        }
    }

    fn anchor_rows(edge_row: usize) -> Vec<Vec<(u8, u8, u8, u8)>> {
        let bg = (29, 29, 32, 255);
        let edge = (150, 150, 151, 255);
        let mut rows = vec![vec![bg; 20]; 10];
        for pixel in &mut rows[edge_row][4..16] {
            *pixel = edge;
        }
        rows
    }
}
