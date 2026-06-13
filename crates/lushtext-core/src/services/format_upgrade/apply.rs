// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit write commands for format-upgrade plans.
//!
//! Inventory and planning remain read-only. This module is the only part of the
//! workflow that backs up, converts, removes, or writes app-owned metadata.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::services::filesystem::{WriteLabel, write as fs_write};
use crate::services::format_upgrade::backup::{
    BackupSession, FormatBackupManifest, read_preservable_item_bytes,
    remove_original_after_manifest,
};
use crate::services::format_upgrade::diagnostics::{FormatClassification, FormatItemPath};
use crate::services::format_upgrade::legacy::ConverterRegistry;
use crate::services::format_upgrade::plan::{FormatPlan, FormatPlanAction};

/// User action being applied to an actionable plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FormatApplyMode {
    /// Convert supported older metadata to the latest format.
    Convert,
    /// Preserve affected metadata and remove it from active app data.
    StartFresh,
}

/// Result of an explicit format-upgrade command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatApplyOutcome {
    /// User action that was applied.
    pub mode: FormatApplyMode,
    /// Backup manifest written before any replacement or removal.
    pub backup_manifest: Option<FormatBackupManifest>,
    /// Item-level failures that kept some data unchanged or retryable.
    pub failures: Vec<FormatApplyFailure>,
    /// Number of latest-format writes completed.
    pub converted_count: usize,
    /// Number of app-data items moved aside for Start Fresh.
    pub start_fresh_count: usize,
}

impl FormatApplyOutcome {
    /// Return whether the command completed without item-level failures.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Item-level failure from a conversion or preservation command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatApplyFailure {
    /// App-data-relative path that failed.
    pub path: FormatItemPath,
    /// Failure detail kept for dialogs and logs.
    pub detail: String,
}

#[derive(Clone, Default)]
struct FormatApplyOptions {
    seams: FormatApplyFailureSeams,
}

#[derive(Clone, Default)]
struct FormatApplyFailureSeams {
    backup_failure_paths: Vec<FormatItemPath>,
    write_failure_paths: Vec<FormatItemPath>,
    group_failure_paths: Vec<FormatItemPath>,
}

impl FormatApplyFailureSeams {
    fn should_fail_backup(&self, path: &FormatItemPath) -> bool {
        self.backup_failure_paths.iter().any(|item| item == path)
    }

    fn should_fail_write(&self, path: &FormatItemPath) -> bool {
        self.write_failure_paths.iter().any(|item| item == path)
    }

    fn should_fail_group(&self, path: &FormatItemPath) -> bool {
        self.group_failure_paths.iter().any(|item| item == path)
    }
}

/// Apply supported Convert actions with the production converter registry.
///
/// Performs blocking filesystem reads and durable writes; callers should run it
/// on a background thread. Every convertible item is copied into a backup
/// manifest before active metadata is replaced.
///
/// # Errors
///
/// Returns an error when the plan has no deterministic Convert action or when a
/// backup manifest cannot be created or written.
pub fn apply_plan(data_dir: &Path, plan: &FormatPlan) -> Result<FormatApplyOutcome> {
    let registry = ConverterRegistry::production();
    apply_plan_with_registry(data_dir, plan, &registry)
}

/// Apply supported Convert actions with explicit converter knowledge.
///
/// # Errors
///
/// Returns an error when the plan has no deterministic Convert action or when a
/// backup manifest cannot be created or written.
pub(crate) fn apply_plan_with_registry(
    data_dir: &Path,
    plan: &FormatPlan,
    registry: &ConverterRegistry,
) -> Result<FormatApplyOutcome> {
    apply_convert_with_options(data_dir, plan, registry, &FormatApplyOptions::default())
}

