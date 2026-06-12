// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded artifact writing for the workspace proof tool.
//!
//! Visual proof artifacts are diagnostic evidence, not user data, but they can
//! still contain paths, logs, and screenshots. This module keeps the filesystem
//! boundary small so runner code cannot accidentally clear broad directories or
//! print large payloads to the terminal.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::{TOOL_VERSION, model};

/// Largest JSON artifact the Rust proof tool writes in one file.
///
/// Eight MiB is intentionally aligned with the existing read cap in `lib.rs`:
/// large enough for current root summaries, but small enough to catch runaway
/// log or snapshot embedding before CI uploads unbounded evidence.
pub(crate) const MAX_WRITTEN_JSON_BYTES: usize = 8 * 1024 * 1024;

/// Artifact classes the proof runner writes through the bounded JSON path.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProofArtifactKind {
    /// Versioned per-case scenario manifest.
    Manifest,
    /// Versioned expanded matrix case.
    ExpandedCase,
    /// Root summary for a whole visual proof run.
    RootSummary,
    /// Per-case summary consumed by artifact-summary tooling.
    CaseSummary,
    /// Protected-region and rendered-anchor comparison report.
    ComparisonReport,
    /// Timestamp-correlated animation-frame report.
    AnimationReport,
    /// Bounded warning scan report.
    WarningScan,
    /// Python/Rust oracle parity report.
    ParityReport,
    /// Host and runtime environment report.
    EnvironmentReport,
    /// Skipped or unsupported-host non-proof report.
    SkipReport,
}

impl ProofArtifactKind {
    /// Return the stable artifact writer label used in diagnostics.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Manifest => "case manifest",
            Self::ExpandedCase => "expanded case",
            Self::RootSummary => "root summary",
            Self::CaseSummary => "case summary",
            Self::ComparisonReport => "comparison report",
            Self::AnimationReport => "animation report",
            Self::WarningScan => "warning scan",
            Self::ParityReport => "parity report",
            Self::EnvironmentReport => "environment report",
            Self::SkipReport => "skip report",
        }
    }
}

const ALL_PROOF_ARTIFACT_KINDS: [ProofArtifactKind; 10] = [
    ProofArtifactKind::Manifest,
    ProofArtifactKind::ExpandedCase,
    ProofArtifactKind::RootSummary,
    ProofArtifactKind::CaseSummary,
    ProofArtifactKind::ComparisonReport,
    ProofArtifactKind::AnimationReport,
    ProofArtifactKind::WarningScan,
    ProofArtifactKind::ParityReport,
    ProofArtifactKind::EnvironmentReport,
    ProofArtifactKind::SkipReport,
];

/// Return all artifact writer labels supported by the bounded writer.
pub(crate) fn artifact_kind_labels() -> Vec<&'static str> {
    ALL_PROOF_ARTIFACT_KINDS
        .iter()
        .map(|kind| kind.label())
        .collect()
}

/// Stable metadata that tells agents which proof engine produced an artifact.
#[derive(Debug, Serialize)]
pub(crate) struct EngineMetadata {
    name: &'static str,
    mode: &'static str,
    tool_version: &'static str,
    authoritative: bool,
}

impl EngineMetadata {
    /// Metadata for the staged Rust runner before live visual parity is proven.
    pub(crate) const fn rust_staged() -> Self {
        Self {
            name: "cargo-gtk-proof",
            mode: "rust-staged-runner",
            tool_version: TOOL_VERSION,
            authoritative: false,
        }
    }

    /// Metadata for authoritative same-session Rust visual proof summaries.
    pub(crate) const fn rust_live() -> Self {
        Self {
            name: "cargo-gtk-proof",
            mode: "rust-live-runner",
            tool_version: TOOL_VERSION,
            authoritative: true,
        }
    }
}

/// Summary of the scenario source loaded by `cargo gtk-proof run`.
#[derive(Debug, Serialize)]
pub(crate) struct ScenarioSourceSummary {
    scenario_dir: String,
    manifest_count: usize,
    expanded_case_count: usize,
    case_filter: Option<String>,
    scenarios: Vec<model::ScenarioOverview>,
}

