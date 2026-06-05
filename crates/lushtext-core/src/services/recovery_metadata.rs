// SPDX-License-Identifier: GPL-3.0-or-later

//! Recovery-aware metadata loading for app-owned JSON state.
//!
//! This service wraps selected persistence files that must never be silently
//! replaced after corruption. It stays GTK-free and depends on the shared
//! filesystem boundary so callers can use it from background workers.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::services::filesystem::{
    PathStatus, WriteLabel, metadata as fs_metadata, read as fs_read, write as fs_write,
};

/// Default cap for app-owned metadata files loaded during recovery.
///
/// Recovery metadata should be small JSON indexes or manifests. Sixteen MiB is
/// generous for real state while preventing a damaged file from turning startup
/// into an unbounded read.
pub const DEFAULT_MAX_METADATA_BYTES: u64 = 16 * 1024 * 1024;
/// App-data directory where damaged metadata is preserved for support triage.
pub const QUARANTINE_DIR: &str = "recovery-quarantine";
/// Maximum attempts to find a free quarantine filename before failing safely.
///
/// Hitting this would require repeated nanosecond timestamp/hash collisions or
/// a hostile prefilled quarantine directory, so a small bound keeps startup from
/// looping indefinitely.
const MAX_QUARANTINE_NAME_ATTEMPTS: u32 = 64;

/// App-owned metadata category used in diagnostics and quarantine filenames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryMetadataClass {
    /// Global tab/session restore state.
    Session,
    /// Draft manifest that maps draft IDs to persisted draft bodies.
    DraftManifest,
    /// Saved-file line bookmark sidecar.
    BookmarkSidecar,
    /// Saved-file rich note sidecar.
    DocumentNoteSidecar,
    /// Workspace-root rich note sidecar.
    WorkspaceNoteSidecar,
    /// Local-history lineage index.
    LocalHistoryIndex,
    /// Replace All undo journal or legacy undo backup metadata.
    ReplaceAllUndoJournal,
    /// Pending post-rename sidecar migration ledger.
    MigrationLedger,
}

impl RecoveryMetadataClass {
    /// Return a stable lowercase slug for paths, logs, and smoke artifacts.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::DraftManifest => "draft-manifest",
            Self::BookmarkSidecar => "bookmark-sidecar",
            Self::DocumentNoteSidecar => "document-note-sidecar",
            Self::WorkspaceNoteSidecar => "workspace-note-sidecar",
            Self::LocalHistoryIndex => "local-history-index",
            Self::ReplaceAllUndoJournal => "replace-all-undo-journal",
            Self::MigrationLedger => "migration-ledger",
        }
    }

    /// Return a short user-facing label for grouped recovery summaries.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::DraftManifest => "draft manifest",
            Self::BookmarkSidecar => "bookmark sidecar",
            Self::DocumentNoteSidecar => "document note",
            Self::WorkspaceNoteSidecar => "workspace note",
            Self::LocalHistoryIndex => "local history",
            Self::ReplaceAllUndoJournal => "replace undo journal",
            Self::MigrationLedger => "migration ledger",
        }
    }
}

/// Integrity problem found while loading or repairing recovery metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryProblem {
    /// The file existed but could not be parsed as the requested JSON shape.
    Malformed {
        /// Parser detail kept for logs and smoke artifacts.
        detail: String,
    },
    /// The file could not be inspected or read.
    Unreadable {
        /// Platform I/O detail kept for logs and smoke artifacts.
        detail: String,
    },
    /// The path exists but is not a regular file that recovery code may replace.
    UnsupportedFileKind {
        /// Coarse kind reported by the filesystem boundary.
        status: PathStatus,
    },
    /// The metadata file is too large to read on the recovery path.
    Oversized {
        /// Current file length in bytes.
        size_bytes: u64,
        /// Configured maximum allowed read size.
        max_bytes: u64,
    },
    /// A caller rebuilt deterministic state from surviving evidence.
    Repaired {
        /// Human-readable summary of what was rebuilt.
        detail: String,
    },
    /// A caller considered repair but could not do it safely.
    RepairSkipped {
        /// Human-readable reason repair was skipped.
        detail: String,
    },
}

