// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for LushText non-GUI services.
//!
//! Tests run against lushtext-core directly (pure filesystem operations,
//! no display server needed). Split into submodules for parallel compilation.
//!
//! Run with: `cargo test --test integration` or `make test-int`

#[path = "integration/common.rs"]
mod common;

#[path = "integration/workspace.rs"]
mod workspace;

#[path = "integration/session.rs"]
mod session;

#[path = "integration/file_tree.rs"]
mod file_tree;