/// Preserve actionable metadata and remove it from active app data.
///
/// Performs blocking filesystem work; callers should run it on a background
/// thread. Active app data is removed only after every affected item is copied
/// and the backup manifest is durable.
///
/// # Errors
///
/// Returns an error when no item requires preservation or when a backup
/// manifest cannot be created or written.
pub fn start_fresh(data_dir: &Path, plan: &FormatPlan) -> Result<FormatApplyOutcome> {
    let registry = ConverterRegistry::production();
    start_fresh_with_registry(data_dir, plan, &registry)
}

/// Preserve actionable metadata using explicit converter knowledge.
///
/// The registry parameter keeps the signature parallel with conversion tests;
/// Start Fresh itself never calls converters.
///
/// # Errors
///
/// Returns an error when no item requires preservation or when a backup
/// manifest cannot be created or written.
pub(crate) fn start_fresh_with_registry(
    data_dir: &Path,
    plan: &FormatPlan,
    _registry: &ConverterRegistry,
) -> Result<FormatApplyOutcome> {
    apply_start_fresh_with_options(data_dir, plan, &FormatApplyOptions::default())
}

#[cfg(test)]
fn apply_plan_with_failure_seams_for_test(
    data_dir: &Path,
    plan: &FormatPlan,
    registry: &ConverterRegistry,
    seams: FormatApplyFailureSeams,
) -> Result<FormatApplyOutcome> {
    apply_convert_with_options(data_dir, plan, registry, &FormatApplyOptions { seams })
}

#[cfg(test)]
fn start_fresh_with_failure_seams_for_test(
    data_dir: &Path,
    plan: &FormatPlan,
    seams: FormatApplyFailureSeams,
) -> Result<FormatApplyOutcome> {
    apply_start_fresh_with_options(data_dir, plan, &FormatApplyOptions { seams })
}

fn apply_convert_with_options(
    data_dir: &Path,
    plan: &FormatPlan,
    registry: &ConverterRegistry,
    options: &FormatApplyOptions,
) -> Result<FormatApplyOutcome> {
    let items = convert_items(plan);
    if items.is_empty() {
        bail!("format-upgrade plan has no supported Convert action");
    }

    let mut backup = BackupSession::create(data_dir, "convert")?;
    let mut failures = Vec::new();
    for planned in items.iter().copied() {
        let item = &planned.item;
        if options.seams.should_fail_backup(&item.path) {
            let detail = "test seam: backup failure".to_string();
            backup.record_failure(item, detail.clone());
            failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail,
            });
            continue;
        }
        if let Err(error) = backup.copy_item(data_dir, item) {
            let detail = error.to_string();
            backup.record_failure(item, detail.clone());
            failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail,
            });
        }
    }

    // Replacement only starts after every actionable item has preservation
    // evidence. This conservative rule also protects guarded groups.
    if !failures.is_empty() {
        let manifest = backup.finish(data_dir)?;
        return Ok(FormatApplyOutcome {
            mode: FormatApplyMode::Convert,
            backup_manifest: Some(manifest),
            failures,
            converted_count: 0,
            start_fresh_count: 0,
        });
    }

    if let Some(group_failure) = items
        .iter()
        .find(|planned| options.seams.should_fail_group(&planned.item.path))
    {
        let detail = "test seam: grouped-item failure after backup".to_string();
        failures.push(FormatApplyFailure {
            path: group_failure.item.path.clone(),
            detail,
        });
        let manifest = backup.finish(data_dir)?;
        return Ok(FormatApplyOutcome {
            mode: FormatApplyMode::Convert,
            backup_manifest: Some(manifest),
            failures,
            converted_count: 0,
            start_fresh_count: 0,
        });
    }

    let manifest = backup.finish(data_dir)?;
    let mut converted_count = 0;
    for planned in items.iter().copied() {
        let item = &planned.item;
        if options.seams.should_fail_write(&item.path) {
            failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail: "test seam: write failure".to_string(),
            });
            continue;
        }
        match convert_item(planned, registry) {
            Ok(bytes) => {
                fs_write::atomic_replace(
                    &item.absolute_path,
                    WriteLabel::from("format-upgrade-convert"),
                    &bytes,
                )
                .map_err(fs_write::DurableWriteError::into_io_error)
                .with_context(|| format!("failed to write {}", item.absolute_path.display()))?;
                converted_count += 1;
            }
            Err(error) => failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail: error.to_string(),
            }),
        }
    }

    Ok(FormatApplyOutcome {
        mode: FormatApplyMode::Convert,
        backup_manifest: Some(manifest),
        failures,
        converted_count,
        start_fresh_count: 0,
    })
}

