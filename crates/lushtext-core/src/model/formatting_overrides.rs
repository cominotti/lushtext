// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-file formatting and save-policy overrides resolved from EditorConfig.
//!
//! Pure domain type with no GTK dependencies. `None` on each field means no
//! override, so the editor falls back to its loaded-document state or global
//! preferences depending on the setting.

use crate::model::encoding::{DocumentEncoding, LineEnding};

/// Per-file formatting overrides from EditorConfig.
///
/// Only covers settings that can vary per-file (formatting). Visual-only
/// settings (line numbers, current line highlight, color scheme, font)
/// are not overrideable and stay as direct GSettings bindings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FormattingOverrides {
    /// Override for GtkSourceView `tab-width` property (1..=12).
    pub tab_width: Option<u32>,
    /// Override for GtkSourceView `insert-spaces-instead-of-tabs` property.
    pub insert_spaces: Option<bool>,
    /// Override for GtkSourceView `indent-width` property.
    /// -1 in GtkSourceView means "inherit from tab-width".
    pub indent_width: Option<i32>,
    /// Save-time line ending requested by EditorConfig `end_of_line`.
    pub line_ending: Option<LineEnding>,
    /// Save-time encoding requested by EditorConfig `charset`.
    ///
    /// Unsupported EditorConfig charsets are ignored instead of approximated,
    /// so `latin1` does not silently become Windows-1252.
    pub save_encoding: Option<DocumentEncoding>,
    /// Whether save should strip spaces and tabs before line endings.
    pub trim_trailing_whitespace: Option<bool>,
    /// Whether save should ensure a non-empty document ends with one newline.
    pub insert_final_newline: Option<bool>,
}

impl FormattingOverrides {
    /// True when no overrides are active (all fields are `None`).
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.tab_width.is_none()
            && self.insert_spaces.is_none()
            && self.indent_width.is_none()
            && self.line_ending.is_none()
            && self.save_encoding.is_none()
            && self.trim_trailing_whitespace.is_none()
            && self.insert_final_newline.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert!(FormattingOverrides::default().is_empty());
    }

    #[test]
    fn non_empty_when_any_field_set() {
        let o = FormattingOverrides {
            tab_width: Some(2),
            ..Default::default()
        };
        assert!(!o.is_empty());

        let o = FormattingOverrides {
            insert_spaces: Some(false),
            ..Default::default()
        };
        assert!(!o.is_empty());

        let o = FormattingOverrides {
            indent_width: Some(4),
            ..Default::default()
        };
        assert!(!o.is_empty());

        let o = FormattingOverrides {
            line_ending: Some(LineEnding::Crlf),
            ..Default::default()
        };
        assert!(!o.is_empty());

        let o = FormattingOverrides {
            save_encoding: Some(DocumentEncoding::Utf8Bom),
            ..Default::default()
        };
        assert!(!o.is_empty());

        let o = FormattingOverrides {
            trim_trailing_whitespace: Some(true),
            ..Default::default()
        };
        assert!(!o.is_empty());

        let o = FormattingOverrides {
            insert_final_newline: Some(true),
            ..Default::default()
        };
        assert!(!o.is_empty());
    }

    #[test]
    fn copy_semantics() {
        let a = FormattingOverrides {
            tab_width: Some(4),
            insert_spaces: Some(true),
            indent_width: Some(2),
            line_ending: Some(LineEnding::Lf),
            save_encoding: Some(DocumentEncoding::Utf8),
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
        };
        let b = a;
        assert_eq!(a, b);
    }
}
