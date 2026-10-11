// SPDX-License-Identifier: GPL-3.0-or-later

//! Live-runner entry point for `cargo gtk-proof run`.
//!
//! The runner keeps process orchestration, pixel comparison, warning scans, and
//! root-summary aggregation in Rust while preserving the Python oracle as an
//! explicit diagnostic mode.

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gtk_lush_proof_spine::{ArtifactEnvelope, ProofStatus};
use serde_json::{Value, json};

use crate::{
    artifacts,
    geometry::{
        Insets, VisualBox, app_pixel_anchor_geometry as shared_app_pixel_anchor_geometry,
        optional_surface, pixel_anchor_box, png_rect, png_rect_from_value, row_offset, safe_name,
        scroll_anchor, selected_surface_rows as select_surface_rows, surface_box, visual_geometry,
    },
    host, live, model, png, policy, process, read_json_value, warnings, write_envelope,
};

/// Theme and font metrics can place the 600px source cap a few pixels around
/// its nominal value while preserving the GNOME Text Editor row contract.
const OPEN_POPOVER_LIST_CAP_TOLERANCE: i64 = 12;
/// GtkPopover snapshots include a small rendered shadow beyond the content box.
const OPEN_POPOVER_VIEWPORT_SHADOW_TOLERANCE: i64 = 8;

/// Execute the `run` subcommand and write one proof-spine envelope.
pub(crate) fn handle_run(
    args: &[String],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<i32> {
    let config = match RunConfig::parse(args) {
        Ok(config) => config,
        Err(detail) => {
            writeln!(stderr, "{detail}")?;
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure("run", ProofStatus::UsageError, detail),
            )?;
            return Ok(2);
        }
    };
    match run_rust_live(&config) {
        Ok(outcome) => {
            write_envelope(stdout, &outcome.envelope)?;
            Ok(outcome.exit_code)
        }
        Err(detail) => {
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure("run", ProofStatus::ArtifactError, detail),
            )?;
            Ok(1)
        }
    }
}

#[derive(Debug)]
struct RunConfig {
    artifact_dir: PathBuf,
    scenario_dir: PathBuf,
    binary: PathBuf,
    case_filter: Option<String>,
    mode: RunMode,
}

impl RunConfig {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut artifact_dir = PathBuf::from("build/smoke/visual-geometry");
        let mut scenario_dir = PathBuf::from("scripts/visual-geometry-scenarios");
        let mut binary = PathBuf::from("target/debug/lushtext");
        let mut case_filter = None;
        let mut oracle_python = false;
        let mut internal_session = false;
        let mut mutter_child = false;
        let mut case_json = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--artifact-dir" => {
                    artifact_dir = value_arg(args, index, "--artifact-dir").map(PathBuf::from)?;
                    index += 2;
                }
                "--scenario-dir" => {
                    scenario_dir = value_arg(args, index, "--scenario-dir").map(PathBuf::from)?;
                    index += 2;
                }
                "--binary" => {
                    binary = value_arg(args, index, "--binary").map(PathBuf::from)?;
                    index += 2;
                }
                "--case-filter" => {
                    case_filter = Some(value_arg(args, index, "--case-filter")?.to_string());
                    index += 2;
                }
                "--oracle" => {
                    let value = value_arg(args, index, "--oracle")?;
                    if value != "python" {
                        return Err(format!("unsupported run oracle: {value}"));
                    }
                    oracle_python = true;
                    index += 2;
                }
                "--internal-session" => {
                    internal_session = true;
                    index += 1;
                }
                "--mutter-child" => {
                    mutter_child = true;
                    index += 1;
                }
                "--case-json" => {
                    case_json = Some(value_arg(args, index, "--case-json").map(PathBuf::from)?);
                    index += 2;
                }
                other => return Err(format!("unknown run argument: {other}")),
            }
        }
        let mode = match (oracle_python, internal_session, mutter_child, case_json) {
            (true, false, false, None) => RunMode::PythonOracle,
            (false, true, false, Some(case_json)) => RunMode::InternalSession { case_json },
            (false, false, true, Some(case_json)) => RunMode::MutterChild { case_json },
            (false, false, false, None) => RunMode::RustLive,
            (false, true, false, None) => {
                return Err("--internal-session requires --case-json".to_string());
            }
            (false, false, true, None) => {
                return Err("--mutter-child requires --case-json".to_string());
            }
            _ => {
                return Err(
                    "run modes --oracle, --internal-session, and --mutter-child are mutually exclusive"
                        .to_string(),
                );
            }
        };
        Ok(Self {
            artifact_dir,
            scenario_dir,
            binary,
            case_filter,
            mode,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RunMode {
    RustLive,
    PythonOracle,
    InternalSession { case_json: PathBuf },
    MutterChild { case_json: PathBuf },
}

struct RunOutcome {
    envelope: ArtifactEnvelope,
    exit_code: i32,
}

fn run_rust_live(config: &RunConfig) -> Result<RunOutcome, String> {
    match &config.mode {
        RunMode::PythonOracle => return run_python_oracle(config),
        RunMode::InternalSession { case_json } => {
            return run_internal_session(case_json);
        }
        RunMode::MutterChild { case_json } => {
            return run_mutter_child(case_json);
        }
        RunMode::RustLive => {}
    }
    artifacts::reset_artifact_dir(&config.artifact_dir)?;
    let runtime = host::RuntimeLayout::prepare(&config.artifact_dir)?;
    let loaded = load_scenarios(&config.scenario_dir, config.case_filter.as_deref())?;
    let cases = loaded.cases.clone();
    materialize_cases(&config.artifact_dir, &config.binary, &cases)?;
    let scenario_source = artifacts::ScenarioSourceSummary::new(
        &config.scenario_dir,
        loaded.scenarios,
        config.case_filter.clone(),
    );
    let probe = host::probe_host(&config.binary);
    let environment_report =
        host::write_environment_report(&config.artifact_dir, &probe, &runtime)?;
    let missing_capabilities = probe.missing_capabilities().to_vec();
    if !missing_capabilities.is_empty() {
        let detail = format!(
            "unsupported host tooling: {}",
            missing_capabilities.join(", ")
        );
        let summary_path = artifacts::write_non_proof_summary(
            &config.artifact_dir,
            scenario_source,
            &detail,
            missing_capabilities.clone(),
        )?;
        return Ok(RunOutcome {
            envelope: ArtifactEnvelope::failure("run", ProofStatus::UnsupportedHost, detail)
                .with_data(json!({
                    "artifact_dir": artifacts::safe_display_path(&config.artifact_dir),
                    "environment_report": artifacts::safe_display_path(&environment_report),
                    "summary": artifacts::safe_display_path(&summary_path),
                    "missing_capabilities": missing_capabilities,
                })),
            exit_code: 3,
        });
    }

    let live_results = run_live_process_sessions(&config.artifact_dir, &cases)?;
    let case_rows = write_live_case_summaries(&config.artifact_dir, &cases, &live_results)?;
    let summary_path = write_live_root_summary(
        &config.artifact_dir,
        &scenario_source,
        config.case_filter.as_deref(),
        &case_rows,
    )?;
    let failed_cases = case_rows
        .iter()
        .filter(|row| row.get("status").and_then(Value::as_str) == Some("failed"))
        .count();
    let skipped_cases = case_rows
        .iter()
        .filter(|row| row.get("status").and_then(Value::as_str) == Some("skipped"))
        .count();
    let detail = if failed_cases == 0 && skipped_cases == 0 {
        "Rust visual geometry proof passed"
    } else if failed_cases > 0 {
        "Rust visual geometry proof failed"
    } else {
        "Rust visual geometry proof skipped"
    };
    let proof_status = if failed_cases == 0 && skipped_cases == 0 {
        ProofStatus::Passed
    } else if failed_cases > 0 {
        ProofStatus::Failed
    } else {
        ProofStatus::Skipped
    };
    let envelope = if proof_status == ProofStatus::Passed {
        ArtifactEnvelope::success("run", detail)
    } else {
        ArtifactEnvelope::failure("run", proof_status, detail)
    };
    Ok(RunOutcome {
        envelope: envelope.with_data(json!({
            "artifact_dir": artifacts::safe_display_path(&config.artifact_dir),
            "environment_report": artifacts::safe_display_path(&environment_report),
            "summary": artifacts::safe_display_path(&summary_path),
            "missing_capabilities": [],
            "live_process_results": live_results,
        })),
        exit_code: i32::from(failed_cases != 0),
    })
}

fn write_live_case_summaries(
    artifact_dir: &Path,
    cases: &[model::ExpandedCaseOverview],
    live_results: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>, String> {
    let mut rows = Vec::with_capacity(cases.len());
    for case in cases {
        let case_dir = artifact_dir.join(&case.case_id);
        let live_result = live_results
            .iter()
            .find(|result| {
                result.get("case_id").and_then(serde_json::Value::as_str)
                    == Some(case.case_id.as_str())
            })
            .cloned()
            .unwrap_or_else(|| json!({"status": "not-run"}));
        let row = if live_result.get("status").and_then(Value::as_str) == Some("launched") {
            aggregate_launched_case(case, &case_dir, &live_result).unwrap_or_else(|error| {
                workflow_failed_case_summary(case, &case_dir, &live_result, &error).unwrap_or_else(
                    |fallback| {
                        json!({
                            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
                            "case_id": case.case_id,
                            "status": "failed",
                            "failure_status": "workflow-failure",
                            "failure_reason": fallback,
                        })
                    },
                )
            })
        } else {
            workflow_failed_case_summary(
                case,
                &case_dir,
                &live_result,
                "Rust live process did not complete successfully",
            )?
        };
        artifacts::write_artifact(
            &case_dir.join("case-summary.json"),
            artifacts::ProofArtifactKind::CaseSummary,
            &row,
        )?;
        rows.push(row);
    }
    Ok(rows)
}

/// Write the authoritative run summary from only case rows that actually passed.
fn write_live_root_summary(
    artifact_dir: &Path,
    scenario_source: &artifacts::ScenarioSourceSummary,
    case_filter: Option<&str>,
    cases: &[Value],
) -> Result<PathBuf, String> {
    let passed = cases
        .iter()
        .filter(|case| case.get("status").and_then(Value::as_str) == Some("passed"))
        .count();
    let failed = cases
        .iter()
        .filter(|case| case.get("status").and_then(Value::as_str) == Some("failed"))
        .count();
    let skipped = cases
        .iter()
        .filter(|case| case.get("status").and_then(Value::as_str) == Some("skipped"))
        .count();
    let status = if failed > 0 {
        "failed"
    } else if skipped > 0 {
        "skipped"
    } else {
        "passed"
    };
    let summary = json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "status": status,
        "case_count": cases.len(),
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "case_filter": case_filter,
        "verified_invariant_ids": aggregate_string_ids(cases, "verified_invariant_ids"),
        "pixel_verified_invariant_ids": aggregate_string_ids(cases, "pixel_verified_invariant_ids"),
        "animation_verified_invariant_ids": aggregate_string_ids(cases, "animation_verified_invariant_ids"),
        "pixel_anchor_assertion_count": cases
            .iter()
            .filter_map(|case| case.get("pixel_anchor_assertion_count").and_then(Value::as_u64))
            .sum::<u64>(),
        "animation_frame_sample_count": cases
            .iter()
            .filter_map(|case| case.get("animation_frame_sample_count").and_then(Value::as_u64))
            .sum::<u64>(),
        "visual_proof_policy": policy::current_visual_proof_policy_metadata()?,
        "engine": artifacts::EngineMetadata::rust_live(),
        "scenario_source": scenario_source,
        "artifact_root": artifacts::safe_display_path(artifact_dir),
        "parity": {
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "status": "not-run",
            "mode": "rust-authoritative-default",
            "python_oracle": "diagnostic-only"
        },
        "missing_capabilities": [],
        "cases": cases,
    });
    let path = artifact_dir.join("summary.json");
    artifacts::write_artifact(&path, artifacts::ProofArtifactKind::RootSummary, &summary)?;
    Ok(path)
}

/// Aggregate invariant lists without allowing failed or skipped cases to count.
fn aggregate_string_ids(cases: &[Value], field: &str) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for case in cases {
        if case.get("status").and_then(Value::as_str) != Some("passed") {
            continue;
        }
        if let Some(values) = case.get(field).and_then(Value::as_array) {
            ids.extend(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned),
            );
        }
    }
    ids.into_iter().collect()
}

