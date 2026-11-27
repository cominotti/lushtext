// SPDX-License-Identifier: GPL-3.0-or-later

//! Feature-gated property-test target for GTK-free application service invariants.

#[path = "properties/draft_orphan_cleanup.rs"]
mod draft_orphan_cleanup;
#[path = "properties/durable_write_metadata.rs"]
mod durable_write_metadata;
#[path = "properties/editor_formatting.rs"]
mod editor_formatting;
#[path = "properties/encoding_analysis.rs"]
mod encoding_analysis;
#[path = "properties/format_upgrade.rs"]
mod format_upgrade;
#[path = "properties/inline_footnotes.rs"]
mod inline_footnotes;
#[path = "properties/markdown_plan.rs"]
mod markdown_plan;
#[path = "properties/palette.rs"]
mod palette;
#[path = "properties/path_rebasing.rs"]
mod path_rebasing;
#[path = "properties/recovery_metadata.rs"]
mod recovery_metadata;
#[path = "properties/replace_journal_recovery.rs"]
mod replace_journal_recovery;
#[path = "properties/replace_undo.rs"]
mod replace_undo;
#[path = "properties/search_replacement.rs"]
mod search_replacement;
#[path = "properties/shell_conformance_drafts.rs"]
mod shell_conformance_drafts;
#[path = "properties/shell_conformance_write.rs"]
mod shell_conformance_write;
#[path = "properties/sidecar_reconciliation.rs"]
mod sidecar_reconciliation;
#[path = "properties/support.rs"]
mod support;
