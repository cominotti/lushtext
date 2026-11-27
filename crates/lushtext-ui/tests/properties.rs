// SPDX-License-Identifier: GPL-3.0-or-later

//! Feature-gated property-test target for UI-coupled helpers and geometry.
//!
//! Pure domain-model and GTK-free service properties live in
//! `lushtext-model` and `lushtext-services` respectively; this target keeps the
//! remaining UI/viewport/widget-coupled properties.

#[path = "properties/paragraph_boundary.rs"]
mod paragraph_boundary;
#[path = "properties/support.rs"]
mod support;
#[path = "properties/text_scans.rs"]
mod text_scans;
#[path = "properties/viewport_slice.rs"]
mod viewport_slice;
#[path = "properties/width_preset.rs"]
mod width_preset;