/// Merge a successful live child run into the case summary consumed by tooling.
fn aggregate_launched_case(
    overview: &model::ExpandedCaseOverview,
    case_dir: &Path,
    live_result: &Value,
) -> Result<Value, String> {
    let case = read_json_value(&case_dir.join("case.json"), "expanded visual case")?;
    let before_snapshot = read_json_value(
        &case_dir.join("before-geometry-snapshot.json"),
        "before geometry snapshot",
    )?;
    let after_snapshot = read_json_value(
        &case_dir.join("after-geometry-snapshot.json"),
        "after geometry snapshot",
    )?;
    let same_session = read_json_value(
        &case_dir.join("same-session-captures.json"),
        "same-session captures",
    )?;
    if same_session.get("status").and_then(Value::as_str) != Some("captured") {
        return Err("same-session capture metadata did not report captured".to_string());
    }
    let comparison_report =
        compare_case_artifacts(case_dir, &case, &before_snapshot, &after_snapshot)?;
    let warning_path = warnings::scan_case_logs(case_dir)?;
    let warning_report = read_json_value(&warning_path, "warning scan")?;
    let animation_report = read_animation_report_if_required(case_dir, &case)?;
    let outcome = case_outcome(
        &comparison_report,
        &warning_report,
        animation_report.as_ref(),
    );
    let invariant_id = case
        .pointer("/manifest/invariant_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let pixel_anchor_evidence = comparison_report
        .pointer("/pixel_anchors/anchors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let app_vs_rendered_disagreements = comparison_report
        .pointer("/pixel_anchors/app_vs_rendered_disagreements")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let animation_frame_evidence = animation_report
        .as_ref()
        .and_then(|report| report.get("animation_frame_evidence"))
        .cloned();
    let animation_invariant = animation_report
        .as_ref()
        .and_then(|report| report.get("invariant_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let pixel_verified = if outcome.status == "passed"
        && !pixel_anchor_evidence.is_empty()
        && invariant_id.is_some()
    {
        invariant_id.iter().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let animation_verified = if outcome.status == "passed"
        && animation_report
            .as_ref()
            .and_then(|report| report.get("status"))
            .and_then(Value::as_str)
            == Some("passed")
        && animation_invariant.is_some()
    {
        animation_invariant.iter().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let verified_invariant_ids = pixel_verified.clone();
    let animation_frame_sample_count = animation_report
        .as_ref()
        .and_then(|report| report.get("sampled_frame_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let final_geometry = comparison_report
        .get("final_geometry")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let warning_artifact = artifacts::safe_display_path(&warning_path);
    let comparison_artifact =
        artifacts::safe_display_path(&case_dir.join("comparisons/comparison-report.json"));
    let row = json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "case_id": overview.case_id,
        "scenario_id": overview.case_id,
        "status": outcome.status,
        "failure_status": outcome.failure_status,
        "failure_reason": outcome.failure_reason,
        "invariant_id": invariant_id,
        "artifact_dir": overview.case_id,
        "manifest": "scenario-manifest.json",
        "comparison_report": comparison_artifact,
        "warning_scan": warning_artifact,
        "same_session": artifacts::safe_display_path(&case_dir.join("same-session-captures.json")),
        "process_report": artifacts::safe_display_path(&case_dir.join("process-report.json")),
        "live_process": live_result,
        "verified_invariant_ids": verified_invariant_ids,
        "pixel_verified_invariant_ids": pixel_verified,
        "animation_verified_invariant_ids": animation_verified,
        "pixel_anchor_assertion_count": pixel_anchor_evidence.len(),
        "animation_frame_sample_count": animation_frame_sample_count,
        "final_geometry": final_geometry,
        "pixel_anchor_evidence": pixel_anchor_evidence,
        "app_vs_rendered_disagreements": app_vs_rendered_disagreements,
        "rendered_anchor_stability": [],
        "animation_frame_evidence": animation_frame_evidence,
    });
    write_case_manifest(
        overview,
        case_dir,
        &case,
        &row,
        &comparison_report,
        &warning_report,
        animation_report.as_ref(),
    )?;
    Ok(row)
}

/// Emit a schema-valid failed case row when orchestration or artifacts are missing.
fn workflow_failed_case_summary(
    overview: &model::ExpandedCaseOverview,
    case_dir: &Path,
    live_result: &Value,
    failure_reason: &str,
) -> Result<Value, String> {
    let row = json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "case_id": overview.case_id,
        "scenario_id": overview.case_id,
        "status": "failed",
        "failure_status": "workflow-failure",
        "failure_reason": failure_reason,
        "artifact_dir": overview.case_id,
        "manifest": "scenario-manifest.json",
        "comparison_report": Value::Null,
        "same_session": artifacts::safe_display_path(&case_dir.join("same-session-captures.json")),
        "process_report": artifacts::safe_display_path(&case_dir.join("process-report.json")),
        "live_process": live_result,
        "verified_invariant_ids": [],
        "pixel_verified_invariant_ids": [],
        "animation_verified_invariant_ids": [],
        "pixel_anchor_assertion_count": 0,
        "animation_frame_sample_count": 0,
        "pixel_anchor_evidence": [],
        "app_vs_rendered_disagreements": [],
        "rendered_anchor_stability": [],
    });
    let manifest = json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "scenario_id": overview.case_id,
        "scenario_type": overview.scenario_type().unwrap_or("unknown"),
        "source_manifest": overview.manifest.get("_manifest_path").and_then(Value::as_str),
        "case": read_json_value(&case_dir.join("case.json"), "expanded visual case").ok(),
        "same_session": {
            "required": true,
            "status": "failed",
            "artifact": artifacts::safe_display_path(&case_dir.join("same-session-captures.json")),
        },
        "status": "failed",
        "failure_status": "workflow-failure",
        "failure_reason": row.get("failure_reason"),
        "skip_reason": Value::Null,
        "process_report": artifacts::safe_display_path(&case_dir.join("process-report.json")),
    });
    artifacts::write_artifact(
        &case_dir.join("scenario-manifest.json"),
        artifacts::ProofArtifactKind::Manifest,
        &manifest,
    )?;
    Ok(row)
}

/// Normalized case result used before writing manifest and root summary rows.
#[derive(Debug)]
struct CaseOutcome {
    status: &'static str,
    failure_status: Option<&'static str>,
    failure_reason: Option<String>,
}

/// Combine comparison, animation, and warning status into the public case status.
fn case_outcome(
    comparison_report: &Value,
    warning_report: &Value,
    animation_report: Option<&Value>,
) -> CaseOutcome {
    if comparison_report.get("status").and_then(Value::as_str) != Some("passed") {
        let pixel_failed = comparison_report
            .pointer("/pixel_anchors/status")
            .and_then(Value::as_str)
            == Some("failed");
        return CaseOutcome {
            status: "failed",
            failure_status: Some(if pixel_failed {
                "pixel-anchor-failed"
            } else {
                "visual-comparison-failed"
            }),
            failure_reason: Some(comparison_failure_reason(comparison_report)),
        };
    }
    if let Some(report) = animation_report
        && report.get("status").and_then(Value::as_str) != Some("passed")
    {
        return CaseOutcome {
            status: "failed",
            failure_status: Some("pixel-anchor-failed"),
            failure_reason: Some(
                report
                    .get("failure_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("animation frame proof failed")
                    .to_string(),
            ),
        };
    }
    if warning_report.get("status").and_then(Value::as_str) != Some("passed") {
        return CaseOutcome {
            status: "failed",
            failure_status: Some("warning-scan-failed"),
            failure_reason: Some("runtime warning scan reported unexpected warnings".to_string()),
        };
    }
    CaseOutcome {
        status: "passed",
        failure_status: None,
        failure_reason: None,
    }
}

/// Keep failure text stable so automation clients can classify proof failures.
fn comparison_failure_reason(comparison_report: &Value) -> String {
    if comparison_report
        .get("regions")
        .and_then(Value::as_array)
        .is_some_and(|regions| {
            regions
                .iter()
                .any(|region| region.get("status").and_then(Value::as_str) == Some("failed"))
        })
    {
        return "protected region comparison failed".to_string();
    }
    if comparison_report
        .pointer("/allowed_changing_regions/status")
        .and_then(Value::as_str)
        == Some("failed")
    {
        return "allowed changing region relationship failed".to_string();
    }
    if comparison_report
        .pointer("/pixel_anchors/status")
        .and_then(Value::as_str)
        == Some("failed")
    {
        return "pixel anchor assertion failed".to_string();
    }
    "visual comparison failed".to_string()
}

/// Load animation evidence only for scenarios that declared a stream requirement.
fn read_animation_report_if_required(
    case_dir: &Path,
    case: &Value,
) -> Result<Option<Value>, String> {
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
    read_json_value(
        &case_dir.join("animation/animation-report.json"),
        "animation report",
    )
    .map(Some)
}

/// Rewrite the scenario manifest with final live-run evidence and artifact links.
fn write_case_manifest(
    overview: &model::ExpandedCaseOverview,
    case_dir: &Path,
    case: &Value,
    row: &Value,
    comparison_report: &Value,
    warning_report: &Value,
    animation_report: Option<&Value>,
) -> Result<(), String> {
    let manifest = json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "scenario_id": overview.case_id,
        "scenario_type": overview.scenario_type().unwrap_or("unknown"),
        "source_manifest": overview.manifest.get("_manifest_path").and_then(Value::as_str),
        "case": case,
        "gsettings": case.get("gsettings").cloned().unwrap_or_default(),
        "invariant_id": row.get("invariant_id"),
        "same_session": {
            "required": true,
            "status": "passed",
            "artifact": artifacts::safe_display_path(&case_dir.join("same-session-captures.json")),
        },
        "screenshots": [
            {"name": "before", "artifact": artifacts::safe_display_path(&case_dir.join("before.png"))},
            {"name": "after", "artifact": artifacts::safe_display_path(&case_dir.join("after.png"))}
        ],
        "geometry_snapshots": [
            {"name": "before", "artifact": artifacts::safe_display_path(&case_dir.join("before-geometry-snapshot.json"))},
            {"name": "after", "artifact": artifacts::safe_display_path(&case_dir.join("after-geometry-snapshot.json"))}
        ],
        "protected_regions": case.pointer("/manifest/protected_regions").cloned().unwrap_or_default(),
        "pixel_anchors": case.pointer("/manifest/pixel_anchors").cloned().unwrap_or_default(),
        "relative_pixel_anchors": case.pointer("/manifest/relative_pixel_anchors").cloned().unwrap_or_default(),
        "allowed_changing_regions": case.pointer("/manifest/allowed_changing_regions").cloned().unwrap_or_default(),
        "animation_sampling": case.pointer("/manifest/animation_sampling").cloned().unwrap_or_default(),
        "comparison_report": artifacts::safe_display_path(&case_dir.join("comparisons/comparison-report.json")),
        "warnings": warning_report,
        "status": row.get("status").and_then(Value::as_str).unwrap_or("failed"),
        "failure_status": row.get("failure_status").cloned().unwrap_or(Value::Null),
        "failure_reason": row.get("failure_reason").cloned().unwrap_or(Value::Null),
        "skip_reason": Value::Null,
        "verified_invariant_ids": row.get("verified_invariant_ids").cloned().unwrap_or_default(),
        "pixel_verified_invariant_ids": row.get("pixel_verified_invariant_ids").cloned().unwrap_or_default(),
        "animation_verified_invariant_ids": row.get("animation_verified_invariant_ids").cloned().unwrap_or_default(),
        "pixel_anchor_assertion_count": row.get("pixel_anchor_assertion_count").cloned().unwrap_or_default(),
        "pixel_anchor_evidence": row.get("pixel_anchor_evidence").cloned().unwrap_or_default(),
        "final_geometry": row.get("final_geometry").cloned().unwrap_or_default(),
        "app_vs_rendered_disagreements": row.get("app_vs_rendered_disagreements").cloned().unwrap_or_default(),
        "rendered_anchor_stability": row.get("rendered_anchor_stability").cloned().unwrap_or_default(),
        "animation_frame_evidence": row.get("animation_frame_evidence").cloned().unwrap_or(Value::Null),
        "animation_frame_sample_count": row.get("animation_frame_sample_count").cloned().unwrap_or_default(),
        "animation_report": animation_report.map(|_| artifacts::safe_display_path(&case_dir.join("animation/animation-report.json"))),
        "comparison": comparison_report,
    });
    artifacts::write_artifact(
        &case_dir.join("scenario-manifest.json"),
        artifacts::ProofArtifactKind::Manifest,
        &manifest,
    )?;
    Ok(())
}

/// Compare all declared rendered regions and pixel anchors for one case.
fn compare_case_artifacts(
    case_dir: &Path,
    case: &Value,
    before_snapshot: &Value,
    after_snapshot: &Value,
) -> Result<Value, String> {
    let comparison_dir = case_dir.join("comparisons");
    fs::create_dir_all(&comparison_dir)
        .map_err(|error| format!("cannot create {}: {error}", comparison_dir.display()))?;
    let before_screenshot = case_dir.join("before.png");
    let after_screenshot = case_dir.join("after.png");
    let mut status = "passed";
    let mut regions = Vec::new();
    if let Some(protected_regions) = case
        .pointer("/manifest/protected_regions")
        .and_then(Value::as_array)
    {
        for region in protected_regions {
            let name = required_str(region, "name")?;
            let surface = required_str(region, "surface")?;
            let before_rect = surface_box(before_snapshot, surface)?;
            let after_rect = surface_box(after_snapshot, surface)?;
            let require_same_rect = region
                .get("require_same_rect")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let mut report = if require_same_rect && before_rect != after_rect {
                json!({
                    "status": "failed",
                    "failure_reason": "protected-region-moved",
                    "before_rect": before_rect,
                    "after_rect": after_rect,
                    "mask_rects": [],
                    "allowed_changing_regions": [],
                    "compared_pixels": 0,
                    "diff_pixels": 0,
                    "first_difference": Value::Null,
                })
            } else {
                let masks = region
                    .get("mask_rects")
                    .and_then(Value::as_array)
                    .map(|rows| {
                        rows.iter()
                            .map(png_rect_from_value)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                png::compare_crops_in_files(
                    &before_screenshot,
                    &after_screenshot,
                    png_rect(before_rect)?,
                    Some(png_rect(after_rect)?),
                    &masks,
                    Some(&comparison_dir.join(name)),
                )?
            };
            if let Some(object) = report.as_object_mut() {
                object.insert("name".to_string(), json!(name));
                object.insert("surface".to_string(), json!(surface));
            }
            if report.get("status").and_then(Value::as_str) != Some("passed") {
                status = "failed";
            }
            regions.push(report);
        }
    }
    let rest_snapshot = read_after_rest_snapshot(case_dir)?;
    let allowed = evaluate_allowed_region_relationships(
        case,
        before_snapshot,
        after_snapshot,
        rest_snapshot.as_ref(),
    );
    if allowed.get("status").and_then(Value::as_str) != Some("passed") {
        status = "failed";
    }
    let pixel_anchors = evaluate_pixel_anchors(
        case,
        before_snapshot,
        after_snapshot,
        &before_screenshot,
        &after_screenshot,
        &comparison_dir,
    )?;
    if pixel_anchors.get("status").and_then(Value::as_str) != Some("passed") {
        status = "failed";
    }
    let report = json!({
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "status": status,
        "invariant_id": case.pointer("/manifest/invariant_id").and_then(Value::as_str),
        "regions": regions,
        "protected_regions": regions,
        "allowed_changing_regions": allowed,
        "pixel_anchors": pixel_anchors,
        "pixel_anchor_evidence": pixel_anchors.get("anchors").cloned().unwrap_or_default(),
        "app_vs_rendered_disagreements": pixel_anchors
            .get("app_vs_rendered_disagreements")
            .cloned()
            .unwrap_or_default(),
        "rendered_anchor_stability": [],
        "final_geometry": final_geometry_summary(before_snapshot, after_snapshot),
    });
    artifacts::write_artifact(
        &comparison_dir.join("comparison-report.json"),
        artifacts::ProofArtifactKind::ComparisonReport,
        &report,
    )?;
    Ok(report)
}

/// Evaluate screenshot-derived anchors and record app-vs-rendered disagreements.
fn evaluate_pixel_anchors(
    case: &Value,
    before_snapshot: &Value,
    after_snapshot: &Value,
    before_screenshot: &Path,
    after_screenshot: &Path,
    comparison_dir: &Path,
) -> Result<Value, String> {
    let Some(anchor_specs) = case
        .pointer("/manifest/pixel_anchors")
        .and_then(Value::as_array)
    else {
        return Ok(json!({"status": "passed", "anchors": [], "relationships": []}));
    };
    if anchor_specs.is_empty() {
        return Ok(json!({"status": "passed", "anchors": [], "relationships": []}));
    }

    let mut status = "passed";
    let mut reports = Vec::new();
    let mut app_vs_rendered_disagreements = Vec::new();
    let mut detections: HashMap<String, PixelAnchorDetectionPair> = HashMap::new();
    for spec in anchor_specs {
        if let Some(reference_surface) = spec.get("reference_surface").and_then(Value::as_str) {
            let report = evaluate_reference_pixel_anchor(
                spec,
                reference_surface,
                after_snapshot,
                after_screenshot,
                comparison_dir,
            )?;
            if report.get("status").and_then(Value::as_str) != Some("passed") {
                status = "failed";
                app_vs_rendered_disagreements.push(json!({
                    "name": report.get("name").cloned().unwrap_or(Value::Null),
                    "status": "app-vs-rendered-anchor-disagreement",
                    "reference_surface": reference_surface,
                    "screen_y_delta": report.get("screen_y_delta").cloned().unwrap_or(Value::Null),
                }));
            }
            reports.push(report);
            continue;
        }
        let name = required_str(spec, "name")?;
        let detector = required_str(spec, "detector")?;
        let min_pixels = anchor_min_pixels(spec);
        let before_rect = rect_for_anchor_search(before_snapshot, spec)?;
        let after_rect = rect_for_anchor_search(after_snapshot, spec)?;
        let before_crop = comparison_dir.join(format!("{}-before-anchor.png", safe_name(name)));
        let after_crop = comparison_dir.join(format!("{}-after-anchor.png", safe_name(name)));
        let before_detection = png::detect_pixel_anchor_in_file(
            before_screenshot,
            name,
            before_rect,
            detector,
            min_pixels,
            Some(&before_crop),
        )?;
        let after_detection = png::detect_pixel_anchor_in_file(
            after_screenshot,
            name,
            after_rect,
            detector,
            min_pixels,
            Some(&after_crop),
        )?;
        let mut row_status = "passed";
        let mut diagnostics = Vec::new();
        if before_detection.status != "passed" || after_detection.status != "passed" {
            row_status = "failed";
            status = "failed";
        }
        let app_geometry = app_pixel_anchor_geometry(before_snapshot, after_snapshot, name);
        let screen_y_delta = before_detection
            .row_y
            .zip(after_detection.row_y)
            .map(|(before, after)| (after - before).abs());
        if let Some(maximum) = spec.get("max_screen_y_delta").and_then(Value::as_i64)
            && let Some(delta) = screen_y_delta
            && i64::from(delta) > maximum
        {
            row_status = "failed";
            status = "failed";
            if let Some(app_delta) = app_geometry
                .as_ref()
                .and_then(|geometry| geometry.get("screen_y_delta"))
                .and_then(Value::as_i64)
                && app_delta <= maximum
            {
                let diagnostic = json!({
                    "name": name,
                    "status": "app-vs-rendered-anchor-disagreement",
                    "app_screen_y_delta": app_delta,
                    "rendered_screen_y_delta": delta,
                    "max_screen_y_delta": maximum,
                });
                diagnostics.push(diagnostic.clone());
                app_vs_rendered_disagreements.push(diagnostic);
            }
        }
        let before_row_offset = row_offset(before_detection.row_y, before_detection.rect);
        let after_row_offset = row_offset(after_detection.row_y, after_detection.rect);
        if let Some(minimum) = spec.get("min_row_offset").and_then(Value::as_i64)
            && (before_row_offset.is_some_and(|offset| i64::from(offset) < minimum)
                || after_row_offset.is_some_and(|offset| i64::from(offset) < minimum))
        {
            row_status = "failed";
            status = "failed";
        }
        if let Some(maximum) = spec.get("max_row_offset").and_then(Value::as_i64)
            && (before_row_offset.is_some_and(|offset| i64::from(offset) > maximum)
                || after_row_offset.is_some_and(|offset| i64::from(offset) > maximum))
        {
            row_status = "failed";
            status = "failed";
        }
        detections.insert(
            name.to_string(),
            PixelAnchorDetectionPair {
                before_row_y: before_detection.row_y,
                after_row_y: after_detection.row_y,
            },
        );
        reports.push(json!({
            "name": name,
            "detector": detector,
            "crop_surface": spec.get("crop_surface").cloned().unwrap_or(Value::Null),
            "crop_insets": spec.get("crop_insets").cloned().unwrap_or_else(|| json!({})),
            "before": before_detection,
            "after": after_detection,
            "before_row_y": before_detection.row_y,
            "after_row_y": after_detection.row_y,
            "screen_y_delta": screen_y_delta,
            "before_row_offset": before_row_offset,
            "after_row_offset": after_row_offset,
            "artifacts": {
                "before_crop": artifacts::safe_display_path(&before_crop),
                "after_crop": artifacts::safe_display_path(&after_crop),
            },
            "app_geometry": app_geometry,
            "diagnostics": diagnostics,
            "status": row_status,
        }));
    }

    let relationships = evaluate_relative_pixel_anchors(case, &detections, &mut status);
    Ok(json!({
        "status": status,
        "anchors": reports,
        "relationships": relationships,
        "app_vs_rendered_disagreements": app_vs_rendered_disagreements,
    }))
}

/// Check one anchor against the app's own geometry in the after capture.
///
/// Used where the before capture has nothing to compare with, such as a row
/// that only comes into view after the action. The detector runs on the after
/// screenshot alone, and its row must equal the top edge of
/// `reference_surface` in the after snapshot plus the manifest's
/// `reference_offset_y` (the detected feature's fixed distance below that
/// edge, `0` by default), within `max_screen_y_delta`. That equality is the
/// claim a snapshot cannot make on its own: that the pixels are drawn where
/// the widget reports them.
fn evaluate_reference_pixel_anchor(
    spec: &Value,
    reference_surface: &str,
    after_snapshot: &Value,
    after_screenshot: &Path,
    comparison_dir: &Path,
) -> Result<Value, String> {
    let name = required_str(spec, "name")?;
    let detector = required_str(spec, "detector")?;
    let min_pixels = anchor_min_pixels(spec);
    let maximum = spec
        .get("max_screen_y_delta")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let offset = spec
        .get("reference_offset_y")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let base = json!({
        "name": name,
        "detector": detector,
        "mode": "after-against-reference-surface",
        "crop_surface": spec.get("crop_surface").cloned().unwrap_or(Value::Null),
        "crop_insets": spec.get("crop_insets").cloned().unwrap_or_else(|| json!({})),
        "reference_surface": reference_surface,
        "reference_offset_y": offset,
        "max_screen_y_delta": maximum,
    });
    let reference = match surface_box(after_snapshot, reference_surface) {
        Ok(reference) => reference,
        Err(error) => {
            let mut report = base;
            report["status"] = json!("failed");
            report["failure_reason"] = json!(format!("reference surface unavailable: {error}"));
            return Ok(report);
        }
    };
    let after_rect = rect_for_anchor_search(after_snapshot, spec)?;
    let after_crop = comparison_dir.join(format!("{}-after-anchor.png", safe_name(name)));
    let detection = png::detect_pixel_anchor_in_file(
        after_screenshot,
        name,
        after_rect,
        detector,
        min_pixels,
        Some(&after_crop),
    )?;
    let row_y = detection.row_y;
    let detected = detection.status == "passed";
    let screen_y_delta = row_y.map(|row_y| (i64::from(row_y) - (reference.y + offset)).abs());
    let passed = detected && screen_y_delta.is_some_and(|delta| delta <= maximum);
    let mut report = base;
    report["after"] = json!(detection);
    report["after_row_y"] = json!(row_y);
    report["reference_top_y"] = json!(reference.y);
    report["screen_y_delta"] = json!(screen_y_delta);
    report["artifacts"] = json!({
        "after_crop": artifacts::safe_display_path(&after_crop),
    });
    report["status"] = json!(if passed { "passed" } else { "failed" });
    if !passed {
        report["failure_reason"] = json!(if detected {
            "rendered-row-differs-from-reported-row"
        } else {
            "row-content-not-detected"
        });
    }
    Ok(report)
}

/// A pixel-anchor spec's `min_pixels`, `1` when omitted.
fn anchor_min_pixels(spec: &Value) -> usize {
    spec.get("min_pixels")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
}

/// Enforce declared relationships between screenshot-derived anchor rows.
fn evaluate_relative_pixel_anchors(
    case: &Value,
    detections: &HashMap<String, PixelAnchorDetectionPair>,
    status: &mut &'static str,
) -> Vec<Value> {
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
            let first_detection = detections.get(first).copied().unwrap_or_default();
            let second_detection = detections.get(second).copied().unwrap_or_default();
            let mut row = json!({
                "from": first,
                "to": second,
                "status": "passed",
            });
            if let (
                Some(before_first),
                Some(before_second),
                Some(after_first),
                Some(after_second),
            ) = (
                first_detection.before_row_y,
                second_detection.before_row_y,
                first_detection.after_row_y,
                second_detection.after_row_y,
            ) {
                let before_delta = before_first - before_second;
                let after_delta = after_first - after_second;
                let delta_change = after_delta - before_delta;
                row["before_delta"] = json!(before_delta);
                row["after_delta"] = json!(after_delta);
                row["delta_change"] = json!(delta_change);
                if spec
                    .get("max_delta_change")
                    .and_then(Value::as_i64)
                    .is_some_and(|max| i64::from(delta_change.abs()) > max)
                    || spec
                        .get("min_delta")
                        .and_then(Value::as_i64)
                        .is_some_and(|min| {
                            i64::from(before_delta) < min || i64::from(after_delta) < min
                        })
                    || spec
                        .get("max_delta")
                        .and_then(Value::as_i64)
                        .is_some_and(|max| {
                            i64::from(before_delta) > max || i64::from(after_delta) > max
                        })
                {
                    row["status"] = json!("failed");
                    *status = "failed";
                }
            } else {
                row["status"] = json!("failed");
                *status = "failed";
            }
            row
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
/// Detected row positions for one pixel anchor before and after an action.
struct PixelAnchorDetectionPair {
    /// Anchor row detected in the before screenshot crop.
    before_row_y: Option<i32>,
    /// Anchor row detected in the after screenshot crop.
    after_row_y: Option<i32>,
}

/// Read the optional second after-readiness snapshot some scenarios capture.
///
/// `workspace-tree-reveal` takes it to prove the revealed row has come to rest;
/// other scenario types never write it, so its absence is not an error here.
fn read_after_rest_snapshot(case_dir: &Path) -> Result<Option<Value>, String> {
    let path = case_dir.join(live::AFTER_REST_SNAPSHOT_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    read_json_value(&path, "after-rest geometry snapshot").map(Some)
}

/// Check app geometry relationships that are allowed to change during a case.
///
/// `rest_snapshot` is a second snapshot taken after readiness, used only by
/// scenario types that prove a surface has stopped moving.
fn evaluate_allowed_region_relationships(
    case: &Value,
    before_snapshot: &Value,
    after_snapshot: &Value,
    rest_snapshot: Option<&Value>,
) -> Value {
    let scenario_type = case
        .pointer("/manifest/scenario_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut status = "passed";
    let mut rows = Vec::new();
    match scenario_type {
        "minimap-sidebar" => {
            push_relationship(
                &mut rows,
                &mut status,
                "editor-width-changes-with-sidebar",
                editor_width_relationship(case, before_snapshot, after_snapshot),
            );
            for surface in [
                "minimap-shell",
                "minimap-source-map",
                "minimap-marker-strip",
            ] {
                push_relationship(
                    &mut rows,
                    &mut status,
                    &format!("{surface}-visible"),
                    surface_box(after_snapshot, surface).map(|_| ()),
                );
            }
            push_relationship(
                &mut rows,
                &mut status,
                "source-view-scroll-anchor",
                source_view_anchor_relationship(case, before_snapshot, after_snapshot),
            );
        }
        "command-palette-overlay" => {
            push_relationship(
                &mut rows,
                &mut status,
                "active-transient-visible",
                surface_box(after_snapshot, "active-transient").map(|_| ()),
            );
        }
        "open-popover" => {
            push_relationship(
                &mut rows,
                &mut status,
                "active-transient-visible",
                surface_box(after_snapshot, "active-transient").map(|_| ()),
            );
            push_relationship(
                &mut rows,
                &mut status,
                "open-popover-search-visible",
                surface_box(after_snapshot, "open-popover-search").map(|_| ()),
            );
            push_relationship(
                &mut rows,
                &mut status,
                "open-popover-chooser-visible",
                surface_box(after_snapshot, "open-popover-chooser").map(|_| ()),
            );
            push_relationship(
                &mut rows,
                &mut status,
                "open-popover-content-state",
                open_popover_content_relationship(case, after_snapshot),
            );
            push_relationship(
                &mut rows,
                &mut status,
                "open-popover-fits-viewport",
                open_popover_fits_viewport(case, after_snapshot),
            );
            push_relationship(
                &mut rows,
                &mut status,
                "header-open-precedes-new-tab",
                header_open_precedes_new_tab(after_snapshot),
            );
        }
        model::WORKSPACE_TREE_REVEAL_SCENARIO => {
            push_workspace_tree_reveal_relationships(
                &mut rows,
                &mut status,
                case,
                before_snapshot,
                after_snapshot,
                rest_snapshot,
            );
        }
        "replace-preview" => {
            push_relationship(
                &mut rows,
                &mut status,
                "bounded-preview-state-visible",
                replace_preview_state_visible(after_snapshot),
            );
            push_relationship(
                &mut rows,
                &mut status,
                "preview-controls-outside-item-scroll",
                replace_preview_controls_outside_scroll(after_snapshot),
            );
        }
        _ => {
            push_relationship(
                &mut rows,
                &mut status,
                "supported-scenario-type",
                Err(format!("unsupported scenario type: {scenario_type}")),
            );
        }
    }
    json!({
        "status": status,
        "specs": case.pointer("/manifest/allowed_changing_regions").cloned().unwrap_or_default(),
        "assertions": rows,
    })
}

/// The revealed row's surface in the visual-geometry snapshot.
const REVEALED_ROW_SURFACE: &str = "workspace-revealed-row";
/// The sidebar's outer scroller viewport surface.
const SIDEBAR_VIEWPORT_SURFACE: &str = "workspace-sidebar-viewport";
/// The revealed row's section header surface.
const REVEALED_SECTION_HEADER_SURFACE: &str = "workspace-revealed-section-header";
/// Prefix of the up to eight unlabelled neighbour row surfaces.
const REVEALED_NEIGHBOUR_PREFIX: &str = "workspace-revealed-neighbour-";
/// The application reports at most this many neighbour rows.
const REVEALED_NEIGHBOUR_LIMIT: usize = 8;

/// Push every relationship a `workspace-tree-reveal` case proves.
///
/// The status check comes first so a failed reveal is named for what it is
/// rather than only through the missing surfaces it leaves behind.
fn push_workspace_tree_reveal_relationships(
    rows: &mut Vec<Value>,
    status: &mut &'static str,
    case: &Value,
    before_snapshot: &Value,
    after_snapshot: &Value,
    rest_snapshot: Option<&Value>,
) {
    push_relationship(
        rows,
        status,
        "reveal-status-revealed",
        reveal_status_is_revealed(after_snapshot),
    );
    push_relationship(
        rows,
        status,
        "revealed-row-inside-sidebar-viewport",
        revealed_row_inside_sidebar_viewport(after_snapshot),
    );
    push_relationship(
        rows,
        status,
        "neighbour-rows-contiguous",
        neighbour_rows_contiguous(after_snapshot),
    );
    push_relationship(
        rows,
        status,
        "revealed-row-at-rest",
        revealed_row_at_rest(after_snapshot, rest_snapshot),
    );
    if case.get("fixture_kind").and_then(Value::as_str) == Some("already-visible") {
        push_relationship(
            rows,
            status,
            "outer-scroll-unchanged",
            outer_scroll_unchanged(before_snapshot, after_snapshot),
        );
        push_relationship(
            rows,
            status,
            "section-header-unchanged",
            section_header_unchanged(before_snapshot, after_snapshot),
        );
    }
}

fn reveal_status_is_revealed(snapshot: &Value) -> Result<(), String> {
    match snapshot
        .pointer("/window/workspace/reveal_status")
        .and_then(Value::as_str)
    {
        Some("revealed") => Ok(()),
        Some(other) => Err(format!("workspace path reveal ended with status {other}")),
        None => Err("snapshot is missing window.workspace.reveal_status".to_string()),
    }
}

fn revealed_row_inside_sidebar_viewport(snapshot: &Value) -> Result<(), String> {
    let row = surface_box(snapshot, REVEALED_ROW_SURFACE)?;
    let viewport = surface_box(snapshot, SIDEBAR_VIEWPORT_SURFACE)?;
    if row.y >= viewport.y
        && row.y + row.height <= viewport.y + viewport.height
        && row.x >= viewport.x
        && row.x + row.width <= viewport.x + viewport.width
    {
        Ok(())
    } else {
        Err(format!(
            "{REVEALED_ROW_SURFACE} {row:?} lies outside {SIDEBAR_VIEWPORT_SURFACE} {viewport:?}"
        ))
    }
}

/// Every reported row, revealed or neighbour, in drawing order.
fn reveal_rows_in_order(snapshot: &Value) -> Result<Vec<(String, VisualBox)>, String> {
    let mut rows = vec![(
        REVEALED_ROW_SURFACE.to_string(),
        surface_box(snapshot, REVEALED_ROW_SURFACE)?,
    )];
    for index in 0..REVEALED_NEIGHBOUR_LIMIT {
        let name = format!("{REVEALED_NEIGHBOUR_PREFIX}{index}");
        if optional_surface(snapshot, &name).is_some() {
            rows.push((name.clone(), surface_box(snapshot, &name)?));
        }
    }
    rows.sort_by_key(|(_, rect)| rect.y);
    Ok(rows)
}

/// Rows on screen around the revealed row tile at one constant pitch.
///
/// A list row's border box excludes its CSS margin (`navigation-sidebar` rows
/// keep a small vertical margin), so consecutive boxes are separated by the
/// same gap rather than touching. The relationship holds when every gap equals
/// the run's common gap, exactly: one row drawn even one pixel off makes the
/// gaps on either side of it differ, and that pair is named. A negative gap is
/// an overlap and always fails.
fn neighbour_rows_contiguous(snapshot: &Value) -> Result<(), String> {
    let rows = reveal_rows_in_order(snapshot)?;
    if rows.len() < 2 {
        return Err("no neighbour rows were reported next to the revealed row".to_string());
    }
    let gaps: Vec<i64> = rows
        .windows(2)
        .map(|pair| pair[1].1.y - (pair[0].1.y + pair[0].1.height))
        .collect();
    let mut counts = std::collections::BTreeMap::new();
    for gap in &gaps {
        *counts.entry(*gap).or_insert(0usize) += 1;
    }
    // The most common gap is the row margin; ties go to the smaller gap.
    let common = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map_or(0, |(gap, _)| *gap);
    for (pair, gap) in rows.windows(2).zip(&gaps) {
        let (previous_name, previous) = &pair[0];
        let (next_name, next) = &pair[1];
        if *gap < 0 || *gap != common {
            return Err(format!(
                "{next_name} {next:?} does not start where {previous_name} {previous:?} ends \
                 plus the rows' common {common}px margin (gap {gap}px)"
            ));
        }
    }
    Ok(())
}

fn revealed_row_at_rest(after: &Value, rest: Option<&Value>) -> Result<(), String> {
    let rest = rest.ok_or_else(|| {
        format!(
            "the after-rest snapshot ({}) was not captured",
            live::AFTER_REST_SNAPSHOT_FILE
        )
    })?;
    let mut names = vec![
        SIDEBAR_VIEWPORT_SURFACE.to_string(),
        REVEALED_SECTION_HEADER_SURFACE.to_string(),
        REVEALED_ROW_SURFACE.to_string(),
    ];
    names.extend(
        reveal_rows_in_order(after)?
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| name != REVEALED_ROW_SURFACE),
    );
    for name in names {
        let first = surface_box(after, &name)?;
        let second = surface_box(rest, &name)?;
        if first != second {
            return Err(format!(
                "{name} moved after readiness: {first:?} then {second:?}"
            ));
        }
    }
    Ok(())
}

fn sidebar_scroll_value_milli(snapshot: &Value) -> Result<i64, String> {
    scroll_anchor(snapshot, SIDEBAR_VIEWPORT_SURFACE)
        .and_then(|row| row.get("y_value_milli"))
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("snapshot has no {SIDEBAR_VIEWPORT_SURFACE} scroll anchor"))
}

fn outer_scroll_unchanged(before: &Value, after: &Value) -> Result<(), String> {
    let before_value = sidebar_scroll_value_milli(before)?;
    let after_value = sidebar_scroll_value_milli(after)?;
    if before_value == after_value {
        Ok(())
    } else {
        Err(format!(
            "the sidebar scrolled from {before_value} to {after_value} (milli-pixels) revealing a visible row"
        ))
    }
}

fn section_header_unchanged(before: &Value, after: &Value) -> Result<(), String> {
    let before_rect = surface_box(before, REVEALED_SECTION_HEADER_SURFACE)?;
    let after_rect = surface_box(after, REVEALED_SECTION_HEADER_SURFACE)?;
    if before_rect == after_rect {
        Ok(())
    } else {
        Err(format!(
            "{REVEALED_SECTION_HEADER_SURFACE} moved from {before_rect:?} to {after_rect:?}"
        ))
    }
}

fn replace_preview_state_visible(snapshot: &Value) -> Result<(), String> {
    let content = snapshot
        .pointer("/window/content_search")
        .ok_or_else(|| "snapshot is missing content_search".to_string())?;
    let preview_mode = content
        .get("replace_preview_mode")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let generated = content
        .get("replace_preview_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let omitted = content
        .get("omitted_replacement_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let skipped = content
        .get("skipped_replacement_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if preview_mode && generated > 0 && skipped > 0 {
        Ok(())
    } else {
        Err(format!(
            "expected generated/omitted/skipped preview state, got mode={preview_mode} generated={generated} omitted={omitted} skipped={skipped}"
        ))
    }
}

fn replace_preview_controls_outside_scroll(snapshot: &Value) -> Result<(), String> {
    let scroll = surface_box(snapshot, "search-results-scroll")?;
    let summary = surface_box(snapshot, "search-preview-summary")?;
    let controls = surface_box(snapshot, "search-replace-controls")?;
    let panel = surface_box(snapshot, "search-panel")?;
    if summary.y >= scroll.y + scroll.height
        && controls.y + controls.height <= scroll.y
        && scroll.x >= panel.x
        && scroll.x + scroll.width <= panel.x + panel.width
    {
        Ok(())
    } else {
        Err(format!(
            "controls or summary entered the item scroller: controls={controls:?} scroll={scroll:?} summary={summary:?} panel={panel:?}"
        ))
    }
}

/// Add one relationship assertion while preserving a single aggregate status.
fn push_relationship(
    rows: &mut Vec<Value>,
    status: &mut &'static str,
    name: &str,
    result: Result<(), String>,
) {
    match result {
        Ok(()) => rows.push(json!({"name": name, "status": "passed"})),
        Err(error) => {
            *status = "failed";
            rows.push(json!({"name": name, "status": "failed", "detail": error}));
        }
    }
}

fn header_open_precedes_new_tab(snapshot: &Value) -> Result<(), String> {
    let header = surface_box(snapshot, "header-bar")?;
    let open = surface_box(snapshot, "header-open-menu-button")?;
    let new_tab = surface_box(snapshot, "header-new-tab-button")?;
    let open_right = open.x + open.width;
    let new_right = new_tab.x + new_tab.width;
    let header_right = header.x + header.width;

    if open.x < header.x || new_right > header_right {
        return Err(format!(
            "header buttons exceed header bounds: header={header:?} open={open:?} new_tab={new_tab:?}"
        ));
    }
    if open_right > new_tab.x {
        return Err(format!(
            "Open button should be before New Tab without overlap: open={open:?} new_tab={new_tab:?}"
        ));
    }
    Ok(())
}

fn open_popover_content_relationship(case: &Value, snapshot: &Value) -> Result<(), String> {
    match case
        .get("fixture_kind")
        .and_then(Value::as_str)
        .unwrap_or("dense")
    {
        "empty" | "all-open" => surface_box(snapshot, "open-popover-empty-state").map(|_| ()),
        "dense" | "awkward" => {
            let list = surface_box(snapshot, "open-popover-recent-list")?;
            let min_height = 600 - OPEN_POPOVER_LIST_CAP_TOLERANCE;
            let max_height = 600 + OPEN_POPOVER_LIST_CAP_TOLERANCE;
            if (min_height..=max_height).contains(&list.height) {
                Ok(())
            } else {
                Err(format!(
                    "open-popover-recent-list height {} did not match the GNOME 600px source cap",
                    list.height
                ))
            }
        }
        _ => surface_box(snapshot, "open-popover-recent-list").map(|_| ()),
    }
}

fn open_popover_fits_viewport(case: &Value, snapshot: &Value) -> Result<(), String> {
    let popover = surface_box(snapshot, "open-popover")?;
    let search = surface_box(snapshot, "open-popover-search")?;
    let chooser = surface_box(snapshot, "open-popover-chooser")?;
    let height = case
        .get("size")
        .and_then(|size| size.get("height"))
        .and_then(Value::as_i64)
        .ok_or_else(|| "open-popover case is missing size.height".to_string())?;
    let content_bottom = [
        search.y + search.height,
        chooser.y + chooser.height,
        surface_box(snapshot, "open-popover-recent-list")
            .or_else(|_| surface_box(snapshot, "open-popover-empty-state"))
            .map(|surface| surface.y + surface.height)?,
    ]
    .into_iter()
    .max()
    .unwrap_or(popover.y + popover.height);
    if popover.y >= 0
        && content_bottom <= height
        && popover.y + popover.height <= height + OPEN_POPOVER_VIEWPORT_SHADOW_TOLERANCE
    {
        Ok(())
    } else {
        Err(format!(
            "open-popover rect y={} height={} content_bottom={} exceeds viewport height {}",
            popover.y, popover.height, content_bottom, height
        ))
    }
}

/// Verify the editor width changes in the expected direction when the sidebar moves.
fn editor_width_relationship(
    case: &Value,
    before_snapshot: &Value,
    after_snapshot: &Value,
) -> Result<(), String> {
    let before_editor = surface_box(before_snapshot, "editor-viewport")?;
    let after_editor = surface_box(after_snapshot, "editor-viewport")?;
    let before_sidebar = surface_box(before_snapshot, "workspace-sidebar")?;
    let after_sidebar = surface_box(after_snapshot, "workspace-sidebar")?;
    match case.get("direction").and_then(Value::as_str) {
        Some("hide") if after_editor.width > before_editor.width => Ok(()),
        Some("show") if after_editor.width < before_editor.width => Ok(()),
        Some("hide")
            if compact_overlay_allowed(case)
                && compact_overlay_sidebar_transition(
                    &before_editor,
                    &after_editor,
                    &before_sidebar,
                    &after_sidebar,
                    "hide",
                ) =>
        {
            Ok(())
        }
        Some("show")
            if compact_overlay_allowed(case)
                && compact_overlay_sidebar_transition(
                    &before_editor,
                    &after_editor,
                    &before_sidebar,
                    &after_sidebar,
                    "show",
                ) =>
        {
            Ok(())
        }
        Some("hide")
            if compact_overlay_sidebar_transition(
                &before_editor,
                &after_editor,
                &before_sidebar,
                &after_sidebar,
                "hide",
            ) =>
        {
            Err("compact overlay sidebar transition occurred outside compact width".to_string())
        }
        Some("show")
            if compact_overlay_sidebar_transition(
                &before_editor,
                &after_editor,
                &before_sidebar,
                &after_sidebar,
                "show",
            ) =>
        {
            Err("compact overlay sidebar transition occurred outside compact width".to_string())
        }
        Some(direction) => Err(format!(
            "sidebar {direction} produced editor width {} from {}",
            after_editor.width, before_editor.width
        )),
        None => Err("minimap-sidebar case is missing direction".to_string()),
    }
}

fn compact_overlay_allowed(case: &Value) -> bool {
    case.pointer("/size/width")
        .and_then(Value::as_i64)
        .is_some_and(|width| width <= 860)
}

fn compact_overlay_sidebar_transition(
    before_editor: &VisualBox,
    after_editor: &VisualBox,
    before_sidebar: &VisualBox,
    after_sidebar: &VisualBox,
    direction: &str,
) -> bool {
    let editor_stayed_overlayed = before_editor.x == 0
        && after_editor.x == 0
        && before_editor.width == after_editor.width
        && before_editor.y == after_editor.y
        && before_editor.height == after_editor.height;
    if !editor_stayed_overlayed {
        return false;
    }

    match direction {
        "show" => before_sidebar.x == -before_sidebar.width && after_sidebar.x == 0,
        "hide" => before_sidebar.x == 0 && after_sidebar.x == -after_sidebar.width,
        _ => false,
    }
}

/// Verify source-view scroll anchoring survives the visual transition.
fn source_view_anchor_relationship(
    case: &Value,
    before_snapshot: &Value,
    after_snapshot: &Value,
) -> Result<(), String> {
    let before_anchor = scroll_anchor(before_snapshot, "source-view")
        .ok_or_else(|| "source-view scroll anchor missing before action".to_string())?;
    let after_anchor = scroll_anchor(after_snapshot, "source-view")
        .ok_or_else(|| "source-view scroll anchor missing after action".to_string())?;
    if after_anchor.get("at_left").and_then(Value::as_bool) != Some(true) {
        return Err("source-view should remain left anchored".to_string());
    }
    if case.get("viewport_position").and_then(Value::as_str) == Some("mid") {
        if before_anchor
            .get("y_value_milli")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            <= 0
            || after_anchor
                .get("y_value_milli")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                <= 0
        {
            return Err("source-view should remain scrolled for mid-file cases".to_string());
        }
    } else if after_anchor.get("at_top").and_then(Value::as_bool) != Some(true) {
        return Err("source-view should remain top anchored".to_string());
    }
    Ok(())
}

/// Preserve the geometry rows humans need when inspecting final proof artifacts.
fn final_geometry_summary(before_snapshot: &Value, after_snapshot: &Value) -> Value {
    json!({
        "before": selected_surface_rows(before_snapshot),
        "after": selected_surface_rows(after_snapshot),
        "native_minimap": {
            "before": visual_geometry(before_snapshot).and_then(|geometry| geometry.get("native_minimap")).cloned(),
            "after": visual_geometry(after_snapshot).and_then(|geometry| geometry.get("native_minimap")).cloned(),
        },
    })
}

/// Select stable surface rows from large Automation1 snapshots for summaries.
fn selected_surface_rows(snapshot: &Value) -> Vec<Value> {
    select_surface_rows(
        snapshot,
        &[
            "workspace-sidebar",
            "workspace-sidebar-transition",
            "editor-viewport",
            "source-view",
            "minimap-shell",
            "minimap-source-map",
            "minimap-native-viewport",
            "minimap-marker-strip",
        ],
    )
}

/// Convert an anchor search declaration into the exact rendered crop rectangle.
fn rect_for_anchor_search(snapshot: &Value, spec: &Value) -> Result<png::Rect, String> {
    let rect = if let Some(surface) = spec.get("crop_surface").and_then(Value::as_str) {
        surface_box(snapshot, surface)?
    } else {
        pixel_anchor_box(snapshot, required_str(spec, "name")?)?
    };
    let rect = if let Some(insets) = spec.get("crop_insets") {
        inset_box(rect, insets)?
    } else {
        rect
    };
    png_rect(rect)
}

/// Attach app-reported anchor geometry as diagnostics without making it proof.
fn app_pixel_anchor_geometry(
    before_snapshot: &Value,
    after_snapshot: &Value,
    pixel_anchor_name: &str,
) -> Option<Value> {
    let app_anchor_name = app_pixel_anchor_alias(pixel_anchor_name);
    shared_app_pixel_anchor_geometry(before_snapshot, after_snapshot, app_anchor_name)
}

fn app_pixel_anchor_alias(name: &str) -> &str {
    match name {
        "minimap-native-viewport-top-edge" => "minimap-viewport-top-edge",
        _ => name,
    }
}

fn inset_box(rect: VisualBox, insets: &Value) -> Result<VisualBox, String> {
    crate::geometry::inset_box(
        rect,
        Insets::from_value(insets),
        "crop insets leave an empty rectangle",
    )
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("visual scenario row is missing {field}"))
}

fn run_live_process_sessions(
    artifact_dir: &Path,
    cases: &[model::ExpandedCaseOverview],
) -> Result<Vec<serde_json::Value>, String> {
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        let case_json = artifact_dir.join(&case.case_id).join("case.json");
        let result = live::run_case_session(&case_json, Duration::from_secs(180))?;
        let status = if result.timed_out {
            "timed-out"
        } else if result.exit_code == Some(0) {
            "launched"
        } else {
            "failed"
        };
        results.push(json!({
            "case_id": case.case_id,
            "status": status,
            "exit_code": result.exit_code,
            "timed_out": result.timed_out,
            "process_report": artifacts::safe_display_path(&result.report_path),
        }));
    }
    Ok(results)
}

fn run_internal_session(case_json: &Path) -> Result<RunOutcome, String> {
    run_hidden_live_child(
        "internal session completed",
        "internal session failed",
        || live::run_internal_session(case_json),
    )
}

fn run_mutter_child(case_json: &Path) -> Result<RunOutcome, String> {
    run_hidden_live_child("mutter child completed", "mutter child failed", || {
        live::run_mutter_child(case_json)
    })
}

fn run_hidden_live_child<F>(
    success_detail: &str,
    failure_detail: &str,
    run: F,
) -> Result<RunOutcome, String>
where
    F: FnOnce() -> Result<(), String>,
{
    match run() {
        Ok(()) => Ok(RunOutcome {
            envelope: ArtifactEnvelope::success("run", success_detail),
            exit_code: 0,
        }),
        Err(error) => Ok(RunOutcome {
            envelope: ArtifactEnvelope::failure(
                "run",
                ProofStatus::Failed,
                format!("{failure_detail}: {error}"),
            ),
            exit_code: 1,
        }),
    }
}

fn run_python_oracle(config: &RunConfig) -> Result<RunOutcome, String> {
    artifacts::reset_artifact_dir(&config.artifact_dir)?;
    let log_path = python_oracle_log_path(&config.artifact_dir);
    let args = python_oracle_args(config);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = process::run_logged_command(
        "/usr/bin/python3",
        &arg_refs,
        &[],
        &log_path,
        Duration::from_secs(30 * 60),
    )?;
    let summary_path = config.artifact_dir.join("summary.json");
    let summary = summary_path
        .is_file()
        .then(|| read_json_value(&summary_path, "Python oracle summary"))
        .transpose()?;
    let summary_status = summary
        .as_ref()
        .and_then(|summary| summary.get("status"))
        .and_then(serde_json::Value::as_str);
    let detail = if result.timed_out {
        "Python visual oracle timed out under Rust supervision".to_string()
    } else if let Some(status) = summary_status {
        format!("Python visual oracle completed with summary status {status}")
    } else {
        "Python visual oracle did not produce summary.json".to_string()
    };
    let proof_status = match (result.timed_out, summary_status) {
        (false, Some("passed")) => ProofStatus::Passed,
        (false, Some("skipped")) => ProofStatus::Skipped,
        (true, _) | (false, Some("failed")) => ProofStatus::Failed,
        (false, _) => ProofStatus::ArtifactError,
    };
    let envelope = if proof_status == ProofStatus::Passed {
        ArtifactEnvelope::success("run", detail)
    } else {
        ArtifactEnvelope::failure("run", proof_status, detail)
    }
    .with_data(json!({
        "artifact_dir": artifacts::safe_display_path(&config.artifact_dir),
        "summary": artifacts::safe_display_path(&summary_path),
        "supervision_log": artifacts::safe_display_path(&log_path),
        "exit_code": result.exit_code,
        "timed_out": result.timed_out,
        "truncated_log_bytes": result.truncated_bytes,
        "engine": {
            "name": "python-visual-oracle",
            "mode": "diagnostic-oracle",
            "supervised_by": "cargo-gtk-proof",
            "authoritative": false,
        },
    }));
    Ok(RunOutcome {
        envelope,
        exit_code: if result.timed_out {
            1
        } else {
            result.exit_code.unwrap_or(1)
        },
    })
}

fn python_oracle_args(config: &RunConfig) -> Vec<String> {
    let mut args = vec![
        repo_root()
            .join("scripts/visual-geometry-smoke.py")
            .to_string_lossy()
            .into_owned(),
        "--artifact-dir".to_string(),
        config.artifact_dir.to_string_lossy().into_owned(),
        "--scenario-dir".to_string(),
        config.scenario_dir.to_string_lossy().into_owned(),
        "--binary".to_string(),
        config.binary.to_string_lossy().into_owned(),
    ];
    if let Some(filter) = &config.case_filter {
        args.push("--case-filter".to_string());
        args.push(filter.clone());
    }
    args
}

fn python_oracle_log_path(artifact_dir: &Path) -> PathBuf {
    let file_name = artifact_dir
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("visual-geometry");
    artifact_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{file_name}.python-oracle-supervision.log"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_scenarios(
    scenario_dir: &Path,
    case_filter: Option<&str>,
) -> Result<LoadedScenarios, String> {
    let entries = std::fs::read_dir(scenario_dir).map_err(|error| {
        format!(
            "cannot read scenario dir {}: {error}",
            scenario_dir.display()
        )
    })?;
    let mut scenarios = Vec::new();
    let mut cases = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot read scenario entry under {}: {error}",
                scenario_dir.display()
            )
        })?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("json")) {
            continue;
        }
        let mut value = read_json_value(&path, "visual scenario manifest")?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "_manifest_path".to_string(),
                serde_json::Value::String(artifacts::safe_display_path(&path)),
            );
        }
        let mut overview = model::visual_scenario_overview(&value).map_err(|error| error.detail)?;
        let mut expanded_cases =
            model::expanded_visual_cases(&value).map_err(|error| error.detail)?;
        if let Some(filter) = case_filter {
            overview.case_count = overview.filtered_case_count(filter);
            expanded_cases.retain(|case| case.case_id.contains(filter));
        }
        cases.extend(expanded_cases);
        scenarios.push(overview);
    }
    scenarios.sort_by(|left, right| left.scenario_id.cmp(&right.scenario_id));
    if scenarios.is_empty() {
        return Err(format!(
            "no visual geometry scenario manifests found in {}",
            scenario_dir.display()
        ));
    }
    if case_filter.is_some() && scenarios.iter().all(|scenario| scenario.case_count == 0) {
        return Err("case filter matched no visual geometry scenarios".to_string());
    }
    cases.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    Ok(LoadedScenarios { scenarios, cases })
}

