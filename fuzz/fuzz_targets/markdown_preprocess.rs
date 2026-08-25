// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target for Markdown preprocessing, parser setup, and GTK-free planning.
//!
//! This is the surface that reaches `plan_markdown_cancellable`, so the
//! checkpoint, sub-slicing, omission, and carried-embed paths are fuzzed here
//! and replayed by the stable `markdown_preprocess` corpus.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lushtext_core::fuzzing::exercise_markdown_for_fuzzing;

fuzz_target!(|data: &[u8]| {
    let result = exercise_markdown_for_fuzzing(data);
    let _ = result.parser_event_count;
    let _ = result.parser_input_len;
    let _ = result.lowered_inline_footnotes;
    let _ = result.plan_batches;
    let _ = result.plan_omissions;
    let _ = result.plan_limited;
});
