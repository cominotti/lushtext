// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace CLI implementation for GTK visual proof tooling.
//!
//! The tool owns schema, summary, corpus, policy, and Rust same-session live
//! visual proof command surfaces. Python remains available only as an explicit
//! Rust-supervised diagnostic/oracle path.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gtk_lush_proof_spine::{ArtifactEnvelope, ProofStatus};
use serde_json::Value;

mod artifacts;
mod automation;
mod capture;
mod host;
mod live;
mod model;
mod png;
mod policy;
mod process;
mod runner;
mod warnings;

/// Tool version recorded in proof envelopes.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

const MAX_ARTIFACT_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// Run the cargo-gtk-proof CLI with caller-provided arguments and writers.
///
/// This function is kept separate from `main` so unit tests can exercise the
/// CLI contract without spawning a process.
///
/// # Errors
///
/// Returns any I/O error raised while writing human-readable output or JSON
/// result envelopes to the supplied writers.
pub fn run_cli<I, S>(args: I, stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<i32>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args: Vec<String> = args
        .into_iter()
        .map(Into::into)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if matches!(
        args.first().map(String::as_str),
        Some("gtk-proof" | "cargo-gtk-proof")
    ) {
        args.remove(0);
    }

    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        write_help(stdout)?;
        return Ok(0);
    }

    match args[0].as_str() {
        "run" => runner::handle_run(&args[1..], stdout, stderr),
        "schema" => handle_schema(&args[1..], stdout, stderr),
        "summarize" => handle_summarize(&args[1..], stdout),
        "corpus" => handle_corpus(&args[1..], stdout),
        "policy" => handle_policy(&args[1..], stdout),
        other => {
            writeln!(stderr, "unknown cargo-gtk-proof command: {other}")?;
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure(
                    "cargo-gtk-proof",
                    ProofStatus::UsageError,
                    "unknown command",
                ),
            )?;
            Ok(2)
        }
    }
}

fn write_help(stdout: &mut impl Write) -> io::Result<()> {
    writeln!(
        stdout,
        "\
cargo-gtk-proof {TOOL_VERSION}

USAGE:
    cargo gtk-proof <COMMAND> [ARGS]

COMMANDS:
    run        Run a visual-geometry scenario set
    schema     List or validate versioned proof schemas
    summarize  Summarize a proof artifact directory
    corpus     Replay the frozen compatibility corpus
    policy     Enforce visual proof policy

DEFAULTS:
    artifact root: build/smoke/visual-geometry
    scenario root: scripts/visual-geometry-scenarios

RUN FLAGS:
    --artifact-dir DIR   Write visual proof artifacts to DIR
    --scenario-dir DIR   Load visual scenario manifests from DIR
    --binary PATH        Use PATH as the LushText binary
    --case-filter TEXT   Run only expanded cases whose id contains TEXT
    --oracle python      Run the legacy Python visual runner as an explicit diagnostic oracle

CORPUS FLAGS:
    --parity             Compare Python-oracle and Rust fixture fields
    --oracle python      Alias for --parity with the Python oracle label

POLICY FLAGS:
    --artifact-dir DIR   Read visual proof artifacts from DIR
    --base-ref REF       Compare visual-sensitive changes against REF
    --require-rust-engine
                         Require authoritative cargo-gtk-proof engine metadata
"
    )
}

fn handle_schema(
    args: &[String],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<i32> {
    match args.first().map(String::as_str) {
        None | Some("list") => {
            write_envelope(
                stdout,
                &ArtifactEnvelope::success("schema", "supported schemas listed").with_data(
                    serde_json::json!({
                    "schemas": [
                        "visual-scenario",
                        "expanded-case",
                        "case-manifest",
                        "root-summary",
                        "case-summary",
                        "comparison-report",
                        "animation-report",
                        "warning-scan",
                        "parity-report",
                        "environment-report",
                        "proof-policy",
                            "artifact-envelope"
                        ],
                        "artifact_writers": artifacts::artifact_kind_labels(),
                        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
                    }),
                ),
            )?;
            Ok(0)
        }
        Some("validate") => {
            let Some(path) = args.get(1) else {
                writeln!(stderr, "schema validate requires a JSON path")?;
                write_envelope(
                    stdout,
                    &ArtifactEnvelope::failure(
                        "schema",
                        ProofStatus::UsageError,
                        "missing JSON path",
                    ),
                )?;
                return Ok(2);
            };
            validate_json_file(Path::new(path), stdout)
        }
        Some(other) => {
            writeln!(stderr, "unknown schema command: {other}")?;
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure(
                    "schema",
                    ProofStatus::UsageError,
                    "unknown schema command",
                ),
            )?;
            Ok(2)
        }
    }
}