struct LoadedScenarios {
    scenarios: Vec<model::ScenarioOverview>,
    cases: Vec<model::ExpandedCaseOverview>,
}

fn materialize_cases(
    artifact_dir: &Path,
    binary: &Path,
    cases: &[model::ExpandedCaseOverview],
) -> Result<(), String> {
    for case in cases {
        let case_dir = artifact_dir.join(&case.case_id);
        let runtime_dir = case_dir.join("runtime");
        let fixture_path = write_fixture(case, &case_dir)?;
        let gsettings = gsettings_plan(case);
        std::fs::create_dir_all(&runtime_dir)
            .map_err(|error| format!("cannot create {}: {error}", runtime_dir.display()))?;
        let case_json = serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "case_id": case.case_id.clone(),
            "manifest": case.manifest.clone(),
            "size": case.size.clone(),
            "color_scheme": case.color_scheme.clone(),
            "artifact_dir": artifacts::safe_display_path(&case_dir),
            "binary": binary,
            "fixture": artifacts::safe_display_path(&fixture_path),
            "word_wrap": case.word_wrap,
            "direction": case.direction.clone(),
            "viewport_position": case.viewport_position.clone(),
            "fixture_kind": case.fixture_kind.clone(),
            "gsettings": gsettings,
        });
        artifacts::write_artifact(
            &case_dir.join("case.json"),
            artifacts::ProofArtifactKind::ExpandedCase,
            &case_json,
        )?;
        let manifest = serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "scenario_id": case.case_id.clone(),
            "scenario_type": case.scenario_type().unwrap_or("unknown"),
            "source_manifest": case.manifest.get("_manifest_path").and_then(serde_json::Value::as_str),
            "case": case_json,
            "gsettings": gsettings,
            "same_session": {
                "required": true,
                "status": "not-run"
            },
            "protected_regions": case.manifest.get("protected_regions").cloned().unwrap_or_default(),
            "pixel_anchors": case.manifest.get("pixel_anchors").cloned().unwrap_or_default(),
            "relative_pixel_anchors": case.manifest.get("relative_pixel_anchors").cloned().unwrap_or_default(),
            "allowed_changing_regions": case.manifest.get("allowed_changing_regions").cloned().unwrap_or_default(),
            "animation_sampling": case.manifest.get("animation_sampling").cloned().unwrap_or_default(),
            "screenshots": {},
            "geometry_snapshots": {},
            "warnings": {
                "status": "not-run"
            },
            "status": "skipped",
            "skip_reason": "rust-live-runner-awaiting-execution"
        });
        artifacts::write_artifact(
            &case_dir.join("scenario-manifest.json"),
            artifacts::ProofArtifactKind::Manifest,
            &manifest,
        )?;
    }
    Ok(())
}

