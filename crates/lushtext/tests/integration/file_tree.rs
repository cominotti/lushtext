// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for file tree scanning with realistic project layouts.

use crate::common::TestContext;
use lushtext_core::services::file_tree::{self, DirectoryEntry};

/// Helper: extract file names from scan results.
fn names(entries: &[DirectoryEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| {
            entry
                .path
                .file_name()
                .expect("expected operation to succeed")
                .to_string_lossy()
                .to_string()
        })
        .collect()
}

#[test]
fn test_scan_realistic_project_layout() {
    let ctx = TestContext::new();

    // Set up a Rust project structure
    let project = ctx.mkdir("project");
    ctx.mkdir("project/src");
    ctx.mkdir("project/tests");
    ctx.write_file("project/src/main.rs", "fn main() {}");
    ctx.write_file("project/src/lib.rs", "pub mod app;");
    ctx.write_file("project/Cargo.toml", "[package]\nname = \"example\"");
    ctx.write_file("project/README.md", "# Example");
    ctx.write_file("project/.gitignore", "target/");
    ctx.mkdir("project/.git");

    let entries = file_tree::scan_directory(&project);
    let entry_names = names(&entries);

    assert_eq!(entry_names, vec!["src", "tests", "Cargo.toml", "README.md"]);

    // Verify is_dir flags
    assert!(entries[0].is_dir, "src should be dir");
    assert!(entries[1].is_dir, "tests should be dir");
    assert!(!entries[2].is_dir, "Cargo.toml should be file");
    assert!(!entries[3].is_dir, "README.md should be file");
}

#[test]
fn test_scan_subdirectory_contents() {
    let ctx = TestContext::new();

    ctx.mkdir("project/src");
    ctx.write_file("project/src/main.rs", "fn main() {}");
    ctx.write_file("project/src/lib.rs", "pub mod app;");
    ctx.write_file("project/src/app.rs", "pub struct App;");

    let src_dir = ctx.path().join("project/src");
    let entries = file_tree::scan_directory(&src_dir);
    let entry_names = names(&entries);

    assert_eq!(entry_names, vec!["app.rs", "lib.rs", "main.rs"]);
    assert!(entries.iter().all(|entry| !entry.is_dir));
}

#[test]
fn test_scan_workspace_with_multiple_roots() {
    let ctx = TestContext::new();

    // Simulate two workspace root directories
    let root1 = ctx.mkdir("frontend");
    ctx.write_file("frontend/index.html", "<html>");
    ctx.mkdir("frontend/src");

    let root2 = ctx.mkdir("backend");
    ctx.write_file("backend/main.go", "package main");
    ctx.mkdir("backend/pkg");

    let entries1 = file_tree::scan_directory(&root1);
    assert_eq!(names(&entries1), vec!["src", "index.html"]);

    let entries2 = file_tree::scan_directory(&root2);
    assert_eq!(names(&entries2), vec!["pkg", "main.go"]);
}