fn validate_json_file(path: &Path, stdout: &mut impl Write) -> io::Result<i32> {
    let json = match read_json_value(path, "artifact JSON") {
        Ok(value) => value,
        Err(detail) => {
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure("schema", ProofStatus::ArtifactError, detail),
            )?;
            return Ok(1);
        }
    };

    let outcome = match model::validate_document(&json) {
        Ok(outcome) => outcome,
        Err(error) => {
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure("schema", error.status, error.detail),
            )?;
            return Ok(1);
        }
    };

    write_envelope(
        stdout,
        &ArtifactEnvelope::success("schema", "schema validation passed").with_data(
            serde_json::json!({
                "document_kind": outcome.kind.to_string(),
                "path": path,
                "schema_version": outcome.schema_version
            }),
        ),
    )?;
    Ok(0)
}

fn handle_summarize(args: &[String], stdout: &mut impl Write) -> io::Result<i32> {
    let root = args.first().map_or_else(
        || PathBuf::from("build/smoke/visual-geometry"),
        PathBuf::from,
    );
    let summary = root.join("summary.json");
    if summary.is_file() {
        validate_json_file(&summary, stdout)
    } else {
        write_envelope(
            stdout,
            &ArtifactEnvelope::failure(
                "summarize",
                ProofStatus::ArtifactError,
                format!("missing {}", summary.display()),
            ),
        )?;
        Ok(1)
    }
}

fn handle_corpus(args: &[String], stdout: &mut impl Write) -> io::Result<i32> {
    let config = match CorpusConfig::parse(args) {
        Ok(config) => config,
        Err(detail) => {
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure("corpus", ProofStatus::UsageError, detail),
            )?;
            return Ok(2);
        }
    };
    let root = config.root;
    let mut compared = 0u64;
    let mut failed = 0u64;
    let mut details = Vec::new();
    let mut status_cases = 0u64;

    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) => {
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure(
                    "corpus",
                    ProofStatus::ArtifactError,
                    format!("cannot read corpus root {}: {error}", root.display()),
                ),
            )?;
            return Ok(1);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                write_envelope(
                    stdout,
                    &ArtifactEnvelope::failure(
                        "corpus",
                        ProofStatus::ArtifactError,
                        format!("cannot read corpus entry under {}: {error}", root.display()),
                    ),
                )?;
                return Ok(1);
            }
        };
        let case_path = entry.path().join("case.json");
        if !case_path.is_file() {
            continue;
        }
        status_cases += 1;
        compared += 1;
        let json = match read_json_value(&case_path, "corpus case") {
            Ok(value) => value,
            Err(detail) => {
                write_envelope(
                    stdout,
                    &ArtifactEnvelope::failure("corpus", ProofStatus::ArtifactError, detail),
                )?;
                return Ok(1);
            }
        };
        if !json.get("python_status").is_some_and(Value::is_string)
            || !json.get("rust_status").is_some_and(Value::is_string)
        {
            write_envelope(
                stdout,
                &ArtifactEnvelope::failure(
                    "corpus",
                    ProofStatus::ArtifactError,
                    format!("malformed corpus case {}", case_path.display()),
                ),
            )?;
            return Ok(1);
        }
        let mismatches = parity_mismatches(&json);
        if !mismatches.is_empty() {
            failed += 1;
        }
        details.push(serde_json::json!({
            "case_id": json.get("case_id").and_then(Value::as_str).unwrap_or("unknown"),
            "status": if mismatches.is_empty() { "passed" } else { "failed" },
            "mismatches": mismatches,
        }));
    }

    if status_cases == 0 {
        write_envelope(
            stdout,
            &ArtifactEnvelope::failure(
                "corpus",
                ProofStatus::ArtifactError,
                format!("no corpus case.json fixtures found in {}", root.display()),
            ),
        )?;
        return Ok(1);
    }

    let png_stats = png::run_embedded_png_corpus();
    compared += png_stats.compared;
    failed += png_stats.failed;
    details.push(serde_json::json!({
        "case_id": "embedded-png-corpus",
        "status": if png_stats.failed == 0 { "passed" } else { "failed" },
        "detail": png_stats.detail,
    }));

    let envelope = if failed == 0 {
        ArtifactEnvelope::success(
            "corpus",
            if config.parity {
                "corpus parity replay passed"
            } else {
                "corpus replay passed"
            },
        )
    } else {
        ArtifactEnvelope::failure(
            "corpus",
            ProofStatus::Failed,
            if config.parity {
                "corpus parity replay mismatch"
            } else {
                "corpus replay mismatch"
            },
        )
    }
    .with_data(serde_json::json!({
        "corpus_root": safe_display_path(&root),
        "mode": if config.parity { "parity-replay" } else { "replay" },
        "schema_version": model::SUPPORTED_SCHEMA_VERSION,
        "rust_engine": {
            "name": "cargo-gtk-proof",
            "tool_version": TOOL_VERSION,
        },
        "oracle_engine": if config.parity {
            serde_json::json!({ "name": "python-visual-oracle", "mode": "fixture-replay" })
        } else {
            Value::Null
        },
        "compared": compared,
        "failed": failed,
        "failed_mismatch_count": failed,
        "checks": details,
    }));
    let code = i32::from(failed != 0);
    write_envelope(stdout, &envelope)?;
    Ok(code)
}