impl ScenarioSourceSummary {
    /// Build a bounded source summary from validated scenario manifests.
    pub(crate) fn new(
        scenario_dir: &Path,
        scenarios: Vec<model::ScenarioOverview>,
        case_filter: Option<String>,
    ) -> Self {
        let expanded_case_count = scenarios.iter().map(|scenario| scenario.case_count).sum();
        Self {
            scenario_dir: safe_display_path(scenario_dir),
            manifest_count: scenarios.len(),
            expanded_case_count,
            case_filter,
            scenarios,
        }
    }

    /// Return the number of expanded cases represented by this source.
    pub(crate) const fn expanded_case_count(&self) -> usize {
        self.expanded_case_count
    }
}

/// Root summary written when Rust cannot yet claim live visual proof.
#[derive(Debug, Serialize)]
struct NonProofRootSummary {
    schema_version: u64,
    status: &'static str,
    skip_reason: String,
    case_count: usize,
    passed: u64,
    failed: u64,
    skipped: usize,
    cases: Vec<Value>,
    engine: EngineMetadata,
    scenario_source: ScenarioSourceSummary,
    artifact_root: String,
    missing_capabilities: Vec<String>,
}

/// Reset an artifact directory after guarding against broad or surprising roots.
pub(crate) fn reset_artifact_dir(artifact_dir: &Path) -> Result<(), String> {
    let resolved = resolve_for_guard(artifact_dir)?;
    let repo_root = repo_root();
    let mut forbidden = vec![PathBuf::from("/"), repo_root.clone()];
    if let Some(home) = home_dir().and_then(|path| path.canonicalize().ok()) {
        forbidden.push(home);
    }
    if let Some(parent) = repo_root.parent() {
        forbidden.push(parent.to_path_buf());
    }
    if forbidden.iter().any(|path| path == &resolved) {
        return Err(format!(
            "refusing to reset unsafe visual geometry artifact dir: {}",
            resolved.display()
        ));
    }
    if artifact_dir.exists() {
        let metadata = fs::symlink_metadata(artifact_dir)
            .map_err(|error| format!("cannot inspect {}: {error}", artifact_dir.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing to reset symlink artifact dir: {}",
                artifact_dir.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(format!(
                "refusing to reset non-directory artifact path: {}",
                artifact_dir.display()
            ));
        }
        ensure_existing_dir_owned_by_current_user(artifact_dir, &metadata)?;
        fs::remove_dir_all(artifact_dir)
            .map_err(|error| format!("cannot reset {}: {error}", artifact_dir.display()))?;
    }
    fs::create_dir_all(artifact_dir)
        .map_err(|error| format!("cannot create {}: {error}", artifact_dir.display()))
}

/// Write a skipped root summary that preserves scenario and host diagnostics.
pub(crate) fn write_non_proof_summary(
    artifact_dir: &Path,
    scenario_source: ScenarioSourceSummary,
    skip_reason: impl Into<String>,
    missing_capabilities: Vec<String>,
) -> Result<PathBuf, String> {
    write_non_proof_summary_with_cases(
        artifact_dir,
        scenario_source,
        skip_reason,
        missing_capabilities,
        Vec::new(),
    )
}

/// Write a skipped root summary with explicit non-proof case rows.
pub(crate) fn write_non_proof_summary_with_cases(
    artifact_dir: &Path,
    scenario_source: ScenarioSourceSummary,
    skip_reason: impl Into<String>,
    missing_capabilities: Vec<String>,
    cases: Vec<Value>,
) -> Result<PathBuf, String> {
    let summary = NonProofRootSummary {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status: "skipped",
        skip_reason: skip_reason.into(),
        case_count: scenario_source.expanded_case_count(),
        passed: 0,
        failed: 0,
        skipped: scenario_source.expanded_case_count(),
        cases,
        engine: EngineMetadata::rust_staged(),
        scenario_source,
        artifact_root: safe_display_path(artifact_dir),
        missing_capabilities,
    };
    let path = artifact_dir.join("summary.json");
    write_artifact(&path, ProofArtifactKind::SkipReport, &summary)?;
    Ok(path)
}

/// Write a typed proof artifact through the bounded JSON writer.
pub(crate) fn write_artifact<T>(
    path: &Path,
    kind: ProofArtifactKind,
    value: &T,
) -> Result<PathBuf, String>
where
    T: Serialize,
{
    write_json(path, value).map_err(|error| format!("cannot write {}: {error}", kind.label()))?;
    Ok(path.to_path_buf())
}

/// Serialize a JSON artifact through a temp file in the destination directory.
pub(crate) fn write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?;
    if bytes.len() > MAX_WRITTEN_JSON_BYTES {
        return Err(format!(
            "{} exceeds JSON byte limit of {}",
            path.display(),
            MAX_WRITTEN_JSON_BYTES
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let mut temp = NamedTempFile::new_in(parent).map_err(|error| {
        format!(
            "cannot create temp artifact in {}: {error}",
            parent.display()
        )
    })?;
    temp.write_all(&bytes)
        .map_err(|error| format!("cannot write temp artifact for {}: {error}", path.display()))?;
    temp.write_all(b"\n").map_err(|error| {
        format!(
            "cannot finish temp artifact for {}: {error}",
            path.display()
        )
    })?;
    temp.flush()
        .map_err(|error| format!("cannot flush temp artifact for {}: {error}", path.display()))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("cannot sync temp artifact for {}: {error}", path.display()))?;
    temp.persist(path)
        .map_err(|error| format!("cannot persist {}: {error}", path.display()))?;
    Ok(())
}

fn ensure_existing_dir_owned_by_current_user(
    artifact_dir: &Path,
    metadata: &fs::Metadata,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;

        let Ok(current_process) = fs::metadata("/proc/self") else {
            return Ok(());
        };
        let current_uid = current_process.uid();
        if metadata.uid() != current_uid {
            return Err(format!(
                "refusing to reset non-owned artifact dir: {}",
                artifact_dir.display()
            ));
        }
    }
    Ok(())
}

/// Display proof paths relative to the repository when possible.
pub(crate) fn safe_display_path(path: &Path) -> String {
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canonical_root = repo_root();
    canonical_path
        .strip_prefix(&canonical_root)
        .unwrap_or(&canonical_path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn resolve_for_guard(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("artifact directory path is empty".to_string());
    }
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|error| format!("cannot resolve {}: {error}", path.display()));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("cannot read current directory: {error}"))?
            .join(path)
    };
    let mut missing_components = Vec::new();
    let mut ancestor = absolute.as_path();
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| format!("{} has no existing parent directory", absolute.display()))?;
        missing_components.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| format!("{} has no existing parent directory", absolute.display()))?;
    }
    let mut resolved = ancestor
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", ancestor.display()))?;
    for component in missing_components.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_artifact_dir_refuses_repo_root() {
        let error = reset_artifact_dir(&repo_root()).expect_err("repo root rejected");

        assert!(error.contains("refusing to reset unsafe"));
    }

    #[test]
    fn reset_artifact_dir_refuses_empty_path() {
        let error = reset_artifact_dir(Path::new("")).expect_err("empty path rejected");

        assert!(error.contains("empty"));
    }

    #[test]
    fn reset_artifact_dir_refuses_non_directory() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let file = tempdir.path().join("artifact-file");
        fs::write(&file, "not a dir").expect("fixture file");

        let error = reset_artifact_dir(&file).expect_err("file rejected");

        assert!(error.contains("non-directory"));
    }

    #[test]
    fn reset_artifact_dir_allows_nested_missing_artifact_path() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let nested = tempdir.path().join("one/two/artifacts");

        reset_artifact_dir(&nested).expect("nested artifact dir");

        assert!(nested.is_dir());
    }

    #[test]
    fn reset_artifact_dir_refuses_symlink() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let target = tempdir.path().join("target");
        let link = tempdir.path().join("link");
        fs::create_dir(&target).expect("target dir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        #[cfg(unix)]
        {
            let error = reset_artifact_dir(&link).expect_err("symlink rejected");
            assert!(error.contains("symlink"));
        }
    }

    #[test]
    fn write_non_proof_summary_is_schema_valid() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let source = ScenarioSourceSummary::new(
            Path::new("scripts/visual-geometry-scenarios"),
            vec![model::ScenarioOverview {
                scenario_id: "example".to_string(),
                scenario_type: "minimap-sidebar".to_string(),
                case_count: 2,
                case_ids: vec![
                    "example--wide--force-light--wrap-true--hide".to_string(),
                    "example--wide--force-light--wrap-true--show".to_string(),
                ],
                readiness_predicates: vec!["visual-geometry-settled".to_string()],
                pixel_anchor_count: 1,
                relative_pixel_anchor_count: 0,
                animation_enabled: true,
            }],
            None,
        );

        let path = write_non_proof_summary(
            tempdir.path(),
            source,
            "missing required command: mutter",
            vec!["mutter".to_string()],
        )
        .expect("summary");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("summary text")).expect("json");

        assert_eq!(value["status"], "skipped");
        assert_eq!(value["engine"]["name"], "cargo-gtk-proof");
        model::validate_document(&value).expect("schema-valid skip summary");
    }

    #[test]
    fn write_json_rejects_oversized_output() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("oversized.json");
        let value = serde_json::json!({
            "payload": "x".repeat(MAX_WRITTEN_JSON_BYTES)
        });

        let error = write_json(&path, &value).expect_err("oversized rejected");

        assert!(error.contains("exceeds JSON byte limit"));
    }

    #[test]
    fn bounded_artifact_writers_create_expected_files() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let manifest = serde_json::json!({
            "schema_version": 1,
            "scenario_id": "example--wide--force-light",
            "scenario_type": "command-palette-overlay",
            "source_manifest": "command-palette-overlay.json",
            "case": { "id": "example--wide--force-light" },
            "same_session": { "required": true }
        });
        let expanded = serde_json::json!({
            "schema_version": 1,
            "case_id": "example--wide--force-light",
            "manifest": {},
            "size": { "id": "wide", "width": 1200, "height": 800 },
            "color_scheme": "force-light",
            "artifact_dir": "example--wide--force-light"
        });
        let root = serde_json::json!({
            "schema_version": 1,
            "status": "skipped",
            "case_count": 0,
            "passed": 0,
            "failed": 0,
            "skipped": 0,
            "cases": []
        });
        let case_summary = serde_json::json!({
            "schema_version": 1,
            "case_id": "example--wide--force-light",
            "status": "skipped"
        });
        let comparison = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "protected_regions": []
        });
        let animation = serde_json::json!({
            "schema_version": 1,
            "status": "skipped"
        });
        let warning = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "matches": []
        });
        let parity = serde_json::json!({
            "schema_version": 1,
            "status": "passed",
            "compared": 1,
            "failed": 0,
            "mismatches": [],
            "rust_engine": { "name": "cargo-gtk-proof" }
        });
        let environment = serde_json::json!({
            "schema_version": 1,
            "status": "unsupported-host",
            "missing_capabilities": ["mutter"],
            "runtime": { "isolated": true }
        });
        let skip = serde_json::json!({
            "schema_version": 1,
            "status": "skipped",
            "case_count": 0,
            "passed": 0,
            "failed": 0,
            "skipped": 0,
            "cases": []
        });

        let writes = [
            write_artifact(
                &tempdir.path().join("case-manifest.json"),
                ProofArtifactKind::Manifest,
                &manifest,
            ),
            write_artifact(
                &tempdir.path().join("expanded-case.json"),
                ProofArtifactKind::ExpandedCase,
                &expanded,
            ),
            write_artifact(
                &tempdir.path().join("summary.json"),
                ProofArtifactKind::RootSummary,
                &root,
            ),
            write_artifact(
                &tempdir.path().join("case-summary.json"),
                ProofArtifactKind::CaseSummary,
                &case_summary,
            ),
            write_artifact(
                &tempdir.path().join("comparison-report.json"),
                ProofArtifactKind::ComparisonReport,
                &comparison,
            ),
            write_artifact(
                &tempdir.path().join("animation-report.json"),
                ProofArtifactKind::AnimationReport,
                &animation,
            ),
            write_artifact(
                &tempdir.path().join("warning-scan.json"),
                ProofArtifactKind::WarningScan,
                &warning,
            ),
            write_artifact(
                &tempdir.path().join("parity-report.json"),
                ProofArtifactKind::ParityReport,
                &parity,
            ),
            write_artifact(
                &tempdir.path().join("environment-report.json"),
                ProofArtifactKind::EnvironmentReport,
                &environment,
            ),
            write_artifact(
                &tempdir.path().join("skip-report.json"),
                ProofArtifactKind::SkipReport,
                &skip,
            ),
        ];

        for write in writes {
            let path = write.expect("artifact write");
            let value: Value = serde_json::from_str(&fs::read_to_string(path).expect("json text"))
                .expect("artifact json");
            model::validate_document(&value).expect("schema-valid artifact");
        }
    }
}
