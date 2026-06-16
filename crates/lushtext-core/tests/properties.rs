// SPDX-License-Identifier: GPL-3.0-or-later

//! Feature-gated property-test target for deterministic LushText logic.
//!
//! This integration target is intentionally separate from the default nextest
//! surface. It exercises pure model/service/helper invariants with bounded
//! generated inputs and leaves GTK widgets, compositor behavior, and live
//! session flows to the existing widget harness.

#[path = "properties/durable_write_metadata.rs"]
mod durable_write_metadata;
#[path = "properties/editor_formatting.rs"]
mod editor_formatting;
#[path = "properties/encoding_sidecar.rs"]
mod encoding_sidecar;
#[path = "properties/format_upgrade.rs"]
mod format_upgrade;
#[path = "properties/inline_footnotes.rs"]
mod inline_footnotes;
#[path = "properties/note.rs"]
mod note;
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
#[path = "properties/session_draft_roundtrip.rs"]
mod session_draft_roundtrip;
#[path = "properties/sidecar_reconciliation.rs"]
mod sidecar_reconciliation;
#[path = "properties/support.rs"]
mod support;