struct CorpusConfig {
    root: PathBuf,
    parity: bool,
}

impl CorpusConfig {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut root = None;
        let mut parity = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--parity" => {
                    parity = true;
                    index += 1;
                }
                "--oracle" => {
                    let Some(value) = args.get(index + 1) else {
                        return Err("missing --oracle value".to_string());
                    };
                    if value != "python" {
                        return Err(format!("unsupported corpus oracle: {value}"));
                    }
                    parity = true;
                    index += 2;
                }
                value if value.starts_with('-') => {
                    return Err(format!("unknown corpus argument: {value}"));
                }
                value => {
                    if root.is_some() {
                        return Err(format!("unexpected extra corpus path: {value}"));
                    }
                    root = Some(PathBuf::from(value));
                    index += 1;
                }
            }
        }
        Ok(Self {
            root: root.unwrap_or_else(default_corpus_root),
            parity,
        })
    }
}

fn parity_mismatches(case: &Value) -> Vec<Value> {
    let pairs = [
        ("status", "python_status", "rust_status"),
        ("exit_class", "python_exit_class", "rust_exit_class"),
        (
            "verified_invariant_ids",
            "python_verified_invariant_ids",
            "rust_verified_invariant_ids",
        ),
        (
            "warning_scan_status",
            "python_warning_scan_status",
            "rust_warning_scan_status",
        ),
        ("summary_path", "python_summary_path", "rust_summary_path"),
        (
            "artifact_root_shape",
            "python_artifact_root_shape",
            "rust_artifact_root_shape",
        ),
        ("bounded_detail", "python_detail", "rust_detail"),
    ];
    let mut mismatches: Vec<Value> = pairs
        .iter()
        .filter_map(|(field, python_key, rust_key)| {
            let python = case.get(*python_key);
            let rust = case.get(*rust_key);
            if python.is_none() && rust.is_none() {
                return None;
            }
            if python == rust {
                None
            } else {
                Some(serde_json::json!({
                    "field": field,
                    "python": python.cloned().unwrap_or(Value::Null),
                    "rust": rust.cloned().unwrap_or(Value::Null),
                }))
            }
        })
        .collect();
    for (field, key) in [
        ("python_engine_metadata", "python_engine"),
        ("rust_engine_metadata", "rust_engine"),
    ] {
        if let Some(engine) = case.get(key)
            && engine
                .get("name")
                .and_then(Value::as_str)
                .is_none_or(|name| name.trim().is_empty())
        {
            mismatches.push(serde_json::json!({
                "field": field,
                "detail": "engine metadata requires non-empty name",
            }));
        }
    }
    mismatches
}