fn apply_start_fresh_with_options(
    data_dir: &Path,
    plan: &FormatPlan,
    options: &FormatApplyOptions,
) -> Result<FormatApplyOutcome> {
    let items = preservation_items(plan);
    if items.is_empty() {
        bail!("format-upgrade plan has no item requiring Start Fresh preservation");
    }

    let mut backup = BackupSession::create(data_dir, "start-fresh")?;
    let mut failures = Vec::new();
    let mut copied_items = Vec::new();
    for planned in items.iter().copied() {
        let item = &planned.item;
        if options.seams.should_fail_backup(&item.path) {
            let detail = "test seam: backup failure".to_string();
            backup.record_failure(item, detail.clone());
            failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail,
            });
            continue;
        }
        if options.seams.should_fail_group(&item.path) {
            let detail = "test seam: grouped-item failure before Start Fresh move".to_string();
            backup.record_failure(item, detail.clone());
            failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail,
            });
            continue;
        }
        match backup.copy_item(data_dir, item) {
            Ok(()) => copied_items.push(planned),
            Err(error) => {
                let detail = error.to_string();
                backup.record_failure(item, detail.clone());
                failures.push(FormatApplyFailure {
                    path: item.path.clone(),
                    detail,
                });
            }
        }
    }

    if !failures.is_empty() {
        // Do not remove any active file unless every planned preservation copy
        // succeeds; partial preservation would split dependent metadata across
        // active and backup storage.
        let manifest = backup.finish(data_dir)?;
        return Ok(FormatApplyOutcome {
            mode: FormatApplyMode::StartFresh,
            backup_manifest: Some(manifest),
            failures,
            converted_count: 0,
            start_fresh_count: 0,
        });
    }

    let manifest = backup.finish(data_dir)?;
    let mut removed = 0;
    for planned in copied_items {
        let item = &planned.item;
        if options.seams.should_fail_write(&item.path) {
            failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail: "test seam: remove failure".to_string(),
            });
            continue;
        }
        match remove_original_after_manifest(item) {
            Ok(()) => removed += 1,
            Err(error) => failures.push(FormatApplyFailure {
                path: item.path.clone(),
                detail: error.to_string(),
            }),
        }
    }

    Ok(FormatApplyOutcome {
        mode: FormatApplyMode::StartFresh,
        backup_manifest: Some(manifest),
        failures,
        converted_count: 0,
        start_fresh_count: removed,
    })
}

fn convert_items(plan: &FormatPlan) -> Vec<&super::plan::FormatPlannedItem> {
    plan.actions()
        .filter(|planned| matches!(planned.action, FormatPlanAction::ConvertToLatest { .. }))
        .collect()
}

fn preservation_items(plan: &FormatPlan) -> Vec<&super::plan::FormatPlannedItem> {
    plan.actions()
        .filter(|planned| {
            planned.item.classification.needs_preservation()
                || matches!(planned.action, FormatPlanAction::StartFreshOnly)
        })
        .collect()
}

