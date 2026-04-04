// SPDX-License-Identifier: GPL-3.0-or-later

//! Domain data types: workspace configuration, session persistence, and
//! command palette search.
//!
//! All types in this layer are pure Rust with no GTK dependencies, making
//! them fully unit-testable and usable from background threads.

pub mod draft;
pub mod palette;
pub mod session;
pub mod workspace;
