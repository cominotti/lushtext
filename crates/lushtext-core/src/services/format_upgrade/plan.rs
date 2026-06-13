// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure plan builder for app-owned metadata upgrade decisions.
//!
//! Planning is query-shaped: it reads the inventory value already produced by
//! `inventory::scan` and returns action groups without touching the filesystem.

use std::collections::BTreeMap;

use crate::services::format_upgrade::diagnostics::{FormatClassification, FormatMetadataKind};
use crate::services::format_upgrade::inventory::{FormatInventory, FormatInventoryItem};
use crate::services::format_upgrade::legacy::ConverterRegistry;

/// User-visible plan assembled from one read-only inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatPlan {
    /// Groups keep dependent metadata together for UI and apply semantics.
    pub groups: Vec<FormatPlanGroup>,
}

impl FormatPlan {
    /// Return whether the plan has no conversion or preservation work.
    #[must_use]
    pub fn has_no_action(&self) -> bool {
        self.groups.iter().all(|group| group.actions.is_empty())
    }

    /// Return whether startup must ask the user before normal metadata consumers run.
    #[must_use]
    pub fn requires_startup_decision(&self) -> bool {
        self.groups.iter().any(|group| {
            group.actions.iter().any(|action| {
                action.item.kind.startup_critical()
                    && matches!(
                        action.action,
                        FormatPlanAction::ConvertToLatest { .. } | FormatPlanAction::StartFreshOnly
                    )
            })
        })
    }

    /// Return whether at least one deterministic Convert action exists.
    #[must_use]
    pub fn offers_convert(&self) -> bool {
        self.groups.iter().any(|group| {
            group
                .actions
                .iter()
                .any(|action| matches!(action.action, FormatPlanAction::ConvertToLatest { .. }))
        })
    }

    /// Return whether future-version data blocks conversion.
    #[must_use]
    pub fn has_future_version_blocker(&self) -> bool {
        self.groups.iter().any(|group| {
            group.actions.iter().any(|action| {
                matches!(
                    action.item.classification,
                    FormatClassification::FutureVersion { .. }
                )
            })
        })
    }

    /// Return action items in stable group order.
    pub fn actions(&self) -> impl Iterator<Item = &FormatPlannedItem> {
        self.groups.iter().flat_map(|group| group.actions.iter())
    }
}

/// Cohesive group of format plan actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatPlanGroup {
    /// Group kind controls partial-apply safety.
    pub kind: FormatPlanGroupKind,
    /// Metadata category shown in grouped UI summaries.
    pub metadata_kind: FormatMetadataKind,
    /// Planned actions for this group.
    pub actions: Vec<FormatPlannedItem>,
}

/// How tightly items in a plan group are coupled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatPlanGroupKind {
    /// Items may be preserved or converted independently.
    Independent,
    /// Items must all preserve successfully before any group member is replaced.
    Guarded,
}

/// One inventory item paired with the command it supports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatPlannedItem {
    /// Inventory item that produced this planned action.
    pub item: FormatInventoryItem,
    /// Action exposed by the service for this item.
    pub action: FormatPlanAction,
}

/// Actionability derived from one item classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatPlanAction {
    /// No command is needed for current or missing metadata.
    NoAction,
    /// Convert is safe because a deterministic converter path exists.
    ConvertToLatest {
        /// Older version read from the metadata envelope.
        from_version: u32,
        /// Latest version this binary will write.
        to_version: u32,
    },
    /// Preserve and remove from active app data because this binary cannot read it safely.
    StartFreshOnly,
    /// Show the issue but do not expose Convert.
    ReportOnly,
}

/// Build a plan using the production converter registry.
#[must_use]
pub fn build_plan(inventory: &FormatInventory) -> FormatPlan {
    let registry = ConverterRegistry::production();
    build_plan_with_registry(inventory, &registry)
}

/// Build a plan using explicit converter knowledge.
#[must_use]
pub(crate) fn build_plan_with_registry(
    inventory: &FormatInventory,
    _registry: &ConverterRegistry,
) -> FormatPlan {
    let mut groups = BTreeMap::<FormatMetadataKind, Vec<FormatPlannedItem>>::new();
    let preserve_draft_bodies = inventory.items.iter().any(|item| {
        matches!(
            item.kind,
            FormatMetadataKind::Session | FormatMetadataKind::DraftManifest
        ) && matches!(action_for_item(item), FormatPlanAction::StartFreshOnly)
    });
    for item in &inventory.items {
        // If session or draft-manifest metadata must move aside, preserve
        // current draft bodies with it so unsaved text is not stranded behind
        // fresh startup defaults.
        let action = if preserve_draft_bodies
            && item.kind == FormatMetadataKind::DraftBody
            && matches!(item.classification, FormatClassification::Current { .. })
        {
            FormatPlanAction::StartFreshOnly
        } else {
            action_for_item(item)
        };
        if matches!(action, FormatPlanAction::NoAction) {
            continue;
        }
        groups
            .entry(item.kind)
            .or_default()
            .push(FormatPlannedItem {
                item: item.clone(),
                action,
            });
    }

    let groups = groups
        .into_iter()
        .map(|(metadata_kind, actions)| FormatPlanGroup {
            kind: group_kind(metadata_kind),
            metadata_kind,
            actions,
        })
        .collect();
    FormatPlan { groups }
}