impl RecoveryProblem {
    /// Return a stable category slug for filenames and grouped diagnostics.
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self {
            Self::Malformed { .. } => "malformed",
            Self::Unreadable { .. } => "unreadable",
            Self::UnsupportedFileKind { .. } => "unsupported-kind",
            Self::Oversized { .. } => "oversized",
            Self::Repaired { .. } => "repaired",
            Self::RepairSkipped { .. } => "repair-skipped",
        }
    }
}

/// How the original metadata was preserved before defaulting or repair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryPreservation {
    /// No preservation was needed for this diagnostic.
    NotNeeded,
    /// The original path was moved into quarantine.
    Quarantined {
        /// App-owned quarantine path containing the original metadata.
        path: PathBuf,
    },
    /// The original bytes were copied into quarantine while the source remains.
    CopiedToQuarantine {
        /// App-owned quarantine path containing the copied metadata bytes.
        path: PathBuf,
    },
    /// The original path remains untouched because it was unsafe to move or copy.
    PreservedInPlace,
    /// Preservation was attempted but failed.
    Failed {
        /// Failure detail kept for logs and smoke artifacts.
        detail: String,
    },
}

impl RecoveryPreservation {
    /// Return the quarantine path when the original evidence has one.
    #[must_use]
    pub fn quarantine_path(&self) -> Option<&Path> {
        match self {
            Self::Quarantined { path } | Self::CopiedToQuarantine { path } => Some(path),
            Self::NotNeeded | Self::PreservedInPlace | Self::Failed { .. } => None,
        }
    }

    /// Return whether it is safe for a caller to write replacement metadata.
    #[must_use]
    pub const fn allows_replacement(&self) -> bool {
        matches!(
            self,
            Self::NotNeeded | Self::Quarantined { .. } | Self::CopiedToQuarantine { .. }
        )
    }
}

/// Structured recovery diagnostic returned to services and later UI adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryDiagnostic {
    /// App-owned metadata class that failed or was repaired.
    pub class: RecoveryMetadataClass,
    /// Original path the caller attempted to load.
    pub original_path: PathBuf,
    /// Integrity problem or repair event.
    pub problem: RecoveryProblem,
    /// Preservation outcome for the original evidence.
    pub preservation: RecoveryPreservation,
    /// Whether callers may safely replace the original metadata path.
    pub replacement_allowed: bool,
}

impl RecoveryDiagnostic {
    /// Build a diagnostic from a preservation attempt.
    #[must_use]
    pub fn with_preservation(
        class: RecoveryMetadataClass,
        original_path: impl Into<PathBuf>,
        problem: RecoveryProblem,
        preservation: RecoveryPreservation,
    ) -> Self {
        let replacement_allowed = preservation.allows_replacement();
        Self {
            class,
            original_path: original_path.into(),
            problem,
            preservation,
            replacement_allowed,
        }
    }

    /// Build a diagnostic for deterministic repair output.
    #[must_use]
    pub fn repaired(
        class: RecoveryMetadataClass,
        original_path: impl Into<PathBuf>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            class,
            original_path: original_path.into(),
            problem: RecoveryProblem::Repaired {
                detail: detail.into(),
            },
            preservation: RecoveryPreservation::NotNeeded,
            replacement_allowed: true,
        }
    }

    /// Build a diagnostic for an intentionally skipped repair attempt.
    #[must_use]
    pub fn repair_skipped(
        class: RecoveryMetadataClass,
        original_path: impl Into<PathBuf>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            class,
            original_path: original_path.into(),
            problem: RecoveryProblem::RepairSkipped {
                detail: detail.into(),
            },
            preservation: RecoveryPreservation::PreservedInPlace,
            replacement_allowed: false,
        }
    }

    /// Return a concise line suitable for smoke artifacts and tracing.
    #[must_use]
    pub fn summary(&self) -> String {
        let quarantine = self
            .preservation
            .quarantine_path()
            .map(|path| format!(" quarantine={}", path.display()))
            .unwrap_or_default();
        format!(
            "{} recovery {} at {}{}",
            self.class.label(),
            self.problem.category(),
            self.original_path.display(),
            quarantine
        )
    }
}

