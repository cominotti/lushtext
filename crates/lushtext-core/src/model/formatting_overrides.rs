// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-file formatting overrides resolved from EditorConfig (or future providers).
//!
//! Pure domain type with no GTK dependencies. `None` on each field means
//! "no override — fall back to the editor's global settings (GSettings)."

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
}

impl FormattingOverrides {
    /// True when no overrides are active (all fields are `None`).
    pub fn is_empty(self) -> bool {
        self.tab_width.is_none() && self.insert_spaces.is_none() && self.indent_width.is_none()
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
    }

    #[test]
    fn copy_semantics() {
        let a = FormattingOverrides {
            tab_width: Some(4),
            insert_spaces: Some(true),
            indent_width: Some(2),
        };
        let b = a;
        assert_eq!(a, b);
    }
}
