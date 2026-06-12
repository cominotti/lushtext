// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace CLI implementation for GTK visual proof tooling.
//!
//! The tool starts with schema, summary, corpus, and policy command surfaces so
//! wrappers can migrate without command drift. Live visual capture is added in
//! later implementation slices; until then, unsupported live commands report a
//! stable proof-spine envelope instead of pretending coverage passed.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gtk_lush_proof_spine::{ArtifactEnvelope, ProofStatus};
use serde_json::Value;

mod model;
mod png;
mod policy;

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
        "run" => write_envelope(
            stdout,
            &ArtifactEnvelope::failure(
                "run",
                ProofStatus::UnsupportedHost,
                "live visual runner is not implemented in this slice",
            ),
        )
        .map(|()| 3),
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
                            "root-summary",
                            "comparison-report",
                            "animation-report",
                            "proof-policy",
                            "artifact-envelope"
                        ],
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
    let root = args.first().map_or_else(default_corpus_root, PathBuf::from);
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
        if json.get("python_status") != json.get("rust_status") {
            failed += 1;
        }
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
    details.push(png_stats.detail);

    let envelope = if failed == 0 {
        ArtifactEnvelope::success("corpus", "corpus replay passed")
    } else {
        ArtifactEnvelope::failure("corpus", ProofStatus::Failed, "corpus replay mismatch")
    }
    .with_data(serde_json::json!({
        "corpus_root": safe_display_path(&root),
        "compared": compared,
        "failed": failed,
        "checks": details,
    }));
    let code = i32::from(failed != 0);
    write_envelope(stdout, &envelope)?;
    Ok(code)
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

    let outcome = policy::check_policy(&artifact_dir, base_ref);
    let envelope = if outcome.ok {
        ArtifactEnvelope::success("policy", outcome.detail)
    } else {
        ArtifactEnvelope::failure("policy", ProofStatus::PolicyFailure, outcome.detail)
    };
    let code = i32::from(!outcome.ok);
    write_envelope(stdout, &envelope)?;
    Ok(code)
}

fn write_envelope(stdout: &mut impl Write, envelope: &ArtifactEnvelope) -> io::Result<()> {
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
}
