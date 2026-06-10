// SPDX-License-Identifier: MIT OR Apache-2.0

//! Placeholder reservation for GTK Lush settle and debounce helpers.
//!
//! GTK Lush crates must remain independently adoptable leaf crates: no GTK
//! control-flow ownership, no custom UI DSL, no state/message framework, no
//! runtime dependency on another GTK Lush crate, and no replacement for
//! Libadwaita adaptive behavior.
//!
//! This `0.0.0` crate intentionally exposes no public API. The
//! `extract-gtk-lush-signals-and-settle` OpenSpec follow-up will design the
//! debounce, settle-burst, and superseding-timer APIs against the program
//! governance in `crates/gtk-lush/GOVERNANCE.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