fn default_corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/proof-corpus")
}

pub(crate) fn read_json_value(path: &Path, label: &str) -> Result<Value, String> {
    let text = read_bounded_text(path, label)?;
    serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("invalid JSON {}: {error}", path.display()))
}

fn read_bounded_text(path: &Path, label: &str) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    if metadata.len() > MAX_ARTIFACT_JSON_BYTES {
        return Err(format!(
            "{label} {} exceeds JSON byte limit of {}",
            path.display(),
            MAX_ARTIFACT_JSON_BYTES
        ));
    }
    fs::read_to_string(path)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))
}

fn safe_display_path(path: &Path) -> String {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canonical_root = repo_root.canonicalize().unwrap_or(repo_root);
    canonical_path
        .strip_prefix(&canonical_root)
        .unwrap_or(&canonical_path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn handle_policy(args: &[String], stdout: &mut impl Write) -> io::Result<i32> {
    if args.iter().any(|arg| arg == "--self-test") {
        return match policy::run_self_tests() {
            Ok(()) => {
                write_envelope(
                    stdout,
                    &ArtifactEnvelope::success("policy", "policy self-tests passed"),
                )?;
                Ok(0)
            }
            Err(detail) => {
                write_envelope(
                    stdout,
                    &ArtifactEnvelope::failure("policy", ProofStatus::PolicyFailure, detail),
                )?;
                Ok(1)
            }
        };
    }
    let mut artifact_dir = PathBuf::from("build/smoke/visual-geometry");
    let mut base_ref = None;
    let mut require_rust_engine = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--artifact-dir" => {
                let Some(value) = args.get(index + 1) else {
                    write_envelope(
                        stdout,
                        &ArtifactEnvelope::failure(
                            "policy",
                            ProofStatus::UsageError,
                            "missing --artifact-dir value",
                        ),
                    )?;
                    return Ok(2);
                };
                artifact_dir = PathBuf::from(value);
                index += 2;
            }
            "--base-ref" => {
                let Some(value) = args.get(index + 1) else {
                    write_envelope(
                        stdout,
                        &ArtifactEnvelope::failure(
                            "policy",
                            ProofStatus::UsageError,
                            "missing --base-ref value",
                        ),
                    )?;
                    return Ok(2);
                };
                base_ref = Some(value.as_str());
                index += 2;
            }
            "--require-rust-engine" => {
                require_rust_engine = true;
                index += 1;
            }
            other => {
                write_envelope(
                    stdout,
                    &ArtifactEnvelope::failure(
                        "policy",
                        ProofStatus::UsageError,
                        format!("unknown policy argument: {other}"),
                    ),
                )?;
                return Ok(2);
            }
        }
    }

    let outcome = policy::check_policy(&artifact_dir, base_ref, require_rust_engine);
    let envelope = if outcome.ok {
        ArtifactEnvelope::success("policy", outcome.detail)
    } else {
        ArtifactEnvelope::failure("policy", ProofStatus::PolicyFailure, outcome.detail)
    };
    let code = i32::from(!outcome.ok);
    write_envelope(stdout, &envelope)?;
    Ok(code)
}

