// SPDX-License-Identifier: GPL-3.0-or-later

//! Domain data types: workspace configuration, session persistence,
//! command palette search, and per-file formatting overrides.
//!
//! All types in this layer are pure Rust with no GTK dependencies, making
//! them fully unit-testable and usable from background threads.

pub mod action_catalog;
pub mod automation;
pub mod bookmark;
pub mod buffer_replacement;
pub mod content_search;
pub mod document_note;
pub mod draft;
pub mod editor_memory;
pub mod encoding;
pub mod file_load;
pub mod folder_note;
pub mod formatting_overrides;
pub mod local_history;
pub mod migration_ledger;
pub mod note;
pub mod palette;
pub mod recent_document;
pub mod session;
pub mod sidecar_identity;
pub mod workspace;