/// High-level outcome of a recovery metadata load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryLoadOutcome {
    /// Metadata was present, valid, and loaded as-is.
    Loaded,
    /// Metadata was absent, so the documented default was used.
    MissingDefault,
    /// Metadata was bad, preserved in quarantine, and defaulted.
    QuarantinedDefault,
    /// Metadata was bad, left in place, and defaulted only with diagnostics.
    PreservedDefault,
    /// Metadata could not be loaded directly but a caller returned partial or repaired state.
    Partial,
}

/// Value plus integrity diagnostics from a recovery-aware load.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryLoad<T> {
    /// Loaded, defaulted, or repaired value returned to the caller.
    pub value: T,
    /// Coarse outcome that callers and tests can branch on.
    pub outcome: RecoveryLoadOutcome,
    /// Diagnostics that must be surfaced or logged by higher layers.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

impl<T> RecoveryLoad<T> {
    /// Return the recovered value and discard diagnostics intentionally.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Return whether all diagnostics allow replacement writes.
    #[must_use]
    pub fn replacement_allowed(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.replacement_allowed)
    }
}

/// Caller-supplied configuration for one metadata load.
pub struct RecoveryLoadConfig<'a> {
    /// App data directory used to locate the default quarantine directory.
    pub data_dir: &'a Path,
    /// Exact metadata path to inspect.
    pub path: &'a Path,
    /// Metadata class used in diagnostics and quarantine filenames.
    pub class: RecoveryMetadataClass,
    /// Maximum bytes to read from a regular metadata file.
    pub max_bytes: u64,
    /// Optional quarantine directory override used by tests and future tooling.
    pub quarantine_dir: Option<PathBuf>,
}

impl<'a> RecoveryLoadConfig<'a> {
    /// Build a config with the default metadata size cap and quarantine directory.
    #[must_use]
    pub const fn new(data_dir: &'a Path, path: &'a Path, class: RecoveryMetadataClass) -> Self {
        Self {
            data_dir,
            path,
            class,
            max_bytes: DEFAULT_MAX_METADATA_BYTES,
            quarantine_dir: None,
        }
    }

    /// Override the byte cap for tests or unusually small metadata classes.
    #[must_use]
    pub const fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Override the quarantine directory for tests or host-specific smoke runs.
    #[must_use]
    pub fn with_quarantine_dir(mut self, quarantine_dir: impl Into<PathBuf>) -> Self {
        self.quarantine_dir = Some(quarantine_dir.into());
        self
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.quarantine_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join(QUARANTINE_DIR))
    }
}

/// Context passed to a conservative repair hook after preservation is attempted.
pub struct RecoveryRepairContext<'a> {
    /// App-owned metadata class being repaired.
    pub class: RecoveryMetadataClass,
    /// Original metadata path.
    pub path: &'a Path,
    /// Original bytes when the file was readable and within the configured cap.
    pub bytes: Option<&'a [u8]>,
    /// Problem that made the direct load fail.
    pub problem: &'a RecoveryProblem,
    /// Preservation outcome that determines whether write-back is safe.
    pub preservation: &'a RecoveryPreservation,
}

/// Result returned by a caller-specific repair hook.
pub enum RecoveryRepair<T> {
    /// No deterministic repair was available.
    Unavailable,
    /// Repair was considered but skipped, with diagnostics explaining why.
    Skipped {
        /// Diagnostics describing why repair was not safe.
        diagnostics: Vec<RecoveryDiagnostic>,
    },
    /// Caller rebuilt a deterministic value and supplied audit diagnostics.
    Repaired {
        /// Rebuilt value that should be treated as partial recovery.
        value: T,
        /// Repair diagnostics to append after the preservation diagnostic.
        diagnostics: Vec<RecoveryDiagnostic>,
    },
}

