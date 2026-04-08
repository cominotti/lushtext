// SPDX-License-Identifier: GPL-3.0-or-later

//! Domain data types: workspace configuration, session persistence,
//! command palette search, and per-file formatting overrides.
//!
//! All types in this layer are pure Rust with no GTK dependencies, making
//! them fully unit-testable and usable from background threads.

pub mod content_search;
pub mod draft;
pub mod formatting_overrides;
pub mod palette;
pub mod session;
pub mod workspace;