fn action_for_item(item: &FormatInventoryItem) -> FormatPlanAction {
    match item.classification {
        FormatClassification::Missing | FormatClassification::Current { .. } => {
            FormatPlanAction::NoAction
        }
        FormatClassification::Upgradeable {
            from_version,
            to_version,
        } => FormatPlanAction::ConvertToLatest {
            from_version,
            to_version,
        },
        FormatClassification::FutureVersion { .. } => FormatPlanAction::StartFreshOnly,
        FormatClassification::UnsupportedOld { .. }
        | FormatClassification::Damaged { .. }
        | FormatClassification::UnsafeToReplace { .. } => FormatPlanAction::ReportOnly,
    }
}

fn group_kind(kind: FormatMetadataKind) -> FormatPlanGroupKind {
    match kind {
        // Multi-file workflows are guarded because partial apply can orphan
        // draft bodies or leave Replace All undo state without its manifest or
        // cleanup marker.
        FormatMetadataKind::Session
        | FormatMetadataKind::DraftManifest
        | FormatMetadataKind::DraftBody
        | FormatMetadataKind::ReplaceUndoManifest
        | FormatMetadataKind::ReplaceUndoEntry
        | FormatMetadataKind::ReplaceUndoCleanupMarker => FormatPlanGroupKind::Guarded,
        _ => FormatPlanGroupKind::Independent,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::services::filesystem::fixture;
    use crate::services::format_upgrade::diagnostics::{FormatClassification, FormatScanBounds};
    use crate::services::format_upgrade::inventory::scan_with_registry;
    use crate::services::format_upgrade::legacy::ConverterRegistry;
    use crate::services::json_format::KIND_SESSION;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn write_json(path: &Path, value: &serde_json::Value) {
        fixture::write_text(path, &serde_json::to_string_pretty(&value).expect("json"));
    }

    #[test]
    fn current_and_missing_inventory_produces_no_plan_actions() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 1, "data": {"tabs": []}}),
        );
        let inventory = crate::services::format_upgrade::scan(dir.path());

        let plan = build_plan(&inventory);

        assert!(plan.has_no_action());
        assert!(!plan.requires_startup_decision());
    }

    #[test]
    fn future_version_never_offers_convert() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 2, "data": {"tabs": []}}),
        );
        let inventory = crate::services::format_upgrade::scan(dir.path());

        let plan = build_plan(&inventory);

        assert!(!plan.offers_convert());
        assert!(plan.has_future_version_blocker());
        assert!(plan.requires_startup_decision());
    }

    #[test]
    fn damaged_metadata_reports_without_startup_decision() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("session.json"), "{not-json");
        let inventory = crate::services::format_upgrade::scan(dir.path());

        let plan = build_plan(&inventory);

        assert!(!plan.offers_convert());
        assert!(!plan.has_future_version_blocker());
        assert!(!plan.requires_startup_decision());
        assert!(
            plan.actions()
                .any(|item| matches!(item.action, FormatPlanAction::ReportOnly))
        );
    }

    #[test]
    fn unsupported_old_without_converter_reports_without_startup_decision() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 0, "data": {"tabs": []}}),
        );
        let inventory = crate::services::format_upgrade::scan(dir.path());

        let plan = build_plan(&inventory);

        assert!(!plan.offers_convert());
        assert!(!plan.has_future_version_blocker());
        assert!(!plan.requires_startup_decision());
        assert!(
            plan.actions()
                .any(|item| matches!(item.action, FormatPlanAction::ReportOnly))
        );
    }

    #[test]
    fn converter_backed_older_version_offers_convert() {
        fn convert(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
            Ok(bytes.to_vec())
        }

        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 0, "data": {"tabs": []}}),
        );
        let registry = ConverterRegistry::production().with_converter(KIND_SESSION, 0, 1, convert);
        let inventory = scan_with_registry(dir.path(), FormatScanBounds::default(), &registry);
        assert!(matches!(
            inventory
                .items
                .iter()
                .find(|item| item.path.relative() == Path::new("session.json"))
                .expect("session")
                .classification,
            FormatClassification::Upgradeable { .. }
        ));

        let plan = build_plan_with_registry(&inventory, &registry);

        assert!(plan.offers_convert());
        assert!(!plan.has_future_version_blocker());
        assert!(plan.requires_startup_decision());
    }
}