/// Return the exact GSettings values the Python live runner applies per case.
fn gsettings_plan(case: &model::ExpandedCaseOverview) -> Vec<GSettingsValue> {
    let mut values = vec![
        GSettingsValue::new("show-minimap", "true"),
        GSettingsValue::new(
            "word-wrap",
            if case.word_wrap.unwrap_or(false) {
                "true"
            } else {
                "false"
            },
        ),
        GSettingsValue::new("split-view-layout-migrated", "true"),
        GSettingsValue::new(
            "workspace-sidebar-visible",
            // A sidebar toggle starts visible only to hide it; a reveal proves
            // rows inside the sidebar, so it stays visible throughout.
            if matches!(case.direction.as_deref(), Some("hide" | "reveal")) {
                "true"
            } else {
                "false"
            },
        ),
        GSettingsValue::new("workspace-sidebar-width-fraction", "0.3"),
        GSettingsValue::new("properties-sidebar-visible", "false"),
    ];
    if let Some(width) = case.size.get("width").and_then(serde_json::Value::as_i64) {
        values.push(GSettingsValue::new("window-width", width.to_string()));
    }
    if let Some(height) = case.size.get("height").and_then(serde_json::Value::as_i64) {
        values.push(GSettingsValue::new("window-height", height.to_string()));
    }
    if case.color_scheme != "default" {
        values.push(GSettingsValue::new(
            "color-scheme",
            case.color_scheme.clone(),
        ));
    }
    values
}

