// SPDX-License-Identifier: GPL-3.0-or-later

//! The branch side of the M9 measurement (stream-replace-all design R3, tasks
//! 2.4, 4.8, 9.2 and 9.3): one `apply_replacements` call over generated
//! files, run as its own process with VmHWM reset first, printing one
//! `sr-evidence` JSON line. Ignored: it measures, it does not test. `main`'s
//! side is the same test over `main`'s signature
//! (`.claude/handoffs/stream-replace-all/measure/main_m9.rs.add`), so both
//! read the same variables:
//!
//! - `SR_M9_FILES` (default 1) files of `SR_M9_BYTES` (default the per-file
//!   limit) bytes each, `needle` on the first line, in a temporary directory
//!   under `SR_M9_DIR` (default the system temporary directory);
//! - `SR_M9_JOURNAL`: recorded only, since the branch always journals (the
//!   journal is under the same temporary directory);
//! - `SR_M9_CHUNK_BYTES`: the stream chunk size for this run (task 2.4).
//!
//! The evidence adds the two passes' wall time (`pass_a_ms`, `pass_b_ms`,
//! each including its own syncs) and the syncs' total (`fsync_ms`).

use super::*;
use crate::filesystem::fixture;
use lushtext_model::workspace::content_search::Replacement;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

fn reset_hwm() {
    fixture::write_text(Path::new("/proc/self/clear_refs"), "5");
}

fn status_kib(field: &str) -> u64 {
    fixture::read_text(Path::new("/proc/self/status"))
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "measurement"]
fn sr_measure_m9() {
    let files_wanted = usize::try_from(env_u64("SR_M9_FILES", 1)).expect("file count");
    let bytes = usize::try_from(env_u64("SR_M9_BYTES", MAX_REPLACE_FILE_BYTES)).expect("bytes");
    let chunk = std::env::var("SR_M9_CHUNK_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    if let Some(chunk) = chunk {
        super::stream::TEST_STREAM_CHUNK_BYTES.with(|slot| slot.set(Some(chunk)));
    }
    let base = std::env::var("SR_M9_DIR").map_or_else(|_| std::env::temp_dir(), PathBuf::from);
    let dir = tempfile::tempdir_in(&base).expect("tempdir");
    let journal_requested = std::env::var("SR_M9_JOURNAL").is_ok();
    let journal = dir.path().join("data");
    fixture::create_dir_all(&journal);
    let files: Vec<_> = (0..files_wanted)
        .map(|index| {
            let file = dir.path().join(format!("cap-{index}.txt"));
            let mut content = "needle\n".to_string();
            content.push_str(&"a".repeat(bytes - content.len()));
            fixture::write_text(&file, &content);
            file
        })
        .collect();
    let replacements: Vec<_> = files
        .iter()
        .map(|file| Replacement {
            match_id: lushtext_model::workspace::content_search::SearchMatchId::from_index(0),
            path: file.clone(),
            line_number: 1,
            original_line: "needle".to_string().into(),
            replaced_line: "thread".to_string(),
            replacement: "thread".to_string().into(),
            match_range: 0..6,
        })
        .collect();
    let cancel = AtomicBool::new(false);
    let _ = super::apply::pass_timing::take();
    let _ = crate::filesystem::write::sync_timing::take();
    reset_hwm();
    let before = status_kib("VmRSS:");
    let start = std::time::Instant::now();
    let outcome =
        apply_replacements(&replacements, &HashSet::new(), &cancel, &journal).expect("replace");
    let wall = start.elapsed();
    let hwm = status_kib("VmHWM:");
    let [pass_a, pass_b] = super::apply::pass_timing::take();
    let fsync = crate::filesystem::write::sync_timing::take();
    let ms = |nanos: u128| nanos / 1_000_000;
    let (result, backup) = outcome.into_parts();
    println!(
        "sr-evidence {{\"m\":\"M9\",\"side\":\"branch\",\"file_bytes\":{bytes},\"files\":{files_wanted},\
         \"journal\":true,\"journal_requested\":{journal_requested},\"chunk_bytes\":{},\
         \"affected\":{},\"skipped\":{},\"backup_entries\":{},\"wall_ms\":{},\
         \"pass_a_ms\":{},\"pass_b_ms\":{},\"fsync_ms\":{},\"rss_before_kib\":{before},\
         \"hwm_kib\":{hwm}}}",
        chunk.unwrap_or(REPLACE_STREAM_CHUNK_BYTES),
        result.files_affected,
        result.skipped_count,
        backup.len(),
        wall.as_millis(),
        ms(pass_a),
        ms(pass_b),
        ms(fsync),
    );
}
