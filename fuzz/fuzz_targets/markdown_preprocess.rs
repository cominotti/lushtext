// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target for Markdown preprocessing and parser setup.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lushtext_core::fuzzing::exercise_markdown_for_fuzzing;

fuzz_target!(|data: &[u8]| {
    let result = exercise_markdown_for_fuzzing(data);
    let _ = result.parser_event_count;
    let _ = result.parser_input_len;
    let _ = result.lowered_inline_footnotes;
});
