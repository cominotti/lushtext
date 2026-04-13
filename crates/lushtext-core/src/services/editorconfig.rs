// SPDX-License-Identifier: GPL-3.0-or-later

//! EditorConfig resolution service — walks the directory tree to find
//! `.editorconfig` files and resolves formatting overrides for a file.
//!
//! This is pure I/O with no GTK dependencies. All functions perform
//! blocking filesystem reads and must be called from a background thread
//! (via `spawn_blocking_then`).

use crate::model::formatting_overrides::FormattingOverrides;
use editorconfig_parser::{EditorConfig, EditorConfigProperty, IndentStyle};
use std::path::Path;

/// Resolve EditorConfig formatting overrides for a file.
///
/// Walks from `file_path`'s parent directory upward, reading each
/// `.editorconfig` file found. Stops at a `root = true` file or the
/// filesystem root. Closer `.editorconfig` files take priority over
/// farther ones.
///
/// Returns `FormattingOverrides::default()` (all `None`) if no
/// `.editorconfig` applies or if resolution fails.
///
/// **Threading:** performs blocking filesystem I/O — call from a
/// background thread only.
#[must_use]
pub fn resolve_for_path(file_path: &Path) -> FormattingOverrides {
    let Some(start_dir) = file_path.parent() else {
        return FormattingOverrides::default();
    };

    // Collect EditorConfig files from closest to farthest.
    // We parse them in this order so closer files can "claim" fields first.
    let mut configs = Vec::new();
    let mut dir = start_dir;

    loop {
        let ec_path = dir.join(".editorconfig");
        if let Ok(content) = std::fs::read_to_string(&ec_path) {
            let config = EditorConfig::parse(&content).with_cwd(dir);
            let is_root = config.root();
            configs.push(config);
            if is_root {
                break;
            }
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent,
            _ => break,
        }
    }

    // Resolve and merge: closest file's properties win.
    // Track which fields have been resolved (by `Value` or `Unset`) to
    // prevent farther files from overriding.
    let mut result = FormattingOverrides::default();
    let mut resolved_tab_width = false;
    let mut resolved_insert_spaces = false;
    let mut resolved_indent_width = false;

    for config in &configs {
        let props = config.resolve(file_path);

        if !resolved_tab_width {
            match props.tab_width {
                EditorConfigProperty::Value(w) => {
                    result.tab_width = Some((w as u32).clamp(1, 12));
                    resolved_tab_width = true;
                }
                EditorConfigProperty::Unset => {
                    // Explicitly unset: no override, but don't inherit from parent.
                    resolved_tab_width = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        if !resolved_insert_spaces {
            match props.indent_style {
                EditorConfigProperty::Value(IndentStyle::Space) => {
                    result.insert_spaces = Some(true);
                    resolved_insert_spaces = true;
                }
                EditorConfigProperty::Value(IndentStyle::Tab) => {
                    result.insert_spaces = Some(false);
                    resolved_insert_spaces = true;
                }
                EditorConfigProperty::Unset => {
                    resolved_insert_spaces = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        if !resolved_indent_width {
            match props.indent_size {
                EditorConfigProperty::Value(s) => {
                    result.indent_width = Some((s as i32).clamp(1, 12));
                    resolved_indent_width = true;
                }
                EditorConfigProperty::Unset => {
                    // Explicit unset → restore GtkSourceView default (-1 = use tab-width).
                    result.indent_width = Some(-1);
                    resolved_indent_width = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        // Early exit: all fields resolved, no need to check parent files.
        if resolved_tab_width && resolved_insert_spaces && resolved_indent_width {
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a `.editorconfig` file with the given content in `dir`.
    fn write_editorconfig(dir: &Path, content: &str) {
        fs::write(dir.join(".editorconfig"), content).unwrap();
    }

    /// Create a file at the given path (empty, just needs to exist for resolution).
    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "").unwrap();
    }

    #[test]
    fn no_editorconfig_returns_default() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("src").join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert!(result.is_empty());
    }

    #[test]
    fn basic_indent_style_and_tab_width() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\nindent_style = space\ntab_width = 2\n",
        );
        let file = tmp.path().join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.tab_width, Some(2));
        assert_eq!(result.insert_spaces, Some(true));
        assert_eq!(result.indent_width, None);
    }

    #[test]
    fn tab_indent_style() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\nindent_style = tab\ntab_width = 4\n",
        );
        let file = tmp.path().join("Makefile");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.tab_width, Some(4));
        assert_eq!(result.insert_spaces, Some(false));
    }

    #[test]
    fn indent_size_maps_to_indent_width() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(tmp.path(), "root = true\n\n[*]\nindent_size = 3\n");
        let file = tmp.path().join("test.py");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.indent_width, Some(3));
    }

    #[test]
    fn root_stops_directory_walk() {
        let tmp = TempDir::new().unwrap();

        // Parent .editorconfig with tab_width = 8
        write_editorconfig(tmp.path(), "[*]\ntab_width = 8\n");

        // Child .editorconfig with root = true, tab_width = 2
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        write_editorconfig(&src, "root = true\n\n[*]\ntab_width = 2\n");

        let file = src.join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        // Child's root = true stops the walk; parent's tab_width = 8 is not seen.
        assert_eq!(result.tab_width, Some(2));
    }

    #[test]
    fn closer_file_overrides_farther() {
        let tmp = TempDir::new().unwrap();

        // Root .editorconfig: tab_width = 8, indent_style = tab
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\ntab_width = 8\nindent_style = tab\n",
        );

        // Nested .editorconfig: tab_width = 2 (overrides root)
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        write_editorconfig(&src, "[*]\ntab_width = 2\n");

        let file = src.join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        // Closer file's tab_width wins.
        assert_eq!(result.tab_width, Some(2));
        // indent_style inherited from root (not overridden by closer file).
        assert_eq!(result.insert_spaces, Some(false));
    }

    #[test]
    fn section_matching_only_applies_to_matching_files() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*.py]\nindent_size = 4\ntab_width = 4\n\n[*.rs]\ntab_width = 2\n",
        );

        let rs_file = tmp.path().join("main.rs");
        touch(&rs_file);
        let rs_result = resolve_for_path(&rs_file);
        assert_eq!(rs_result.tab_width, Some(2));
        assert_eq!(rs_result.indent_width, None); // indent_size only in [*.py]

        let py_file = tmp.path().join("main.py");
        touch(&py_file);
        let py_result = resolve_for_path(&py_file);
        assert_eq!(py_result.tab_width, Some(4));
        assert_eq!(py_result.indent_width, Some(4));
    }

    #[test]
    fn tab_width_clamped_to_valid_range() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(tmp.path(), "root = true\n\n[*]\ntab_width = 100\n");
        let file = tmp.path().join("test.txt");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.tab_width, Some(12)); // Clamped to max
    }

    #[test]
    fn indent_size_clamped_to_valid_range() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(tmp.path(), "root = true\n\n[*]\nindent_size = 0\n");
        let file = tmp.path().join("test.txt");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.indent_width, Some(1)); // Clamped to min
    }

    #[test]
    fn no_matching_section_returns_default() {
        let tmp = TempDir::new().unwrap();
        write_editorconfig(tmp.path(), "root = true\n\n[*.py]\ntab_width = 4\n");
        let file = tmp.path().join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert!(result.is_empty());
    }

    #[test]
    fn file_at_root_with_no_parent() {
        // Pathological case: file_path has no parent.
        let result = resolve_for_path(Path::new("orphan.txt"));
        assert!(result.is_empty());
    }
}
