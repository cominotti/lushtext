// SPDX-License-Identifier: GPL-3.0-or-later

//! Fuzz target for editor byte decoding and file-health classification.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lushtext_core::fuzzing::exercise_editor_bytes_for_fuzzing;

fuzz_target!(|data: &[u8]| {
    let report = exercise_editor_bytes_for_fuzzing(data);
    let _ = report.decode_runs;
    let _ = report.decoded_bytes;
    let _ = report.file_health_findings;
});
