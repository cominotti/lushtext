// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target for bounded structured editor/service operation scripts.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lushtext_core::fuzzing::exercise_operation_script_for_fuzzing;

fuzz_target!(|data: &[u8]| {
    let report = exercise_operation_script_for_fuzzing(data);
    let _ = report.operations_run;
    let _ = report.decode_runs;
    let _ = report.formatting_runs;
    let _ = report.markdown_runs;
    let _ = report.replacement_previews;
    let _ = report.session_roundtrips;
    let _ = report.draft_roundtrips;
    let _ = report.session_raw_decodes;
    let _ = report.draft_raw_decodes;
    let _ = report.max_text_len_seen;
});
