// SPDX-License-Identifier: GPL-3.0-or-later

//! Sealed app-owned metadata format upgrade workflow.
//!
//! This GTK-free service is the only place where older app-owned metadata
//! formats may be recognized or converted. Normal runtime readers stay
//! latest-only; UI adapters ask this service for a read-only inventory/plan and
//! call explicit apply commands only after the user chooses an action.

mod apply;
mod backup;
mod diagnostics;
mod inventory;
mod legacy;
mod plan;

pub use apply::{FormatApplyFailure, FormatApplyMode, FormatApplyOutcome, apply_plan, start_fresh};
pub use backup::{FORMAT_UPGRADE_BACKUP_DIR, FormatBackupManifest, FormatBackupRecord};
pub use diagnostics::{
    FormatClassification, FormatInventoryDiagnostic, FormatItemPath, FormatMetadataKind,
    FormatScanBounds,
};
pub use inventory::{FormatInventory, FormatInventoryItem, scan};
pub use plan::{
    FormatPlan, FormatPlanAction, FormatPlanGroup, FormatPlanGroupKind, FormatPlannedItem,
    build_plan,
};

#[cfg(feature = "test-utils")]
pub mod test_support {
    //! Test-only hooks for exercising synthetic old-format converter paths.

    pub use super::legacy::{ConverterFn, ConverterRegistry, ProductionRegistryOverride};
}
