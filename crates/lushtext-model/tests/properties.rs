// SPDX-License-Identifier: GPL-3.0-or-later

//! Feature-gated property-test target for pure domain model invariants.

#[path = "properties/attention_refresh.rs"]
mod attention_refresh;
#[path = "properties/encoding_sidecar.rs"]
mod encoding_sidecar;
#[path = "properties/file_load.rs"]
mod file_load;
#[path = "properties/note.rs"]
mod note;
#[path = "properties/paced_slices.rs"]
mod paced_slices;
#[path = "properties/protected_fold.rs"]
mod protected_fold;
#[path = "properties/protected_install.rs"]
mod protected_install;
#[path = "properties/responsiveness_budget.rs"]
mod responsiveness_budget;
#[path = "properties/session_draft_roundtrip.rs"]
mod session_draft_roundtrip;
#[path = "properties/settings_write_batch.rs"]
mod settings_write_batch;
#[path = "properties/support.rs"]
mod support;
#[path = "properties/values.rs"]
mod values;
