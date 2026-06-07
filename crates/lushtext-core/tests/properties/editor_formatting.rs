// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for EditorConfig save-only formatting rewrites.
//!
//! Save-time whitespace and final-newline policy is pure string processing.
//! Keeping it in the property lane exercises mixed line endings and awkward
//! trailing whitespace without involving GTK buffers or filesystem writes.

use lushtext_core::model::encoding::{DocumentEncoding, LineEnding};
use lushtext_core::model::formatting_overrides::FormattingOverrides;
use lushtext_core::services::editor_io::apply_save_formatting_overrides;
use proptest::prelude::*;

use crate::support;

/// Line-ending values that can appear in EditorConfig-derived overrides.
const LINE_ENDINGS: [LineEnding; 4] = [
    LineEnding::Lf,
    LineEnding::Crlf,
    LineEnding::Cr,
    LineEnding::Mixed,
];

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn editorconfig_save_formatting_is_idempotent(
        text in support::save_formatting_text(),
        overrides in formatting_overrides(),
    ) {
        let once = apply_save_formatting_overrides(&text, overrides);
        let twice = apply_save_formatting_overrides(&once, overrides);

        prop_assert_eq!(twice, once);
    }
}

/// Generate all EditorConfig override fields, including ones the save-formatting helper ignores.
fn formatting_overrides() -> impl Strategy<Value = FormattingOverrides> {
    (
        prop::option::of(1u32..=12),
        prop::option::of(any::<bool>()),
        prop::option::of(indent_width()),
        prop::option::of(prop::sample::select(&LINE_ENDINGS)),
        prop::option::of(prop::sample::select(&DocumentEncoding::COMMON)),
        prop::option::of(any::<bool>()),
        prop::option::of(any::<bool>()),
    )
        .prop_map(
            |(
                tab_width,
                insert_spaces,
                indent_width,
                line_ending,
                save_encoding,
                trim_trailing_whitespace,
                insert_final_newline,
            )| FormattingOverrides {
                tab_width,
                insert_spaces,
                indent_width,
                line_ending,
                save_encoding,
                trim_trailing_whitespace,
                insert_final_newline,
            },
        )
}

/// Generate the GtkSourceView indent-width domain used by EditorConfig.
fn indent_width() -> impl Strategy<Value = i32> {
    prop_oneof![Just(-1), 1i32..=12]
}
