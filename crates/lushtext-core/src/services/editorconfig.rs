// SPDX-License-Identifier: GPL-3.0-or-later

//! EditorConfig resolution service — walks the directory tree to find
//! `.editorconfig` files and resolves formatting overrides for a file.
//!
//! This is pure I/O with no GTK dependencies. All functions perform
//! blocking filesystem reads and must be called from a background thread
//! (via `spawn_blocking_then`).

use crate::model::encoding::{DocumentEncoding, LineEnding};
use crate::model::formatting_overrides::FormattingOverrides;
use editorconfig_parser::{
    Charset, EditorConfig, EditorConfigProperties, EditorConfigProperty, EndOfLine, IndentStyle,
};
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
    let mut resolved_line_ending = false;
    let mut resolved_save_encoding = false;
    let mut resolved_trim_trailing_whitespace = false;
    let mut resolved_insert_final_newline = false;

    for config in &configs {
        let props = resolve_preserving_unset(config, file_path);

        if !resolved_tab_width {
            match props.tab_width {
                EditorConfigProperty::Value(w) => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "The value is clamped to the GtkSourceView tab-width range before converting to u32"
                    )]
                    let w = (w as u32).clamp(1, 12);
                    result.tab_width = Some(w);
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
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "EditorConfig indent sizes are clamped to the GtkSourceView-supported range before converting to u32"
                    )]
                    let s = (s as i32).clamp(1, 12);
                    result.indent_width = Some(s);
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

        if !resolved_line_ending {
            match props.end_of_line {
                EditorConfigProperty::Value(value) => {
                    result.line_ending = Some(map_line_ending(value));
                    resolved_line_ending = true;
                }
                EditorConfigProperty::Unset => {
                    resolved_line_ending = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        if !resolved_save_encoding {
            match props.charset {
                EditorConfigProperty::Value(value) => {
                    result.save_encoding = map_charset(value);
                    resolved_save_encoding = true;
                }
                EditorConfigProperty::Unset => {
                    resolved_save_encoding = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        if !resolved_trim_trailing_whitespace {
            match props.trim_trailing_whitespace {
                EditorConfigProperty::Value(value) => {
                    result.trim_trailing_whitespace = Some(value);
                    resolved_trim_trailing_whitespace = true;
                }
                EditorConfigProperty::Unset => {
                    resolved_trim_trailing_whitespace = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        if !resolved_insert_final_newline {
            match props.insert_final_newline {
                EditorConfigProperty::Value(value) => {
                    result.insert_final_newline = Some(value);
                    resolved_insert_final_newline = true;
                }
                EditorConfigProperty::Unset => {
                    resolved_insert_final_newline = true;
                }
                EditorConfigProperty::None => {}
            }
        }

        // Early exit: all fields resolved, no need to check parent files.
        if resolved_tab_width
            && resolved_insert_spaces
            && resolved_indent_width
            && resolved_line_ending
            && resolved_save_encoding
            && resolved_trim_trailing_whitespace
            && resolved_insert_final_newline
        {
            break;
        }
    }

    result
}

/// Resolve one parsed file while preserving `unset` so parent files cannot refill that field.
fn resolve_preserving_unset(config: &EditorConfig, file_path: &Path) -> EditorConfigProperties {
    let path = if let Some(cwd) = config.cwd() {
        file_path.strip_prefix(cwd).unwrap_or(file_path)
    } else {
        file_path
    };
    let mut properties = EditorConfigProperties::default();
    for section in config.sections() {
        if section
            .matcher
            .as_ref()
            .is_some_and(|matcher| matcher.is_match(path))
        {
            merge_property(
                &mut properties.indent_style,
                &section.properties.indent_style,
            );
            merge_property(&mut properties.indent_size, &section.properties.indent_size);
            merge_property(&mut properties.tab_width, &section.properties.tab_width);
            merge_property(&mut properties.end_of_line, &section.properties.end_of_line);
            merge_property(&mut properties.charset, &section.properties.charset);
            merge_property(
                &mut properties.trim_trailing_whitespace,
                &section.properties.trim_trailing_whitespace,
            );
            merge_property(
                &mut properties.insert_final_newline,
                &section.properties.insert_final_newline,
            );
        }
    }
    properties
}

fn merge_property<T: Copy>(target: &mut EditorConfigProperty<T>, source: &EditorConfigProperty<T>) {
    match source {
        EditorConfigProperty::Value(value) => {
            *target = EditorConfigProperty::Value(*value);
        }
        EditorConfigProperty::Unset => {
            *target = EditorConfigProperty::Unset;
        }
        EditorConfigProperty::None => {}
    }
}

/// Map EditorConfig line-ending values onto LushText's save-policy vocabulary.
#[must_use]
fn map_line_ending(value: EndOfLine) -> LineEnding {
    match value {
        EndOfLine::Lf => LineEnding::Lf,
        EndOfLine::Cr => LineEnding::Cr,
        EndOfLine::Crlf => LineEnding::Crlf,
    }
}

/// Map exact EditorConfig charsets to save encodings LushText can preserve.
#[must_use]
fn map_charset(value: Charset) -> Option<DocumentEncoding> {
    match value {
        Charset::Utf8 => Some(DocumentEncoding::Utf8),
        Charset::Utf8bom => Some(DocumentEncoding::Utf8Bom),
        Charset::Utf16be => Some(DocumentEncoding::Utf16Be),
        Charset::Utf16le => Some(DocumentEncoding::Utf16Le),
        Charset::Latin1 => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a `.editorconfig` file with the given content in `dir`.
    fn write_editorconfig(dir: &Path, content: &str) {
        fs::write(dir.join(".editorconfig"), content).expect("expected operation to succeed");
    }

    /// Create a file at the given path (empty, just needs to exist for resolution).
    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("expected operation to succeed");
        }
        fs::write(path, "").expect("expected operation to succeed");
    }

    #[test]
    fn no_editorconfig_returns_default() {
        let tmp = TempDir::new().expect("expected operation to succeed");
        let file = tmp.path().join("src").join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert!(result.is_empty());
    }

    #[test]
    fn basic_indent_style_and_tab_width() {
        let tmp = TempDir::new().expect("expected operation to succeed");
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
    fn save_policy_properties_are_resolved() {
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\nend_of_line = crlf\ncharset = utf-8-bom\ntrim_trailing_whitespace = true\ninsert_final_newline = true\n",
        );
        let file = tmp.path().join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.line_ending, Some(LineEnding::Crlf));
        assert_eq!(result.save_encoding, Some(DocumentEncoding::Utf8Bom));
        assert_eq!(result.trim_trailing_whitespace, Some(true));
        assert_eq!(result.insert_final_newline, Some(true));
    }

    #[test]
    fn unsupported_latin1_charset_marks_charset_resolved_without_guessing() {
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\ncharset = utf-8\n\n[*.txt]\ncharset = latin1\n",
        );
        let file = tmp.path().join("note.txt");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.save_encoding, None);
    }

    #[test]
    fn tab_indent_style() {
        let tmp = TempDir::new().expect("expected operation to succeed");
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
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(tmp.path(), "root = true\n\n[*]\nindent_size = 3\n");
        let file = tmp.path().join("test.py");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.indent_width, Some(3));
    }

    #[test]
    fn indent_size_unset_restores_tab_width_default() {
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\nindent_size = 4\n\n[*.rs]\nindent_size = unset\n",
        );
        let file = tmp.path().join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.indent_width, Some(-1));
    }

    #[test]
    fn root_stops_directory_walk() {
        let tmp = TempDir::new().expect("expected operation to succeed");

        // Parent .editorconfig with tab_width = 8
        write_editorconfig(tmp.path(), "[*]\ntab_width = 8\n");

        // Child .editorconfig with root = true, tab_width = 2
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).expect("expected operation to succeed");
        write_editorconfig(&src, "root = true\n\n[*]\ntab_width = 2\n");

        let file = src.join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        // Child's root = true stops the walk; parent's tab_width = 8 is not seen.
        assert_eq!(result.tab_width, Some(2));
    }

    #[test]
    fn closer_file_overrides_farther() {
        let tmp = TempDir::new().expect("expected operation to succeed");

        // Root .editorconfig: tab_width = 8, indent_style = tab
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\ntab_width = 8\nindent_style = tab\n",
        );

        // Nested .editorconfig: tab_width = 2 (overrides root)
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).expect("expected operation to succeed");
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
    fn partial_closer_config_still_inherits_unresolved_parent_fields() {
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(
            tmp.path(),
            "root = true\n\n[*]\ntrim_trailing_whitespace = true\ninsert_final_newline = true\n",
        );

        let src = tmp.path().join("src");
        fs::create_dir_all(&src).expect("expected operation to succeed");
        write_editorconfig(
            &src,
            "[*]\ntab_width = 2\nindent_style = space\nindent_size = 3\nend_of_line = lf\ncharset = utf-8\ntrim_trailing_whitespace = false\n",
        );

        let file = src.join("main.rs");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.tab_width, Some(2));
        assert_eq!(result.insert_spaces, Some(true));
        assert_eq!(result.indent_width, Some(3));
        assert_eq!(result.line_ending, Some(LineEnding::Lf));
        assert_eq!(result.save_encoding, Some(DocumentEncoding::Utf8));
        assert_eq!(result.trim_trailing_whitespace, Some(false));
        assert_eq!(result.insert_final_newline, Some(true));
    }

    #[test]
    fn section_matching_only_applies_to_matching_files() {
        let tmp = TempDir::new().expect("expected operation to succeed");
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
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(tmp.path(), "root = true\n\n[*]\ntab_width = 100\n");
        let file = tmp.path().join("test.txt");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.tab_width, Some(12)); // Clamped to max
    }

    #[test]
    fn indent_size_clamped_to_valid_range() {
        let tmp = TempDir::new().expect("expected operation to succeed");
        write_editorconfig(tmp.path(), "root = true\n\n[*]\nindent_size = 0\n");
        let file = tmp.path().join("test.txt");
        touch(&file);

        let result = resolve_for_path(&file);
        assert_eq!(result.indent_width, Some(1)); // Clamped to min
    }

    #[test]
    fn no_matching_section_returns_default() {
        let tmp = TempDir::new().expect("expected operation to succeed");
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