#[derive(Clone, Debug, serde::Serialize)]
struct GSettingsValue {
    key: String,
    value: String,
}

impl GSettingsValue {
    fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

fn write_fixture(case: &model::ExpandedCaseOverview, case_dir: &Path) -> Result<PathBuf, String> {
    let fixture_dir = case_dir.join("fixtures");
    std::fs::create_dir_all(&fixture_dir)
        .map_err(|error| format!("cannot create {}: {error}", fixture_dir.display()))?;
    let extension = if case.fixture_kind() == "markdown-dense" {
        "md"
    } else {
        "txt"
    };
    let path = fixture_dir.join(format!("{}.{}", case.case_id, extension));
    let text = match case.scenario_type() {
        Some("minimap-sidebar") => minimap_fixture_text(case),
        Some("open-popover") => "Open popover visual geometry fixture\n".to_string(),
        Some("replace-preview") => replace_preview_fixture_text(),
        Some(model::WORKSPACE_TREE_REVEAL_SCENARIO) => {
            "Workspace tree reveal visual geometry fixture\n".to_string()
        }
        _ => "Command palette visual geometry fixture\n".to_string(),
    };
    std::fs::write(&path, text)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(path)
}

fn replace_preview_fixture_text() -> String {
    let mut lines = vec![format!("{}needle{}", "a".repeat(5000), "b".repeat(5000))];
    lines.extend((0..140).map(|index| format!("dense needle row {index:03} — 界/awkward path")));
    lines.join("\n") + "\n"
}

fn minimap_fixture_text(case: &model::ExpandedCaseOverview) -> String {
    if case.fixture_kind() == "markdown-dense" {
        return dense_markdown_minimap_fixture();
    }

    let long_tail = "x".repeat(150);
    let mut lines = Vec::new();
    for index in 0..280 {
        if case.word_wrap.unwrap_or(false) {
            lines.push(format!("line {index:04} {long_tail}"));
        } else {
            lines.push(format!("line {index:04}"));
        }
    }
    lines.join("\n") + "\n"
}

fn dense_markdown_minimap_fixture() -> String {
    let mut lines = vec![
        "# Volume3 Synology Residual Defrag Evidence".to_string(),
        String::new(),
        "Date: 2026-06-08 23:59:31 -0300".to_string(),
        String::new(),
        "Scope: targeted cleanup of residual files left for inspection.".to_string(),
        String::new(),
        "## Result".to_string(),
        String::new(),
        "- Before 256K scan: mapped_candidates=11 logical_GiB=1.630783".to_string(),
        "- After 256K scan: mapped_candidates=0 logical_GiB=0.000000".to_string(),
    ];
    for index in 0..60 {
        lines.push(format!(
            "- Detail {index:02}: path-{index:02} logical={:04} slack={} abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            index + 1,
            index % 7
        ));
    }
    lines.join("\n") + "\n"
}

fn value_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {flag} value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_rejects_unknown_argument() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code =
            handle_run(&["--mystery".to_string()], &mut stdout, &mut stderr).expect("run command");
        let output: serde_json::Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 2);
        assert_eq!(output["status"], "usage-error");
        assert!(
            String::from_utf8(stderr)
                .expect("stderr")
                .contains("--mystery")
        );
    }

    #[test]
    fn rust_live_run_writes_schema_valid_non_proof_summary() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let artifact_dir = tempdir.path().join("artifacts");
        let scenario_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/visual-geometry-scenarios");
        let config = RunConfig {
            artifact_dir: artifact_dir.clone(),
            scenario_dir,
            binary: PathBuf::from("/definitely/missing/lushtext"),
            case_filter: Some("live-threshold".to_string()),
            mode: RunMode::RustLive,
        };

        let outcome = run_rust_live(&config).expect("rust live run");
        let summary_path = artifact_dir.join("summary.json");
        let summary: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&summary_path).expect("summary text"))
                .expect("summary json");

        assert_eq!(outcome.exit_code, 3);
        assert_eq!(outcome.envelope.status, ProofStatus::UnsupportedHost);
        assert_eq!(summary["status"], "skipped");
        assert_eq!(summary["case_count"], 2);
        assert!(artifact_dir.join("environment-report.json").is_file());
        let case_dir = artifact_dir
            .join("minimap-sidebar-live-threshold--live-1822x1272--force-light--wrap-true--hide");
        assert!(case_dir.join("case.json").is_file());
        assert!(case_dir.join("scenario-manifest.json").is_file());
        assert!(case_dir
            .join("fixtures/minimap-sidebar-live-threshold--live-1822x1272--force-light--wrap-true--hide.txt")
            .is_file());
        let expanded_case: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(case_dir.join("case.json")).expect("case text"),
        )
        .expect("case json");
        let case_manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(case_dir.join("scenario-manifest.json"))
                .expect("case manifest text"),
        )
        .expect("case manifest json");
        assert_gsettings_value(&expanded_case, "show-minimap", "true");
        assert_gsettings_value(&expanded_case, "word-wrap", "true");
        assert_gsettings_value(&expanded_case, "workspace-sidebar-visible", "true");
        assert_gsettings_value(&expanded_case, "workspace-sidebar-width-fraction", "0.3");
        assert_gsettings_value(&expanded_case, "properties-sidebar-visible", "false");
        assert_gsettings_value(&expanded_case, "window-width", "1822");
        assert_gsettings_value(&expanded_case, "window-height", "1272");
        assert_gsettings_value(&expanded_case, "color-scheme", "force-light");
        assert_gsettings_value(&case_manifest, "split-view-layout-migrated", "true");
        model::validate_document(&case_manifest).expect("schema-valid case manifest");
        model::validate_document(&summary).expect("schema-valid summary");
    }

    #[test]
    fn run_parse_accepts_explicit_python_oracle_mode() {
        let config = RunConfig::parse(&[
            "--oracle".to_string(),
            "python".to_string(),
            "--case-filter".to_string(),
            "mini".to_string(),
        ])
        .expect("parse run config");

        assert_eq!(config.mode, RunMode::PythonOracle);
        assert_eq!(config.case_filter.as_deref(), Some("mini"));
    }

    #[test]
    fn run_parse_rejects_unknown_oracle_mode() {
        let error = RunConfig::parse(&["--oracle".to_string(), "shell".to_string()])
            .expect_err("unknown oracle should fail");

        assert!(error.contains("unsupported run oracle: shell"));
    }

    #[test]
    fn run_parse_accepts_hidden_internal_session_mode() {
        let config = RunConfig::parse(&[
            "--internal-session".to_string(),
            "--case-json".to_string(),
            "/tmp/case.json".to_string(),
        ])
        .expect("parse internal session config");

        assert_eq!(
            config.mode,
            RunMode::InternalSession {
                case_json: PathBuf::from("/tmp/case.json")
            }
        );
    }

    #[test]
    fn run_parse_requires_case_json_for_hidden_modes() {
        let error = RunConfig::parse(&["--mutter-child".to_string()])
            .expect_err("mutter child without case json should fail");

        assert!(error.contains("--mutter-child requires --case-json"));
    }

    #[test]
    fn live_case_summaries_do_not_overclaim_missing_launched_artifacts() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case = model::ExpandedCaseOverview {
            schema_version: model::SUPPORTED_SCHEMA_VERSION,
            case_id: "filtered-case".to_string(),
            manifest: serde_json::json!({"scenario_type": "minimap-sidebar"}),
            size: serde_json::json!({"width": 800, "height": 600}),
            color_scheme: "force-light".to_string(),
            artifact_dir: "filtered-case".to_string(),
            word_wrap: Some(true),
            direction: Some("hide".to_string()),
            viewport_position: Some("top".to_string()),
            fixture_kind: Some("plain-lines".to_string()),
        };
        std::fs::create_dir_all(tempdir.path().join("filtered-case")).expect("case dir");

        let rows = write_live_case_summaries(
            tempdir.path(),
            std::slice::from_ref(&case),
            &[serde_json::json!({
                "case_id": "filtered-case",
                "status": "launched"
            })],
        )
        .expect("case summaries");

        assert_eq!(rows[0]["status"], "failed");
        assert_eq!(rows[0]["failure_status"], "workflow-failure");
        assert!(
            rows[0]["verified_invariant_ids"]
                .as_array()
                .expect("verified invariant ids")
                .is_empty()
        );
        assert!(
            rows[0]["pixel_verified_invariant_ids"]
                .as_array()
                .expect("pixel invariant ids")
                .is_empty()
        );
        assert!(
            rows[0]["animation_verified_invariant_ids"]
                .as_array()
                .expect("animation invariant ids")
                .is_empty()
        );
    }

    #[test]
    fn live_case_summary_passes_paired_capture_and_pixel_anchor_fixture() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case_dir = tempdir.path().join("passing-case");
        std::fs::create_dir_all(&case_dir).expect("case dir");
        let case = model::ExpandedCaseOverview {
            schema_version: model::SUPPORTED_SCHEMA_VERSION,
            case_id: "passing-case".to_string(),
            manifest: serde_json::json!({
                "scenario_type": "minimap-sidebar",
                "invariant_id": "native-minimap-highlight-anchors",
                "protected_regions": [{
                    "name": "header-bar",
                    "surface": "header-bar",
                    "require_same_rect": true,
                    "mask_rects": []
                }],
                "pixel_anchors": [{
                    "name": "minimap-native-viewport-top-edge",
                    "crop_surface": "minimap-shell",
                    "detector": "native-minimap-viewport-top-edge-row",
                    "min_pixels": 8,
                    "max_screen_y_delta": 0
                }],
                "relative_pixel_anchors": [],
                "allowed_changing_regions": []
            }),
            size: serde_json::json!({"width": 80, "height": 40}),
            color_scheme: "force-light".to_string(),
            artifact_dir: "passing-case".to_string(),
            word_wrap: Some(true),
            direction: Some("hide".to_string()),
            viewport_position: Some("top".to_string()),
            fixture_kind: Some("plain-lines".to_string()),
        };
        let case_json = serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "case_id": case.case_id,
            "manifest": case.manifest,
            "size": case.size,
            "color_scheme": case.color_scheme,
            "direction": case.direction,
            "viewport_position": case.viewport_position,
        });
        artifacts::write_json(&case_dir.join("case.json"), &case_json).expect("case json");
        artifacts::write_json(
            &case_dir.join("before-geometry-snapshot.json"),
            &runner_snapshot(0, 20),
        )
        .expect("before snapshot");
        artifacts::write_json(
            &case_dir.join("after-geometry-snapshot.json"),
            &runner_snapshot(-10, 30),
        )
        .expect("after snapshot");
        artifacts::write_json(
            &case_dir.join("same-session-captures.json"),
            &serde_json::json!({"schema_version": 1, "status": "captured"}),
        )
        .expect("same-session");
        crate::png::write_rgba_fixture(&case_dir.join("before.png"), &runner_anchor_rows(3))
            .expect("before image");
        crate::png::write_rgba_fixture(&case_dir.join("after.png"), &runner_anchor_rows(3))
            .expect("after image");

        let rows = write_live_case_summaries(
            tempdir.path(),
            std::slice::from_ref(&case),
            &[serde_json::json!({
                "case_id": "passing-case",
                "status": "launched"
            })],
        )
        .expect("case summaries");

        assert_eq!(rows[0]["status"], "passed");
        assert_eq!(
            rows[0]["pixel_verified_invariant_ids"],
            serde_json::json!(["native-minimap-highlight-anchors"])
        );
        assert_eq!(rows[0]["pixel_anchor_assertion_count"], 1);
        assert!(
            case_dir
                .join("comparisons/comparison-report.json")
                .is_file()
        );
        assert!(case_dir.join("case-summary.json").is_file());
        assert_eq!(
            read_json_value(&case_dir.join("scenario-manifest.json"), "manifest")
                .expect("manifest")["status"],
            "passed"
        );
    }

    #[test]
    fn live_root_summary_identifies_authoritative_engine_and_policy_metadata() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let source = artifacts::ScenarioSourceSummary::new(
            Path::new("scripts/visual-geometry-scenarios"),
            vec![model::ScenarioOverview {
                scenario_id: "minimap-sidebar-top".to_string(),
                scenario_type: "minimap-sidebar".to_string(),
                case_count: 1,
                case_ids: vec!["passing-case".to_string()],
                readiness_predicates: vec!["visual-geometry-settled".to_string()],
                pixel_anchor_count: 1,
                relative_pixel_anchor_count: 0,
                animation_enabled: true,
            }],
            None,
        );
        let cases = vec![serde_json::json!({
            "schema_version": model::SUPPORTED_SCHEMA_VERSION,
            "case_id": "passing-case",
            "status": "passed",
            "pixel_verified_invariant_ids": ["native-minimap-highlight-anchors"],
            "animation_verified_invariant_ids": ["native-minimap-animation-highlight-anchors"],
            "pixel_anchor_assertion_count": 1,
            "animation_frame_sample_count": 2,
            "pixel_anchor_evidence": [{
                "before_row_y": 3,
                "after_row_y": 3
            }],
            "final_geometry": {"before": [], "after": []},
            "animation_frame_evidence": {
                "status": "passed",
                "capture_mode": "stream",
                "sampled_frame_count": 2,
                "mapped_intermediate_frame_count": 1,
                "max_sample_skew_ms": 80,
                "max_sample_skew_observed_ms": 8,
                "frames": [{
                    "status": "passed",
                    "mapped_sample_elapsed_ms": 48,
                    "sample_skew_ms": 8,
                    "sidebar_phase": "intermediate",
                    "anchors": [{
                        "status": "passed",
                        "baseline_row_y": 3,
                        "frame_row_y": 3
                    }]
                }]
            }
        })];

        let summary_path =
            write_live_root_summary(tempdir.path(), &source, None, &cases).expect("root summary");
        let summary = read_json_value(&summary_path, "root summary").expect("summary json");

        assert_eq!(summary["status"], "passed");
        assert_eq!(summary["engine"]["name"], "cargo-gtk-proof");
        assert_eq!(summary["engine"]["authoritative"], true);
        assert_eq!(summary["scenario_source"]["manifest_count"], 1);
        assert_eq!(summary["parity"]["status"], "not-run");
        assert!(summary["visual_proof_policy"]["changed_files_digest"].is_string());
        model::validate_document(&summary).expect("schema-valid root summary");
    }

    fn runner_anchor_rows(edge_row: usize) -> Vec<Vec<(u8, u8, u8, u8)>> {
        let bg = (29, 29, 32, 255);
        let edge = (150, 150, 151, 255);
        let mut rows = vec![vec![bg; 40]; 20];
        for pixel in &mut rows[edge_row][5..20] {
            *pixel = edge;
        }
        rows
    }

    fn runner_snapshot(sidebar_x: i64, editor_width: i64) -> serde_json::Value {
        runner_snapshot_with_editor(sidebar_x, 10, editor_width)
    }

    fn runner_snapshot_with_editor(
        sidebar_x: i64,
        editor_x: i64,
        editor_width: i64,
    ) -> serde_json::Value {
        serde_json::json!({
            "window": {
                "visual_geometry": {
                    "surfaces": [
                        surface_row("header-bar", 0, 0, 40, 2),
                        surface_row("workspace-sidebar", sidebar_x, 2, 10, 18),
                        surface_row("editor-viewport", editor_x, 2, editor_width, 18),
                        surface_row("source-view", editor_x, 2, editor_width, 18),
                        surface_row("minimap-shell", 0, 0, 40, 20),
                        surface_row("minimap-source-map", 0, 0, 40, 20),
                        surface_row("minimap-native-viewport", 0, 3, 40, 4),
                        surface_row("minimap-marker-strip", 35, 0, 5, 20)
                    ],
                    "pixel_anchors": [{
                        "name": "minimap-viewport-top-edge",
                        "surface": "minimap-native-viewport",
                        "visible": true,
                        "rect": {"x": 0, "y": 3, "width": 40, "height": 4}
                    }],
                    "scroll_anchors": [{
                        "name": "source-view",
                        "at_left": true,
                        "at_top": true,
                        "y_value_milli": 0
                    }],
                    "native_minimap": {"visible": true}
                }
            }
        })
    }

    #[test]
    fn editor_width_relationship_accepts_compact_overlay_sidebar_transition() {
        let show_case = serde_json::json!({"direction": "show", "size": {"width": 837}});
        let hide_case = serde_json::json!({"direction": "hide", "size": {"width": 837}});
        let wide_show_case = serde_json::json!({"direction": "show", "size": {"width": 1100}});
        let hidden_overlay = runner_snapshot_with_editor(-10, 0, 30);
        let visible_overlay = runner_snapshot_with_editor(0, 0, 30);

        assert!(editor_width_relationship(&show_case, &hidden_overlay, &visible_overlay).is_ok());
        assert!(editor_width_relationship(&hide_case, &visible_overlay, &hidden_overlay).is_ok());
        assert!(
            editor_width_relationship(&wide_show_case, &hidden_overlay, &visible_overlay)
                .expect_err("non-compact overlay is rejected")
                .contains("outside compact width")
        );
    }

    #[test]
    fn header_open_precedes_new_tab_requires_open_left_of_new_button() {
        let valid = runner_header_snapshot(4, 18, 40);
        let overlapping = runner_header_snapshot(20, 18, 40);
        let clipped = runner_header_snapshot(4, 34, 40);

        assert!(header_open_precedes_new_tab(&valid).is_ok());
        assert!(
            header_open_precedes_new_tab(&overlapping)
                .expect_err("overlapping controls should fail")
                .contains("before New Tab")
        );
        assert!(
            header_open_precedes_new_tab(&clipped)
                .expect_err("clipped controls should fail")
                .contains("exceed header bounds")
        );
    }

    fn runner_header_snapshot(open_x: i64, new_x: i64, header_width: i64) -> serde_json::Value {
        serde_json::json!({
            "window": {
                "visual_geometry": {
                    "surfaces": [
                        surface_row("header-bar", 0, 0, header_width, 12),
                        surface_row("header-open-menu-button", open_x, 2, 12, 8),
                        surface_row("header-new-tab-button", new_x, 2, 8, 8)
                    ]
                }
            }
        })
    }

    /// A reveal snapshot: viewport at y=100..500, header at `header_y`, the
    /// revealed row at `row_y`, two neighbours above and two below, each 34 px
    /// tall, and the sidebar scroll anchor at `scroll_milli`.
    fn reveal_snapshot(
        status: &str,
        header_y: i64,
        row_y: i64,
        gap_after_first: i64,
        scroll_milli: i64,
    ) -> serde_json::Value {
        let mut surfaces = vec![
            surface_row("workspace-sidebar-viewport", 0, 100, 300, 400),
            surface_row("workspace-revealed-section-header", 0, header_y, 300, 40),
            surface_row("workspace-revealed-row", 6, row_y, 288, 34),
        ];
        let tops = [
            row_y - 68 - gap_after_first,
            row_y - 34,
            row_y + 34,
            row_y + 68,
        ];
        for (index, top) in tops.into_iter().enumerate() {
            surfaces.push(surface_row(
                &format!("workspace-revealed-neighbour-{index}"),
                6,
                top,
                288,
                34,
            ));
        }
        serde_json::json!({
            "window": {
                "workspace": {"reveal_status": status, "reveal_generation": 3, "reveal_depth": 1},
                "visual_geometry": {
                    "surfaces": surfaces,
                    "scroll_anchors": [{
                        "name": "workspace-sidebar-viewport",
                        "at_top": scroll_milli == 0,
                        "y_value_milli": scroll_milli
                    }]
                }
            }
        })
    }

    /// A 120x80 screenshot of a sidebar row whose top edge is drawn at
    /// `drawn_top`: label ink from 10px below it, ending 25px below it.
    fn sidebar_row_screenshot(path: &Path, drawn_top: usize) {
        let background = (250, 250, 250, 255);
        let ink = (46, 52, 54, 255);
        let mut rows = vec![vec![background; 120]; 80];
        for row in rows.iter_mut().skip(drawn_top + 10).take(16) {
            for x in (20..100).step_by(3) {
                row[x] = ink;
            }
        }
        png::write_rgba_fixture(path, &rows).expect("write screenshot");
    }

    fn reveal_anchor_spec() -> Value {
        serde_json::json!({
            "name": "workspace-revealed-row-ink",
            "crop_surface": "workspace-revealed-row",
            "detector": "workspace-row-ink-bottom-edge",
            "reference_surface": "workspace-revealed-row",
            "reference_offset_y": 25,
            "min_pixels": 3,
            "max_screen_y_delta": 0
        })
    }

    fn reveal_anchor_snapshot(row_y: i64) -> Value {
        serde_json::json!({
            "window": {"visual_geometry": {"surfaces": [
                surface_row("workspace-revealed-row", 6, row_y, 108, 36)
            ]}}
        })
    }

    #[test]
    fn reference_pixel_anchor_passes_when_the_row_is_drawn_at_the_reported_row() {
        let root = tempfile::tempdir().expect("tempdir");
        let screenshot = root.path().join("after.png");
        sidebar_row_screenshot(&screenshot, 30);

        let report = evaluate_reference_pixel_anchor(
            &reveal_anchor_spec(),
            "workspace-revealed-row",
            &reveal_anchor_snapshot(30),
            &screenshot,
            root.path(),
        )
        .expect("anchor report");

        assert_eq!(report["status"], "passed", "{report}");
        assert_eq!(report["screen_y_delta"], 0);
    }

    #[test]
    fn reference_pixel_anchor_fails_a_row_drawn_two_pixels_below_its_report() {
        // The v0.8.1 content-box defect: drawn 2px off while every snapshot
        // relationship still holds.
        let root = tempfile::tempdir().expect("tempdir");
        let screenshot = root.path().join("after.png");
        sidebar_row_screenshot(&screenshot, 32);

        let report = evaluate_reference_pixel_anchor(
            &reveal_anchor_spec(),
            "workspace-revealed-row",
            &reveal_anchor_snapshot(30),
            &screenshot,
            root.path(),
        )
        .expect("anchor report");

        assert_eq!(report["status"], "failed", "{report}");
        assert_eq!(report["screen_y_delta"], 2);
        assert_eq!(
            report["failure_reason"],
            "rendered-row-differs-from-reported-row"
        );
    }

    #[test]
    fn reference_pixel_anchor_fails_when_no_reveal_surface_is_reported() {
        let root = tempfile::tempdir().expect("tempdir");
        let screenshot = root.path().join("after.png");
        sidebar_row_screenshot(&screenshot, 30);
        let snapshot = serde_json::json!({"window": {"visual_geometry": {"surfaces": []}}});

        let report = evaluate_reference_pixel_anchor(
            &reveal_anchor_spec(),
            "workspace-revealed-row",
            &snapshot,
            &screenshot,
            root.path(),
        )
        .expect("anchor report");

        assert_eq!(report["status"], "failed");
        assert!(
            report["failure_reason"]
                .as_str()
                .expect("reason")
                .contains("reference surface unavailable")
        );
    }

    #[test]
    fn reveal_cases_keep_the_workspace_sidebar_visible() {
        let value: Value = serde_json::from_str(include_str!(
            "../../../scripts/visual-geometry-scenarios/workspace-tree-reveal.json"
        ))
        .expect("scenario json");
        let cases = model::expanded_visual_cases(&value).expect("cases");
        let plan = serde_json::json!({ "gsettings": gsettings_plan(&cases[0]) });

        assert_gsettings_value(&plan, "workspace-sidebar-visible", "true");
    }

    fn reveal_case(fixture_kind: &str) -> serde_json::Value {
        serde_json::json!({
            "fixture_kind": fixture_kind,
            "manifest": {"scenario_type": "workspace-tree-reveal"}
        })
    }

    fn relationship_row<'a>(report: &'a Value, name: &str) -> &'a Value {
        report["assertions"]
            .as_array()
            .expect("assertion rows")
            .iter()
            .find(|row| row["name"] == name)
            .unwrap_or_else(|| panic!("missing relationship {name}: {report}"))
    }

    #[test]
    fn reveal_relationships_pass_for_a_contiguous_resting_revealed_row() {
        let before = reveal_snapshot("none", 60, 200, 0, 0);
        let after = reveal_snapshot("revealed", 60, 200, 0, 0);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("already-visible"),
            &before,
            &after,
            Some(&after),
        );

        assert_eq!(report["status"], "passed", "{report}");
        for name in [
            "reveal-status-revealed",
            "revealed-row-inside-sidebar-viewport",
            "neighbour-rows-contiguous",
            "revealed-row-at-rest",
            "outer-scroll-unchanged",
            "section-header-unchanged",
        ] {
            assert_eq!(
                relationship_row(&report, name)["status"],
                "passed",
                "{name}"
            );
        }
    }

    #[test]
    fn scrolling_reveal_cases_do_not_require_an_unchanged_outer_scroller() {
        let before = reveal_snapshot("none", 60, 200, 0, 0);
        let after = reveal_snapshot("revealed", -9_000, 200, 0, 9_000_000);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("beyond-cap"),
            &before,
            &after,
            Some(&after),
        );

        assert_eq!(report["status"], "passed", "{report}");
        let names: Vec<_> = report["assertions"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(!names.contains(&"outer-scroll-unchanged".to_string()));
        assert!(!names.contains(&"section-header-unchanged".to_string()));
    }

    #[test]
    fn rows_separated_by_a_constant_css_margin_are_contiguous() {
        let mut after = reveal_snapshot("revealed", 60, 200, 0, 0);
        // Re-space every row at a 36px pitch: 34px boxes with a 2px margin.
        let surfaces = after["window"]["visual_geometry"]["surfaces"]
            .as_array_mut()
            .expect("surfaces");
        for surface in surfaces.iter_mut() {
            let name = surface["name"].as_str().unwrap_or_default().to_string();
            let offset = match name.as_str() {
                "workspace-revealed-neighbour-0" => -2,
                "workspace-revealed-neighbour-1" => -1,
                "workspace-revealed-neighbour-2" => 1,
                "workspace-revealed-neighbour-3" => 2,
                _ => continue,
            };
            let y = surface["rect"]["y"].as_i64().expect("y");
            surface["rect"]["y"] = serde_json::json!(y + offset * 2);
        }
        assert_eq!(neighbour_rows_contiguous(&after), Ok(()));
    }

    #[test]
    fn a_gapped_neighbour_row_fails_contiguity_and_names_both_rectangles() {
        let after = reveal_snapshot("revealed", 60, 200, 1, 0);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("beyond-cap"),
            &after,
            &after,
            Some(&after),
        );

        assert_eq!(report["status"], "failed");
        let row = relationship_row(&report, "neighbour-rows-contiguous");
        assert_eq!(row["status"], "failed");
        let detail = row["detail"].as_str().expect("detail");
        assert!(
            detail.contains("workspace-revealed-neighbour-0"),
            "{detail}"
        );
        assert!(
            detail.contains("workspace-revealed-neighbour-1"),
            "{detail}"
        );
    }

    #[test]
    fn a_non_revealed_status_fails_the_case_with_the_status_named() {
        let after = reveal_snapshot("unsettled", 60, 200, 0, 0);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("beyond-cap"),
            &after,
            &after,
            Some(&after),
        );

        assert_eq!(report["status"], "failed");
        let row = relationship_row(&report, "reveal-status-revealed");
        assert!(
            row["detail"]
                .as_str()
                .expect("detail")
                .contains("unsettled"),
            "{row}"
        );
    }

    #[test]
    fn a_revealed_row_outside_the_viewport_fails_the_inside_relationship() {
        let after = reveal_snapshot("revealed", 60, 480, 0, 0);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("beyond-cap"),
            &after,
            &after,
            Some(&after),
        );

        let row = relationship_row(&report, "revealed-row-inside-sidebar-viewport");
        assert_eq!(row["status"], "failed");
        assert!(row["detail"].as_str().expect("detail").contains("outside"));
    }

    #[test]
    fn a_moving_revealed_row_or_missing_rest_snapshot_fails_at_rest() {
        let after = reveal_snapshot("revealed", 60, 200, 0, 0);
        let moved = reveal_snapshot("revealed", 60, 201, 0, 0);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("beyond-cap"),
            &after,
            &after,
            Some(&moved),
        );
        let row = relationship_row(&report, "revealed-row-at-rest");
        assert_eq!(row["status"], "failed");
        assert!(
            row["detail"]
                .as_str()
                .expect("detail")
                .contains("workspace-revealed-row"),
            "{row}"
        );

        let report =
            evaluate_allowed_region_relationships(&reveal_case("beyond-cap"), &after, &after, None);
        let row = relationship_row(&report, "revealed-row-at-rest");
        assert!(
            row["detail"]
                .as_str()
                .expect("detail")
                .contains("after-rest snapshot"),
            "{row}"
        );
    }

    #[test]
    fn an_at_rest_reveal_that_scrolls_or_moves_the_header_fails() {
        let before = reveal_snapshot("revealed", 60, 200, 0, 0);
        let scrolled = reveal_snapshot("revealed", 58, 198, 0, 2_000);
        let report = evaluate_allowed_region_relationships(
            &reveal_case("already-visible"),
            &before,
            &scrolled,
            Some(&scrolled),
        );

        assert_eq!(report["status"], "failed");
        assert_eq!(
            relationship_row(&report, "outer-scroll-unchanged")["status"],
            "failed"
        );
        assert_eq!(
            relationship_row(&report, "section-header-unchanged")["status"],
            "failed"
        );
    }

    fn surface_row(name: &str, x: i64, y: i64, width: i64, height: i64) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "visible": true,
            "rect": {"x": x, "y": y, "width": width, "height": height},
            "allocation": {"x": x, "y": y, "width": width, "height": height}
        })
    }

    fn assert_gsettings_value(payload: &serde_json::Value, key: &str, expected: &str) {
        let values = payload["gsettings"].as_array().expect("gsettings array");
        assert!(
            values
                .iter()
                .any(|value| value["key"] == key && value["value"] == expected),
            "missing gsettings {key}={expected}"
        );
    }
}