pub(crate) fn write_envelope(
    stdout: &mut impl Write,
    envelope: &ArtifactEnvelope,
) -> io::Result<()> {
    let mut envelope = envelope.clone();
    if envelope.version.tool_version.is_none() {
        envelope.version = envelope.version.with_tool_version(TOOL_VERSION);
    }
    serde_json::to_writer_pretty(&mut *stdout, &envelope)?;
    writeln!(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_required_subcommands() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(["--help"], &mut stdout, &mut stderr).expect("run help");
        let output = String::from_utf8(stdout).expect("utf8 help");

        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        for command in ["run", "schema", "summarize", "corpus", "policy"] {
            assert!(output.contains(command), "missing {command} from help");
        }
    }

    #[test]
    fn cargo_external_subcommand_prefix_is_accepted() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(["gtk-proof", "schema", "list"], &mut stdout, &mut stderr)
            .expect("run prefixed schema list");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 0);
        assert_eq!(output["ok"], true);
        assert_eq!(output["command"], "schema");
        assert_eq!(output["version"]["tool_version"], TOOL_VERSION);
    }

    #[test]
    fn schema_validate_rejects_missing_version() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("missing-version.json");
        fs::write(&path, "{}").expect("fixture");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            ["schema", "validate", path.to_str().expect("utf8 path")],
            &mut stdout,
            &mut stderr,
        )
        .expect("run schema");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "malformed-field");
    }

    #[test]
    fn schema_validate_reports_missing_file() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            ["schema", "validate", "/definitely/missing/proof.json"],
            &mut stdout,
            &mut stderr,
        )
        .expect("run schema");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "artifact-error");
    }

    #[test]
    fn invalid_arguments_return_usage_envelope_shape() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code =
            run_cli(["policy", "--unknown"], &mut stdout, &mut stderr).expect("run invalid policy");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 2);
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "usage-error");
        assert_eq!(output["command"], "policy");
        assert!(
            output["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("--unknown"))
        );
        assert_eq!(output["version"]["schema_version"], 1);
        assert!(output["data"].is_object());
    }

    #[test]
    fn corpus_reports_compared_cases() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(["corpus"], &mut stdout, &mut stderr).expect("run corpus");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 0);
        assert_eq!(output["data"]["failed"], 0);
        assert!(
            output["data"]["compared"].as_u64().unwrap_or_default() >= 2,
            "expected checked-in corpus fixtures"
        );
    }

    #[test]
    fn corpus_parity_reports_oracle_metadata() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code =
            run_cli(["corpus", "--parity"], &mut stdout, &mut stderr).expect("run corpus parity");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 0);
        assert_eq!(output["ok"], true);
        assert_eq!(output["data"]["mode"], "parity-replay");
        assert_eq!(
            output["data"]["oracle_engine"]["name"],
            "python-visual-oracle"
        );
        assert_eq!(output["data"]["failed_mismatch_count"], 0);
        assert!(output["data"]["compared"].as_u64().unwrap_or_default() >= 12);
    }

    #[test]
    fn corpus_reports_representative_artifact_status_classes() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code =
            run_cli(["corpus", "--parity"], &mut stdout, &mut stderr).expect("run corpus parity");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");
        let checks = output["data"]["checks"]
            .as_array()
            .expect("corpus checks array");

        assert_eq!(code, 0);
        for case_id in [
            "pass-basic",
            "fail-basic",
            "skip-basic",
            "unsupported-host",
            "malformed-artifact",
            "warning-scan",
            "missing-stream-mode",
            "missing-intermediate-frame",
            "png-comparison-detectors",
            "embedded-png-corpus",
        ] {
            assert!(
                checks.iter().any(|check| check["case_id"] == case_id),
                "missing representative corpus case {case_id}"
            );
        }
    }

    #[test]
    fn corpus_parity_mismatch_fails() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case_dir = tempdir.path().join("mismatch");
        fs::create_dir(&case_dir).expect("case dir");
        fs::write(
            case_dir.join("case.json"),
            serde_json::json!({
                "schema_version": 1,
                "case_id": "mismatch",
                "python_status": "passed",
                "rust_status": "failed",
                "python_exit_class": "ok",
                "rust_exit_class": "failed"
            })
            .to_string(),
        )
        .expect("fixture");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            [
                "corpus",
                "--parity",
                tempdir.path().to_str().expect("utf8 path"),
            ],
            &mut stdout,
            &mut stderr,
        )
        .expect("run corpus parity");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["status"], "failed");
        assert_eq!(output["data"]["failed_mismatch_count"], 1);
        assert_eq!(
            output["data"]["checks"][0]["mismatches"][0]["field"],
            "status"
        );
    }

    #[test]
    fn corpus_rejects_missing_root() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            ["corpus", "/definitely/missing/proof-corpus"],
            &mut stdout,
            &mut stderr,
        )
        .expect("run corpus");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["status"], "artifact-error");
    }

    #[test]
    fn run_output_is_bounded_and_points_to_relative_artifacts() {
        let artifact_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/tmp/cargo-gtk-proof-terminal-output");
        let _ = fs::remove_dir_all(&artifact_dir);
        let scenario_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/visual-geometry-scenarios");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            [
                "run",
                "--artifact-dir",
                artifact_dir.to_str().expect("utf8 artifact path"),
                "--scenario-dir",
                scenario_dir.to_str().expect("utf8 scenario path"),
                "--binary",
                "target/debug/missing-lushtext",
                "--case-filter",
                "live-threshold",
            ],
            &mut stdout,
            &mut stderr,
        )
        .expect("run rust visual proof");
        let output_text = String::from_utf8(stdout.clone()).expect("utf8 output");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 3);
        assert_eq!(output["status"], "unsupported-host");
        for field in ["artifact_dir", "environment_report", "summary"] {
            let path = output["data"][field].as_str().expect("relative path field");
            assert!(
                path.starts_with("build/tmp/cargo-gtk-proof-terminal-output"),
                "{field} should be repo-relative, got {path}"
            );
        }
        for private_term in [
            "draft bodies",
            "local-history contents",
            "complete search result text",
            "iVBOR",
            "raw image data",
        ] {
            assert!(
                !output_text.contains(private_term),
                "terminal output leaked forbidden term {private_term}"
            );
        }
        assert!(
            output_text.len() < 16 * 1024,
            "unsupported-host terminal envelope should stay bounded"
        );
        assert!(stderr.is_empty());
        fs::remove_dir_all(&artifact_dir).expect("cleanup artifact dir");
    }

    #[test]
    fn run_rejects_unsafe_artifact_root_with_usage_envelope() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/visual-geometry-scenarios");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            [
                "run",
                "--artifact-dir",
                repo_root.to_str().expect("utf8 repo root"),
                "--scenario-dir",
                scenario_dir.to_str().expect("utf8 scenario path"),
                "--binary",
                "target/debug/missing-lushtext",
            ],
            &mut stdout,
            &mut stderr,
        )
        .expect("run unsafe root");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["status"], "artifact-error");
        assert!(
            output["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("refusing to reset unsafe"))
        );
    }

    #[test]
    fn corpus_rejects_malformed_case_json() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let case_dir = tempdir.path().join("bad");
        fs::create_dir(&case_dir).expect("case dir");
        fs::write(case_dir.join("case.json"), "{").expect("bad fixture");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            ["corpus", tempdir.path().to_str().expect("utf8 path")],
            &mut stdout,
            &mut stderr,
        )
        .expect("run corpus");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["status"], "artifact-error");
    }

    #[test]
    fn schema_validate_accepts_current_scenario_manifest() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/visual-geometry-scenarios/minimap-sidebar-live-threshold.json");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            ["schema", "validate", path.to_str().expect("utf8 path")],
            &mut stdout,
            &mut stderr,
        )
        .expect("run schema");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 0);
        assert_eq!(output["ok"], true);
        assert_eq!(output["data"]["document_kind"], "visual-scenario");
    }

    #[test]
    fn schema_validate_rejects_unsupported_version_distinctly() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("future.json");
        fs::write(
            &path,
            serde_json::json!({
                "schema_version": 999,
                "scenario_id": "future",
                "scenario_type": "minimap-sidebar",
                "matrix": { "sizes": [], "color_schemes": [] },
                "protected_regions": []
            })
            .to_string(),
        )
        .expect("fixture");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            ["schema", "validate", path.to_str().expect("utf8 path")],
            &mut stdout,
            &mut stderr,
        )
        .expect("run schema");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["status"], "unsupported-schema-version");
    }

    #[test]
    fn read_json_rejects_oversized_input() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("oversized.json");
        let oversized = " ".repeat(usize::try_from(MAX_ARTIFACT_JSON_BYTES).expect("limit") + 1);
        fs::write(&path, oversized).expect("fixture");

        let error = read_json_value(&path, "artifact JSON").expect_err("oversized rejected");

        assert!(error.contains("exceeds JSON byte limit"));
        assert!(error.contains("artifact JSON"));
    }

    #[test]
    fn policy_self_test_runs_real_policy_checks() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(["policy", "--self-test"], &mut stdout, &mut stderr)
            .expect("run policy self-test");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 0);
        assert_eq!(output["ok"], true);
        assert_eq!(output["detail"], "policy self-tests passed");
    }

    #[test]
    fn policy_strict_engine_rejects_python_only_summary_after_migration() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let summary = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "case_count": 1,
            "passed": 1,
            "failed": 0,
            "skipped": 0,
            "visual_proof_policy": {
                "changed_files_digest": policy_digest_for_test(policy_visual_sensitive_status_paths_for_test())
            },
            "pixel_verified_invariant_ids": ["native-minimap-highlight-anchors"],
            "animation_verified_invariant_ids": ["native-minimap-animation-highlight-anchors"],
            "engine": {
                "name": "python-visual-oracle",
                "authoritative": false
            },
            "cases": [{
                "status": "passed",
                "pixel_verified_invariant_ids": ["native-minimap-highlight-anchors"],
                "pixel_anchor_evidence": [{
                    "name": "minimap-native-viewport-top-edge",
                    "before_row_y": 10,
                    "after_row_y": 10
                }],
                "final_geometry": {"before": [], "after": []},
                "animation_verified_invariant_ids": ["native-minimap-animation-highlight-anchors"],
                "animation_frame_evidence": {
                    "status": "passed",
                    "capture_mode": "stream",
                    "sampled_frame_count": 1,
                    "mapped_intermediate_frame_count": 1,
                    "max_sample_skew_ms": 80,
                    "max_sample_skew_observed_ms": 12,
                    "frames": [{
                        "status": "passed",
                        "mapped_sample_elapsed_ms": 48,
                        "sample_skew_ms": 12,
                        "sidebar_phase": "intermediate",
                        "anchors": [{
                            "status": "passed",
                            "baseline_row_y": 10,
                            "frame_row_y": 10
                        }]
                    }]
                }
            }]
        });
        fs::write(
            tempdir.path().join("summary.json"),
            serde_json::to_string_pretty(&summary).expect("summary json"),
        )
        .expect("write summary");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_cli(
            [
                "policy",
                "--artifact-dir",
                tempdir.path().to_str().expect("utf8 tempdir"),
                "--base-ref",
                "HEAD",
                "--require-rust-engine",
            ],
            &mut stdout,
            &mut stderr,
        )
        .expect("run policy");
        let output: Value = serde_json::from_slice(&stdout).expect("json output");

        assert_eq!(code, 1);
        assert_eq!(output["status"], "policy-failure");
        assert!(
            output["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("not cargo-gtk-proof"))
        );
        assert!(stderr.is_empty());
    }

    fn policy_digest_for_test(paths: Vec<String>) -> String {
        use sha2::{Digest, Sha256};

        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut entries = Vec::new();
        for path in paths {
            let absolute = repo_root.join(&path);
            let data = fs::read(&absolute).expect("test visual-sensitive file");
            entries.push(serde_json::json!({
                "path": path,
                "state": "file",
                "size": data.len(),
                "sha256": format!("{:x}", Sha256::digest(&data)),
            }));
        }
        let encoded = serde_json::to_vec(&entries).expect("digest json");
        format!("{:x}", Sha256::digest(&encoded))
    }

    fn path_has_extension(path: &str, expected: &str) -> bool {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
    }

    fn policy_visual_sensitive_status_paths_for_test() -> Vec<String> {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain=v1", "--untracked-files=all"])
            .current_dir(&repo_root)
            .output()
            .expect("git status");
        assert!(output.status.success());
        let mut paths = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.get(3..))
            .map(|path| {
                path.split_once(" -> ")
                    .map_or(path, |(_old, new)| new)
                    .replace('\\', "/")
            })
            .filter(|path| {
                path == "crates/lushtext-core/src/model/automation.rs"
                    || path == "scripts/check-visual-proof-policy.py"
                    || path == "scripts/lushtext-automation.py"
                    || path == "scripts/test-visual-geometry.py"
                    || path == "scripts/visual-geometry-smoke.py"
                    || path == "scripts/visual_geometry_png.py"
                    || path.starts_with("crates/lushtext-core/src/ui/")
                    || path.starts_with("crates/lushtext/tests/widget/")
                    || path.starts_with("resources/ui/")
                    || path.starts_with("resources/style/")
                    || path.starts_with("scripts/visual-geometry-scenarios/")
                    || path_has_extension(path, "blp")
                    || path_has_extension(path, "css")
                    || path_has_extension(path, "ui")
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        assert!(
            !paths.is_empty(),
            "strict policy fixture expects this change to be visual-sensitive"
        );
        paths
    }
}
