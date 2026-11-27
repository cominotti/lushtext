// SPDX-License-Identifier: GPL-3.0-or-later

//! Domain data types organized by bounded context.
//!
//! All types in this layer are pure Rust with no GTK or filesystem dependencies.

pub mod automation;
pub mod budgets;
pub mod document;
pub mod persistence;
pub mod values;
pub mod workspace;