fn convert_item(
    planned: &super::plan::FormatPlannedItem,
    registry: &ConverterRegistry,
) -> Result<Vec<u8>> {
    let Some(kind) = planned.item.kind.json_kind() else {
        bail!("{} is not JSON-convertible", planned.item.kind.label());
    };
    let FormatClassification::Upgradeable { from_version, .. } = planned.item.classification else {
        bail!("{} is not upgradeable", planned.item.path.display());
    };
    let bytes = read_preservable_item_bytes(&planned.item)?;
    registry.convert(kind, from_version, &bytes)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::services::filesystem::{fixture, metadata as fs_metadata};
    use crate::services::format_upgrade::FormatScanBounds;
    use crate::services::format_upgrade::inventory::scan_with_registry;
    use crate::services::format_upgrade::plan::build_plan_with_registry;
    use crate::services::json_format::KIND_SESSION;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn write_json(path: &Path, value: &serde_json::Value) {
        fixture::write_text(path, &serde_json::to_string_pretty(&value).expect("json"));
    }

    fn convert_session_v0_to_v1(_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(
            &json!({"kind": KIND_SESSION, "version": 1, "data": {"tabs": []}}),
        )?)
    }

    fn upgradeable_plan(dir: &TempDir) -> (FormatPlan, ConverterRegistry) {
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 0, "data": {"tabs": []}}),
        );
        let registry = ConverterRegistry::production().with_converter(
            KIND_SESSION,
            0,
            1,
            convert_session_v0_to_v1,
        );
        let inventory = scan_with_registry(dir.path(), FormatScanBounds::default(), &registry);
        (build_plan_with_registry(&inventory, &registry), registry)
    }

    #[test]
    fn convert_backs_up_before_replacing_original() {
        let dir = TempDir::new().expect("temp dir");
        let (plan, registry) = upgradeable_plan(&dir);

        let outcome = apply_plan_with_registry(dir.path(), &plan, &registry).expect("apply");

        assert!(outcome.is_success());
        assert_eq!(outcome.converted_count, 1);
        let saved = fixture::read_text(&dir.path().join("session.json"));
        assert!(saved.contains(r#""version": 1"#));
        let manifest = outcome.backup_manifest.expect("manifest");
        let record = manifest.records.first().expect("record");
        let backup_path = dir
            .path()
            .join(record.backup_relative_path.as_ref().expect("backup path"));
        let backup = fixture::read_text(&backup_path);
        assert!(backup.contains(r#""version": 0"#));
    }

    #[test]
    fn backup_failure_prevents_any_replacement_write() {
        let dir = TempDir::new().expect("temp dir");
        let (plan, registry) = upgradeable_plan(&dir);
        let session_path = FormatItemPath::from_relative("session.json");
        let seams = FormatApplyFailureSeams {
            backup_failure_paths: vec![session_path],
            ..Default::default()
        };

        let outcome = apply_plan_with_failure_seams_for_test(dir.path(), &plan, &registry, seams)
            .expect("apply");

        assert!(!outcome.is_success());
        assert_eq!(outcome.converted_count, 0);
        let saved = fixture::read_text(&dir.path().join("session.json"));
        assert!(saved.contains(r#""version": 0"#));
    }

    #[test]
    fn grouped_failure_after_backup_prevents_replacement_write() {
        let dir = TempDir::new().expect("temp dir");
        let (plan, registry) = upgradeable_plan(&dir);
        let session_path = FormatItemPath::from_relative("session.json");
        let seams = FormatApplyFailureSeams {
            group_failure_paths: vec![session_path],
            ..Default::default()
        };

        let outcome = apply_plan_with_failure_seams_for_test(dir.path(), &plan, &registry, seams)
            .expect("apply");

        assert!(!outcome.is_success());
        assert_eq!(outcome.converted_count, 0);
        let saved = fixture::read_text(&dir.path().join("session.json"));
        assert!(saved.contains(r#""version": 0"#));
        let manifest = outcome.backup_manifest.expect("manifest");
        assert_eq!(manifest.records[0].result, "copied-to-backup");
    }

    #[test]
    fn write_failure_leaves_backup_and_original_retryable() {
        let dir = TempDir::new().expect("temp dir");
        let (plan, registry) = upgradeable_plan(&dir);
        let session_path = FormatItemPath::from_relative("session.json");
        let seams = FormatApplyFailureSeams {
            write_failure_paths: vec![session_path],
            ..Default::default()
        };

        let outcome = apply_plan_with_failure_seams_for_test(dir.path(), &plan, &registry, seams)
            .expect("apply");

        assert!(!outcome.is_success());
        assert_eq!(outcome.converted_count, 0);
        let saved = fixture::read_text(&dir.path().join("session.json"));
        assert!(saved.contains(r#""version": 0"#));
        assert!(outcome.backup_manifest.is_some());
    }

    #[test]
    fn start_fresh_moves_future_metadata_before_startup_defaults() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 2, "data": {"tabs": []}}),
        );
        let inventory = crate::services::format_upgrade::scan(dir.path());
        let plan = crate::services::format_upgrade::build_plan(&inventory);

        let outcome = start_fresh(dir.path(), &plan).expect("start fresh");

        assert!(outcome.is_success());
        assert_eq!(outcome.start_fresh_count, 1);
        assert!(!fs_metadata::exists(&dir.path().join("session.json")));
        let manifest = outcome.backup_manifest.expect("manifest");
        let record = manifest.records.first().expect("record");
        assert_eq!(record.result, "copied-to-backup");
    }

    #[test]
    fn start_fresh_failure_seam_preserves_original_in_place() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 2, "data": {"tabs": []}}),
        );
        let inventory = crate::services::format_upgrade::scan(dir.path());
        let plan = crate::services::format_upgrade::build_plan(&inventory);
        let seams = FormatApplyFailureSeams {
            backup_failure_paths: vec![FormatItemPath::from_relative("session.json")],
            ..Default::default()
        };

        let outcome =
            start_fresh_with_failure_seams_for_test(dir.path(), &plan, seams).expect("start fresh");

        assert!(!outcome.is_success());
        assert!(fs_metadata::exists(&dir.path().join("session.json")));
        assert_eq!(outcome.start_fresh_count, 0);
    }

    #[test]
    fn start_fresh_preserves_dependent_draft_bodies_with_future_session() {
        let dir = TempDir::new().expect("temp dir");
        fixture::create_dir_all(&dir.path().join("drafts"));
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 2, "data": {"tabs": []}}),
        );
        fixture::write_text(&dir.path().join("drafts/body.draft"), "unsaved body");
        let inventory = crate::services::format_upgrade::scan(dir.path());
        let plan = crate::services::format_upgrade::build_plan(&inventory);

        let outcome = start_fresh(dir.path(), &plan).expect("start fresh");

        assert!(outcome.is_success());
        assert_eq!(outcome.start_fresh_count, 2);
        assert!(!fs_metadata::exists(&dir.path().join("session.json")));
        assert!(!fs_metadata::exists(&dir.path().join("drafts/body.draft")));
        let manifest = outcome.backup_manifest.expect("manifest");
        assert!(manifest.records.iter().any(|record| {
            record.original_relative_path == "drafts/body.draft"
                && record.result == "copied-to-backup"
        }));
    }

    #[test]
    fn oversized_preservation_item_keeps_all_originals_retryable() {
        let dir = TempDir::new().expect("temp dir");
        fixture::create_dir_all(&dir.path().join("drafts"));
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 2, "data": {"tabs": []}}),
        );
        let draft_path = dir.path().join("drafts/huge.draft");
        fixture::write_repeated_bytes(
            &draft_path,
            b"x",
            crate::services::recovery_metadata::DEFAULT_MAX_METADATA_BYTES + 1,
        );
        let inventory = crate::services::format_upgrade::scan(dir.path());
        let plan = crate::services::format_upgrade::build_plan(&inventory);

        let outcome = start_fresh(dir.path(), &plan).expect("start fresh");

        assert!(!outcome.is_success());
        assert_eq!(outcome.start_fresh_count, 0);
        assert!(fs_metadata::exists(&dir.path().join("session.json")));
        assert!(fs_metadata::exists(&draft_path));
        assert!(outcome.failures.iter().any(|failure| {
            failure.path.relative() == Path::new("drafts/huge.draft")
                && failure.detail.contains("backup limit")
        }));
    }
}
