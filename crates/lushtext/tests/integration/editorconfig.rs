// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for EditorConfig resolution.
//!
//! These tests exercise the full resolution pipeline: directory walking,
//! file parsing, section matching, and multi-level override merging.

use super::common::TestContext;
use lushtext_core::services::editorconfig;

#[test]
fn single_editorconfig_with_root() {
    let ctx = TestContext::new();
    ctx.write_file(
        ".editorconfig",
        "root = true\n\n[*]\nindent_style = space\ntab_width = 2\nindent_size = 4\n",
    );
    let file = ctx.write_file("src/main.rs", "fn main() {}");

    let overrides = editorconfig::resolve_for_path(&file);
    assert_eq!(overrides.tab_width, Some(2));
    assert_eq!(overrides.insert_spaces, Some(true));
    assert_eq!(overrides.indent_width, Some(4));
}

#[test]
fn nested_editorconfig_files_merge() {
    let ctx = TestContext::new();

    // Root: tabs, tab_width=4
    ctx.write_file(
        ".editorconfig",
        "root = true\n\n[*]\nindent_style = tab\ntab_width = 4\n",
    );

    // Subdirectory: override tab_width to 2, inherit indent_style
    ctx.write_file("src/.editorconfig", "[*]\ntab_width = 2\n");

    let file = ctx.write_file("src/lib.rs", "// lib");

    let overrides = editorconfig::resolve_for_path(&file);
    assert_eq!(overrides.tab_width, Some(2)); // From closer file
    assert_eq!(overrides.insert_spaces, Some(false)); // Inherited from root
}

#[test]
fn section_glob_pattern_matching() {
    let ctx = TestContext::new();

    ctx.write_file(
        ".editorconfig",
        "root = true\n\n\
         [*.rs]\nindent_style = space\ntab_width = 4\n\n\
         [*.py]\nindent_style = space\ntab_width = 4\nindent_size = 4\n\n\
         [Makefile]\nindent_style = tab\ntab_width = 8\n",
    );

    let rs = ctx.write_file("src/main.rs", "");
    let py = ctx.write_file("script.py", "");
    let mk = ctx.write_file("Makefile", "");
    let txt = ctx.write_file("readme.txt", "");

    let rs_result = editorconfig::resolve_for_path(&rs);
    assert_eq!(rs_result.insert_spaces, Some(true));
    assert_eq!(rs_result.tab_width, Some(4));
    assert_eq!(rs_result.indent_width, None);

    let py_result = editorconfig::resolve_for_path(&py);
    assert_eq!(py_result.insert_spaces, Some(true));
    assert_eq!(py_result.tab_width, Some(4));
    assert_eq!(py_result.indent_width, Some(4));

    let mk_result = editorconfig::resolve_for_path(&mk);
    assert_eq!(mk_result.insert_spaces, Some(false));
    assert_eq!(mk_result.tab_width, Some(8));

    // txt doesn't match any section
    let txt_result = editorconfig::resolve_for_path(&txt);
    assert!(txt_result.is_empty());
}

#[test]
fn no_editorconfig_in_tree() {
    let ctx = TestContext::new();
    let file = ctx.write_file("src/deep/nested/file.rs", "");

    let overrides = editorconfig::resolve_for_path(&file);
    assert!(overrides.is_empty());
}

#[test]
fn root_true_stops_walk_at_subdirectory() {
    let ctx = TestContext::new();

    // Top-level: tab_width=8
    ctx.write_file(".editorconfig", "[*]\ntab_width = 8\n");

    // Nested root: tab_width=2, root=true
    ctx.write_file(
        "project/.editorconfig",
        "root = true\n\n[*]\ntab_width = 2\n",
    );

    let file = ctx.write_file("project/src/main.rs", "");

    let overrides = editorconfig::resolve_for_path(&file);
    // root=true stops the walk, so top-level tab_width=8 is never seen
    assert_eq!(overrides.tab_width, Some(2));
}

#[test]
fn deeply_nested_file_inherits_from_root() {
    let ctx = TestContext::new();
    ctx.write_file(
        ".editorconfig",
        "root = true\n\n[*]\nindent_style = space\ntab_width = 4\n",
    );

    // File is deeply nested, but no intermediate .editorconfig files
    let file = ctx.write_file("a/b/c/d/e/f.rs", "");

    let overrides = editorconfig::resolve_for_path(&file);
    assert_eq!(overrides.tab_width, Some(4));
    assert_eq!(overrides.insert_spaces, Some(true));
}

#[test]
fn partial_overrides_merge_correctly() {
    let ctx = TestContext::new();

    // Root provides all three settings
    ctx.write_file(
        ".editorconfig",
        "root = true\n\n[*]\nindent_style = space\ntab_width = 4\nindent_size = 4\n",
    );

    // Nested overrides only tab_width
    ctx.write_file("src/.editorconfig", "[*]\ntab_width = 2\n");

    let file = ctx.write_file("src/main.rs", "");

    let overrides = editorconfig::resolve_for_path(&file);
    assert_eq!(overrides.tab_width, Some(2)); // From closer
    assert_eq!(overrides.insert_spaces, Some(true)); // From root
    assert_eq!(overrides.indent_width, Some(4)); // From root
}

#[test]
fn overrides_model_is_empty_check() {
    let ctx = TestContext::new();

    ctx.write_file(
        ".editorconfig",
        "root = true\n\n[*.rs]\nindent_style = space\n",
    );

    // .rs file should have overrides
    let rs_file = ctx.write_file("main.rs", "");
    let rs_result = editorconfig::resolve_for_path(&rs_file);
    assert!(!rs_result.is_empty());

    // .txt file should not
    let txt_file = ctx.write_file("readme.txt", "");
    let txt_result = editorconfig::resolve_for_path(&txt_file);
    assert!(txt_result.is_empty());
}