/// Load a JSON metadata file, defaulting with diagnostics when integrity fails.
#[must_use]
pub fn load_json_or_default<T>(config: &RecoveryLoadConfig<'_>) -> RecoveryLoad<T>
where
    T: DeserializeOwned + Default,
{
    load_json_with_repair(config, |_| RecoveryRepair::Unavailable)
}

/// Load an optional JSON metadata file.
///
/// Missing files return `None` without diagnostics. Present valid files return
/// `Some(T)`, while malformed or unreadable files return `None` with recovery
/// diagnostics and preservation behavior.
#[must_use]
pub fn load_json_optional<T>(config: &RecoveryLoadConfig<'_>) -> RecoveryLoad<Option<T>>
where
    T: DeserializeOwned,
{
    load_json_with_repair(config, |_| RecoveryRepair::Unavailable)
}

/// Load a JSON metadata file and allow deterministic caller-specific repair.
#[must_use]
pub fn load_json_with_repair<T, F>(config: &RecoveryLoadConfig<'_>, repair: F) -> RecoveryLoad<T>
where
    T: DeserializeOwned + Default,
    F: FnOnce(RecoveryRepairContext<'_>) -> RecoveryRepair<T>,
{
    match fs_metadata::path_status(config.path) {
        Ok(PathStatus::Missing) => RecoveryLoad {
            value: T::default(),
            outcome: RecoveryLoadOutcome::MissingDefault,
            diagnostics: Vec::new(),
        },
        Ok(PathStatus::Directory | PathStatus::Other) => default_after_problem(
            config,
            &RecoveryProblem::UnsupportedFileKind {
                status: fs_metadata::path_status(config.path).unwrap_or(PathStatus::Other),
            },
            None,
            repair,
        ),
        Ok(PathStatus::File) => load_existing_json_file(config, repair),
        Err(error) => default_after_problem(
            config,
            &RecoveryProblem::Unreadable {
                detail: error.to_string(),
            },
            None,
            repair,
        ),
    }
}

/// Write recovery metadata through the same durable JSON path as other state.
///
/// Callers should only use this after checking `replacement_allowed()` on the
/// load result that made the write necessary.
///
/// # Errors
///
/// Returns an error when the parent directory is absent or unwritable, the value
/// cannot serialize, or the durable replacement fails.
pub fn save_json_path<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .context("recovery metadata path has no parent directory")?;
    fs_write::create_dir_all_durable(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    fs_write::atomic_replace_stream(path, WriteLabel::JSON, |writer| {
        serde_json::to_writer_pretty(writer, value).map_err(io::Error::other)
    })
    .map_err(fs_write::DurableWriteError::into_io_error)
    .with_context(|| format!("failed to write {}", path.display()))
}

/// Load one existing JSON metadata file through the bounded recovery state machine.
fn load_existing_json_file<T, F>(config: &RecoveryLoadConfig<'_>, repair: F) -> RecoveryLoad<T>
where
    T: DeserializeOwned + Default,
    F: FnOnce(RecoveryRepairContext<'_>) -> RecoveryRepair<T>,
{
    match fs_metadata::file_facts(config.path) {
        Ok(facts) if facts.byte_size > config.max_bytes => {
            return default_after_problem(
                config,
                &RecoveryProblem::Oversized {
                    size_bytes: facts.byte_size,
                    max_bytes: config.max_bytes,
                },
                None,
                repair,
            );
        }
        Ok(_) => {}
        Err(error) => {
            return default_after_problem(
                config,
                &RecoveryProblem::Unreadable {
                    detail: error.to_string(),
                },
                None,
                repair,
            );
        }
    }

    let bytes = match fs_read::bytes(config.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return RecoveryLoad {
                value: T::default(),
                outcome: RecoveryLoadOutcome::MissingDefault,
                diagnostics: Vec::new(),
            };
        }
        Err(error) => {
            return default_after_problem(
                config,
                &RecoveryProblem::Unreadable {
                    detail: error.to_string(),
                },
                None,
                repair,
            );
        }
    };

    match serde_json::from_slice(&bytes) {
        Ok(value) => RecoveryLoad {
            value,
            outcome: RecoveryLoadOutcome::Loaded,
            diagnostics: Vec::new(),
        },
        Err(error) => default_after_problem(
            config,
            &RecoveryProblem::Malformed {
                detail: error.to_string(),
            },
            Some(&bytes),
            repair,
        ),
    }
}

/// Preserve damaged metadata, run the caller repair hook, and return a default or repaired value.
fn default_after_problem<T, F>(
    config: &RecoveryLoadConfig<'_>,
    problem: &RecoveryProblem,
    bytes: Option<&[u8]>,
    repair: F,
) -> RecoveryLoad<T>
where
    T: Default,
    F: FnOnce(RecoveryRepairContext<'_>) -> RecoveryRepair<T>,
{
    let preservation = preserve_original(config, problem, bytes);
    let base = RecoveryDiagnostic::with_preservation(
        config.class,
        config.path,
        problem.clone(),
        preservation.clone(),
    );

    let context = RecoveryRepairContext {
        class: config.class,
        path: config.path,
        bytes,
        problem,
        preservation: &preservation,
    };

    match repair(context) {
        RecoveryRepair::Unavailable => RecoveryLoad {
            value: T::default(),
            outcome: outcome_for_preservation(&preservation),
            diagnostics: vec![base],
        },
        RecoveryRepair::Skipped { mut diagnostics } => {
            let mut all = vec![base];
            all.append(&mut diagnostics);
            RecoveryLoad {
                value: T::default(),
                outcome: outcome_for_preservation(&preservation),
                diagnostics: all,
            }
        }
        RecoveryRepair::Repaired {
            value,
            mut diagnostics,
        } => {
            let mut all = vec![base];
            all.append(&mut diagnostics);
            RecoveryLoad {
                value,
                outcome: RecoveryLoadOutcome::Partial,
                diagnostics: all,
            }
        }
    }
}

fn outcome_for_preservation(preservation: &RecoveryPreservation) -> RecoveryLoadOutcome {
    if preservation.allows_replacement() {
        RecoveryLoadOutcome::QuarantinedDefault
    } else {
        RecoveryLoadOutcome::PreservedDefault
    }
}

/// Move damaged metadata out of the normal load path before callers write replacements.
///
/// If durable rename fails but bounded bytes were already read, copying those
/// bytes to quarantine still preserves evidence; callers must honor
/// `replacement_allowed()` before writing any repaired/default value.
fn preserve_original(
    config: &RecoveryLoadConfig<'_>,
    problem: &RecoveryProblem,
    bytes: Option<&[u8]>,
) -> RecoveryPreservation {
    if matches!(problem, RecoveryProblem::UnsupportedFileKind { .. }) {
        return RecoveryPreservation::PreservedInPlace;
    }

    let quarantine_dir = config.quarantine_dir();
    if let Err(error) = fs_write::create_dir_all_durable(&quarantine_dir) {
        return RecoveryPreservation::Failed {
            detail: format!(
                "failed to create quarantine directory {}: {error}",
                quarantine_dir.display()
            ),
        };
    }

    let quarantine_path = match next_quarantine_path(&quarantine_dir, config, problem) {
        Ok(path) => path,
        Err(error) => {
            return RecoveryPreservation::Failed {
                detail: format!("failed to allocate quarantine path: {error}"),
            };
        }
    };

    match fs_write::rename_durable(config.path, &quarantine_path) {
        Ok(()) => RecoveryPreservation::Quarantined {
            path: quarantine_path,
        },
        Err(rename_error) => match bytes {
            Some(bytes) => {
                match fs_write::atomic_replace(
                    &quarantine_path,
                    WriteLabel::RECOVERY_QUARANTINE,
                    bytes,
                ) {
                    Ok(()) => RecoveryPreservation::CopiedToQuarantine {
                        path: quarantine_path,
                    },
                    Err(copy_error) => RecoveryPreservation::Failed {
                        detail: format!(
                            "rename failed: {rename_error}; copy failed: {}",
                            copy_error.into_io_error()
                        ),
                    },
                }
            }
            None => RecoveryPreservation::Failed {
                detail: format!(
                    "rename failed and no bounded bytes were available: {rename_error}"
                ),
            },
        },
    }
}

/// Allocate a unique quarantine filename without overwriting earlier evidence.
fn next_quarantine_path(
    quarantine_dir: &Path,
    config: &RecoveryLoadConfig<'_>,
    problem: &RecoveryProblem,
) -> io::Result<PathBuf> {
    for attempt in 0..MAX_QUARANTINE_NAME_ATTEMPTS {
        let name = quarantine_file_name(config, problem, attempt);
        let candidate = quarantine_dir.join(name);
        match fs_metadata::path_status(&candidate) {
            Ok(PathStatus::Missing) => return Ok(candidate),
            Ok(PathStatus::File | PathStatus::Directory | PathStatus::Other) => {}
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not find a free quarantine filename",
    ))
}

/// Build the human-inspectable quarantine filename for one recovery problem.
fn quarantine_file_name(
    config: &RecoveryLoadConfig<'_>,
    problem: &RecoveryProblem,
    attempt: u32,
) -> String {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = quarantine_hash(config, problem);
    let extension = sanitized_extension(config.path);
    let suffix = if attempt == 0 {
        String::new()
    } else {
        format!("-{attempt}")
    };
    format!(
        "{now_nanos}-{}-{}-{hash:016x}{suffix}.{extension}",
        config.class.slug(),
        problem.category()
    )
}

fn quarantine_hash(config: &RecoveryLoadConfig<'_>, problem: &RecoveryProblem) -> u64 {
    let mut hasher = DefaultHasher::new();
    config.path.hash(&mut hasher);
    config.class.slug().hash(&mut hasher);
    problem.category().hash(&mut hasher);
    hasher.finish()
}

fn sanitized_extension(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(sanitize_component)
        .filter(|extension| !extension.is_empty());

    extension.unwrap_or_else(|| "metadata".to_string())
}

fn sanitize_component(component: &str) -> String {
    component
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
    struct TestMetadata {
        name: String,
        count: u32,
    }

    fn config<'a>(data_dir: &'a Path, path: &'a Path) -> RecoveryLoadConfig<'a> {
        RecoveryLoadConfig::new(data_dir, path, RecoveryMetadataClass::Session)
    }

    #[test]
    fn missing_metadata_returns_default_without_diagnostic() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");

        let result: RecoveryLoad<TestMetadata> = load_json_or_default(&config(dir.path(), &path));

        assert_eq!(result.outcome, RecoveryLoadOutcome::MissingDefault);
        assert_eq!(result.value, TestMetadata::default());
        assert!(result.diagnostics.is_empty());
        assert!(result.replacement_allowed());
    }

    #[test]
    fn valid_metadata_loads_without_diagnostic() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");
        fixture::write_text(&path, r#"{"name":"ok","count":7}"#);

        let result: RecoveryLoad<TestMetadata> = load_json_or_default(&config(dir.path(), &path));

        assert_eq!(result.outcome, RecoveryLoadOutcome::Loaded);
        assert_eq!(
            result.value,
            TestMetadata {
                name: "ok".to_string(),
                count: 7
            }
        );
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn malformed_metadata_is_quarantined_before_default() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");
        fixture::write_text(&path, "not json");

        let result: RecoveryLoad<TestMetadata> = load_json_or_default(&config(dir.path(), &path));

        assert_eq!(result.outcome, RecoveryLoadOutcome::QuarantinedDefault);
        assert_eq!(result.value, TestMetadata::default());
        assert_eq!(result.diagnostics.len(), 1);
        assert!(matches!(
            result.diagnostics[0].problem,
            RecoveryProblem::Malformed { .. }
        ));
        let quarantine_path = result.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("quarantine path");
        assert_eq!(fixture::read_text(quarantine_path), "not json");
        assert!(result.replacement_allowed());
    }

    #[test]
    fn quarantine_failure_preserves_original_and_disallows_replacement() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");
        let blocked_quarantine = dir.path().join("not-a-dir");
        fixture::write_text(&path, "not json");
        fixture::write_text(&blocked_quarantine, "blocks create_dir_all");

        let result: RecoveryLoad<TestMetadata> = load_json_or_default(
            &config(dir.path(), &path).with_quarantine_dir(&blocked_quarantine),
        );

        assert_eq!(result.outcome, RecoveryLoadOutcome::PreservedDefault);
        assert_eq!(fixture::read_text(&path), "not json");
        assert!(!result.replacement_allowed());
        assert!(matches!(
            result.diagnostics[0].preservation,
            RecoveryPreservation::Failed { .. }
        ));
    }

    #[test]
    fn non_file_metadata_path_is_rejected_without_overwrite() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");
        fixture::create_dir(&path);

        let result: RecoveryLoad<TestMetadata> = load_json_or_default(&config(dir.path(), &path));

        assert_eq!(result.outcome, RecoveryLoadOutcome::PreservedDefault);
        assert!(matches!(
            result.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFileKind {
                status: PathStatus::Directory
            }
        ));
        assert!(!result.replacement_allowed());
        assert!(fs_metadata::path_status(&path).is_ok_and(PathStatus::is_directory));
    }

    #[test]
    fn metadata_errors_are_reported_as_unreadable() {
        let dir = TempDir::new().expect("tempdir");
        let file_parent = dir.path().join("not-a-directory");
        fixture::write_text(&file_parent, "parent is a file");
        let path = file_parent.join("session.json");

        let result: RecoveryLoad<TestMetadata> = load_json_or_default(&config(dir.path(), &path));

        assert_eq!(result.outcome, RecoveryLoadOutcome::PreservedDefault);
        assert!(matches!(
            result.diagnostics[0].problem,
            RecoveryProblem::Unreadable { .. }
        ));
        assert!(!result.replacement_allowed());
    }

    #[test]
    fn oversized_metadata_is_not_read_and_can_be_quarantined() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");
        fixture::write_text(&path, "123456789");

        let result: RecoveryLoad<TestMetadata> =
            load_json_or_default(&config(dir.path(), &path).with_max_bytes(4));

        assert_eq!(result.outcome, RecoveryLoadOutcome::QuarantinedDefault);
        assert!(matches!(
            result.diagnostics[0].problem,
            RecoveryProblem::Oversized {
                size_bytes: 9,
                max_bytes: 4
            }
        ));
        let quarantine_path = result.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("quarantine path");
        assert_eq!(fixture::read_text(quarantine_path), "123456789");
    }

    #[test]
    fn repair_hook_can_return_partial_value_after_preservation() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("session.json");
        fixture::write_text(&path, "not json");

        let result: RecoveryLoad<TestMetadata> =
            load_json_with_repair(&config(dir.path(), &path), |context| {
                assert_eq!(context.class, RecoveryMetadataClass::Session);
                assert!(context.bytes.is_some());
                assert!(matches!(context.problem, RecoveryProblem::Malformed { .. }));
                assert!(context.preservation.allows_replacement());
                RecoveryRepair::Repaired {
                    value: TestMetadata {
                        name: "repaired".to_string(),
                        count: 1,
                    },
                    diagnostics: vec![RecoveryDiagnostic::repaired(
                        context.class,
                        context.path,
                        "rebuilt from deterministic evidence",
                    )],
                }
            });

        assert_eq!(result.outcome, RecoveryLoadOutcome::Partial);
        assert_eq!(result.value.name, "repaired");
        assert_eq!(result.diagnostics.len(), 2);
        assert!(matches!(
            result.diagnostics[1].problem,
            RecoveryProblem::Repaired { .. }
        ));
    }

    #[test]
    fn save_json_path_writes_pretty_recovery_metadata() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("nested").join("state.json");
        let value = TestMetadata {
            name: "saved".to_string(),
            count: 3,
        };

        save_json_path(&path, &value).expect("save recovery metadata");

        let text = fixture::read_text(&path);
        assert!(text.contains('\n'));
        let loaded: TestMetadata = serde_json::from_str(&text).expect("parse saved JSON");
        assert_eq!(loaded, value);
    }
}
