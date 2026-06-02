// SPDX-License-Identifier: GPL-3.0-or-later

//! Feature-gated property-test target for deterministic LushText logic.
//!
//! This integration target is intentionally separate from the default nextest
//! surface. It exercises pure model/service/helper invariants with bounded
//! generated inputs and leaves GTK widgets, compositor behavior, and live
//! session flows to the existing widget harness.

#[path = "properties/encoding_sidecar.rs"]
mod encoding_sidecar;
#[path = "properties/inline_footnotes.rs"]
mod inline_footnotes;
#[path = "properties/palette.rs"]
mod palette;
#[path = "properties/path_rebasing.rs"]
mod path_rebasing;
#[path = "properties/search_replacement.rs"]
mod search_replacement;
#[path = "properties/support.rs"]
mod support;
